use crate::checksummer::Checksummer;
use async_trait::async_trait;
use color_eyre::eyre::{Result, WrapErr};
use sha2::Digest;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Sha256 {}

#[async_trait]
impl Checksummer for Sha256 {
    async fn compute(&self, file_path: &str) -> Result<String> {
        let mut file = File::open(file_path)
            .await
            .wrap_err_with(|| format!("Failed to open file: {}", file_path))?;

        let mut hasher = sha2::Sha256::new();
        let mut buffer = vec![0; 16 * 1024]; // 16KB buffer for better performance

        loop {
            let bytes_read = file
                .read(&mut buffer)
                .await
                .wrap_err_with(|| format!("Failed to read from file: {}", file_path))?;

            if bytes_read == 0 {
                break;
            }

            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
