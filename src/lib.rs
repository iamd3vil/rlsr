use crate::utils::{get_latest_tag, get_tag_at_head, is_repo_clean};
use chrono::{SecondsFormat, Utc};
use color_eyre::eyre::{bail, Context, Result};
use log::{debug, error, info, trace, warn};
use semver::Version;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::{fs, sync::Mutex};

mod build;
mod buildx;
mod changelog_formatter;
mod checksum;
mod checksummer;
pub mod config;
mod go_build;
mod release_provider;
mod rust_build;
mod split;
mod templating;
mod utils;

use config::{Build, BuildType, BuildxConfig, Config, Release};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Opts {
    pub skip_publish: bool,
    /// Build without a release tag or a clean tree, and never publish.
    pub snapshot: bool,
    pub rm_dist: bool,
    pub skip_build: bool,
    /// Build and package only, and write a manifest for a later merge run.
    pub split: bool,
    /// Skip builds; checksum and publish what split runs produced.
    pub merge: bool,
    /// Only run builds for these targets.
    pub targets: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateMeta {
    pub tag: String,
    pub version: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: String,
    pub commit: String,
    pub short_commit: String,
    pub branch: String,
    pub previous_tag: String,
    pub project_name: String,
    pub release_url: String,
    pub is_snapshot: bool,
    pub is_prerelease: bool,
    pub is_dirty: bool,
    pub env: HashMap<String, String>,
    pub date: String,
    pub timestamp: String,
    pub now: String,
}

impl crate::templating::TemplateContext for TemplateMeta {
    fn env(&self) -> &HashMap<String, String> {
        &self.env
    }

    fn date(&self) -> &str {
        &self.date
    }

    fn timestamp(&self) -> &str {
        &self.timestamp
    }

    fn now(&self) -> &str {
        &self.now
    }
}

// --- Helper Functions ---

#[derive(Debug, Clone)]
pub struct VersionMeta {
    pub tag: String,
    pub version: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: String,
}

pub(crate) fn parse_version_meta(tag: &str) -> VersionMeta {
    // Allow v/V-prefixed tags; non-semver tags fall back to empty version fields.
    let version_str = tag
        .strip_prefix('v')
        .or_else(|| tag.strip_prefix('V'))
        .unwrap_or(tag);
    if let Ok(version) = Version::parse(version_str) {
        VersionMeta {
            tag: tag.to_string(),
            version: version_str.to_string(),
            major: u32::try_from(version.major).unwrap_or(0),
            minor: u32::try_from(version.minor).unwrap_or(0),
            patch: u32::try_from(version.patch).unwrap_or(0),
            prerelease: version.pre.as_str().to_string(),
        }
    } else {
        VersionMeta {
            tag: tag.to_string(),
            version: String::new(),
            major: 0,
            minor: 0,
            patch: 0,
            prerelease: String::new(),
        }
    }
}

async fn build_template_meta(tag: String, previous_tag: String) -> Result<TemplateMeta> {
    let version_meta = parse_version_meta(&tag);
    let commit = utils::get_full_commit_hash().await?;
    let short_commit = utils::get_latest_commit_hash().await?;
    let branch = utils::get_current_branch().await?;
    let project_name = utils::get_project_name();
    let release_url = utils::get_github_release_url(&tag)
        .await
        .unwrap_or_default();
    let is_dirty = !is_repo_clean().await.unwrap_or(false);
    let is_prerelease = !version_meta.prerelease.is_empty();
    let env = std::env::vars().collect::<HashMap<String, String>>();
    let now = Utc::now();
    let date = now.format("%Y-%m-%d").to_string();
    let timestamp = now.timestamp().to_string();
    let now = now.to_rfc3339_opts(SecondsFormat::Secs, true);

    Ok(TemplateMeta {
        tag,
        version: version_meta.version,
        major: version_meta.major,
        minor: version_meta.minor,
        patch: version_meta.patch,
        prerelease: version_meta.prerelease,
        commit,
        short_commit,
        branch,
        previous_tag,
        project_name,
        release_url,
        is_snapshot: false,
        is_prerelease,
        is_dirty,
        env,
        date,
        timestamp,
        now,
    })
}

/// Handles the --rm-dist option and creates the distribution directory.
async fn prepare_dist_directory(release: &Release, rm_dist: bool) -> Result<()> {
    // Delete the dist directory if rm_dist is provided.
    if rm_dist {
        trace!("Deleting dist folder for release: {}", &release.dist_folder);
        if let Ok(meta) = fs::metadata(&release.dist_folder).await {
            if meta.is_dir() {
                fs::remove_dir_all(&release.dist_folder)
                    .await
                    .with_context(|| {
                        format!("Failed to remove dist dir: {}", release.dist_folder)
                    })?;
            } else {
                bail!(
                    "Error deleting dist dir '{}': Not a directory",
                    release.dist_folder
                );
            }
        }
        // If metadata check fails, it likely doesn't exist, which is fine.
    }

    // Create dist directory.
    trace!("Creating dist folder: {}", &release.dist_folder);
    fs::create_dir_all(&release.dist_folder)
        .await
        .with_context(|| format!("Failed to create dist dir: {}", release.dist_folder))?;

    Ok(())
}

const DEFAULT_SNAPSHOT_VERSION: &str =
    "{{ meta.version|incpatch }}-snapshot+{{ meta.short_commit }}";

/// Builds the template metadata for this run. A release needs a clean tree
/// with a tag at HEAD; a snapshot takes its version from
/// `snapshot.version_template` instead.
async fn get_template_metadata(cfg: &Config, opts: &Opts) -> Result<Arc<TemplateMeta>> {
    if !opts.snapshot {
        if !is_repo_clean().await? {
            bail!(
                "the working tree has uncommitted changes; commit them to release, or pass --snapshot to build anyway"
            );
        }
        let Some(tag) = get_tag_at_head().await? else {
            bail!(
                "HEAD isn't tagged; tag it to release, or pass --snapshot to build without a tag"
            );
        };
        // Describing HEAD^ skips HEAD's own tag. Empty on the first release.
        let previous_tag = utils::get_previous_tag().await.unwrap_or_default();
        let meta = build_template_meta(tag, previous_tag).await?;
        debug!("Tag for templating: {}", meta.tag);
        return Ok(Arc::new(meta));
    }

    // Snapshots version off the latest tag, and list the changes since it.
    let latest_tag = get_latest_tag().await.ok();
    let base_tag = latest_tag.clone().unwrap_or_else(|| "v0.0.0".to_string());
    let mut meta = build_template_meta(base_tag, latest_tag.unwrap_or_default()).await?;
    let template = cfg
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.version_template.as_deref())
        .unwrap_or(DEFAULT_SNAPSHOT_VERSION);
    let version = templating::render_template(template, &meta);
    set_version(&mut meta, &version);
    meta.tag = version;
    meta.is_snapshot = true;
    meta.release_url = String::new();
    info!("Building snapshot {}", meta.tag);
    Ok(Arc::new(meta))
}

/// Replaces the version fields with those parsed from `version`.
fn set_version(meta: &mut TemplateMeta, version: &str) {
    let parsed = parse_version_meta(version);
    meta.version = version.trim_start_matches(['v', 'V']).to_string();
    meta.major = parsed.major;
    meta.minor = parsed.minor;
    meta.patch = parsed.patch;
    meta.is_prerelease = !parsed.prerelease.is_empty();
    meta.prerelease = parsed.prerelease;
}

/// Executes a list of hook commands.
async fn execute_hooks(
    hooks: &Option<Vec<String>>,
    env: &Option<Vec<String>>,
    hook_type: HookType,
    template_meta: &TemplateMeta,
) -> Result<()> {
    if let Some(commands) = hooks {
        info!("Executing {} hooks...", hook_type);
        let envs = templating::render_envs(env, template_meta);
        for command in commands {
            let command = templating::render_template(command, template_meta);
            info!("Executing hook: {}", command);
            let output = utils::execute_command(&command, &envs)
                .await
                .with_context(|| {
                    format!(
                        "Failed to execute {} hook command: '{}'",
                        hook_type, command
                    )
                })?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                error!(
                    "{} hook command '{}' failed with status {:?}. Stderr:
{}",
                    hook_type,
                    command,
                    output.status.code(),
                    stderr
                );
                bail!(
                    "{} hook failed: '{}'. Status: {:?}. Stderr: {}",
                    hook_type,
                    command,
                    output.status.code(),
                    stderr
                );
            }
        }
    }
    Ok(())
}

/// Runs all builds for a given release configuration.
async fn run_builds_for_release(
    release: &Release,
    all_archives: Arc<Mutex<Vec<String>>>,
    all_image_tags: Arc<Mutex<Vec<String>>>,
    template_meta: Arc<TemplateMeta>,
    skip_build: bool,
    targets: &[String],
) -> Result<()> {
    info!("Starting builds for release: '{}'", release.name);

    let mut expanded_builds = expand_builds(release)?;
    if !targets.is_empty() {
        expanded_builds.retain(|expanded| {
            expanded
                .build
                .target
                .as_ref()
                .is_some_and(|target| targets.contains(target))
        });
        if expanded_builds.is_empty() {
            warn!(
                "No builds in release '{}' match --targets {}",
                release.name,
                targets.join(",")
            );
            return Ok(());
        }
    }

    if release.builds_sequential {
        run_builds_sequentially(
            release,
            &expanded_builds,
            all_archives,
            all_image_tags,
            template_meta,
            skip_build,
        )
        .await
    } else {
        run_builds_in_parallel(
            release,
            &expanded_builds,
            all_archives,
            all_image_tags,
            template_meta,
            skip_build,
        )
        .await
    }
}

async fn run_builds_sequentially(
    release: &Release,
    expanded_builds: &[ExpandedBuild],
    all_archives: Arc<Mutex<Vec<String>>>,
    all_image_tags: Arc<Mutex<Vec<String>>>,
    template_meta: Arc<TemplateMeta>,
    skip_build: bool,
) -> Result<()> {
    let mut build_failures = Vec::new();
    let mut successful_builds = 0;

    for (index, expanded_build) in expanded_builds.iter().enumerate() {
        let build_config = &expanded_build.build;
        let build_name = &build_config.name;
        info!("Executing build: {}", build_name);
        let result = build::run_build(
            release,
            build_config,
            &template_meta,
            &expanded_build.matrix,
            skip_build,
        )
        .await;

        match result {
            Ok(build_result) => {
                if let Some(archive_path) = build_result.archive_path.as_ref() {
                    debug!(
                        "Build '{}' successful, archive: {}",
                        build_name, archive_path
                    );
                    all_archives.lock().await.push(archive_path.clone());
                } else {
                    debug!("Build '{}' successful with no archive", build_name);
                }
                if !build_result.image_tags.is_empty() {
                    // Aggregate buildx tags for later publish steps.
                    all_image_tags.lock().await.extend(build_result.image_tags);
                }
                successful_builds += 1;
            }
            Err(e) => {
                error!("Build '{}' failed: {:?}", build_name, e);
                build_failures.push(e.wrap_err(format!("Build #{} failed", index)));
            }
        }
    }

    finalize_build_results(
        release,
        expanded_builds.len(),
        successful_builds,
        build_failures,
    )
}

async fn run_builds_in_parallel(
    release: &Release,
    expanded_builds: &[ExpandedBuild],
    all_archives: Arc<Mutex<Vec<String>>>,
    all_image_tags: Arc<Mutex<Vec<String>>>,
    template_meta: Arc<TemplateMeta>,
    skip_build: bool,
) -> Result<()> {
    let mut build_handles = vec![];

    for expanded_build in expanded_builds {
        let release_clone = release.clone();
        let build_config_clone = expanded_build.build.clone();
        let all_archives_clone = all_archives.clone();
        let all_image_tags_clone = all_image_tags.clone();
        let template_meta_clone = template_meta.clone();
        let matrix_clone = expanded_build.matrix.clone();

        build_handles.push(tokio::spawn(async move {
            let build_name = &build_config_clone.name;
            info!("Executing build: {}", build_name);
            let result = build::run_build(
                &release_clone,
                &build_config_clone,
                &template_meta_clone,
                &matrix_clone,
                skip_build,
            )
            .await;

            match result {
                Ok(build_result) => {
                    if let Some(archive_path) = build_result.archive_path.as_ref() {
                        debug!(
                            "Build '{}' successful, archive: {}",
                            build_name, archive_path
                        );
                        all_archives_clone.lock().await.push(archive_path.clone());
                    } else {
                        debug!("Build '{}' successful with no archive", build_name);
                    }
                    if !build_result.image_tags.is_empty() {
                        // Aggregate buildx tags for later publish steps.
                        all_image_tags_clone
                            .lock()
                            .await
                            .extend(build_result.image_tags);
                    }
                    Ok(())
                }
                Err(e) => {
                    error!("Build '{}' failed: {:?}", build_name, e);
                    Err(e.wrap_err(format!("Build '{}' execution failed", build_name)))
                }
            }
        }));
    }

    let build_results = futures::future::join_all(build_handles).await;

    let mut build_failures = Vec::new();
    let mut successful_builds = 0;

    for (index, join_result) in build_results.into_iter().enumerate() {
        match join_result {
            Ok(task_result) => match task_result {
                Ok(_) => successful_builds += 1,
                Err(build_err) => {
                    error!("Build #{} failed: {:?}", index, build_err);
                    build_failures.push(build_err.wrap_err(format!("Build #{} failed", index)));
                }
            },
            Err(join_err) => {
                error!("Build task #{} panicked: {}", index, join_err);
                build_failures.push(color_eyre::eyre::eyre!(
                    "Build task #{} panicked: {}",
                    index,
                    join_err
                ));
            }
        }
    }

    finalize_build_results(
        release,
        expanded_builds.len(),
        successful_builds,
        build_failures,
    )
}

fn finalize_build_results(
    release: &Release,
    total_builds: usize,
    successful_builds: usize,
    build_failures: Vec<color_eyre::Report>,
) -> Result<()> {
    if !build_failures.is_empty() {
        let failure_details = build_failures
            .iter()
            .map(|e| format!("  - {:?}", e))
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        warn!(
            "{} out of {} builds failed for release '{}'. Failures:
{}",
            build_failures.len(),
            total_builds,
            release.name,
            failure_details
        );
        bail!(
            "Build process aborted due to {} failures in release '{}'. See logs for details.",
            build_failures.len(),
            release.name
        );
    }

    info!(
        "All {} builds for release '{}' completed successfully.",
        successful_builds, release.name
    );
    Ok(())
}

#[derive(Clone, Debug)]
struct ExpandedBuild {
    build: Build,
    matrix: BTreeMap<String, String>,
}

fn expand_builds(release: &Release) -> Result<Vec<ExpandedBuild>> {
    let mut expanded = Vec::new();

    for build in &release.builds {
        if let Some(matrix_groups) = &build.matrix {
            if matrix_groups.is_empty() {
                bail!(
                    "matrix for build '{}' must define at least one entry",
                    build.name
                );
            }

            for group in matrix_groups {
                let combinations = expand_matrix_group(group, &build.name)?;
                for matrix_values in combinations {
                    let mut build_clone = build.clone();
                    build_clone.matrix = None;
                    apply_matrix_to_build(&mut build_clone, &matrix_values);
                    for build in expand_targets(release, build_clone)? {
                        expanded.push(ExpandedBuild {
                            build,
                            matrix: matrix_values.clone(),
                        });
                    }
                }
            }
        } else {
            for build in expand_targets(release, build.clone())? {
                expanded.push(ExpandedBuild {
                    build,
                    matrix: BTreeMap::new(),
                });
            }
        }
    }

    Ok(expanded)
}

/// Expands rust and go builds into one build per target. Other build types
/// pass through unchanged.
fn expand_targets(release: &Release, build: Build) -> Result<Vec<Build>> {
    let mut builds = match build.build_type {
        BuildType::Custom | BuildType::Buildx => return Ok(vec![build]),
        _ if build.command.is_some() => bail!(
            "build '{}' sets `command`, but rust and go builds generate their own",
            build.name
        ),
        BuildType::Rust => rust_build::expand(&build, release)?,
        BuildType::Go => go_build::expand(&build, release)?,
    };

    if builds.len() > 1 && !build.archive_name.contains("{{") {
        bail!(
            "build '{}' has multiple targets, so archive_name needs a template like \
             {{{{ meta.os }}}}-{{{{ meta.arch }}}} to keep the archives apart",
            build.name
        );
    }

    // Tell targets apart in logs unless the name is already templated.
    for build in &mut builds {
        if !build.name.contains("{{") {
            if let Some(target) = &build.target {
                build.name = format!("{} ({})", build.name, target);
            }
        }
    }

    Ok(builds)
}

fn expand_matrix_group(
    group: &BTreeMap<String, Vec<String>>,
    build_name: &str,
) -> Result<Vec<BTreeMap<String, String>>> {
    let mut combinations = vec![BTreeMap::new()];

    for (key, values) in group {
        if values.is_empty() {
            bail!(
                "matrix field '{}' for build '{}' must include at least one value",
                key,
                build_name
            );
        }

        let mut next = Vec::new();
        for existing in &combinations {
            for value in values {
                let mut updated = existing.clone();
                updated.insert(key.clone(), value.clone());
                next.push(updated);
            }
        }
        combinations = next;
    }

    Ok(combinations)
}

fn ensure_buildx_config(build: &mut Build) -> &mut BuildxConfig {
    if build.buildx.is_none() {
        build.buildx = Some(BuildxConfig::default());
    }
    build
        .buildx
        .as_mut()
        .expect("buildx config should be initialized")
}

fn parse_bool_value(value: &str) -> Option<bool> {
    value.parse::<bool>().ok()
}

fn apply_matrix_to_build(build: &mut Build, matrix: &BTreeMap<String, String>) {
    for (key, value) in matrix {
        match key.as_str() {
            "os" => build.os = Some(value.clone()),
            "arch" => build.arch = Some(value.clone()),
            "arm" => build.arm = Some(value.clone()),
            "target" => {
                // For buildx builds, map target to the Dockerfile stage.
                if build.build_type == BuildType::Buildx {
                    let buildx = ensure_buildx_config(build);
                    buildx.target = Some(value.clone());
                } else {
                    build.target = Some(value.clone());
                }
            }
            "platforms" => {
                let buildx = ensure_buildx_config(build);
                buildx.platforms = Some(vec![value.clone()]);
            }
            "tags" => {
                let buildx = ensure_buildx_config(build);
                buildx.tags = Some(vec![value.clone()]);
            }
            "cache_from" => {
                let buildx = ensure_buildx_config(build);
                buildx.cache_from = Some(vec![value.clone()]);
            }
            "cache_to" => {
                let buildx = ensure_buildx_config(build);
                buildx.cache_to = Some(vec![value.clone()]);
            }
            "outputs" => {
                let buildx = ensure_buildx_config(build);
                buildx.outputs = Some(vec![value.clone()]);
            }
            "secrets" => {
                let buildx = ensure_buildx_config(build);
                buildx.secrets = Some(vec![value.clone()]);
            }
            "ssh" => {
                let buildx = ensure_buildx_config(build);
                buildx.ssh = Some(vec![value.clone()]);
            }
            "annotations" => {
                if let Some((annotation_key, annotation_value)) = value.split_once('=') {
                    let buildx = ensure_buildx_config(build);
                    let annotations = buildx.annotations.get_or_insert_with(BTreeMap::new);
                    annotations.insert(annotation_key.to_string(), annotation_value.to_string());
                }
            }
            "builder" => {
                let buildx = ensure_buildx_config(build);
                buildx.builder = Some(value.clone());
            }
            "context" => {
                let buildx = ensure_buildx_config(build);
                buildx.context = Some(value.clone());
            }
            "dockerfile" => {
                let buildx = ensure_buildx_config(build);
                buildx.dockerfile = Some(value.clone());
            }
            "load" => {
                if let Some(parsed) = parse_bool_value(value) {
                    let buildx = ensure_buildx_config(build);
                    buildx.load = Some(parsed);
                }
            }
            "provenance" => {
                if let Some(parsed) = parse_bool_value(value) {
                    let buildx = ensure_buildx_config(build);
                    buildx.provenance = Some(parsed);
                }
            }
            "sbom" => {
                if let Some(parsed) = parse_bool_value(value) {
                    let buildx = ensure_buildx_config(build);
                    buildx.sbom = Some(parsed);
                }
            }
            _ => {
                // Allow matrix entries to target buildx maps via dotted keys.
                if let Some(arg_key) = key.strip_prefix("build_args.") {
                    let buildx = ensure_buildx_config(build);
                    let args = buildx.build_args.get_or_insert_with(BTreeMap::new);
                    args.insert(arg_key.to_string(), value.clone());
                    continue;
                }
                if let Some(label_key) = key.strip_prefix("labels.") {
                    let buildx = ensure_buildx_config(build);
                    let labels = buildx.labels.get_or_insert_with(BTreeMap::new);
                    labels.insert(label_key.to_string(), value.clone());
                    continue;
                }
                if let Some(annotation_key) = key.strip_prefix("annotations.") {
                    let buildx = ensure_buildx_config(build);
                    let annotations = buildx.annotations.get_or_insert_with(BTreeMap::new);
                    annotations.insert(annotation_key.to_string(), value.clone());
                }
            }
        }
    }
}

/// Creates checksums if configured for the release.
async fn create_checksums_if_needed(release: &Release, all_archives: Vec<String>) -> Result<()> {
    if release.checksum.is_some() {
        info!("Creating checksums for release '{}'...", release.name);
        checksum::create_checksums(release, all_archives)
            .await
            .context("Failed to create checksums")?;
        info!("Checksum creation complete for release '{}'.", release.name);
    } else {
        debug!("Checksum creation skipped for release '{}'.", release.name);
    }
    Ok(())
}

/// Publishes release artifacts using configured providers.
async fn publish_release_artifacts(
    release: &Release,
    changelog_config: Option<config::Changelog>,
    all_archives: Vec<String>,
    image_tags: Vec<String>,
    meta: &TemplateMeta,
) -> Result<()> {
    info!("Publishing for tag: {}", meta.tag);

    let providers = utils::get_release_providers(release, changelog_config)
        .context("Failed to initialize release providers")?;

    if providers.is_empty() {
        info!(
            "No release providers configured for release '{}'. Skipping publish step.",
            release.name
        );
        return Ok(());
    }

    let mut publish_errors = Vec::new();
    info!(
        "Publishing artifacts for release '{}' using {} providers...",
        release.name,
        providers.len()
    );

    for provider in providers {
        let provider_description = provider.name();
        info!("Publishing via {}", provider_description);
        let archives_clone = all_archives.clone();
        let image_tags_clone = image_tags.clone();
        match provider
            .publish(release, archives_clone, image_tags_clone, meta)
            .await
        {
            Ok(_) => info!("Successfully published via {}", provider_description),
            Err(err) => {
                error!("Failed to publish via {}: {:?}", provider_description, err);
                // Use wrap_err for existing eyre::Error
                publish_errors
                    .push(err.wrap_err(format!("Provider '{}' failed", provider_description)));
            }
        }
    }

    if !publish_errors.is_empty() {
        let error_details = publish_errors
            .iter()
            .map(|e| format!("  - {:?}", e)) // Use debug format
            .collect::<Vec<_>>()
            .join(
                "
",
            );
        bail!(
            "{} publish errors occurred for release '{}':
{}",
            publish_errors.len(),
            release.name,
            error_details
        );
    }

    info!(
        "Successfully published artifacts for release '{}'.",
        release.name
    );
    Ok(())
}

/// Runs the hooks and builds for a release and returns the archives and image
/// tags they produced.
async fn build_release(
    release_config: &Release,
    opts: &Opts,
    template_meta: Arc<TemplateMeta>,
) -> Result<(Vec<String>, Vec<String>)> {
    // Prepare distribution directory
    prepare_dist_directory(release_config, opts.rm_dist)
        .await
        .with_context(|| format!("Failed preparation for release '{}'", release_config.name))?;

    // Execute before hooks
    if let Some(hooks) = &release_config.hooks {
        execute_hooks(
            &hooks.before,
            &release_config.env,
            HookType::Before,
            template_meta.as_ref(),
        )
        .await
        .with_context(|| format!("Before hooks failed for release '{}'", release_config.name))?;
    }

    // Run builds
    let all_archives = Arc::new(Mutex::new(Vec::new()));
    let all_image_tags = Arc::new(Mutex::new(Vec::new()));
    run_builds_for_release(
        release_config,
        all_archives.clone(),
        all_image_tags.clone(),
        template_meta.clone(),
        opts.skip_build,
        &opts.targets,
    )
    .await
    .with_context(|| format!("Build process failed for release '{}'", release_config.name))?;

    // Execute after hooks
    if let Some(hooks) = &release_config.hooks {
        execute_hooks(
            &hooks.after,
            &release_config.env,
            HookType::After,
            template_meta.as_ref(),
        )
        .await
        .with_context(|| format!("After hooks failed for release '{}'", release_config.name))?;
    }

    // Collect archive paths
    let collected_archives = {
        let lock = all_archives.lock().await;
        lock.clone()
    };
    let collected_image_tags = {
        let lock = all_image_tags.lock().await;
        lock.clone()
    };
    Ok((collected_archives, collected_image_tags))
}

pub async fn run(cfg: Config, opts: Opts) -> Result<()> {
    let publish = !opts.skip_publish && !opts.snapshot;
    if opts.skip_publish {
        warn!("--skip-publish is set, publishing will be skipped for all releases.");
    }
    if opts.skip_build {
        warn!("--skip-build is set, using existing artifacts instead of running builds.");
    }

    let template_meta = get_template_metadata(&cfg, &opts).await?;

    let releases = Arc::new(cfg.releases); // Use Arc for safe sharing across potential async boundaries if needed later

    for release_config in releases.iter() {
        info!("Processing release: '{}'", release_config.name);

        let (collected_archives, collected_image_tags) = if opts.merge {
            let (archives, image_tags) = split::read_manifests(release_config)
                .await
                .with_context(|| format!("Merge failed for release '{}'", release_config.name))?;
            info!(
                "Merged {} archives and {} image tags from split runs for release '{}'",
                archives.len(),
                image_tags.len(),
                release_config.name
            );
            (archives, image_tags)
        } else {
            build_release(release_config, &opts, template_meta.clone()).await?
        };

        if opts.split {
            if collected_archives.is_empty() && collected_image_tags.is_empty() {
                warn!(
                    "No archives or images were produced for release '{}'. Skipping split manifest.",
                    release_config.name
                );
                continue;
            }
            let manifest = split::write_manifest(
                release_config,
                &opts.targets,
                &collected_archives,
                &collected_image_tags,
            )
            .await?;
            info!(
                "Wrote split manifest {}; checksums and publishing happen in the --merge run.",
                manifest
            );
            continue;
        }

        if collected_archives.is_empty() && collected_image_tags.is_empty() {
            warn!(
                "No archives or images were produced for release '{}'. Skipping checksums and publishing.",
                release_config.name
            );
            continue;
        }

        if collected_archives.is_empty() {
            warn!(
                "No archives were produced for release '{}'. Skipping checksum generation.",
                release_config.name
            );
        } else {
            // Create checksums
            create_checksums_if_needed(release_config, collected_archives.clone())
                .await
                .with_context(|| {
                    format!(
                        "Checksum creation failed for release '{}'",
                        release_config.name
                    )
                })?;
        }

        // Providers build their own changelog; this one is only logged as a
        // preview, so a run that doesn't publish shouldn't fail on it (for
        // example, github_notes without a GITHUB_TOKEN).
        match utils::get_changelog(&cfg.changelog.clone().unwrap_or_default(), &template_meta).await
        {
            Ok(changelog) => debug!(
                "Changelog for release '{}': \n{}",
                release_config.name, changelog
            ),
            Err(err) if !publish => warn!(
                "Couldn't generate the changelog for release '{}': {:#}",
                release_config.name, err
            ),
            Err(err) => return Err(err),
        }

        // Publish artifacts if enabled for this run
        if publish {
            publish_release_artifacts(
                release_config,
                cfg.changelog.clone(),
                collected_archives,
                collected_image_tags,
                &template_meta,
            )
            .await
            .with_context(|| format!("Publishing failed for release '{}'", release_config.name))?;
        } else {
            info!(
                "Publishing skipped for release '{}' (--skip-publish or --snapshot).",
                release_config.name
            );
        }

        info!("Successfully processed release: '{}'", release_config.name);
    } // End loop over releases

    info!("All releases processed.");
    Ok(())
}

// Placeholder: Define HookType here. Ideally, move this to src/config.rs and make it public.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookType {
    Before,
    After,
}

impl From<&str> for HookType {
    fn from(value: &str) -> Self {
        match value {
            "after" => HookType::After,
            "before" => HookType::Before,
            _ => panic!("Invalid hook type: {}", value),
        }
    }
}

// Implement Display for nice printing in logs
impl std::fmt::Display for HookType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ArchiveFormat;

    fn base_build(build_type: BuildType) -> Build {
        Build {
            build_type,
            command: None,
            buildx: None,
            rust: None,
            go: None,
            artifact: "./bin/app".to_string(),
            bin_name: None,
            archive_name: "app".to_string(),
            name: "Matrix build".to_string(),
            os: None,
            arch: None,
            arm: None,
            target: None,
            matrix: None,
            env: None,
            prehook: None,
            posthook: None,
            no_archive: None,
            additional_files: None,
            archive_format: ArchiveFormat::default(),
        }
    }

    #[test]
    fn test_apply_matrix_to_buildx_fields() {
        let mut build = base_build(BuildType::Buildx);
        let mut matrix = BTreeMap::new();
        matrix.insert("platforms".to_string(), "linux/amd64".to_string());
        matrix.insert("tags".to_string(), "example:latest".to_string());
        matrix.insert(
            "cache_from".to_string(),
            "type=registry,ref=cache".to_string(),
        );
        matrix.insert("cache_to".to_string(), "type=inline".to_string());
        matrix.insert("outputs".to_string(), "type=registry".to_string());
        matrix.insert("secrets".to_string(), "id=token,src=./token".to_string());
        matrix.insert("ssh".to_string(), "default".to_string());
        matrix.insert("builder".to_string(), "rlsr-builder".to_string());
        matrix.insert("context".to_string(), "./context".to_string());
        matrix.insert("dockerfile".to_string(), "./Dockerfile".to_string());
        matrix.insert("target".to_string(), "release".to_string());
        matrix.insert("provenance".to_string(), "true".to_string());
        matrix.insert("sbom".to_string(), "false".to_string());
        matrix.insert("load".to_string(), "true".to_string());
        matrix.insert("build_args.RUST_VERSION".to_string(), "1.75".to_string());
        matrix.insert(
            "labels.org.opencontainers.image.title".to_string(),
            "rlsr".to_string(),
        );
        matrix.insert(
            "annotations.org.opencontainers.image.description".to_string(),
            "desc".to_string(),
        );

        apply_matrix_to_build(&mut build, &matrix);

        let buildx = build.buildx.as_ref().expect("buildx should be initialized");
        assert_eq!(
            buildx.platforms.clone().unwrap(),
            vec!["linux/amd64".to_string()]
        );
        assert_eq!(
            buildx.tags.clone().unwrap(),
            vec!["example:latest".to_string()]
        );
        assert_eq!(
            buildx.cache_from.clone().unwrap(),
            vec!["type=registry,ref=cache".to_string()]
        );
        assert_eq!(
            buildx.cache_to.clone().unwrap(),
            vec!["type=inline".to_string()]
        );
        assert_eq!(
            buildx.outputs.clone().unwrap(),
            vec!["type=registry".to_string()]
        );
        assert_eq!(
            buildx.secrets.clone().unwrap(),
            vec!["id=token,src=./token".to_string()]
        );
        assert_eq!(buildx.ssh.clone().unwrap(), vec!["default".to_string()]);
        assert_eq!(buildx.builder.as_deref(), Some("rlsr-builder"));
        assert_eq!(buildx.context.as_deref(), Some("./context"));
        assert_eq!(buildx.dockerfile.as_deref(), Some("./Dockerfile"));
        assert_eq!(buildx.target.as_deref(), Some("release"));
        assert_eq!(buildx.provenance, Some(true));
        assert_eq!(buildx.sbom, Some(false));
        assert_eq!(buildx.load, Some(true));
        assert_eq!(build.target, None);
        assert_eq!(
            buildx
                .build_args
                .as_ref()
                .and_then(|args| args.get("RUST_VERSION").cloned()),
            Some("1.75".to_string())
        );
        assert_eq!(
            buildx
                .labels
                .as_ref()
                .and_then(|labels| labels.get("org.opencontainers.image.title").cloned()),
            Some("rlsr".to_string())
        );
        assert_eq!(
            buildx
                .annotations
                .as_ref()
                .and_then(|annotations| annotations
                    .get("org.opencontainers.image.description")
                    .cloned()),
            Some("desc".to_string())
        );
    }

    fn release_with(builds: Vec<Build>) -> Release {
        Release {
            name: "Test".to_string(),
            dist_folder: "dist".to_string(),
            builds,
            targets: config::ReleaseTargets {
                github: None,
                gitlab: None,
                docker: None,
            },
            checksum: None,
            env: None,
            builds_sequential: false,
            additional_files: None,
            hooks: None,
        }
    }

    fn rust_build(targets: &[&str]) -> Build {
        let mut build = base_build(BuildType::Rust);
        build.artifact = String::new();
        build.name = "Rust".to_string();
        build.archive_name = "app-{{ meta.os }}-{{ meta.arch }}".to_string();
        build.rust = Some(config::RustConfig {
            targets: targets.iter().map(|t| t.to_string()).collect(),
            binary: Some("app".to_string()),
            ..Default::default()
        });
        build
    }

    #[test]
    fn test_expand_builds_per_rust_target_with_matrix() {
        let mut build = rust_build(&["x86_64-unknown-linux-musl", "aarch64-apple-darwin"]);
        build.matrix = Some(vec![BTreeMap::from([(
            "features".to_string(),
            vec!["a".to_string(), "b".to_string()],
        )])]);

        let expanded = expand_builds(&release_with(vec![build])).unwrap();

        assert_eq!(expanded.len(), 4);
        assert_eq!(expanded[0].build.name, "Rust (x86_64-unknown-linux-musl)");
        assert_eq!(expanded[0].matrix.get("features").unwrap(), "a");
        assert_eq!(expanded[1].build.name, "Rust (aarch64-apple-darwin)");
        assert_eq!(expanded[3].matrix.get("features").unwrap(), "b");
    }

    #[test]
    fn test_expand_builds_keeps_templated_name() {
        let mut build = rust_build(&["x86_64-unknown-linux-musl"]);
        build.name = "Rust {{ meta.os }}".to_string();
        let expanded = expand_builds(&release_with(vec![build])).unwrap();
        assert_eq!(expanded[0].build.name, "Rust {{ meta.os }}");
    }

    #[test]
    fn test_expand_builds_rejects_shared_archive_name() {
        let mut build = rust_build(&["x86_64-unknown-linux-musl", "aarch64-apple-darwin"]);
        build.archive_name = "app".to_string();
        assert!(expand_builds(&release_with(vec![build])).is_err());

        let mut build = rust_build(&["x86_64-unknown-linux-musl"]);
        build.archive_name = "app".to_string();
        assert!(expand_builds(&release_with(vec![build])).is_ok());
    }

    #[test]
    fn test_expand_builds_rejects_command_on_native_build() {
        let mut build = rust_build(&["x86_64-unknown-linux-musl"]);
        build.command = Some("cargo build".to_string());
        assert!(expand_builds(&release_with(vec![build])).is_err());
    }

    fn release_meta() -> TemplateMeta {
        TemplateMeta {
            tag: "v1.2.3".to_string(),
            version: "1.2.3".to_string(),
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: String::new(),
            commit: "abcdef1234567890".to_string(),
            short_commit: "abc1234".to_string(),
            branch: "main".to_string(),
            previous_tag: "v1.2.2".to_string(),
            project_name: "rlsr".to_string(),
            release_url: String::new(),
            is_snapshot: false,
            is_prerelease: false,
            is_dirty: false,
            env: HashMap::new(),
            date: "2025-01-25".to_string(),
            timestamp: "1706180400".to_string(),
            now: "2025-01-25T10:30:00Z".to_string(),
        }
    }

    #[test]
    fn test_default_snapshot_version() {
        let mut meta = release_meta();
        let version = templating::render_template(DEFAULT_SNAPSHOT_VERSION, &meta);
        assert_eq!(version, "1.2.4-snapshot+abc1234");

        set_version(&mut meta, &version);
        assert_eq!(meta.version, "1.2.4-snapshot+abc1234");
        assert_eq!((meta.major, meta.minor, meta.patch), (1, 2, 4));
        assert_eq!(meta.prerelease, "snapshot");
        assert!(meta.is_prerelease);
    }

    #[test]
    fn test_set_version_strips_v_prefix() {
        let mut meta = release_meta();
        set_version(&mut meta, "v2.0.0");
        assert_eq!(meta.version, "2.0.0");
        assert_eq!((meta.major, meta.minor, meta.patch), (2, 0, 0));
        assert!(!meta.is_prerelease);
    }

    #[test]
    fn test_apply_matrix_to_custom_target_field() {
        let mut build = base_build(BuildType::Custom);
        let mut matrix = BTreeMap::new();
        matrix.insert("target".to_string(), "x86_64-unknown-linux-gnu".to_string());

        apply_matrix_to_build(&mut build, &matrix);

        assert_eq!(build.target.as_deref(), Some("x86_64-unknown-linux-gnu"));
        assert!(build.buildx.is_none());
    }
}
