use std::collections::HashMap;

use async_trait::async_trait;
use color_eyre::eyre::{Context, Result};
use octocrab::Octocrab;
use tokio::sync::Mutex;

use super::{Commit, Formatter};

pub struct GithubFormatter {
    ghclient: Octocrab,

    // cached_handles stores already discovered github handles.
    cached_handles: Mutex<HashMap<String, String>>,
}

impl GithubFormatter {
    pub fn new(token: String) -> Result<Self> {
        let ghclient = octocrab::OctocrabBuilder::default()
            .personal_token(token.clone())
            .build()
            .wrap_err("error creating octocrab client")?;

        // Initialize the cache.
        let cached_handles = Mutex::new(HashMap::new());

        Ok(Self {
            ghclient,
            cached_handles,
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
    async fn format(&self, commits: &[Commit]) -> Result<String> {
        let mut formatted = String::new();

        for commit in commits {
            let handle = self.get_github_handle(&commit.email).await?;
            formatted.push_str(&format!(
                "{}: {} (@{})\n",
                commit.hash, commit.subject, handle
            ));
        }

        Ok(formatted)
    }
}
