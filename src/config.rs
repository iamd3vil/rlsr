use eyre::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;

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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub command: String,
    pub artifact: String,
    pub bin_name: String,
    pub name: String,

    // Doesn't an archive if given true.
    pub no_archive: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub releases: Vec<Release>,
}

pub async fn parse_config(cfg_path: &str) -> Result<Config> {
    let cfg_str = fs::read_to_string(&cfg_path)
        .await
        .with_context(|| format!("error reading config file at {}", cfg_path))?;
    let cfg: Config = serde_yaml::from_str(&cfg_str)?;
    Ok(cfg)
}
