use anyhow::{bail, Ok, Result};
use async_zip::write::{EntryOptions, ZipFileWriter};
use std::path::Path;
use tokio::process::Command;

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
    Ok(out.split('\n').map(String::from).collect())
}

async fn get_previous_tag() -> Result<String> {
    // Get previous tag's commit.
    let mut cmd = Command::new("git");
    cmd.args(vec!["rev-list", "--tags", "--skip=1",  "--max-count=1"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting previous tag: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    let prev_tag_commit = String::from_utf8_lossy(&output.stdout).to_string();
    
    // Get tag for the commit.
    let mut cmd = Command::new("git");
    cmd.args(vec!["describe", "--abbrev=0", "--tags", &prev_tag_commit]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting previous tag: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Get formatted git log.
pub async fn get_all_git_log() -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.args(vec!["log", "--format='%h: %B'"]);
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
    let mut cmd = Command::new("git");
    cmd.args(vec!["log", "--format='%h: %B'"]);
    let output = cmd.output().await?;
    if !output.status.success() {
        bail!(
            "error getting git log: {}",
            String::from_utf8_lossy(&output.stdout).to_string()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// Creates an zip archive with the file given.
pub async fn archive_file(filename: &str, dist: &str, name: &str) -> Result<String> {
    let mut f = tokio::fs::File::open(filename).await?;
    // Create a zip file.
    let mut zip_path = Path::new(&dist).join(name);
    zip_path.set_extension("zip");
    let mut zip_file = tokio::fs::File::create(&zip_path).await?;
    let mut zip = ZipFileWriter::new(&mut zip_file);

    // Get only filename for the archive.
    let fpath = Path::new(filename);
    let fname = fpath.file_name().unwrap().to_str().unwrap();

    let options = EntryOptions::new(String::from(fname), async_zip::Compression::Deflate);
    let mut zw = zip.write_entry_stream(options).await?;
    tokio::io::copy(&mut f, &mut zw).await?;

    zw.close().await?;
    zip.close().await?;
    if let Some(path) = zip_path.to_str() {
        Ok(String::from(path))
    } else {
        bail!("error getting archive");
    }
}
