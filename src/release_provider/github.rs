use crate::config::{Changelog, Release};
use crate::release_provider::ReleaseProvider;
use crate::utils::{get_all_git_log, get_all_tags, get_changelog};
use async_trait::async_trait;
use camino::Utf8Path;
use color_eyre::eyre::{bail, Result};
use log::{debug, error, info};
use reqwest::{Body, Client};
use std::sync::Arc;
use tokio::fs;
use tokio::sync::Mutex;
use tokio_util::codec::{BytesCodec, FramedRead};

const MEDIA_TYPE: &str = "application/vnd.github.v3+json";

#[allow(clippy::needless_arbitrary_self_type)]
#[async_trait]
impl ReleaseProvider for Github {
    async fn publish(
        self: &Self,
        release: &Release,
        all_archives: Arc<Mutex<Vec<String>>>,
        latest_tag: String,
    ) -> Result<()> {
        self.publish_build(release, all_archives, self.ghtoken.clone(), latest_tag)
            .await?;
        Ok(())
    }
}

pub struct Github {
    ghtoken: String,
    changelog: Changelog,
}

impl Github {
    pub fn new(ghtoken: String, changelog: Changelog) -> Self {
        Github { ghtoken, changelog }
    }

    async fn publish_build(
        &self,
        release: &Release,
        all_archives: Arc<Mutex<Vec<String>>>,
        ghtoken: String,
        latest_tag: String,
    ) -> Result<()> {
        let gh = match &release.targets.github {
            Some(gh) => gh,
            None => {
                bail!("github repo is blank, skipping publishing");
            }
        };

        debug!("creating release in {}/{}", gh.owner, gh.repo);

        if ghtoken.is_empty() {
            bail!("GITHUB_TOKEN is blank, skipping publishing build");
        }

        let ghclient = octocrab::OctocrabBuilder::default()
            .personal_token(ghtoken.clone())
            .build()?;

        // Get changelog.
        let tags = get_all_tags().await?;
        let changelog = if tags.len() == 1 {
            get_all_git_log().await?
        } else {
            get_changelog(&self.changelog).await?
        };

        let res = ghclient
            .repos(&gh.owner, &gh.repo)
            .releases()
            .create(&latest_tag)
            .body(&changelog)
            .send()
            .await?;

        let release_id = res.id.0;
        let github = release.targets.github.clone();
        let (owner, repo) = match github {
            Some(gh) => (gh.owner, gh.repo),
            None => bail!("couldn't find github details to publish release"),
        };
        let ghtoken = ghtoken.clone();
        let mut checksum_path = Utf8Path::new(&release.dist_folder)
            .join("checksums.txt")
            .to_string();
        if release.checksum.is_none() {
            checksum_path = String::from("");
        }
        // Upload all archives.
        Self::upload_archives(
            all_archives.lock().await.to_vec(),
            release_id,
            owner,
            repo,
            ghtoken,
            checksum_path,
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
        checksum_path: String,
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
            let filename = String::from(Utf8Path::new(&archives[i]).file_name().unwrap());

            let upload_owner = owner.clone();
            let upload_ghtoken = ghtoken.clone();
            let ghclient = client.clone();
            let upload_url = format!(
                "https://uploads.github.com/repos/{}/{}/releases/{}/assets?name={}",
                upload_owner, repo, release_id, filename
            );
            all_uploads.push(tokio::spawn(async move {
                debug!("uploading to url: {}", upload_url);
                let res = Self::upload_file(
                    upload_url,
                    archives[i].clone(),
                    ghclient,
                    upload_owner,
                    upload_ghtoken,
                )
                .await;
                if let Err(err) = res {
                    error!("error uploading archive {}: {}", archives[i], err);
                    std::process::exit(1);
                }
            }));
        }
        // Upload checksum.
        if !checksum_path.is_empty() {
            debug!("uploading checksums file");
            let ghclient = client.clone();
            let checksum_owner = owner.clone();
            let checksum_ghtoken = ghtoken.clone();
            let upload_url = format!(
                "https://uploads.github.com/repos/{}/{}/releases/{}/assets?name={}",
                checksum_owner, repo, release_id, "checksums.txt",
            );
            all_uploads.push(tokio::spawn(async move {
                let res = Self::upload_file(
                    upload_url,
                    checksum_path.clone(),
                    ghclient,
                    checksum_owner,
                    checksum_ghtoken,
                )
                .await;
                if let Err(err) = res {
                    error!("error uploading checksum file {}: {}", checksum_path, err);
                    std::process::exit(1);
                }
            }))
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
        let mime_type = match infer::get_from_path(&filepath)? {
            Some(mime_type) => mime_type.to_string(),
            None => {
                String::from("application/octet-stream")
                // let ext = Utf8Path::new(&filepath).extension();
                // if ext.is_some() && ext.unwrap() == "txt" {
                //     String::from("text/plain")
                // } else {
                // }
            }
        };

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
