use crate::config::Release;
use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// ReleaseProvider is the trait which needs to be implemented for all the
/// different types of release targets. For example, we can implement a provider
/// for github or docker and just call it from our main execution loop.
#[async_trait]
pub trait ReleaseProvider {
    async fn publish(
        self: &Self,
        cfg: &Release,
        all_archives: Arc<Mutex<Vec<String>>>,
    ) -> Result<()>;
}
