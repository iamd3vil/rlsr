use async_trait::async_trait;
use color_eyre::eyre::Result;

use crate::utils;

mod default_formatter;
mod github;

pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub email: String,
}

#[async_trait]
#[allow(clippy::needless_arbitrary_self_type)]
pub trait Formatter: Send {
    async fn format(&self, commits: &[Commit]) -> Result<String>;
}

pub fn get_new_formatter(format: &str) -> Result<Box<dyn Formatter + Send>> {
    match format {
        "github" => {
            let token = utils::get_github_token();
            Ok(Box::new(github::GithubFormatter::new(token)?))
        }

        _ => Ok(Box::new(default_formatter::DefaultFormatter())),
    }
}
