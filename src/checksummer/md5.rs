use crate::checksummer::Checksummer;
use async_trait::async_trait;
use color_eyre::eyre::{Result, WrapErr};
use md5::{Digest, Md5 as Md5Hasher};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Md5 {}

#[async_trait]
impl Checksummer for Md5 {
    async fn compute(&self, file_path: &str) -> Result<String> {
        let mut file = File::open(file_path)
            .await
            .wrap_err_with(|| format!("Failed to open file: {}", file_path))?;

        let mut hasher = Md5Hasher::new();
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
    fn test_compute_md5() {
        // Create a temporary file with known content
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, world!";
        temp_file.write_all(test_data).unwrap();
        let temp_path = temp_file.path().to_str().unwrap().to_string();

        // Expected MD5 hash for "Hello, world!"
        let expected = "6cd3556deb0da54bca060b4c39479839";

        // Create runtime and compute the hash
        let rt = Runtime::new().unwrap();
        let result = rt
            .block_on(async {
                let md5 = Md5 {};
                md5.compute(&temp_path).await
            })
            .unwrap();

        assert_eq!(result, expected);
    }
}
