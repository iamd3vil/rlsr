use crate::checksummer::Checksummer;
use async_trait::async_trait;
use blake2::Digest;
use color_eyre::eyre::{Result, WrapErr};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Blake2s {}

#[async_trait]
impl Checksummer for Blake2s {
    async fn compute(&self, file_path: &str) -> Result<String> {
        let mut file = File::open(file_path)
            .await
            .wrap_err_with(|| format!("Failed to open file: {}", file_path))?;

        let mut hasher = blake2::Blake2s256::new();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use tokio::runtime::Runtime;

    #[test]
    fn test_compute_blake2s() {
        // Create a temporary file with known content
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, world!";
        temp_file.write_all(test_data).unwrap();
        let temp_path = temp_file.path().to_str().unwrap().to_string();

        // Expected BLAKE2s-256 hash for "Hello, world!"
        let expected = "30d8777f0e178582ec8cd2fcdc18af57c828ee2f89e978df52c8e7af078bd5cf";

        // Create runtime and compute the hash
        let rt = Runtime::new().unwrap();
        let result = rt
            .block_on(async {
                let blake2s = Blake2s {};
                blake2s.compute(&temp_path).await
            })
            .unwrap();

        assert_eq!(result, expected);
    }
}
