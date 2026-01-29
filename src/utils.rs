use camino::Utf8Path;
use chrono::{DateTime as ChronoDateTime, Datelike, Timelike, Utc};
use color_eyre::eyre::{bail, Context, Result};
use flate2::write::GzEncoder;
use flate2::Compression;
use log::debug;
use std::cmp::Ord;
use std::process::Output;
use std::{env, fs, io};
use tokio::{process::Command, task};
use zip::DateTime;

use crate::changelog_formatter;
use crate::config::{ArchiveFormat, Changelog, Release};
use crate::release_provider::github::Github;
use crate::release_provider::{docker, ReleaseProvider};
use regex::Regex;

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
                debug!("setting env var: {}={}", parts[0], parts[1]);
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
    cmd.args(vec!["describe", "--abbrev=0", "--tags"]);
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

pub async fn get_previous_tag() -> Result<String> {
    // Use HEAD^ so the current tag isn't returned when HEAD is tagged.
    let mut cmd = Command::new("git");
    cmd.args(vec!["describe", "--tags", "--abbrev=0", "HEAD^"]);
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

pub async fn get_github_release_url(tag: &str) -> Result<String> {
    let Some((owner, repo)) = get_github_repo_from_remote().await? else {
        return Ok(String::new());
    };
    Ok(format!(
        "https://github.com/{}/{}/releases/tag/{}",
        owner, repo, tag
    ))
}

async fn get_github_repo_from_remote() -> Result<Option<(String, String)>> {
    let mut cmd = Command::new("git");
    cmd.args(["config", "--get", "remote.origin.url"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        return Ok(None);
    }
    let remote = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(parse_github_repo_url(&remote))
}

fn parse_github_repo_url(remote: &str) -> Option<(String, String)> {
    let remote = remote.trim();
    let rest = if let Some(rest) = remote.strip_prefix("git@github.com:") {
        rest
    } else if let Some(rest) = remote.strip_prefix("https://github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("http://github.com/") {
        rest
    } else if let Some(rest) = remote.strip_prefix("ssh://git@github.com/") {
        rest
    } else {
        return None;
    };

    let rest = rest.strip_suffix(".git").unwrap_or(rest);
    let mut parts = rest.splitn(3, '/');
    let owner = parts.next()?.trim();
    let repo = parts.next()?.trim();
    if owner.is_empty() || repo.is_empty() {
        return None;
    }
    Some((owner.to_string(), repo.to_string()))
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
                        Some(changelog_formatter::Commit::from_parts(
                            hash.to_owned(),
                            subject.to_owned(),
                            email.to_owned(),
                        ))
                    }
                }
                _ => None,
            }
        })
        .collect();

    let ch_fmt = cfg.format.clone().unwrap_or("".to_string());

    // Initialize changelog formatter.
    let fmter = changelog_formatter::get_new_formatter(&ch_fmt, cfg.template.clone())
        .await
        .wrap_err("error getting changelog formatter")?;

    let template_meta = crate::build_template_meta(latest_tag.clone()).await?;
    fmter
        .format(&commits, &template_meta)
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

pub async fn get_full_commit_hash() -> Result<String> {
    let output = Command::new("git")
        .arg("rev-parse")
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

pub async fn get_current_branch() -> Result<String> {
    let output = Command::new("git")
        .arg("rev-parse")
        .arg("--abbrev-ref")
        .arg("HEAD")
        .output()
        .await
        .wrap_err_with(|| "error running git rev-parse")?;

    if !output.status.success() {
        bail!("Failed to fetch git branch: {}", &output.status);
    }

    let branch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    Ok(branch)
}

pub fn get_project_name() -> String {
    // Prefer manifest-defined names; fall back to directory name.
    if let Ok(contents) = fs::read_to_string("Cargo.toml") {
        if let Some(name) = parse_cargo_package_name(&contents) {
            return name;
        }
    }

    if let Ok(contents) = fs::read_to_string("package.json") {
        if let Some(name) = parse_package_json_name(&contents) {
            return name;
        }
    }

    if let Ok(contents) = fs::read_to_string("go.mod") {
        if let Some(name) = parse_go_module_name(&contents) {
            return name;
        }
    }

    env::current_dir()
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().to_string())
        })
        .unwrap_or_default()
}

fn parse_cargo_package_name(contents: &str) -> Option<String> {
    // Lightweight parser to avoid pulling in a full TOML dependency.
    let mut in_package_section = false;

    for line in contents.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_package_section = trimmed == "[package]";
            continue;
        }

        if !in_package_section || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        let (key, value) = match trimmed.split_once('=') {
            Some(parts) => parts,
            None => continue,
        };
        if key.trim() != "name" {
            continue;
        }

        let value = value.trim();
        let value = value.strip_prefix('"')?;
        let end = value.find('"')?;
        return Some(value[..end].to_string());
    }

    None
}

fn parse_package_json_name(contents: &str) -> Option<String> {
    // Regex keeps this minimal and avoids a JSON dependency.
    let re = Regex::new(r#""name"\s*:\s*"([^"]+)""#).ok()?;
    let caps = re.captures(contents)?;
    Some(caps.get(1)?.as_str().to_string())
}

fn parse_go_module_name(contents: &str) -> Option<String> {
    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("module ") {
            let module = rest.split_whitespace().next()?;
            let module = module.trim_matches('"');
            return Some(module.rsplit('/').next().unwrap_or(module).to_string());
        }
    }

    None
}

/// Creates an archive with the files given using the specified format.
pub async fn archive_files(
    filenames: Vec<ArchiveFile>,
    dist: String,
    name: String,
    format: ArchiveFormat,
) -> Result<String> {
    let path: Result<String> = task::spawn_blocking(move || {
        let base_path = Utf8Path::new(&dist).join(&name);
        let archive_path = format!("{}{}", base_path, format.extension());
        debug!("creating archive: {:?} with format {:?}", archive_path, format);

        match format {
            ArchiveFormat::Zip => create_zip_archive(&filenames, &archive_path),
            ArchiveFormat::TarGz => create_tar_gz_archive(&filenames, &archive_path),
            ArchiveFormat::TarZstd => create_tar_zstd_archive(&filenames, &archive_path),
            ArchiveFormat::TarLz4 => create_tar_lz4_archive(&filenames, &archive_path),
        }?;

        Ok(archive_path)
    })
    .await?;
    path
}

fn create_zip_archive(filenames: &[ArchiveFile], archive_path: &str) -> Result<()> {
    let zip_file = fs::File::create(archive_path)?;
    let mut zip = zip::ZipWriter::new(zip_file);

    for file in filenames {
        let mut f = fs::File::open(&file.disk_path)?;
        let mod_time = f.metadata().ok().and_then(|meta| meta.modified().ok());
        let zip_time = mod_time
            .and_then(|mod_time| {
                let dt: ChronoDateTime<Utc> = mod_time.into();
                let year = u16::try_from(dt.year()).ok()?;
                DateTime::from_date_and_time(
                    year,
                    dt.month() as u8,
                    dt.day() as u8,
                    dt.hour() as u8,
                    dt.minute() as u8,
                    dt.second() as u8,
                )
                .ok()
            })
            .unwrap_or_else(DateTime::default_for_write);

        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(zip_time)
            .unix_permissions(0o744);
        zip.start_file(&file.archive_filename, options)?;
        io::copy(&mut f, &mut zip)?;
    }

    zip.finish()?;
    Ok(())
}

fn create_tar_gz_archive(filenames: &[ArchiveFile], archive_path: &str) -> Result<()> {
    let file = fs::File::create(archive_path)?;
    let encoder = GzEncoder::new(file, Compression::default());
    let mut tar = tar::Builder::new(encoder);

    for archive_file in filenames {
        append_file_to_tar(&mut tar, archive_file)?;
    }

    let encoder = tar.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn create_tar_zstd_archive(filenames: &[ArchiveFile], archive_path: &str) -> Result<()> {
    let file = fs::File::create(archive_path)?;
    let encoder = zstd::stream::Encoder::new(file, 0)?;
    let mut tar = tar::Builder::new(encoder);

    for archive_file in filenames {
        append_file_to_tar(&mut tar, archive_file)?;
    }

    let encoder = tar.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn create_tar_lz4_archive(filenames: &[ArchiveFile], archive_path: &str) -> Result<()> {
    let file = fs::File::create(archive_path)?;
    let encoder = lz4_flex::frame::FrameEncoder::new(file);
    let mut tar = tar::Builder::new(encoder);

    for archive_file in filenames {
        append_file_to_tar(&mut tar, archive_file)?;
    }

    let encoder = tar.into_inner()?;
    encoder.finish()?;
    Ok(())
}

fn append_file_to_tar<W: io::Write>(
    tar: &mut tar::Builder<W>,
    archive_file: &ArchiveFile,
) -> Result<()> {
    let mut f = fs::File::open(&archive_file.disk_path)?;
    let metadata = f.metadata()?;

    let mut header = tar::Header::new_gnu();
    header.set_size(metadata.len());
    header.set_mode(0o744);

    if let Ok(modified) = metadata.modified() {
        let duration = modified
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default();
        header.set_mtime(duration.as_secs());
    }

    header.set_cksum();

    tar.append_data(&mut header, &archive_file.archive_filename, &mut f)
        .with_context(|| format!("error adding file to tar: {}", archive_file.disk_path))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn cwd_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    struct DirGuard {
        original: std::path::PathBuf,
    }

    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = env::set_current_dir(&self.original);
        }
    }

    fn with_temp_dir<F: FnOnce(&std::path::Path)>(f: F) {
        let _lock = cwd_lock().lock().unwrap();
        let original = env::current_dir().unwrap();
        let temp = tempdir().unwrap();
        env::set_current_dir(temp.path()).unwrap();
        let _guard = DirGuard { original };
        f(temp.path());
    }

    #[test]
    fn test_parse_cargo_package_name() {
        let contents = r#"
[package]
name = "example"
version = "0.1.0"
"#;
        assert_eq!(
            parse_cargo_package_name(contents),
            Some("example".to_string())
        );
    }

    #[test]
    fn test_parse_package_json_name() {
        let contents = r#"{ "name": "example-app", "version": "1.0.0" }"#;
        assert_eq!(
            parse_package_json_name(contents),
            Some("example-app".to_string())
        );
    }

    #[test]
    fn test_parse_go_module_name() {
        let contents = r#"
module example.com/acme/widget

go 1.22
"#;
        assert_eq!(parse_go_module_name(contents), Some("widget".to_string()));
    }

    #[test]
    fn test_get_project_name_prefers_cargo() {
        with_temp_dir(|dir| {
            fs::write(
                dir.join("Cargo.toml"),
                r#"
[package]
name = "cargo-proj"
version = "0.1.0"
"#,
            )
            .unwrap();
            fs::write(dir.join("package.json"), r#"{ "name": "npm-proj" }"#).unwrap();
            fs::write(dir.join("go.mod"), "module example.com/go-proj\n").unwrap();

            assert_eq!(get_project_name(), "cargo-proj");
        });
    }

    #[test]
    fn test_get_project_name_falls_back_to_go_mod() {
        with_temp_dir(|dir| {
            fs::write(dir.join("go.mod"), "module example.com/go-proj\n").unwrap();
            assert_eq!(get_project_name(), "go-proj");
        });
    }

    #[test]
    fn test_get_project_name_falls_back_to_dir() {
        with_temp_dir(|dir| {
            let expected = dir.file_name().unwrap().to_string_lossy().to_string();
            assert_eq!(get_project_name(), expected);
        });
    }

    #[test]
    fn test_parse_github_repo_url_variants() {
        assert_eq!(
            parse_github_repo_url("git@github.com:owner/repo.git"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            parse_github_repo_url("https://github.com/owner/repo"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert_eq!(
            parse_github_repo_url("ssh://git@github.com/owner/repo.git"),
            Some(("owner".to_string(), "repo".to_string()))
        );
        assert!(parse_github_repo_url("https://gitlab.com/owner/repo").is_none());
    }

    #[tokio::test]
    async fn test_archive_files_creates_zip() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        let file_path = dir.join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let files = vec![ArchiveFile {
            disk_path: file_path.to_string_lossy().to_string(),
            archive_filename: "test.txt".to_string(),
        }];

        let archive_path = archive_files(
            files,
            dir.to_string_lossy().to_string(),
            "test-archive".to_string(),
            ArchiveFormat::Zip,
        ).await.unwrap();

        assert!(archive_path.ends_with(".zip"));
        assert!(std::path::Path::new(&archive_path).exists());
    }

    #[tokio::test]
    async fn test_archive_files_creates_tar_gz() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        let file_path = dir.join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let files = vec![ArchiveFile {
            disk_path: file_path.to_string_lossy().to_string(),
            archive_filename: "test.txt".to_string(),
        }];

        let archive_path = archive_files(
            files,
            dir.to_string_lossy().to_string(),
            "test-archive".to_string(),
            ArchiveFormat::TarGz,
        ).await.unwrap();

        assert!(archive_path.ends_with(".tar.gz"));
        assert!(std::path::Path::new(&archive_path).exists());
    }

    #[tokio::test]
    async fn test_archive_files_creates_tar_zstd() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        let file_path = dir.join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let files = vec![ArchiveFile {
            disk_path: file_path.to_string_lossy().to_string(),
            archive_filename: "test.txt".to_string(),
        }];

        let archive_path = archive_files(
            files,
            dir.to_string_lossy().to_string(),
            "test-archive".to_string(),
            ArchiveFormat::TarZstd,
        ).await.unwrap();

        assert!(archive_path.ends_with(".tar.zstd"));
        assert!(std::path::Path::new(&archive_path).exists());
    }

    #[tokio::test]
    async fn test_archive_files_creates_tar_lz4() {
        let temp = tempdir().unwrap();
        let dir = temp.path();
        let file_path = dir.join("test.txt");
        fs::write(&file_path, "hello world").unwrap();

        let files = vec![ArchiveFile {
            disk_path: file_path.to_string_lossy().to_string(),
            archive_filename: "test.txt".to_string(),
        }];

        let archive_path = archive_files(
            files,
            dir.to_string_lossy().to_string(),
            "test-archive".to_string(),
            ArchiveFormat::TarLz4,
        ).await.unwrap();

        assert!(archive_path.ends_with(".tar.lz4"));
        assert!(std::path::Path::new(&archive_path).exists());
    }
}
