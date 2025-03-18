use crate::checksummer::get_new_checksummer;
use crate::Release;
use camino::Utf8Path;
use color_eyre::eyre::{Context, Result};
use log::debug;
use tokio::{fs, io::AsyncWriteExt};

pub async fn create_checksums(rls: &Release, archives: Vec<String>) -> Result<()> {
    let cm_path = Utf8Path::new(&rls.dist_folder).join("checksums.txt");
    if fs::metadata(&cm_path).await.is_ok() {
        // Remove checksums file if it exists.
        fs::remove_file(&cm_path)
            .await
            .wrap_err_with(|| "error deleting checksums file")?;
    }

    // Open the file with options set to both create (if it doesn't exist) and append
    let mut file = fs::OpenOptions::new()
        .create(true) // create if it doesn't exist
        .append(true) // set to append mode
        .open(&cm_path)
        .await
        .wrap_err_with(|| "error creating checksums file")?;
    for arc in archives {
        let path = Utf8Path::new(&arc);

        let cm = get_new_checksummer(rls.checksum.as_ref().unwrap().algorithm.as_ref())?;

        let checksum = cm.compute(&arc).await?;

        debug!(
            "writing to checksums file: {}, {}",
            path.file_name().unwrap(),
            &checksum
        );
        // Write the name and checksum to the file
        file.write_all(format!("{}\t{}\n", path.file_name().unwrap(), checksum).as_bytes())
            .await
            .wrap_err_with(|| "error writing checksums to file")?;

        file.flush().await?;
    }

    Ok(())
}
