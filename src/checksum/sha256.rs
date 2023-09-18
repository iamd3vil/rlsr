use crate::checksum::Checksummer;
use async_trait::async_trait;
use eyre::Result;
use sha2::Digest;
use tokio::fs::File;
use tokio::io::AsyncReadExt;

pub struct Sha256 {}

#[allow(clippy::needless_arbitrary_self_type)]
#[async_trait]
impl Checksummer for Sha256 {
    async fn compute(self: &Self, file_path: &str) -> Result<String> {
        let mut file = File::open(file_path).await?;
        let mut buffer = vec![0; 4096];
        let mut hasher = sha2::Sha256::new();

        loop {
            let bytes_read = file.read(&mut buffer).await?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }
}
