use async_zip::write::{EntryOptions, ZipFileWriter};
use std::{path::Path, sync::Arc};
use tokio::{fs, process::Command};
use futures;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    name: String,
    dist_folder: String,
    jobs: Vec<Job>,
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

pub async fn run(cfg: &Config) -> Result<()> {
    println!("builds: {:?}", cfg.builds);
    // let builds = cfg.builds.clone();
    let shared = Arc::from(&cfg.builds);
    // let all_builds = vec![];
    for i in 0..cfg.builds.len() {
        let builds = shared.clone();
        let shared_jobs = Arc::from(&builds[i].jobs);
        
        for j in 0..builds[i].jobs.len() {
            let builds = shared.clone();
            tokio::spawn(async move {
                run_job(&builds[i], &builds[i].jobs[j]).await;
            });
        }
    }

    // let cfg = Arc::from(cfg);
    // let cfg = cfg.clone();
    // let mut all_builds = vec![];
    // let builds = &cfg.builds;
    // for build in builds {
    //     println!("executing build \"{}\"", build.name);
    //     for job in &build.jobs {
    //         all_builds.push(tokio::spawn(async {
    //             let res = run_job(&build, job).await;
    //             match res {
    //                 Err(err) => {
    //                     println!("error executing the job: {}", err);
    //                     return
    //                 },
    //                 Ok(()) => return
    //             }
    //         }));
    //     }
    // }
    // future::join(all_builds)
    // futures::future::join_all(&mut all_builds).await;
    println!("Done executing all jobs");
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
