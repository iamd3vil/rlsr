use crate::checksummer::Checksummer;
use async_trait::async_trait;
use color_eyre::eyre::{Result, WrapErr};
use sha3::Digest;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Sha3_512 {}

#[async_trait]
impl Checksummer for Sha3_512 {
    async fn compute(&self, file_path: &str) -> Result<String> {
        let mut file = File::open(file_path)
            .await
            .wrap_err_with(|| format!("Failed to open file: {}", file_path))?;

        let mut hasher = sha3::Sha3_512::new();
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
    fn test_compute_sha3_512() {
        // Create a temporary file with known content
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, world!";
        temp_file.write_all(test_data).unwrap();
        let temp_path = temp_file.path().to_str().unwrap().to_string();

        // Expected SHA3-512 hash for "Hello, world!"
        let expected = "8e47f1185ffd014d238fabd02a1a32defe698cbf38c037a90e3c0a0a32370fb52cbd641250508502295fcabcbf676c09470b27443868c8e5f70e26dc337288af";

        // Create runtime and compute the hash
        let rt = Runtime::new().unwrap();
        let result = rt
            .block_on(async {
                let sha3_512 = Sha3_512 {};
                sha3_512.compute(&temp_path).await
            })
            .unwrap();

        assert_eq!(result, expected);
    }
}
