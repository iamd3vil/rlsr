use super::{Commit, Formatter};
use async_trait::async_trait;
use color_eyre::Result;

pub struct DefaultFormatter();

#[async_trait]
impl Formatter for DefaultFormatter {
    async fn format(&self, commits: &Vec<Commit>) -> Result<String> {
        let mut formatted = String::new();

        for commit in commits {
            formatted.push_str(&format!("{}: {}\n", commit.hash, commit.subject));
        }

        Ok(formatted)
    }
}
