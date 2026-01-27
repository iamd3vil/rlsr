use crate::utils::{get_latest_commit_hash, get_latest_tag, is_at_latest_tag, is_repo_clean};
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
mod release_provider;
mod templating;
mod utils;

use config::{Build, BuildType, BuildxConfig, Config, Release};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Opts {
    pub skip_publish: bool,
    pub rm_dist: bool,
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

fn parse_version_meta(tag: &str) -> VersionMeta {
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

pub async fn build_template_meta(tag: String) -> Result<TemplateMeta> {
    let version_meta = parse_version_meta(&tag);
    // Git metadata is best-effort; previous tag may be empty on first release.
    let commit = utils::get_full_commit_hash().await?;
    let short_commit = utils::get_latest_commit_hash().await?;
    let branch = utils::get_current_branch().await?;
    let previous_tag = utils::get_previous_tag().await.unwrap_or_default();
    let project_name = utils::get_project_name();
    let release_url = utils::get_github_release_url(&tag)
        .await
        .unwrap_or_default();
    let is_snapshot = !is_at_latest_tag().await.unwrap_or(false);
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
        is_snapshot,
        is_prerelease,
        is_dirty,
        env,
        date,
        timestamp,
        now,
    })
}

/// Checks repository status (cleanliness, tag) and updates the publish flag.
async fn check_repo_status(publish: &mut bool) -> Result<()> {
    let is_clean = is_repo_clean()
        .await
        .context("Failed to check repo cleanliness")?;
    let at_latest_tag = is_at_latest_tag()
        .await
        .context("Failed to check if at latest tag")?;

    debug!("is_clean: {}, at_latest_tag: {}", is_clean, at_latest_tag);

    if !is_clean || !at_latest_tag {
        warn!("Repo is not clean or the latest commit is not tagged, skipping publishing.");
        *publish = false;
    }
    Ok(())
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

/// Determines the appropriate tag or commit hash for templating.
async fn get_template_metadata() -> Result<Arc<TemplateMeta>> {
    let tag = if is_at_latest_tag().await? {
        get_latest_tag().await?
    } else {
        get_latest_commit_hash().await?
    };
    let template_meta = build_template_meta(tag).await?;
    debug!("Tag/hash for templating: {}", template_meta.tag);
    Ok(Arc::new(template_meta))
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
) -> Result<()> {
    info!("Starting builds for release: '{}'", release.name);

    let expanded_builds = expand_builds(&release.builds)?;

    if release.builds_sequential {
        run_builds_sequentially(
            release,
            &expanded_builds,
            all_archives,
            all_image_tags,
            template_meta,
        )
        .await
    } else {
        run_builds_in_parallel(
            release,
            &expanded_builds,
            all_archives,
            all_image_tags,
            template_meta,
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
) -> Result<()> {
    let mut build_failures = Vec::new();
    let mut successful_builds = 0;

    for (index, expanded_build) in expanded_builds.iter().enumerate() {
        let build_config = &expanded_build.build;
        let build_name = &build_config.name;
        info!("Executing build: {}", build_name);
        let result =
            build::run_build(release, build_config, &template_meta, &expanded_build.matrix).await;

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
                    all_image_tags
                        .lock()
                        .await
                        .extend(build_result.image_tags);
                }
                successful_builds += 1;
            }
            Err(e) => {
                error!("Build '{}' failed: {:?}", build_name, e);
                build_failures.push(e.wrap_err(format!("Build #{} failed", index)));
            }
        }
    }

    finalize_build_results(release, expanded_builds.len(), successful_builds, build_failures)
}

async fn run_builds_in_parallel(
    release: &Release,
    expanded_builds: &[ExpandedBuild],
    all_archives: Arc<Mutex<Vec<String>>>,
    all_image_tags: Arc<Mutex<Vec<String>>>,
    template_meta: Arc<TemplateMeta>,
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

    finalize_build_results(release, expanded_builds.len(), successful_builds, build_failures)
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

fn expand_builds(builds: &[Build]) -> Result<Vec<ExpandedBuild>> {
    let mut expanded = Vec::new();

    for build in builds {
        if let Some(matrix_groups) = &build.matrix {
            if matrix_groups.is_empty() {
                bail!("matrix for build '{}' must define at least one entry", build.name);
            }

            for group in matrix_groups {
                let combinations = expand_matrix_group(group, &build.name)?;
                for matrix_values in combinations {
                    let mut build_clone = build.clone();
                    build_clone.matrix = None;
                    apply_matrix_to_build(&mut build_clone, &matrix_values);
                    expanded.push(ExpandedBuild {
                        build: build_clone,
                        matrix: matrix_values,
                    });
                }
            }
        } else {
            expanded.push(ExpandedBuild {
                build: build.clone(),
                matrix: BTreeMap::new(),
            });
        }
    }

    Ok(expanded)
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
) -> Result<()> {
    let latest_tag = match get_latest_tag().await {
        Ok(tag) => {
            info!("Publishing for tag: {}", tag);
            tag
        }
        Err(e) => {
            bail!("Cannot publish without a valid tag: {}", e);
        }
    };

    debug!("Latest tag for publishing: {}", latest_tag);

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
        // Use type_name_of_val for a more descriptive placeholder
        let provider_description = std::any::type_name_of_val(&*provider);
        info!("Publishing via {}", provider_description);
        let archives_clone = all_archives.clone();
        let image_tags_clone = image_tags.clone();
        match provider
            .publish(release, archives_clone, image_tags_clone, latest_tag.clone())
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

pub async fn run(cfg: Config, opts: Opts) -> Result<()> {
    let mut publish = !opts.skip_publish;
    if opts.skip_publish {
        warn!("--skip-publish is set, publishing will be skipped for all releases.");
    }

    // Initial repo status check affects default publish decision
    check_repo_status(&mut publish)
        .await
        .context("Initial repository status check failed")?;

    let releases = Arc::new(cfg.releases); // Use Arc for safe sharing across potential async boundaries if needed later

    for release_config in releases.iter() {
        info!("Processing release: '{}'", release_config.name);

        // Determine template metadata once per release run
        let template_meta = get_template_metadata()
            .await
            .context("Failed to get template metadata")?;

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
            .with_context(|| {
                format!("Before hooks failed for release '{}'", release_config.name)
            })?;
        }

        // Run builds
        let all_archives = Arc::new(Mutex::new(Vec::new()));
        let all_image_tags = Arc::new(Mutex::new(Vec::new()));
        run_builds_for_release(
            release_config,
            all_archives.clone(),
            all_image_tags.clone(),
            template_meta.clone(),
        )
        .await
        .with_context(|| {
            format!("Build process failed for release '{}'", release_config.name)
        })?;

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

        let changelog = utils::get_changelog(&cfg.changelog.clone().unwrap_or_default()).await?;
        debug!(
            "Changelog for release '{}': \n{}",
            release_config.name, changelog
        );

        // Publish artifacts if enabled for this run
        if publish {
            publish_release_artifacts(
                release_config,
                cfg.changelog.clone(),
                collected_archives,
                collected_image_tags,
            )
            .await
            .with_context(|| {
                format!("Publishing failed for release '{}'", release_config.name)
            })?;
        } else {
            info!(
                "Publishing skipped for release '{}' due to previous checks or --skip-publish flag.",
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

    fn base_build(build_type: BuildType) -> Build {
        Build {
            build_type,
            command: None,
            buildx: None,
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
        }
    }

    #[test]
    fn test_apply_matrix_to_buildx_fields() {
        let mut build = base_build(BuildType::Buildx);
        let mut matrix = BTreeMap::new();
        matrix.insert("platforms".to_string(), "linux/amd64".to_string());
        matrix.insert("tags".to_string(), "example:latest".to_string());
        matrix.insert("cache_from".to_string(), "type=registry,ref=cache".to_string());
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
        matrix.insert(
            "build_args.RUST_VERSION".to_string(),
            "1.75".to_string(),
        );
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
        assert_eq!(buildx.platforms.clone().unwrap(), vec!["linux/amd64".to_string()]);
        assert_eq!(buildx.tags.clone().unwrap(), vec!["example:latest".to_string()]);
        assert_eq!(buildx.cache_from.clone().unwrap(), vec!["type=registry,ref=cache".to_string()]);
        assert_eq!(buildx.cache_to.clone().unwrap(), vec!["type=inline".to_string()]);
        assert_eq!(buildx.outputs.clone().unwrap(), vec!["type=registry".to_string()]);
        assert_eq!(buildx.secrets.clone().unwrap(), vec!["id=token,src=./token".to_string()]);
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
            buildx.build_args.as_ref().and_then(|args| args.get("RUST_VERSION").cloned()),
            Some("1.75".to_string())
        );
        assert_eq!(
            buildx.labels.as_ref().and_then(|labels| labels.get("org.opencontainers.image.title").cloned()),
            Some("rlsr".to_string())
        );
        assert_eq!(
            buildx.annotations.as_ref().and_then(|annotations| annotations.get("org.opencontainers.image.description").cloned()),
            Some("desc".to_string())
        );
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
