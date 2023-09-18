mod sha256;

use async_trait::async_trait;
use eyre::{bail, Result};

use self::sha256::Sha256;

#[async_trait]
#[allow(clippy::needless_arbitrary_self_type)]
pub trait Checksummer: Send {
    async fn compute(self: &Self, file_path: &str) -> Result<String>;
}

pub fn get_new_checksummer(algorithm: &str) -> Result<Box<dyn Checksummer + Send>> {
    match algorithm {
        "sha256" => Ok(Box::new(Sha256 {})),
        _ => {
            bail!("invalid algoirithm for checksum");
        }
    }
}
