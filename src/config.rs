use color_eyre::eyre::{bail, Context, Result};
use config::FileFormat;
use serde::{Deserialize, Serialize};
// use tokio::fs;
// use config::Config;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Github {
    pub owner: String,
    pub repo: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Docker {
    pub dockerfile: String,
    pub image: String,
    pub context: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReleaseTargets {
    pub github: Option<Github>,
    pub docker: Option<Docker>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Release {
    pub name: String,
    pub dist_folder: String,
    pub builds: Vec<Build>,
    pub targets: ReleaseTargets,
    pub checksum: Option<Checksum>,

    // Additonal files to be included in the archive.
    pub additional_files: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checksum {
    pub algorithm: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub command: String,
    pub artifact: String,
    pub bin_name: String,
    pub name: String,

    // Doesn't create an archive if given true.
    pub no_archive: Option<bool>,

    // Additonal files to be included in the archive.
    pub additional_files: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub releases: Vec<Release>,
}

pub fn parse_config(cfg_path: &str) -> Result<Config> {
    let settings = config::Config::builder()
        .add_source(config::File::new(cfg_path, FileFormat::Yaml))
        .build();

    match settings {
        Ok(c) => {
            let cfg: Config = c
                .try_deserialize()
                .wrap_err_with(|| "error deserializing to config")?;
            Ok(cfg)
        }
        Err(err) => {
            bail!("error reading config: {}", err);
        }
    }
}
