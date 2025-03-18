use crate::config::Release;
use async_trait::async_trait;
use color_eyre::eyre::Result;

pub mod docker;
pub mod github;

/// ReleaseProvider is the trait which needs to be implemented for all the
/// different types of release targets. For example, we can implement a provider
/// for github or docker and just call it from our main execution loop.
#[async_trait]
#[allow(clippy::needless_arbitrary_self_type)]
pub trait ReleaseProvider {
    async fn publish(
        self: &Self,
        cfg: &Release,
        all_archives: Vec<String>,
        latest_tag: String,
    ) -> Result<()>;
}
