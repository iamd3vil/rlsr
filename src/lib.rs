use anyhow::{bail, Context, Result};
use log::{debug, error, info, warn};
use std::{
    env,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::{fs, process::Command};

pub mod config;
mod github;
mod utils;
use config::{Build, Config, Job};
use github::publish_build;
use utils::archive_file;

#[derive(Debug, Clone)]
pub struct Opts {
    pub publish: bool,
    pub rm_dist: bool,
}

pub async fn run(cfg: Config, opts: Opts) -> Result<()> {
    if opts.publish {
        let ghtoken = get_github_token()?;
        if !ghtoken.is_empty() {
            octocrab::initialise(octocrab::Octocrab::builder().personal_token(ghtoken))?;
        }
    } else {
        warn!("--publish isn't given, so skipping publishing")
    }

    // let builds = cfg.builds.clone();
    let num = cfg.builds.len();
    let shared: Arc<Vec<Build>> = Arc::from(cfg.builds);
    // let all_builds = vec![];
    for i in 0..num {
        let builds = shared.clone();
        let mut all_jobs = vec![];
        let all_archives = Arc::new(Mutex::new(vec![]));
        for j in 0..builds[i].jobs.len() {
            let builds = shared.clone();
            let all_archives = all_archives.clone();
            all_jobs.push(tokio::spawn(async move {
                info!("executing build: {}", &builds[i].name);
                let res = run_job(&builds[i], &builds[i].jobs[j], opts.rm_dist).await;
                match res {
                    Err(err) => {
                        error!("error executing the job: {}", err);
                    }
                    Ok(archive) => {
                        all_archives.lock().unwrap().push(archive);
                    }
                }
            }));
        }

        // Wait until all jobs are finished in a build.
        futures::future::join_all(&mut all_jobs).await;

        debug!("all archives generated: {:?}", all_archives);
        if opts.publish {
            let ghtoken = get_github_token()?;
            match publish_build(&builds[i], all_archives, ghtoken).await {
                Ok(_) => continue,
                Err(err) => {
                    error!("{}", err);
                }
            }
        }
    }
    Ok(())
}

pub async fn run_job(build: &Build, job: &Job, rm_dist: bool) -> Result<String> {
    // Split cmd into command, args.
    let cmds = job.command.split(' ').collect::<Vec<&str>>();
    let output = Command::new(cmds[0]).args(&cmds[1..]).output().await?;

    // If the job executed succesfully, copy the artifact to dist folder.
    if output.status.success() {
        // Delete the dist directory if rm_dist is provided.
        if rm_dist {
            fs::remove_dir_all(&build.dist_folder).await?;
        }

        // Create dist directory.
        fs::create_dir_all(&build.dist_folder).await?;
        fs::copy(
            &job.artifact,
            Path::new(&build.dist_folder).join(&job.bin_name),
        )
        .await
        .with_context(|| format!("error while copying artifact: {}", job.artifact))?;

        let dist_folder = Path::new(&build.dist_folder).join(&job.bin_name);
        let bin_path = dist_folder.to_str();
        let bin_path = match bin_path {
            None => {
                bail!("error creating bin path");
            }
            Some(bin_path) => bin_path,
        };

        // Create an archive.
        debug!("creating an archive for {}", &job.name);
        let zip_path = archive_file(bin_path, &build.dist_folder, &job.name)
            .await
            .with_context(|| format!("error while creating archive for job: {}", job.name))?;
        return Ok(zip_path);
    }

    Ok(String::from(""))
}

fn get_github_token() -> Result<String> {
    // Check if `GITHUB_TOKEN` is present.
    match env::var("GITHUB_TOKEN") {
        Ok(token) => Ok(token),
        Err(_) => Ok(String::from("")),
    }
}
