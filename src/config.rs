use camino::Utf8Path;
use color_eyre::eyre::{bail, Result};
use config::FileFormat;
use serde::{Deserialize, Serialize};

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
    pub env: Option<Vec<String>>,

    // Additonal files to be included in the archive.
    pub additional_files: Option<Vec<String>>,

    // Commands to run before starting the builds.
    pub hooks: Option<Hooks>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hooks {
    pub before: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checksum {
    pub algorithm: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Changelog {
    // Default will be "github".
    pub format: String,
    pub exclude: Option<Vec<String>>,
}

// Implement default for ChangeLog.
impl Default for Changelog {
    fn default() -> Self {
        Changelog {
            format: "".to_string(),
            exclude: None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    pub command: String,
    pub artifact: String,
    pub bin_name: Option<String>,
    pub archive_name: String,
    pub name: String,

    /// Prehook to run before the build.
    pub prehook: Option<String>,

    /// Posthook to run after the build.
    pub posthook: Option<String>,

    // Doesn't create an archive if given true.
    pub no_archive: Option<bool>,

    // Additonal files to be included in the archive.
    pub additional_files: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub releases: Vec<Release>,
    pub changelog: Option<Changelog>,
}

pub fn parse_config(cfg_path: &str) -> Result<Config> {
    // Parse config according to the file format.
    let file_ext = Utf8Path::new(&cfg_path);
    let source = match file_ext.extension() {
        Some("toml") => FileFormat::Toml,
        Some("yaml") => FileFormat::Yaml,
        Some("yml") => FileFormat::Yaml,
        Some("json") => FileFormat::Json,
        _ => {
            bail!("unsupported file format");
        }
    };

    let settings = config::Config::builder()
        .add_source(config::File::new(cfg_path, source))
        .build();

    match settings {
        Ok(c) => {
            let cfg: Result<_, _> = c.try_deserialize();
            match cfg {
                Ok(cfg) => Ok(cfg),
                Err(err) => {
                    bail!("error parsing config: {}", err);
                }
            }
        }
        Err(err) => {
            bail!("error reading config: {}", err);
        }
    }
}
