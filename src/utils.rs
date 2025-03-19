use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, Result};
use log::debug;
use std::cmp::Ord;
use std::process::Output;
use std::{env, fs, io};
use tokio::{process::Command, task};

use crate::changelog_formatter;
use crate::config::{Changelog, Release};
use crate::release_provider::github::Github;
use crate::release_provider::{docker, ReleaseProvider};
use minijinja::{context, Environment};
use regex::Regex;
use std::fmt::Debug;

/// ArchiveFile has the filename on the disk and the filename in the archive.
#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub(crate) struct ArchiveFile {
    pub disk_path: String,
    pub archive_filename: String,
}

pub async fn execute_command(cmd: &str, envs: &Option<Vec<String>>) -> Result<Output> {
    let mut command = if cfg!(target_os = "windows") {
        let mut cmd_process = Command::new("cmd");
        cmd_process.args(["/C", cmd]);
        cmd_process
    } else {
        let mut cmd_process = Command::new("sh");
        cmd_process.args(["-c", cmd]);
        cmd_process
    };

    if let Some(envs) = envs {
        for env in envs {
            let parts: Vec<&str> = env.split('=').collect();
            if parts.len() >= 2 {
                command.env(parts[0], parts[1..].join("="));
            }
        }
    }

    let output = command.output().await?;
    Ok(output)
}

pub fn get_release_providers(
    release: &Release,
    changelog: Option<Changelog>,
) -> Result<Vec<Box<dyn ReleaseProvider>>> {
    let mut providers: Vec<Box<dyn ReleaseProvider>> = vec![];

    // Check if github details are provided.
    if release.targets.github.is_some() {
        let ghtoken = get_github_token();
        let gh = Github::new(ghtoken, changelog.unwrap_or_default());
        providers.push(Box::new(gh));
    }

    if release.targets.docker.is_some() {
        providers.push(Box::new(docker::Docker::new()));
    }

    Ok(providers)
}

// Gets the latest tag if it exists.
pub async fn get_latest_tag() -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(vec!["describe", "--abbrev=0"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!("error getting latest tag");
    }
    Ok(String::from(
        String::from_utf8_lossy(&output.stdout).to_string().trim(),
    ))
}

// Gets all the tags for the current repo.
pub async fn get_all_tags() -> Result<Vec<String>> {
    let mut cmd = Command::new("git");
    cmd.args(vec!["tag", "--list"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting all tags: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    let out = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(out
        .split('\n')
        .map(String::from)
        .filter(|tag| !tag.is_empty())
        .collect())
}

async fn get_previous_tag() -> Result<String> {
    // Get previous tag's commit.
    let mut cmd = Command::new("git");
    cmd.args(vec!["rev-list", "--tags", "--skip=1", "--max-count=1"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting previous tag commit: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    let prev_tag_commit = String::from_utf8_lossy(&output.stdout).to_string();
    let prev_tag_commit = prev_tag_commit.trim();

    // Get tag for the commit.
    let mut cmd = Command::new("git");
    cmd.args(vec!["describe", "--abbrev=0", "--tags", prev_tag_commit]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting previous tag: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    let prev_tag = String::from_utf8_lossy(&output.stdout).to_string();
    Ok(String::from(prev_tag.trim()))
}

// Get formatted git log.
pub async fn get_all_git_log() -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(vec!["log", "--format=%h: %B"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting git log: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

pub async fn get_changelog(cfg: &Changelog) -> Result<String> {
    let latest_tag = get_latest_tag().await?;

    // Try to get the previous tag, use empty string if it fails
    let prev_tag = get_previous_tag().await.unwrap_or_default();

    let range = if prev_tag.is_empty() {
        latest_tag.clone() // Use only the latest tag if there's no previous tag
    } else {
        format!("{}..{}", prev_tag, latest_tag)
    };

    let mut cmd = Command::new("git");
    cmd.args(vec![
        "log",
        "--pretty=format:%h%n%s%n%ae%n--end-commit--",
        &range,
    ]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting changelog: {}",
            String::from_utf8_lossy(&output.stderr).to_string()
        );
    }

    let log_output = match std::str::from_utf8(&output.stdout) {
        Ok(output) => output,
        Err(e) => bail!("error converting output to utf-8: {}", e),
    };

    let exclude_patterns: Vec<Regex> = cfg
        .exclude
        .as_ref()
        .map(|patterns| {
            patterns
                .iter()
                .filter_map(|pat| Regex::new(pat).ok())
                .collect()
        })
        .unwrap_or_default();

    let commits: Vec<changelog_formatter::Commit> = log_output
        .split_terminator("--end-commit--")
        .filter_map(|commit| {
            let mut lines = commit.lines().filter_map(|line| {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    Some(trimmed)
                } else {
                    None
                }
            });

            match (lines.next(), lines.next(), lines.next()) {
                (Some(hash), Some(subject), Some(email)) => {
                    if exclude_patterns.iter().any(|regex| regex.is_match(subject)) {
                        None
                    } else {
                        Some(changelog_formatter::Commit {
                            hash: hash.to_owned(),
                            subject: subject.to_owned(),
                            email: email.to_owned(),
                        })
                    }
                }
                _ => None,
            }
        })
        .collect();

    // Initialize changelog formatter.
    let fmter = changelog_formatter::get_new_formatter(&cfg.format)
        .wrap_err("error getting changelog formatter")?;

    fmter
        .format(&commits)
        .await
        .wrap_err("error formatting changelog")
}

pub fn get_github_token() -> String {
    // Check if `GITHUB_TOKEN` is present.
    env::var("GITHUB_TOKEN").unwrap_or_else(|_| String::new())
}

pub async fn is_repo_clean() -> Result<bool> {
    let output = Command::new("git")
        .arg("status")
        .arg("--porcelain")
        .arg("-uno")
        .output()
        .await?;

    Ok(output.stdout.is_empty())
}

pub async fn is_at_latest_tag() -> Result<bool> {
    let head_output = Command::new("git")
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .await?;

    let tag_output = Command::new("git")
        .arg("rev-list")
        .arg("--tags")
        .arg("--max-count=1")
        .output()
        .await?;

    let head_commit = String::from_utf8_lossy(&head_output.stdout)
        .trim()
        .to_string();
    let latest_tag_commit = String::from_utf8_lossy(&tag_output.stdout)
        .trim()
        .to_string();

    Ok(head_commit == latest_tag_commit)
}

pub async fn get_latest_commit_hash() -> Result<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--short")
        .arg("HEAD")
        .output()
        .await
        .wrap_err_with(|| "error running git rev-parse")?;

    if !output.status.success() {
        bail!("Failed to fetch git commit ID: {}", &output.status);
    }

    let commit_id = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(commit_id)
}

/// render_template renders a template with the given context using minijinja.
pub fn render_template<S: serde::Serialize + Debug>(tmpl: &str, meta: S) -> String {
    let mut env = Environment::new();
    env.add_template("tmpl", tmpl).unwrap();
    let tpl = env.get_template("tmpl").unwrap();
    tpl.render(context!(meta => meta)).unwrap()
}

// Creates an zip archive with the file given.
pub async fn archive_files(
    filenames: Vec<ArchiveFile>,
    dist: String,
    name: String,
) -> Result<String> {
    let path: Result<String> = task::spawn_blocking(move || {
        let zip_file_name = Utf8Path::new(&dist).join(name);
        let zip_path = format!("{}.zip", zip_file_name);
        debug!("creating archive: {:?}", zip_path);
        let zip_file = fs::File::create(&zip_path)?;
        let mut zip = zip::ZipWriter::new(zip_file);
        for file in filenames {
            let mut f = fs::File::open(&file.disk_path)?;

            let options = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o744);
            zip.start_file(file.archive_filename, options)?;
            io::copy(&mut f, &mut zip)?;
        }
        Ok(zip_path.to_string())
    })
    .await?;
    path
}
