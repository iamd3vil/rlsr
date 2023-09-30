use color_eyre::eyre::{bail, Result};
// use async_zip::write::{EntryOptions, ZipFileWriter};
use camino::Utf8Path;
use std::{fs, io};
use tokio::{process::Command, task};

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

pub async fn get_changelog() -> Result<String> {
    // Get previous tag.
    let prev_tag = get_previous_tag().await?;
    let latest_tag = get_latest_tag().await?;

    let mut cmd = Command::new("git");
    cmd.args(vec![
        "log",
        "--format=%h: %B",
        &format!("{}..{}", prev_tag, latest_tag),
    ]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting changelog: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
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

// Creates an zip archive with the file given.
pub async fn archive_files(filenames: Vec<String>, dist: String, name: String) -> Result<String> {
    let path: Result<String> = task::spawn_blocking(move || {
        let mut zip_path = Utf8Path::new(&dist).join(name);
        zip_path.set_extension("zip");
        let zip_file = fs::File::create(&zip_path)?;
        let mut zip = zip::ZipWriter::new(zip_file);
        for filename in filenames {
            let mut f = fs::File::open(&filename)?;
            // Get only filename for the archive.
            let fpath = Utf8Path::new(&filename);
            let fname = fpath.file_name().unwrap();

            let options = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o744);
            zip.start_file(fname, options)?;
            io::copy(&mut f, &mut zip)?;
        }
        Ok(zip_path.to_string())
    })
    .await?;
    path
}
