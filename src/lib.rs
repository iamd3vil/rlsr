use crate::utils::{get_latest_commit_hash, get_latest_tag, is_at_latest_tag, is_repo_clean};
use color_eyre::eyre::{bail, Result};
use log::{debug, error, info, trace, warn};
use std::sync::Arc;
use tokio::{fs, sync::Mutex};

mod build;
mod changelog_formatter;
mod checksum;
mod checksummer;
pub mod config;
mod release_provider;
mod utils;

use config::{Config, Release};
use serde::Serialize;
use utils::get_changelog;

#[derive(Debug, Clone)]
pub struct Opts {
    pub publish: bool,
    pub rm_dist: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TemplateMeta {
    pub tag: String,
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
            trace!("deleting dist folder for release: {}", &releases[i].name);
            if let Ok(meta) = fs::metadata(&releases[i].dist_folder).await {
                if meta.is_dir() {
                    fs::remove_dir_all(&releases[i].dist_folder).await?;
                } else {
                    bail!("error deleting dist dir: not a directory");
                }
            }
        }

        let template_meta = {
            let tag = if is_at_latest_tag().await? {
                get_latest_tag().await?
            } else {
                get_latest_commit_hash().await?
            };
            debug!("tag found: {}", tag);
            let template_meta = TemplateMeta { tag };

            Arc::new(template_meta)
        };

        // Execute if there is a before hook.
        if let Some(hooks) = &releases[i].hooks {
            if let Some(before) = &hooks.before {
                // Execute the commands in the before hook.
                for command in before {
                    info!("executing before hook: {}", command);
                    let output = utils::execute_command(command, &releases[i].env).await?;
                    if !output.status.success() {
                        bail!("before hook failed: {}", command);
                    }
                }
            }
        }

        // Create dist directory.
        trace!("creating dist folder: {}", &releases[i].dist_folder);
        fs::create_dir_all(&releases[i].dist_folder).await?;

        for b in 0..releases[i].builds.len() {
            let releases = shared.clone();
            let all_archives = all_archives.clone();
            let template_meta = template_meta.clone();
            all_builds.push(tokio::spawn(async move {
                let name = &releases[i].builds[b].name;
                info!("executing build: {}", name);
                let res =
                    build::run_build(&releases[i], &releases[i].builds[b], &template_meta).await;
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
            let providers = utils::get_release_providers(&releases[i], cfg.changelog.clone())?;
            let mut publish_errors = Vec::new();
            for prov in providers {
                let all_archives = all_archives.clone();
                match prov
                    .publish(&releases[i], all_archives, latest_tag.clone())
                    .await
                {
                    Ok(_) => continue,
                    Err(err) => {
                        error!("{}", err);
                        publish_errors.push(err);
                    }
                }
            }

            if !publish_errors.is_empty() {
                bail!(
                    "Failed to publish with {} provider(s)",
                    publish_errors.len()
                );
            }
        }
    }
    Ok(())
}
