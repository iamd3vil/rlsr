use crate::{checksum::get_new_checksummer, utils::get_latest_tag};
use camino::Utf8Path;
use eyre::{bail, Context, Result};
use log::{debug, error, info, warn};
use release_provider::{docker, github::Github};
use std::{env, sync::Arc};
use tokio::io::AsyncWriteExt;
use tokio::{fs, process::Command, sync::Mutex};

mod checksum;
pub mod config;
mod release_provider;
mod utils;

use crate::release_provider::ReleaseProvider;
use config::{Build, Config, Release};
use utils::archive_file;

#[derive(Debug, Clone)]
pub struct Opts {
    pub publish: bool,
    pub rm_dist: bool,
}

pub async fn run(cfg: Config, opts: Opts) -> Result<()> {
    if !opts.publish {
        warn!("--publish isn't given, so skipping publishing")
    }

    let num = cfg.releases.len();
    let shared: Arc<Vec<Release>> = Arc::from(cfg.releases);
    for i in 0..num {
        let releases = shared.clone();
        let mut all_builds = vec![];
        let all_archives = Arc::new(Mutex::new(vec![]));
        for b in 0..releases[i].builds.len() {
            let builds = shared.clone();
            let all_archives = all_archives.clone();
            all_builds.push(tokio::spawn(async move {
                info!("executing build: {}", &builds[i].name);
                let res = run_build(&builds[i], &builds[i].builds[b], opts.rm_dist).await;
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

        let rls = &releases[i];

        if rls.checksum.is_some() {
            create_checksums(rls, all_archives.clone()).await?;
        }

        debug!("all archives generated: {:?}", all_archives);
        if opts.publish {
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
            let providers = get_release_providers(&releases[i])?;
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

fn get_release_providers(release: &Release) -> Result<Vec<Box<dyn ReleaseProvider>>> {
    let mut providers: Vec<Box<dyn ReleaseProvider>> = vec![];

    // Check if github details are provided.
    if release.targets.github.is_some() {
        let ghtoken = get_github_token();
        let gh = Github::new(ghtoken);
        providers.push(Box::new(gh));
    }

    if release.targets.docker.is_some() {
        providers.push(Box::new(docker::Docker::new()));
    }

    Ok(providers)
}

pub async fn run_build(release: &Release, build: &Build, rm_dist: bool) -> Result<String> {
    // Split cmd into command, args.
    let cmds = build.command.split(' ').collect::<Vec<&str>>();
    let output = Command::new(cmds[0]).args(&cmds[1..]).output().await?;

    // If the build executed succesfully, copy the artifact to dist folder.
    if output.status.success() {
        // Delete the dist directory if rm_dist is provided.
        if rm_dist {
            if let Ok(meta) = fs::metadata(&release.dist_folder).await {
                if meta.is_dir() {
                    fs::remove_dir_all(&release.dist_folder).await?;
                } else {
                    bail!("error deleting dist dir: not a directory");
                }
            }
        }

        // Create dist directory.
        fs::create_dir_all(&release.dist_folder).await?;
        fs::copy(
            &build.artifact,
            Utf8Path::new(&release.dist_folder).join(&build.bin_name),
        )
        .await
        .with_context(|| format!("error while copying artifact: {}", build.artifact))?;

        let dist_folder = Utf8Path::new(&release.dist_folder).join(&build.bin_name);
        let bin_path = dist_folder.to_string();

        let no_archive = build.no_archive.map_or(false, |val| val);
        if !no_archive {
            // Create an archive.
            debug!("creating an archive for {}", &build.name);
            let zip_path = archive_file(
                bin_path.to_owned(),
                release.dist_folder.clone(),
                build.name.clone(),
            )
            .await
            .with_context(|| format!("error while creating archive for build: {}", build.name))?;
            return Ok(zip_path);
        }

        // Copy the binary to the given name.
        fs::copy(
            &build.artifact,
            Utf8Path::new(&release.dist_folder).join(&build.name),
        )
        .await
        .with_context(|| "error while copying artifact to given name")?;

        return Ok(Utf8Path::new(&release.dist_folder)
            .join(&build.name)
            .to_string());
    }

    Ok(String::from(""))
}

fn get_github_token() -> String {
    // Check if `GITHUB_TOKEN` is present.
    env::var("GITHUB_TOKEN").unwrap_or_else(|_| String::new())
}

async fn create_checksums(rls: &Release, all_archives: Arc<Mutex<Vec<String>>>) -> Result<()> {
    let cm_path = Utf8Path::new(&rls.dist_folder).join("checksums.txt");
    if let Ok(_) = fs::metadata(&cm_path).await {
        // Remove checksums file if it exists.
        fs::remove_file(&cm_path).await?;
    }

    // Open the file with options set to both create (if it doesn't exist) and append
    let mut file = fs::OpenOptions::new()
        .create(true) // create if it doesn't exist
        .append(true) // set to append mode
        .open(&cm_path)
        .await?;
    let archives = all_archives.lock().await.clone();
    for arc in archives {
        let path = Utf8Path::new(&arc);

        let cm = get_new_checksummer(rls.checksum.as_ref().unwrap().algorithm.as_ref())?;

        let checksum = cm.compute(&arc).await?;

        debug!(
            "writing to checksums file: {}, {}",
            path.file_name().unwrap(),
            &checksum
        );
        // Write the name and checksum to the file
        file.write(format!("{}\t{}\n", path.file_name().unwrap(), checksum).as_bytes())
            .await?;

        file.flush().await?;
    }

    Ok(())
}
