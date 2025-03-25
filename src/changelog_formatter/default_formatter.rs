use super::{Commit, Formatter};
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
        let mut env = Environment::new();
        env.add_template_owned("tmpl", content)
            .wrap_err("error adding template")?;
        Ok(Self { tmpl: env })
    }
}

#[async_trait]
impl Formatter for DefaultFormatter {
    async fn format(&self, commits: &[Commit]) -> Result<String> {
        let mut formatted = String::new();

        // Render the minijinja template.
        let tmpl = self.tmpl.get_template("tmpl").unwrap();

        // Create a context with the commits data for the template
        let ctx = context!(
            commits => commits
        );

        // Render the template with the context
        let rendered = tmpl.render(ctx).wrap_err("error rendering template")?;
        formatted.push_str(&rendered);

        Ok(formatted)
    }
}
