use std::collections::HashMap;

use camino::Utf8Path;
use color_eyre::{
    eyre::{bail, Context, ContextCompat},
    Result,
};
use log::{debug, info};
use new_string_template::template::Template;
use tokio::fs;

use crate::{
    config::{Build, Release},
    utils::{self, archive_files, ArchiveFile},
};

pub async fn run_build(
    release: &Release,
    build: &Build,
    meta: &HashMap<&str, String>,
) -> Result<String> {
    // Check if there is a prehook.
    // If there is a prehook, execute it.
    if let Some(prehook) = &build.prehook {
        // Build a build specific build_meta.
        // This is used to render the prehook template.
        let mut build_meta = meta.clone();
        build_meta.insert("build_name", build.name.clone());
        let prehook = Template::new(prehook).render(&build_meta)?;

        info!("executing prehook: `{}` for build: {}", prehook, build.name);

        let output = utils::execute_command(&prehook).await?;
        if !output.status.success() {
            bail!("prehook failed: {}", prehook);
        }
    }

    debug!("executing command: {}", build.command);
    // Split cmd into command, args.
    let output = utils::execute_command(&build.command).await?;

    // If the build executed succesfully, copy the artifact to dist folder.
    if output.status.success() {
        // If the build executed succesfully, execute the posthook.
        // If there is a posthook, execute it.
        if let Some(posthook) = &build.posthook {
            // Build a build specific build_meta.
            // This is used to render the posthook template.

            info!(
                "executing posthook: `{}` for build: {}",
                posthook, build.name
            );

            let output = utils::execute_command(&posthook).await?;
            if !output.status.success() {
                bail!("posthook failed: {}", posthook);
            }
        }

        let bin_name = build.bin_name.as_ref().unwrap_or(&build.archive_name);
        let bin_name = Template::new(bin_name).render(meta)?;
        fs::copy(
            &build.artifact,
            Utf8Path::new(&release.dist_folder).join(&bin_name),
        )
        .await
        .with_context(|| format!("error while copying artifact: {}", build.artifact))?;

        let bin_path = Utf8Path::new(&release.dist_folder)
            .join(&bin_name)
            .to_string();

        let archive_name_tpl = Template::new(&build.archive_name);
        let archive_name = archive_name_tpl.render(meta)?;
        let no_archive = build.no_archive.map_or(false, |val| val);
        if !no_archive {
            // Create an archive.
            debug!("creating an archive for {}", &archive_name);

            // Get the binary name from artifact.
            // This is the filename of the binary in the archive.

            let artifact_path = Utf8Path::new(&build.artifact);
            let bin_name = artifact_path.file_name().with_context(|| {
                format!("error getting filename from artifact: {}", &build.artifact)
            })?;

            // Add all files that needs to be archived.
            let bin_file = ArchiveFile {
                disk_path: bin_path.clone(),
                archive_filename: bin_name.to_string(),
            };
            let mut files = vec![bin_file];
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

            // Add any additional files defined in the release.
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

            // Sort and only keep the uniq files.
            files.sort_unstable();
            files.dedup();

            debug!("files being archived: {:?}", files);

            let zip_path = archive_files(files, release.dist_folder.clone(), archive_name.clone())
                .await
                .with_context(|| {
                    format!("error while creating archive for build: {}", archive_name)
                })?;
            return Ok(zip_path);
        }

        // Copy the binary to the given name.
        fs::copy(
            &build.artifact,
            Utf8Path::new(&release.dist_folder).join(&archive_name),
        )
        .await
        .with_context(|| "error while copying artifact to given name")?;

        return Ok(Utf8Path::new(&release.dist_folder)
            .join(&archive_name)
            .to_string());
    }

    Ok(String::from(""))
}
