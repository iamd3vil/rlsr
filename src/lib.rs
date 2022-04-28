use async_zip::write::{EntryOptions, ZipFileWriter};
use futures;
use log::{debug, error, info, warn};
use std::{path::Path, sync::Arc, env};
use tokio::{fs, process::Command};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
struct Github {
    owner: String,
    repo: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    name: String,
    dist_folder: String,
    jobs: Vec<Job>,
    github: Option<Github>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    command: String,
    artifact: String,
    bin_name: String,
    name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    builds: Vec<Build>,
}

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

    if ghtoken != "" {
        octocrab::initialise(octocrab::Octocrab::builder().personal_token(ghtoken.clone()))?;
    }

    // let builds = cfg.builds.clone();
    let num = cfg.builds.len();
    let shared: Arc<Vec<Build>> = Arc::from(cfg.builds);
    // let all_builds = vec![];
    for i in 0..num {
        let builds = shared.clone();
        let mut all_jobs = vec![];
        for j in 0..builds[i].jobs.len() {
            let builds = shared.clone();
            all_jobs.push(tokio::spawn(async move {
                let res = run_job(&builds[i], &builds[i].jobs[j]).await;
                match res {
                    Err(err) => {
                        println!("error executing the job: {}", err);
                        return;
                    }
                    Ok(_) => return,
                }
            }));
        }

        // Wait until all jobs are finished in a build.
        futures::future::join_all(&mut all_jobs).await;

        // Publish to github if we can find a latest tag or github repo configured in config.
        let latest_tag = match get_latest_tag().await {
            Ok(tag) => {
                info!("found out latest tag: {}", tag);
                tag
            }
            Err(_) => {
                warn!("error finding tag, skipping publishing");
                continue;
            }
        };
        debug!("latest tag: {}", latest_tag);

        let gh = match &builds[i].github {
            Some(gh) => gh,
            None => {
                warn!("github repo is blank, skipping publishing");
                continue
            }
        };

        
        if ghtoken == "" {
            error!("GITHUB_TOKEN is blank, skipping publishing build");
            continue
        }

        let ghclient = octocrab::instance();

        let res = ghclient
            .repos(&gh.owner, &gh.repo)
            .releases()
            .create(&latest_tag)
            .body("")
            .send()
            .await?;
        info!("release created: {:?}", res);
    }
    println!("Done executing all builds");
    Ok(())
}

pub async fn run_job(build: &Build, job: &Job) -> Result<()> {
    // Split cmd into command, args.
    let cmds = job.command.split(" ").collect::<Vec<&str>>();
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
        println!("creating an archive for {}", &job.name);
        archive_file(bin_path, &build.dist_folder, &job.name)
            .await
            .with_context(|| format!("error while creating archive for job: {}", job.name))?;
    }

    Ok(())
}

async fn archive_file(filename: &str, dist: &str, name: &str) -> Result<()> {
    let mut f = tokio::fs::File::open(filename).await?;
    // Create a zip file.
    let mut zip_path = Path::new(&dist).join(name);
    zip_path.set_extension("zip");
    let mut zip_file = tokio::fs::File::create(zip_path).await?;
    let mut zip = ZipFileWriter::new(&mut zip_file);
    let options = EntryOptions::new(filename.to_owned(), async_zip::Compression::Zstd);
    let mut zw = zip.write_entry_stream(options).await?;
    // zip.start_file(filename, options)?;
    tokio::io::copy(&mut f, &mut zw).await?;

    zw.close().await?;
    zip.close().await?;
    Ok(())
}

async fn get_latest_tag() -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(vec!["describe", "--abbrev=0"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!("error getting latest tag");
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
