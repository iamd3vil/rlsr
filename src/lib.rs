use anyhow::{bail, Context, Result};
use log::{debug, error, info, warn};
use std::{
    env,
    path::Path,
    sync::{Arc, Mutex},
};
use tokio::{fs, process::Command};

mod config;
mod github;
mod utils;
use utils::archive_file;
use config::{Build, Config, Job};
use github::publish_build;

pub async fn parse_config(cfg_path: &str) -> Result<Config> {
    let cfg_str = fs::read_to_string(&cfg_path)
        .await
        .with_context(|| format!("error reading config file at {}", cfg_path))?;
    let cfg: Config = serde_yaml::from_str(&cfg_str)?;
    Ok(cfg)
}

pub async fn run(cfg: Config) -> Result<()> {
    // Check if `GITHUB_TOKEN` is present.
    let ghtoken = match env::var("GITHUB_TOKEN") {
        Ok(token) => token,
        Err(_) => {
            warn!("`GITHUB_TOKEN` isn't set, won't publish to github.");
            String::from("")
        }
    };

    if !ghtoken.is_empty() {
        octocrab::initialise(octocrab::Octocrab::builder().personal_token(ghtoken.clone()))?;
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
                let res = run_job(&builds[i], &builds[i].jobs[j]).await;
                match res {
                    Err(err) => {
                        println!("error executing the job: {}", err);
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
        match publish_build(&builds[i], all_archives, ghtoken.clone()).await {
            Ok(_) => continue,
            Err(err) => {
                error!("{}", err);
            }
        }
    }
    Ok(())
}

pub async fn run_job(build: &Build, job: &Job) -> Result<String> {
    // Split cmd into command, args.
    let cmds = job.command.split(' ').collect::<Vec<&str>>();
    let output = Command::new(cmds[0]).args(&cmds[1..]).output().await?;

    // If the job executed succesfully, copy the artifact to dist folder.
    if output.status.success() {
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
