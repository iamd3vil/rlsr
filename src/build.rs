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
    execute_posthook(release, build).await?;

    // Copy artifact and create archive if needed
    process_artifacts(release, build, meta, &build_meta).await
}

fn create_build_meta(build: &Build, meta: &TemplateMeta) -> BuildMeta {
    BuildMeta {
        build_name: build.name.clone(),
        tag: meta.tag.clone(),
    }
}

async fn execute_prehook(release: &Release, build: &Build, build_meta: &BuildMeta) -> Result<()> {
    if let Some(prehook) = &build.prehook {
        let prehook = utils::render_template(prehook, build_meta);

        info!("executing prehook: `{}` for build: {}", prehook, build.name);

        let output = utils::execute_command(&prehook, &release.env).await?;
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

    let cmd = utils::render_template(&build.command, build_meta);
    utils::execute_command(&cmd, &release.env).await
}

async fn execute_posthook(release: &Release, build: &Build) -> Result<()> {
    if let Some(posthook) = &build.posthook {
        info!(
            "executing posthook: `{}` for build: {}",
            posthook, build.name
        );

        let output = utils::execute_command(posthook, &release.env).await?;
        if !output.status.success() {
            bail!("posthook failed: {}", posthook);
        }
    }
    Ok(())
}

async fn process_artifacts(
    release: &Release,
    build: &Build,
    meta: &TemplateMeta,
    build_meta: &BuildMeta,
) -> Result<String> {
    let bin_name = build.bin_name.as_ref().unwrap_or(&build.archive_name);
    let bin_name = utils::render_template(bin_name, meta);

    // Copy artifact to dist folder
    let bin_path = copy_artifact_to_dist(release, build, &bin_name).await?;

    let archive_name = utils::render_template(&build.archive_name, build_meta);
    let no_archive = build.no_archive.is_some_and(|val| val);

    if !no_archive {
        // Create an archive
        debug!("creating an archive for {}", &archive_name);

        let files = prepare_archive_files(release, build, &bin_path).await?;

        let zip_path = archive_files(files, release.dist_folder.clone(), archive_name.clone())
            .await
            .with_context(|| format!("error while creating archive for build: {}", archive_name))?;

        Ok(zip_path)
    } else {
        // Copy artifact with the final name
        copy_artifact_with_name(release, build, &archive_name).await?;

        Ok(Utf8Path::new(&release.dist_folder)
            .join(&archive_name)
            .to_string())
    }
}

async fn copy_artifact_to_dist(release: &Release, build: &Build, bin_name: &str) -> Result<String> {
    let bin_path = Utf8Path::new(&release.dist_folder).join(bin_name);

    fs::copy(&build.artifact, &bin_path)
        .await
        .with_context(|| format!("error while copying artifact: {}", build.artifact))?;

    Ok(bin_path.to_string())
}

async fn prepare_archive_files(
    release: &Release,
    build: &Build,
    bin_path: &str,
) -> Result<Vec<ArchiveFile>> {
    // Get the binary name from artifact
    let artifact_path = Utf8Path::new(&build.artifact);
    let bin_name = artifact_path
        .file_name()
        .with_context(|| format!("error getting filename from artifact: {}", &build.artifact))?;

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
                .map(|f| ArchiveFile {
                    disk_path: f.clone(),
                    archive_filename: Utf8Path::new(f).file_name().unwrap().to_string(),
                })
                .collect::<Vec<ArchiveFile>>(),
        );
    }

    // Add release-specific additional files
    if let Some(rls_additional_files) = &release.additional_files {
        files.extend(
            rls_additional_files
                .iter()
                .map(|f| ArchiveFile {
                    disk_path: f.clone(),
                    archive_filename: Utf8Path::new(f).file_name().unwrap().to_string(),
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
    build: &Build,
    archive_name: &str,
) -> Result<()> {
    fs::copy(
        &build.artifact,
        Utf8Path::new(&release.dist_folder).join(archive_name),
    )
    .await
    .with_context(|| "error while copying artifact to given name")?;

    Ok(())
}
