use crate::config::Release;
use crate::TemplateMeta;
use async_trait::async_trait;
use color_eyre::eyre::Result;

pub mod docker;
pub mod github;
pub mod gitlab;

/// ReleaseProvider is the trait which needs to be implemented for all the
/// different types of release targets. For example, we can implement a provider
/// for github or docker and just call it from our main execution loop.
#[async_trait]
pub trait ReleaseProvider {
    /// Name used in logs, such as "GitHub".
    fn name(&self) -> &'static str;

    async fn publish(
        &self,
        cfg: &Release,
        all_archives: Vec<String>,
        image_tags: Vec<String>,
        meta: &TemplateMeta,
    ) -> Result<()>;
}
