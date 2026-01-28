//! checksummer creates a

mod blake2b;
mod blake2s;
mod md5;
mod sha1;
mod sha256;
mod sha3_256;
mod sha3_512;
mod sha512;

use async_trait::async_trait;
use color_eyre::eyre::{bail, Result};

use self::blake2b::Blake2b;
use self::blake2s::Blake2s;
use self::md5::Md5;
use self::sha1::Sha1;
use self::sha256::Sha256;
use self::sha3_256::Sha3_256;
use self::sha3_512::Sha3_512;
use self::sha512::Sha512;

#[async_trait]
pub trait Checksummer: Send {
    async fn compute(&self, file_path: &str) -> Result<String>;
}

pub fn get_new_checksummer(algorithm: &str) -> Result<Box<dyn Checksummer + Send>> {
    match algorithm {
        "sha256" => Ok(Box::new(Sha256 {})),
        "sha512" => Ok(Box::new(Sha512 {})),
        "sha3_256" => Ok(Box::new(Sha3_256 {})),
        "sha3_512" => Ok(Box::new(Sha3_512 {})),
        "blake2b" => Ok(Box::new(Blake2b {})),
        "blake2s" => Ok(Box::new(Blake2s {})),
        "md5" => Ok(Box::new(Md5 {})),
        "sha1" => Ok(Box::new(Sha1 {})),
        _ => {
            bail!("invalid algoirithm for checksum");
        }
    }
}
