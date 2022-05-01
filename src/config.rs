use serde::{Deserialize, Serialize};

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