use crate::checksummer::Checksummer;
use async_trait::async_trait;
use blake2::Digest;
use color_eyre::eyre::{Result, WrapErr};
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Blake2b {}

#[async_trait]
impl Checksummer for Blake2b {
    async fn compute(&self, file_path: &str) -> Result<String> {
        let mut file = File::open(file_path)
            .await
            .wrap_err_with(|| format!("Failed to open file: {}", file_path))?;

        let mut hasher = blake2::Blake2b512::new();
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
    fn test_compute_blake2b() {
        // Create a temporary file with known content
        let mut temp_file = NamedTempFile::new().unwrap();
        let test_data = b"Hello, world!";
        temp_file.write_all(test_data).unwrap();
        let temp_path = temp_file.path().to_str().unwrap().to_string();

        // Expected BLAKE2b-512 hash for "Hello, world!"
        let expected = "a2764d133a16816b5847a737a786f2ece4c148095c5faa73e24b4cc5d666c3e45ec271504e14dc6127ddfce4e144fb23b91a6f7b04b53d695502290722953b0f";

        // Create runtime and compute the hash
        let rt = Runtime::new().unwrap();
        let result = rt
            .block_on(async {
                let blake2b = Blake2b {};
                blake2b.compute(&temp_path).await
            })
            .unwrap();

        assert_eq!(result, expected);
    }
}
