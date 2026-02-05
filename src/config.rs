use camino::Utf8Path;
use color_eyre::eyre::{bail, Result};
use config::FileFormat;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum HookType {
    #[serde(rename = "before")]
    Before,
    #[serde(rename = "after")]
    After,
}

// Implement Display for nice printing in logs
impl std::fmt::Display for HookType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Github {
    pub owner: String,
    pub repo: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Gitlab {
    pub owner: String,
    pub repo: String,
    /// GitLab instance URL (defaults to https://gitlab.com)
    #[serde(default = "default_gitlab_url")]
    pub url: String,
}

fn default_gitlab_url() -> String {
    "https://gitlab.com".to_string()
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Docker {
    pub dockerfile: Option<String>,
    pub image: Option<String>,
    pub context: Option<String>,

    // Optional list of image references to push without running a docker build.
    #[serde(default)]
    pub images: Option<Vec<String>>,

    // Controls whether the Docker target performs docker push for handled images.
    #[serde(default = "default_true")]
    pub push: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReleaseTargets {
    pub github: Option<Github>,
    pub gitlab: Option<Gitlab>,
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

    #[serde(default)]
    pub builds_sequential: bool,

    // Additonal files to be included in the archive.
    pub additional_files: Option<Vec<String>>,

    // Commands to run before starting the builds.
    pub hooks: Option<Hooks>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hooks {
    pub before: Option<Vec<String>>,
    pub after: Option<Vec<String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checksum {
    pub algorithm: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Changelog {
    pub format: Option<String>,
    pub exclude: Option<Vec<String>>,
    pub template: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum BuildType {
    #[serde(rename = "custom")]
    #[default]
    Custom,
    #[serde(rename = "buildx")]
    Buildx,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum ArchiveFormat {
    #[serde(rename = "zip")]
    #[default]
    Zip,
    #[serde(alias = "tar.gz", rename = "tar_gz")]
    TarGz,
    #[serde(alias = "tar.zstd", rename = "tar_zstd")]
    TarZstd,
    #[serde(alias = "tar.lz4", rename = "tar_lz4")]
    TarLz4,
}

impl ArchiveFormat {
    /// Returns the file extension for this archive format.
    pub fn extension(&self) -> &'static str {
        match self {
            ArchiveFormat::Zip => ".zip",
            ArchiveFormat::TarGz => ".tar.gz",
            ArchiveFormat::TarZstd => ".tar.zstd",
            ArchiveFormat::TarLz4 => ".tar.lz4",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct BuildxConfig {
    pub context: Option<String>,
    pub dockerfile: Option<String>,
    pub tags: Option<Vec<String>>,
    pub platforms: Option<Vec<String>>,
    pub builder: Option<String>,
    pub load: Option<bool>,
    pub build_args: Option<BTreeMap<String, String>>,
    pub labels: Option<BTreeMap<String, String>>,
    pub cache_from: Option<Vec<String>>,
    pub cache_to: Option<Vec<String>>,
    pub target: Option<String>,
    pub outputs: Option<Vec<String>>,
    pub provenance: Option<bool>,
    pub sbom: Option<bool>,
    pub secrets: Option<Vec<String>>,
    pub ssh: Option<Vec<String>>,
    pub annotations: Option<BTreeMap<String, String>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Build {
    #[serde(rename = "type", default)]
    pub build_type: BuildType,

    #[serde(default)]
    pub command: Option<String>,

    pub buildx: Option<BuildxConfig>,
    pub artifact: String,
    pub bin_name: Option<String>,
    pub archive_name: String,
    pub name: String,
    pub os: Option<String>,
    pub arch: Option<String>,
    pub arm: Option<String>,
    pub target: Option<String>,
    pub matrix: Option<Vec<BTreeMap<String, Vec<String>>>>,

    /// Environment variables to set for the build.
    pub env: Option<Vec<String>>,

    /// Prehook to run before the build.
    pub prehook: Option<String>,

    /// Posthook to run after the build.
    pub posthook: Option<String>,

    // Doesn't create an archive if given true.
    pub no_archive: Option<bool>,

    // Additonal files to be included in the archive.
    pub additional_files: Option<Vec<String>>,

    /// Archive format for the build (zip, tar_gz, tar_zstd, tar_lz4).
    /// Defaults to zip.
    #[serde(default)]
    pub archive_format: ArchiveFormat,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_target_fields_deserialize() {
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      github:
        owner: "owner"
        repo: "repo"
    builds:
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app"
        name: "Linux build"
        os: "linux"
        arch: "amd64"
        arm: "7"
        target: "x86_64-unknown-linux-musl"
      - command: "echo build2"
        artifact: "./bin/app2"
        archive_name: "app2"
        name: "Default build"
"#;

        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let builds = &cfg.releases[0].builds;

        assert_eq!(builds[0].os.as_deref(), Some("linux"));
        assert_eq!(builds[0].arch.as_deref(), Some("amd64"));
        assert_eq!(builds[0].arm.as_deref(), Some("7"));
        assert_eq!(
            builds[0].target.as_deref(),
            Some("x86_64-unknown-linux-musl")
        );
        assert_eq!(builds[0].build_type, BuildType::Custom);
        assert_eq!(builds[0].command.as_deref(), Some("echo build"));

        assert!(builds[1].os.is_none());
        assert!(builds[1].arch.is_none());
        assert!(builds[1].arm.is_none());
        assert!(builds[1].target.is_none());
        assert_eq!(builds[1].build_type, BuildType::Custom);
        assert_eq!(builds[1].command.as_deref(), Some("echo build2"));
    }

    #[test]
    fn test_gitlab_target_deserialize() {
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      gitlab:
        owner: "group/subgroup"
        repo: "project"
    builds:
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app"
        name: "Default build"
"#;

        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let targets = &cfg.releases[0].targets;
        let gitlab = targets.gitlab.as_ref().expect("gitlab target should exist");

        assert_eq!(gitlab.owner, "group/subgroup");
        assert_eq!(gitlab.repo, "project");
        assert_eq!(gitlab.url, "https://gitlab.com"); // default value
        assert!(targets.github.is_none());
    }

    #[test]
    fn test_gitlab_target_custom_url() {
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      gitlab:
        owner: "myteam"
        repo: "myproject"
        url: "https://gitlab.example.com"
    builds:
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app"
        name: "Default build"
"#;

        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let gitlab = cfg.releases[0].targets.gitlab.as_ref().expect("gitlab target should exist");

        assert_eq!(gitlab.owner, "myteam");
        assert_eq!(gitlab.repo, "myproject");
        assert_eq!(gitlab.url, "https://gitlab.example.com");
    }

    #[test]
    fn test_buildx_type_deserialize() {
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      github:
        owner: "owner"
        repo: "repo"
    builds:
      - type: "buildx"
        artifact: "./bin/app"
        archive_name: "app"
        name: "Buildx build"
        buildx:
          context: "."
          dockerfile: "./Dockerfile"
          tags:
            - "example:latest"
          platforms:
            - "linux/amd64"
          builder: "default"
          load: true
          build_args:
            FOO: "bar"
          labels:
            org.opencontainers.image.title: "rlsr"
          cache_from:
            - "type=registry,ref=example:cache"
          cache_to:
            - "type=inline"
          target: "release"
          outputs:
            - "type=registry"
          provenance: true
          sbom: true
          secrets:
            - "id=token,src=./token"
          ssh:
            - "default"
          annotations:
            org.opencontainers.image.description: "desc"
"#;

        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let build = &cfg.releases[0].builds[0];

        assert_eq!(build.build_type, BuildType::Buildx);
        assert!(build.command.is_none());
        let buildx = build.buildx.as_ref().expect("buildx config should exist");
        assert_eq!(buildx.context.as_deref(), Some("."));
        assert_eq!(buildx.dockerfile.as_deref(), Some("./Dockerfile"));
        assert_eq!(
            buildx.tags.as_deref(),
            Some(&["example:latest".to_string()][..])
        );
        assert_eq!(
            buildx.platforms.as_deref(),
            Some(&["linux/amd64".to_string()][..])
        );
        assert_eq!(buildx.builder.as_deref(), Some("default"));
        assert_eq!(buildx.load, Some(true));
        assert_eq!(
            buildx
                .build_args
                .as_ref()
                .and_then(|args| args.get("FOO").cloned()),
            Some("bar".to_string())
        );
        assert_eq!(
            buildx
                .labels
                .as_ref()
                .and_then(|labels| labels.get("org.opencontainers.image.title").cloned()),
            Some("rlsr".to_string())
        );
        assert_eq!(
            buildx.cache_from.as_deref(),
            Some(&["type=registry,ref=example:cache".to_string()][..])
        );
        assert_eq!(
            buildx.cache_to.as_deref(),
            Some(&["type=inline".to_string()][..])
        );
        assert_eq!(buildx.target.as_deref(), Some("release"));
        assert_eq!(
            buildx.outputs.as_deref(),
            Some(&["type=registry".to_string()][..])
        );
        assert_eq!(buildx.provenance, Some(true));
        assert_eq!(buildx.sbom, Some(true));
        assert_eq!(
            buildx.secrets.as_deref(),
            Some(&["id=token,src=./token".to_string()][..])
        );
        assert_eq!(buildx.ssh.as_deref(), Some(&["default".to_string()][..]));
        assert_eq!(
            buildx
                .annotations
                .as_ref()
                .and_then(|annotations| annotations
                    .get("org.opencontainers.image.description")
                    .cloned()),
            Some("desc".to_string())
        );
    }

    #[test]
    fn test_archive_format_extension() {
        assert_eq!(ArchiveFormat::Zip.extension(), ".zip");
        assert_eq!(ArchiveFormat::TarGz.extension(), ".tar.gz");
        assert_eq!(ArchiveFormat::TarZstd.extension(), ".tar.zstd");
        assert_eq!(ArchiveFormat::TarLz4.extension(), ".tar.lz4");
    }

    #[test]
    fn test_archive_format_defaults_to_zip() {
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      github:
        owner: "owner"
        repo: "repo"
    builds:
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app"
        name: "Default format build"
"#;
        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let build = &cfg.releases[0].builds[0];
        assert_eq!(build.archive_format, ArchiveFormat::Zip);
    }

    #[test]
    fn test_archive_format_deserialize_all_variants() {
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      github:
        owner: "owner"
        repo: "repo"
    builds:
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-zip"
        name: "Zip build"
        archive_format: zip
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-targz"
        name: "Tar.gz build"
        archive_format: tar_gz
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-tarzstd"
        name: "Tar.zstd build"
        archive_format: tar_zstd
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-tarlz4"
        name: "Tar.lz4 build"
        archive_format: tar_lz4
"#;
        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let builds = &cfg.releases[0].builds;

        assert_eq!(builds[0].archive_format, ArchiveFormat::Zip);
        assert_eq!(builds[1].archive_format, ArchiveFormat::TarGz);
        assert_eq!(builds[2].archive_format, ArchiveFormat::TarZstd);
        assert_eq!(builds[3].archive_format, ArchiveFormat::TarLz4);
    }

    #[test]
    fn test_archive_format_alias_with_dot() {
        // Test that aliases like "tar.gz" work (using quotes in YAML)
        let yaml = r#"
releases:
  - name: "Test Release"
    dist_folder: "./dist"
    targets:
      github:
        owner: "owner"
        repo: "repo"
    builds:
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-targz"
        name: "Tar.gz build with dot alias"
        archive_format: "tar.gz"
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-tarzstd"
        name: "Tar.zstd build with dot alias"
        archive_format: "tar.zstd"
      - command: "echo build"
        artifact: "./bin/app"
        archive_name: "app-tarlz4"
        name: "Tar.lz4 build with dot alias"
        archive_format: "tar.lz4"
"#;
        let cfg: Config = serde_yaml::from_str(yaml).expect("config should deserialize");
        let builds = &cfg.releases[0].builds;

        assert_eq!(builds[0].archive_format, ArchiveFormat::TarGz);
        assert_eq!(builds[1].archive_format, ArchiveFormat::TarZstd);
        assert_eq!(builds[2].archive_format, ArchiveFormat::TarLz4);
    }
}
