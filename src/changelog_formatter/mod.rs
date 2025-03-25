use async_trait::async_trait;
use color_eyre::eyre::Result;
use serde::Serialize;

use crate::utils;

mod default_formatter;
mod github;

#[derive(Debug, Serialize, Clone)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub email: String,
    pub handle: Option<String>, // Will be used in GH formatter.
}

#[async_trait]
#[allow(clippy::needless_arbitrary_self_type)]
pub trait Formatter: Send {
    async fn format(&self, commits: &[Commit]) -> Result<String>;
}

pub async fn get_new_formatter(
    format: &str,
    tmpl: Option<String>,
) -> Result<Box<dyn Formatter + Send>> {
    match format {
        "github" => get_github_formatter(tmpl).await,
        _ => get_default_formatter(tmpl).await,
    }
}

/// Creates a new instance of the default formatter
pub async fn get_default_formatter(tmpl: Option<String>) -> Result<Box<dyn Formatter + Send>> {
    Ok(Box::new(
        default_formatter::DefaultFormatter::new(tmpl).await?,
    ))
}

pub async fn get_github_formatter(tmpl: Option<String>) -> Result<Box<dyn Formatter + Send>> {
    let token = utils::get_github_token();
    Ok(Box::new(github::GithubFormatter::new(token, tmpl).await?))
}
