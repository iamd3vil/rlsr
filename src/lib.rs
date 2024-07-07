use crate::utils::{get_latest_commit_hash, get_latest_tag, is_at_latest_tag, is_repo_clean};
use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, ContextCompat, Result};
use log::{debug, error, info, warn};
use new_string_template::template::Template;
use release_provider::{docker, github::Github};
use std::sync::Arc;
use tokio::{fs, process::Command, sync::Mutex};

mod changelog_formatter;
mod checksum;
mod checksummer;
pub mod config;
mod release_provider;
mod utils;

use crate::release_provider::ReleaseProvider;
use config::{Build, Changelog, Config, Release};
use std::collections::HashMap;
use utils::{archive_files, get_changelog, ArchiveFile};

#[derive(Debug, Clone)]
pub struct Opts {
    pub publish: bool,
    pub rm_dist: bool,
}

pub async fn run(cfg: Config, opts: Opts) -> Result<()> {
    let mut publish = opts.publish;
    if !opts.publish {
        warn!("--publish isn't given, so skipping publishing");
        publish = false
    }

    let is_clean = is_repo_clean().await?;
    let at_latest_tag = is_at_latest_tag().await?;

    debug!("is_clean: {}, at_latest_tag: {}", is_clean, at_latest_tag);

    // Check if the repo is in a clean state and tagged.
    if !(is_clean && at_latest_tag) {
        warn!("repo is not clean and the latest commit is not tagged, skipping publishing");
        publish = false
    }

    let num = cfg.releases.len();
    let shared: Arc<Vec<Release>> = Arc::from(cfg.releases);
    for i in 0..num {
        let releases = shared.clone();
        let mut all_builds = vec![];
        let all_archives = Arc::new(Mutex::new(vec![]));
        // Delete the dist directory if rm_dist is provided.
        if opts.rm_dist {
            debug!("deleting dist folder for release: {}", &releases[i].name);
            if let Ok(meta) = fs::metadata(&releases[i].dist_folder).await {
                if meta.is_dir() {
                    fs::remove_dir_all(&releases[i].dist_folder).await?;
                } else {
                    bail!("error deleting dist dir: not a directory");
                }
            }
        }

        // Create dist directory.
        fs::create_dir_all(&releases[i].dist_folder).await?;

        let template_meta = {
            let mut template_meta: HashMap<&str, String> = HashMap::new();
            let tag = if is_at_latest_tag().await? {
                get_latest_tag().await?
            } else {
                get_latest_commit_hash().await?
            };
            debug!("tag found: {}", tag);
            template_meta.insert("tag", tag);

            Arc::new(template_meta)
        };

        for b in 0..releases[i].builds.len() {
            let releases = shared.clone();
            let all_archives = all_archives.clone();
            let template_meta = template_meta.clone();
            all_builds.push(tokio::spawn(async move {
                let name = &releases[i].builds[b].name;
                if let Some(name) = name {
                    info!("executing build: {}", name);
                }
                let res = run_build(&releases[i], &releases[i].builds[b], &template_meta).await;
                match res {
                    Err(err) => {
                        error!("error executing the build: {}", err);
                    }
                    Ok(archive) => {
                        all_archives.lock().await.push(archive);
                    }
                }
            }));
        }

        // Wait until all builds are finished in a release.
        futures::future::join_all(&mut all_builds).await;

        let changelog_cfg = cfg.changelog.clone();
        let changelog = get_changelog(&changelog_cfg.unwrap_or_default()).await?;
        println!("{}", changelog);

        let rls = &releases[i];

        if rls.checksum.is_some() {
            checksum::create_checksums(rls, all_archives.clone()).await?;
        }

        debug!("all builds are done: {:?}", all_archives);
        if publish {
            let latest_tag = match get_latest_tag().await {
                Ok(tag) => {
                    info!("found out latest tag: {}", tag);
                    tag
                }
                Err(_) => {
                    bail!("error finding tag, skipping publishing");
                }
            };
            debug!("latest tag: {}", latest_tag);

            // Make release providers from given config.
            let providers = get_release_providers(&releases[i], cfg.changelog.clone())?;
            for prov in providers {
                let all_archives = all_archives.clone();
                match prov
                    .publish(&releases[i], all_archives, latest_tag.clone())
                    .await
                {
                    Ok(_) => continue,
                    Err(err) => {
                        error!("{}", err);
                    }
                }
            }
        }
    }
    Ok(())
}

fn get_release_providers(
    release: &Release,
    changelog: Option<Changelog>,
) -> Result<Vec<Box<dyn ReleaseProvider>>> {
    let mut providers: Vec<Box<dyn ReleaseProvider>> = vec![];

    // Check if github details are provided.
    if release.targets.github.is_some() {
        let ghtoken = utils::get_github_token();
        let gh = Github::new(ghtoken, changelog.unwrap_or_default());
        providers.push(Box::new(gh));
    }

    if release.targets.docker.is_some() {
        providers.push(Box::new(docker::Docker::new()));
    }

    Ok(providers)
}

pub async fn run_build(
    release: &Release,
    build: &Build,
    meta: &HashMap<&str, String>,
) -> Result<String> {
    debug!("executing command: {}", build.command);
    // Split cmd into command, args.
    let cmds = build.command.split(' ').collect::<Vec<&str>>();
    let output = Command::new(cmds[0]).args(&cmds[1..]).output().await?;

    // If the build executed succesfully, copy the artifact to dist folder.
    if output.status.success() {
        let bin_name = build.bin_name.as_ref().unwrap_or(&build.archive_name);
        let bin_name_tmpl = Template::new(bin_name);
        let bin_name = bin_name_tmpl.render(meta)?;
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
