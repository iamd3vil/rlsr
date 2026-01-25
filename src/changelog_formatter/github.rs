use std::collections::HashMap;

use async_trait::async_trait;
use color_eyre::eyre::{Context, Result};
use minijinja::{context, Environment};
use octocrab::Octocrab;
use tokio::{fs, sync::Mutex};

use crate::TemplateMeta;

use super::{get_minijinja_env, Commit, Formatter};

const DEFAULT_GH_TEMPLATE: &str = include_str!("tmpls/default_github_template.tpl");

pub struct GithubFormatter {
    ghclient: Octocrab,

    tmpl: Environment<'static>,

    // cached_handles stores already discovered github handles.
    cached_handles: Mutex<HashMap<String, String>>,
}

impl GithubFormatter {
    pub async fn new(token: String, tmpl: Option<String>) -> Result<Self> {
        let ghclient = octocrab::OctocrabBuilder::default()
            .personal_token(token.clone())
            .build()
            .wrap_err("error creating octocrab client")?;

        // Initialize the cache.
        let cached_handles = Mutex::new(HashMap::new());

        let content = match tmpl {
            Some(path) => fs::read_to_string(path).await?,
            None => DEFAULT_GH_TEMPLATE.to_string(),
        };

        // Use get_minijinja_env for consistency
        let env = get_minijinja_env(content)?;

        Ok(Self {
            ghclient,
            cached_handles,
            tmpl: env,
        })
    }

    async fn get_github_handle(&self, email: &str) -> Result<String> {
        // Check if the handle is already cached.
        let cached_handles = self.cached_handles.lock().await;
        if let Some(handle) = cached_handles.get(email) {
            return Ok(handle.clone());
        }
        drop(cached_handles);

        let user = self.ghclient.search().users(email).send().await?;

        // Check if there is a user.
        if user.items.is_empty() {
            return Ok(email.to_string());
        }

        // Cache the handle.
        let mut cached_handles = self.cached_handles.lock().await;
        cached_handles.insert(email.to_string(), user.items[0].login.clone());

        Ok(user.items[0].login.clone())
    }
}

#[async_trait]
impl Formatter for GithubFormatter {
    async fn format(&self, commits: &[Commit], meta: &TemplateMeta) -> Result<String> {
        let mut formatted = String::new();
        let mut commits = commits.to_vec();

        for commit in commits.iter_mut() {
            let handle = self.get_github_handle(&commit.email).await?;
            commit.handle = Some(handle);
        }

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
