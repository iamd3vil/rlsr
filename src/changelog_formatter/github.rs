use std::collections::HashMap;

use async_trait::async_trait;
use color_eyre::eyre::{eyre, Context, Result};
use minijinja::{context, Environment};
use octocrab::Octocrab;
use tokio::{fs, sync::Mutex};

use crate::{utils, TemplateMeta};

use super::{get_minijinja_env, Commit, Formatter};

const DEFAULT_GH_TEMPLATE: &str = include_str!("tmpls/default_github_template.tpl");

pub struct GithubFormatter {
    ghclient: Octocrab,

    tmpl: Environment<'static>,

    // wants_notes is set when the template uses `github_notes`, so the API
    // call is skipped otherwise.
    wants_notes: bool,

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

        let wants_notes = content.contains("github_notes");

        // Use get_minijinja_env for consistency
        let env = get_minijinja_env(content)?;

        Ok(Self {
            ghclient,
            cached_handles,
            tmpl: env,
            wants_notes,
        })
    }

    // generate_notes fetches GitHub's auto-generated release notes (What's
    // Changed, New Contributors, Full Changelog) for the release tag.
    async fn generate_notes(&self, meta: &TemplateMeta) -> Result<String> {
        let (owner, repo) = utils::get_github_repo_from_remote()
            .await?
            .ok_or_else(|| eyre!("github_notes needs a GitHub remote.origin.url"))?;
        let repo = self.ghclient.repos(owner, repo);
        let releases = repo.releases();
        // target_commitish matters when the tag doesn't exist yet or points
        // elsewhere, as with a nightly's rolling tag.
        let mut req = releases
            .generate_release_notes(&meta.tag)
            .target_commitish(&meta.commit);
        if !meta.previous_tag.is_empty() {
            req = req.previous_tag_name(&meta.previous_tag);
        }
        let notes = req
            .send()
            .await
            .wrap_err("error generating github release notes")?;
        Ok(notes.body)
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

        let github_notes = if self.wants_notes {
            self.generate_notes(meta).await?
        } else {
            String::new()
        };

        // Render the minijinja template.
        let tmpl = self.tmpl.get_template("tmpl").unwrap();

        // Create a context with the commits data for the template
        let ctx = context!(
            commits => commits,
            github_notes => github_notes,
            meta => meta,
            env => &meta.env,
            date => &meta.date,
            timestamp => &meta.timestamp,
            now => &meta.now,
        );

        // Render the template with the context
        let rendered = tmpl.render(ctx).wrap_err("error rendering template")?;
        formatted.push_str(&rendered);

        Ok(formatted)
    }
}
