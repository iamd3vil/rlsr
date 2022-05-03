use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Github {
    pub owner: String,
    pub repo: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub name: String,
    pub dist_folder: String,
    pub jobs: Vec<Job>,
    pub github: Option<Github>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Job {
    pub command: String,
    pub artifact: String,
    pub bin_name: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub builds: Vec<Build>,
}

pub async fn parse_config(cfg_path: &str) -> Result<Config> {
    let cfg_str = fs::read_to_string(&cfg_path)
        .await
        .with_context(|| format!("error reading config file at {}", cfg_path))?;
    let cfg: Config = serde_yaml::from_str(&cfg_str)?;
    Ok(cfg)
}
