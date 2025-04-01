use async_trait::async_trait;
use color_eyre::eyre::Result;
use color_eyre::eyre::WrapErr;
use minijinja::Environment;
use serde::Serialize;

use crate::utils;
use crate::TemplateMeta;

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
    async fn format(&self, commits: &[Commit], meta: &TemplateMeta) -> Result<String>;
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

pub fn get_minijinja_env(content: String) -> Result<Environment<'static>> {
    let mut env = Environment::new();
    env.add_template_owned("tmpl", content)
        .wrap_err("error adding template")?;
    env.add_filter("starts_with", starts_with_filter);
    env.add_filter("ends_with", ends_with_filter);
    env.set_lstrip_blocks(true);
    env.set_trim_blocks(true);
    Ok(env)
}

fn starts_with_filter(value: String, prefix: String) -> bool {
    value.starts_with(&prefix)
}

fn ends_with_filter(value: String, suffix: String) -> bool {
    value.ends_with(&suffix)
}
