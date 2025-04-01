use crate::TemplateMeta;

use super::{get_minijinja_env, Commit, Formatter};
use async_trait::async_trait;
use color_eyre::{eyre::Context, Result};
use minijinja::{context, Environment};
use tokio::fs;

const DEFAULT_TEMPLATE: &str = include_str!("tmpls/default.tpl");

pub struct DefaultFormatter {
    tmpl: Environment<'static>,
}

impl DefaultFormatter {
    pub async fn new(tmpl: Option<String>) -> Result<Self> {
        let content = match tmpl {
            Some(path) => fs::read_to_string(path).await?,
            None => DEFAULT_TEMPLATE.to_string(),
        };
        // Use the static version that takes ownership of content
        let env = get_minijinja_env(content)?;
        Ok(Self { tmpl: env })
    }
}

#[async_trait]
impl Formatter for DefaultFormatter {
    async fn format(&self, commits: &[Commit], meta: &TemplateMeta) -> Result<String> {
        let mut formatted = String::new();

        // Render the minijinja template.
        let tmpl = self.tmpl.get_template("tmpl").unwrap();

        // Create a context with the commits data for the template
        let ctx = context!(
            commits => commits,
            meta => meta,
        );

        // Render the template with the context
        let rendered = tmpl.render(ctx).wrap_err("error rendering template")?;
        formatted.push_str(&rendered);

        Ok(formatted)
    }
}
