use std::{
    fs::{self, File},
    io,
    path::Path,
    process::Command,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Build {
    name: String,
    dist_folder: String,
    jobs: Vec<Job>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Job {
    command: String,
    artifact: String,
    bin_name: String,
    name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    builds: Vec<Build>,
}

pub fn parse_config(cfg_path: &str) -> Result<Config> {
    let cfg_str = fs::read_to_string(&cfg_path)
        .with_context(|| format!("error reading config file at {}", cfg_path))?;
    let cfg: Config = serde_yaml::from_str(&cfg_str)?;
    Ok(cfg)
}

pub fn run(cfg: &Config) -> Result<()> {
    println!("builds: {:?}", cfg.builds);
    for build in &cfg.builds {
        println!("executing build \"{}\"", build.name);
        for job in &build.jobs {
            run_job(build, job)?;
        }
    }
    Ok(())
}

fn run_job(build: &Build, job: &Job) -> Result<()> {
    // Split cmd into command, args.
    let cmds = job.command.split(" ").collect::<Vec<&str>>();
    let output = Command::new(cmds[0]).args(&cmds[1..]).output()?;

    // If the job executed succesfully, copy the artifact to dist folder.
    if output.status.success() {
        // Create dist directory.
        fs::create_dir_all(&build.dist_folder)?;
        fs::copy(
            &job.artifact,
            Path::new(&build.dist_folder).join(&job.bin_name),
        )
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
            .with_context(|| format!("error while creating archive for job: {}", job.name))?;
    }

    Ok(())
}

fn archive_file(filename: &str, dist: &str, name: &str) -> Result<()> {
    let mut f = File::open(filename)?;
    // Create a zip file.
    let mut zip_path = Path::new(&dist).join(name);
    zip_path.set_extension("zip");
    let mut zip_file = File::create(zip_path)?;
    let mut zip = zip::ZipWriter::new(&mut zip_file);

    let options =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Bzip2);
    zip.start_file(filename, options)?;
    io::copy(&mut f, &mut zip)?;

    zip.finish()?;
    Ok(())
}
