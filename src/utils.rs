use anyhow::{bail, Result};
use async_zip::write::{EntryOptions, ZipFileWriter};
use std::path::Path;
use tokio::process::Command;

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
