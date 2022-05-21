use crate::config::Release;
use crate::release_provider::ReleaseProvider;
use crate::utils::{get_all_git_log, get_all_tags, get_changelog, get_latest_tag};
use anyhow::{bail, Result};
use async_trait::async_trait;
use log::{debug, error, info};
use reqwest::{Body, Client};
use std::path::Path;
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use tokio_util::codec::{BytesCodec, FramedRead};

const MEDIA_TYPE: &str = "application/vnd.github.v3+json";

#[async_trait]
impl ReleaseProvider for Github {
    async fn publish(
        self: &Self,
        release: &Release,
        all_archives: Arc<Mutex<Vec<String>>>,
    ) -> Result<()> {
        Self::publish_build(release, all_archives, self.ghtoken.clone()).await?;
        Ok(())
    }
}

pub struct Github {
    ghtoken: String,
}

impl Github {
    pub fn new(ghtoken: String) -> Self {
        Github { ghtoken }
    }

    async fn publish_build(
        release: &Release,
        all_archives: Arc<Mutex<Vec<String>>>,
        ghtoken: String,
    ) -> Result<()> {
        // Publish to github if we can find a latest tag or github repo configured in config.
        let latest_tag = match get_latest_tag().await {
            Ok(tag) => {
                info!("found out latest tag: {}", tag);
                tag
            }
            Err(_) => {
                bail!("error finding tag, skipping publishing");
            }
        };
        debug!("latest tag: {}", latest_tag);

        let gh = match &release.github {
            Some(gh) => gh,
            None => {
                bail!("github repo is blank, skipping publishing");
            }
        };

        debug!("creating release in {}/{}", gh.owner, gh.repo);

        if ghtoken.is_empty() {
            bail!("GITHUB_TOKEN is blank, skipping publishing build");
        }

        if !ghtoken.is_empty() {
            octocrab::initialise(octocrab::Octocrab::builder().personal_token(ghtoken.clone()))?;
        }

        let ghclient = octocrab::instance();

        // Get changelog.
        let tags = get_all_tags().await?;
        let changelog = if tags.len() == 1 {
            get_all_git_log().await?
        } else {
            get_changelog().await?
        };

        let res = ghclient
            .repos(&gh.owner, &gh.repo)
            .releases()
            .create(&latest_tag)
            .body(&changelog)
            .send()
            .await?;

        let release_id = res.id.0;
        let github = release.github.clone();
        let (owner, repo) = match github {
            Some(gh) => (gh.owner, gh.repo),
            None => bail!("couldn't find github details to publish release"),
        };
        let ghtoken = ghtoken.clone();
        // Upload all archives.
        Self::upload_archives(
            all_archives.lock().await.to_vec(),
            release_id,
            owner,
            repo,
            ghtoken,
        )
        .await?;

        info!("release created");
        Ok(())
    }

    async fn upload_archives(
        archives: Vec<String>,
        release_id: u64,
        owner: String,
        repo: String,
        ghtoken: String,
    ) -> Result<()> {
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(100))
            .build()?;
        let client = Arc::new(client);
        let mut all_uploads = vec![];
        let num = archives.len();
        let archives = Arc::new(archives);
        for i in 0..num {
            let archives = archives.clone();
            let filename = String::from(
                Path::new(&archives[i])
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap(),
            );
            let upload_url = format!(
                "https://uploads.github.com/repos/{}/{}/releases/{}/assets?name={}",
                owner, repo, release_id, filename
            );
            let ghclient = client.clone();
            let ghtoken = ghtoken.clone();
            let owner = owner.clone();
            all_uploads.push(tokio::spawn(async move {
                debug!("uploading to url: {}", upload_url);
                let res =
                    Self::upload_file(upload_url, archives[i].clone(), ghclient, owner, ghtoken)
                        .await;
                if let Err(err) = res {
                    error!("error uploading archive {}: {}", archives[i], err);
                    std::process::exit(1);
                }
            }));
        }

        futures::future::join_all(all_uploads).await;
        Ok(())
    }

    async fn upload_file(
        url: String,
        filepath: String,
        ghclient: Arc<Client>,
        owner: String,
        ghtoken: String,
    ) -> Result<()> {
        // Stat the file to get the size of the file.
        let meta = fs::metadata(&filepath).await?;
        let size = meta.len();

        // Guess mime.
        let mime_type = infer::get_from_path(&filepath)?.unwrap().mime_type();

        // Open file.
        let f = tokio::fs::File::open(&filepath).await?;
        let res = ghclient
            .post(url)
            .basic_auth(owner, Some(ghtoken))
            .body(file_to_body(f))
            .header("Content-Length", size)
            .header("Content-Type", mime_type)
            .header("Accept", MEDIA_TYPE)
            .send()
            .await?;
        if res.status() != reqwest::StatusCode::CREATED {
            bail!(
                "error uploading to github, status: {}, error: {}",
                res.status(),
                res.text().await?
            );
        }

        Ok(())
    }
}

fn file_to_body(file: tokio::fs::File) -> Body {
    let stream = FramedRead::new(file, BytesCodec::new());
    Body::wrap_stream(stream)
}
