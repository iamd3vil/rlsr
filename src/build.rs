use crate::TemplateMeta;
use camino::Utf8Path;
use color_eyre::{
    eyre::{bail, Context, ContextCompat},
    Result,
};
use log::{debug, info};
use serde::Serialize;
use tokio::fs;

use crate::{
    config::{Build, Release},
    utils::{self, archive_files, ArchiveFile},
};

#[derive(Debug, Clone, Serialize)]
pub struct BuildMeta {
    pub build_name: String,
    pub tag: String,
    pub version: String,
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
    pub prerelease: String,
    pub short_commit: String,
    pub env: std::collections::HashMap<String, String>,
    pub date: String,
    pub timestamp: String,
    pub now: String,
    pub os: String,
    pub arch: String,
    pub arm: String,
    pub target: String,
}

impl utils::TemplateContext for BuildMeta {
    fn env(&self) -> &std::collections::HashMap<String, String> {
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

pub async fn run_build(release: &Release, build: &Build, meta: &TemplateMeta) -> Result<String> {
    let build_meta = create_build_meta(build, meta);

    // Execute prehook if present
    execute_prehook(release, build, &build_meta).await?;

    // Execute main build command
    let output = execute_build_command(release, build, &build_meta).await?;

    // Check if build was successful.
    if !output.status.success() {
        bail!("build failed: {}", build.name);
    }

    // Execute posthook if present
    execute_posthook(release, build, &build_meta).await?;

    // Copy artifact and create archive if needed
    process_artifacts(release, build, &build_meta).await
}

fn create_build_meta(build: &Build, meta: &TemplateMeta) -> BuildMeta {
    BuildMeta {
        build_name: build.name.clone(),
        tag: meta.tag.clone(),
        version: meta.version.clone(),
        major: meta.major,
        minor: meta.minor,
        patch: meta.patch,
        prerelease: meta.prerelease.clone(),
        short_commit: meta.short_commit.clone(),
        env: meta.env.clone(),
        date: meta.date.clone(),
        timestamp: meta.timestamp.clone(),
        now: meta.now.clone(),
        os: build.os.clone().unwrap_or_default(),
        arch: build.arch.clone().unwrap_or_default(),
        arm: build.arm.clone().unwrap_or_default(),
        target: build.target.clone().unwrap_or_default(),
    }
}

fn collect_envs(release: &Release, build: &Build, build_meta: &BuildMeta) -> Option<Vec<String>> {
    let mut envs = Vec::new();
    if let Some(release_envs) = utils::render_envs(&release.env, build_meta) {
        envs.extend(release_envs);
    }
    if let Some(build_envs) = utils::render_envs(&build.env, build_meta) {
        envs.extend(build_envs);
    }
    if envs.is_empty() {
        None
    } else {
        Some(envs)
    }
}

async fn execute_prehook(release: &Release, build: &Build, build_meta: &BuildMeta) -> Result<()> {
    if let Some(prehook) = &build.prehook {
        let prehook = utils::render_template(prehook, build_meta);

        info!("executing prehook: `{}` for build: {}", prehook, build.name);

        let envs = collect_envs(release, build, build_meta);

        let output = utils::execute_command(&prehook, &envs).await?;
        if !output.status.success() {
            bail!("prehook failed: {}", prehook);
        }
    }
    Ok(())
}

async fn execute_build_command(
    release: &Release,
    build: &Build,
    build_meta: &BuildMeta,
) -> Result<std::process::Output> {
    debug!("executing command: {}", build.command);

    let envs = collect_envs(release, build, build_meta);

    debug!("envs: {:?}", envs);

    let cmd = utils::render_template(&build.command, build_meta);
    utils::execute_command(&cmd, &envs).await
}

async fn execute_posthook(release: &Release, build: &Build, build_meta: &BuildMeta) -> Result<()> {
    if let Some(posthook) = &build.posthook {
        let posthook = utils::render_template(posthook, build_meta);
        info!(
            "executing posthook: `{}` for build: {}",
            posthook, build.name
        );

        let envs = collect_envs(release, build, build_meta);

        let output = utils::execute_command(&posthook, &envs).await?;
        if !output.status.success() {
            bail!("posthook failed: {}", posthook);
        }
    }
    Ok(())
}

async fn process_artifacts(
    release: &Release,
    build: &Build,
    build_meta: &BuildMeta,
) -> Result<String> {
    let bin_name = build.bin_name.as_ref().unwrap_or(&build.archive_name);
    let bin_name = utils::render_template(bin_name, build_meta);
    let artifact = utils::render_template(&build.artifact, build_meta);

    // Copy artifact to dist folder
    let bin_path = copy_artifact_to_dist(release, &artifact, &bin_name).await?;

    let archive_name = utils::render_template(&build.archive_name, build_meta);
    let no_archive = build.no_archive.is_some_and(|val| val);

    if !no_archive {
        // Create an archive
        debug!("creating an archive for {}", &archive_name);

        let files = prepare_archive_files(release, build, &artifact, &bin_path, build_meta).await?;

        let zip_path = archive_files(files, release.dist_folder.clone(), archive_name.clone())
            .await
            .with_context(|| format!("error while creating archive for build: {}", archive_name))?;

        Ok(zip_path)
    } else {
        // Copy artifact with the final name
        copy_artifact_with_name(release, &artifact, &archive_name).await?;

        Ok(Utf8Path::new(&release.dist_folder)
            .join(&archive_name)
            .to_string())
    }
}

async fn copy_artifact_to_dist(
    release: &Release,
    artifact: &str,
    bin_name: &str,
) -> Result<String> {
    let bin_path = Utf8Path::new(&release.dist_folder).join(bin_name);

    fs::copy(artifact, &bin_path)
        .await
        .with_context(|| format!("error while copying artifact: {}", artifact))?;

    Ok(bin_path.to_string())
}

async fn prepare_archive_files(
    release: &Release,
    build: &Build,
    artifact: &str,
    bin_path: &str,
    build_meta: &BuildMeta,
) -> Result<Vec<ArchiveFile>> {
    // Get the binary name from artifact
    let artifact_path = Utf8Path::new(artifact);
    let bin_name = artifact_path
        .file_name()
        .with_context(|| format!("error getting filename from artifact: {}", artifact))?;

    // Add all files that need to be archived
    let bin_file = ArchiveFile {
        disk_path: bin_path.to_string(),
        archive_filename: bin_name.to_string(),
    };

    let mut files = vec![bin_file];

    // Add build-specific additional files
    if let Some(additional_files) = &build.additional_files {
        files.extend(
            additional_files
                .iter()
                .map(|f| utils::render_template(f, build_meta))
                .map(|f| ArchiveFile {
                    disk_path: f.clone(),
                    archive_filename: Utf8Path::new(&f).file_name().unwrap().to_string(),
                })
                .collect::<Vec<ArchiveFile>>(),
        );
    }

    // Add release-specific additional files
    if let Some(rls_additional_files) = &release.additional_files {
        files.extend(
            rls_additional_files
                .iter()
                .map(|f| utils::render_template(f, build_meta))
                .map(|f| ArchiveFile {
                    disk_path: f.clone(),
                    archive_filename: Utf8Path::new(&f).file_name().unwrap().to_string(),
                })
                .collect::<Vec<ArchiveFile>>(),
        );
    }

    // Sort and only keep unique files
    files.sort_unstable();
    files.dedup();

    debug!("files being archived: {:?}", files);

    Ok(files)
}

async fn copy_artifact_with_name(
    release: &Release,
    artifact: &str,
    archive_name: &str,
) -> Result<()> {
    fs::copy(
        artifact,
        Utf8Path::new(&release.dist_folder).join(archive_name),
    )
    .await
    .with_context(|| "error while copying artifact to given name")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReleaseTargets;
    use crate::TemplateMeta;

    fn test_template_meta() -> TemplateMeta {
        let mut env = std::collections::HashMap::new();
        env.insert("RLSR_TEST".to_string(), "1".to_string());
        TemplateMeta {
            tag: "v1.2.3".to_string(),
            version: "1.2.3".to_string(),
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: String::new(),
            commit: "abcdef1234567890".to_string(),
            short_commit: "abcdef1".to_string(),
            branch: "main".to_string(),
            previous_tag: "v1.2.2".to_string(),
            project_name: "rlsr".to_string(),
            env,
            date: "2025-01-25".to_string(),
            timestamp: "1706180400".to_string(),
            now: "2025-01-25T10:30:00Z".to_string(),
        }
    }

    fn base_release() -> Release {
        Release {
            name: "Test Release".to_string(),
            dist_folder: "./dist".to_string(),
            builds: Vec::new(),
            targets: ReleaseTargets {
                github: None,
                docker: None,
            },
            checksum: None,
            env: None,
            additional_files: None,
            hooks: None,
        }
    }

    fn base_build() -> Build {
        Build {
            command: "echo build".to_string(),
            artifact: "./bin/rlsr".to_string(),
            bin_name: None,
            archive_name: "rlsr.tar.gz".to_string(),
            name: "Linux build".to_string(),
            os: None,
            arch: None,
            arm: None,
            target: None,
            env: None,
            prehook: None,
            posthook: None,
            no_archive: None,
            additional_files: None,
        }
    }

    #[test]
    fn test_create_build_meta_populates_version_and_target_fields() {
        let mut build = base_build();
        build.os = Some("linux".to_string());
        build.arch = Some("amd64".to_string());
        build.arm = Some("7".to_string());
        build.target = Some("x86_64-unknown-linux-musl".to_string());

        let meta = test_template_meta();
        let build_meta = create_build_meta(&build, &meta);

        assert_eq!(build_meta.build_name, build.name);
        assert_eq!(build_meta.tag, meta.tag);
        assert_eq!(build_meta.version, meta.version);
        assert_eq!(build_meta.major, meta.major);
        assert_eq!(build_meta.minor, meta.minor);
        assert_eq!(build_meta.patch, meta.patch);
        assert_eq!(build_meta.prerelease, meta.prerelease);
        assert_eq!(build_meta.short_commit, meta.short_commit);
        assert_eq!(build_meta.env, meta.env);
        assert_eq!(build_meta.date, meta.date);
        assert_eq!(build_meta.timestamp, meta.timestamp);
        assert_eq!(build_meta.now, meta.now);
        assert_eq!(build_meta.os, "linux");
        assert_eq!(build_meta.arch, "amd64");
        assert_eq!(build_meta.arm, "7");
        assert_eq!(build_meta.target, "x86_64-unknown-linux-musl");
    }

    #[test]
    fn test_create_build_meta_defaults_empty_strings_for_missing_target_fields() {
        let build = base_build();
        let meta = test_template_meta();
        let build_meta = create_build_meta(&build, &meta);

        assert!(build_meta.os.is_empty());
        assert!(build_meta.arch.is_empty());
        assert!(build_meta.arm.is_empty());
        assert!(build_meta.target.is_empty());
    }

    #[test]
    fn test_collect_envs_renders_templates() {
        let mut release = base_release();
        release.env = Some(vec!["RELEASE={{ meta.tag }}".to_string()]);

        let mut build = base_build();
        build.env = Some(vec![
            "BUILD={{ meta.build_name }}".to_string(),
            "TARGET={{ meta.target }}".to_string(),
        ]);
        build.target = Some("x86_64-unknown-linux-musl".to_string());

        let meta = test_template_meta();
        let build_meta = create_build_meta(&build, &meta);
        let envs = collect_envs(&release, &build, &build_meta).unwrap();

        assert_eq!(
            envs,
            vec![
                "RELEASE=v1.2.3".to_string(),
                "BUILD=Linux build".to_string(),
                "TARGET=x86_64-unknown-linux-musl".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_prepare_archive_files_renders_additional_files() {
        let mut release = base_release();
        release.additional_files = Some(vec!["dist/{{ meta.tag }}/README.md".to_string()]);

        let mut build = base_build();
        build.additional_files = Some(vec!["dist/{{ meta.os }}/LICENSE".to_string()]);
        build.os = Some("linux".to_string());

        let meta = test_template_meta();
        let build_meta = create_build_meta(&build, &meta);
        let files =
            prepare_archive_files(&release, &build, "target/bin/app", "dist/app", &build_meta)
                .await
                .unwrap();

        assert!(files.contains(&ArchiveFile {
            disk_path: "dist/v1.2.3/README.md".to_string(),
            archive_filename: "README.md".to_string(),
        }));
        assert!(files.contains(&ArchiveFile {
            disk_path: "dist/linux/LICENSE".to_string(),
            archive_filename: "LICENSE".to_string(),
        }));
    }
}
