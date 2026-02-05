use crate::config::{Changelog, Release};
use crate::release_provider::ReleaseProvider;
use crate::utils::{get_all_git_log, get_all_tags, get_changelog};
use async_trait::async_trait;
use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, Result};
use log::{debug, error, info};
use reqwest::{Body, Client};
use serde::Serialize;
use std::sync::Arc;
use tokio::fs;
use tokio_util::codec::{BytesCodec, FramedRead};

#[async_trait]
impl ReleaseProvider for Gitlab {
    async fn publish(
        &self,
        release: &Release,
        all_archives: Vec<String>,
        _image_tags: Vec<String>,
        latest_tag: String,
    ) -> Result<()> {
        self.publish_build(release, all_archives, self.token.clone(), latest_tag)
            .await
            .with_context(|| {
                format!(
                    "error publishing release to gitlab for release: {}",
                    release.name
                )
            })?;
        Ok(())
    }
}

pub struct Gitlab {
    token: String,
    changelog: Changelog,
}

impl Gitlab {
    pub fn new(token: String, changelog: Changelog) -> Self {
        Gitlab { token, changelog }
    }

    async fn publish_build(
        &self,
        release: &Release,
        all_archives: Vec<String>,
        token: String,
        latest_tag: String,
    ) -> Result<()> {
        let gl = match &release.targets.gitlab {
            Some(gl) => gl,
            None => {
                bail!("gitlab config is blank, skipping publishing");
            }
        };

        debug!("creating release in {}/{}", gl.owner, gl.repo);

        if token.is_empty() {
            bail!("GITLAB_TOKEN is blank, skipping publishing build");
        }

        // URL-encode the project path (owner/repo)
        let project_path = format!("{}/{}", gl.owner, gl.repo);
        let encoded_project = urlencoding::encode(&project_path);
        let base_url = gl.url.trim_end_matches('/');

        // Get changelog.
        let tags = get_all_tags().await?;
        let changelog = if tags.len() == 1 {
            get_all_git_log().await?
        } else {
            get_changelog(&self.changelog).await?
        };

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(100))
            .build()?;
        let client = Arc::new(client);

        // First, upload all archives as generic package files and collect their URLs
        let mut asset_links = Vec::new();

        let mut checksum_path = Utf8Path::new(&release.dist_folder)
            .join("checksums.txt")
            .to_string();
        if release.checksum.is_none() {
            checksum_path = String::from("");
        }

        // Upload archives
        for archive in &all_archives {
            let filename = Utf8Path::new(archive)
                .file_name()
                .unwrap_or("artifact")
                .to_string();

            let download_url = Self::upload_package_file(
                client.clone(),
                base_url,
                &encoded_project,
                &token,
                &latest_tag,
                archive,
                &filename,
            )
            .await
            .with_context(|| format!("error uploading archive {}", archive))?;

            asset_links.push(AssetLink {
                name: filename,
                url: download_url,
                link_type: "package".to_string(),
            });
        }

        // Upload checksum file if it exists
        if !checksum_path.is_empty() && tokio::fs::metadata(&checksum_path).await.is_ok() {
            debug!("uploading checksums file");
            let download_url = Self::upload_package_file(
                client.clone(),
                base_url,
                &encoded_project,
                &token,
                &latest_tag,
                &checksum_path,
                "checksums.txt",
            )
            .await
            .with_context(|| "error uploading checksum file")?;

            asset_links.push(AssetLink {
                name: "checksums.txt".to_string(),
                url: download_url,
                link_type: "other".to_string(),
            });
        }

        // Create the release with asset links
        let create_release_url = format!(
            "{}/api/v4/projects/{}/releases",
            base_url, encoded_project
        );

        let release_request = CreateReleaseRequest {
            tag_name: latest_tag.clone(),
            description: changelog,
            assets: ReleaseAssets {
                links: asset_links,
            },
        };

        debug!("creating gitlab release for tag: {}", latest_tag);
        let res = client
            .post(&create_release_url)
            .header("PRIVATE-TOKEN", &token)
            .header("Content-Type", "application/json")
            .json(&release_request)
            .send()
            .await
            .context("error creating release in gitlab")?;

        if !res.status().is_success() {
            let status = res.status();
            let error_body = res.text().await.unwrap_or_default();
            error!("gitlab release creation failed: {} - {}", status, error_body);
            bail!(
                "error creating gitlab release, status: {}, error: {}",
                status,
                error_body
            );
        }

        info!("gitlab release created successfully");
        Ok(())
    }

    /// Upload a file to GitLab's generic package registry and return the download URL
    async fn upload_package_file(
        client: Arc<Client>,
        base_url: &str,
        encoded_project: &str,
        token: &str,
        version: &str,
        filepath: &str,
        filename: &str,
    ) -> Result<String> {
        // Sanitize version for package registry (remove 'v' prefix if present)
        let package_version = version.strip_prefix('v').unwrap_or(version);

        // Upload URL for generic packages
        let upload_url = format!(
            "{}/api/v4/projects/{}/packages/generic/release/{}/{}",
            base_url, encoded_project, package_version, filename
        );

        debug!("uploading file {} to {}", filepath, upload_url);

        // Get file size
        let meta = fs::metadata(filepath)
            .await
            .context("error getting file metadata")?;
        let size = meta.len();

        // Open file and create streaming body
        let file = tokio::fs::File::open(filepath).await?;
        let stream = FramedRead::new(file, BytesCodec::new());
        let body = Body::wrap_stream(stream);

        let res = client
            .put(&upload_url)
            .header("PRIVATE-TOKEN", token)
            .header("Content-Length", size)
            .body(body)
            .send()
            .await
            .context("error uploading file to gitlab")?;

        if !res.status().is_success() {
            let status = res.status();
            let error_body = res.text().await.unwrap_or_default();
            bail!(
                "error uploading to gitlab package registry, status: {}, error: {}",
                status,
                error_body
            );
        }

        // Construct the download URL (GitLab returns just {"message": "201 Created"})
        let download_url = format!(
            "{}/api/v4/projects/{}/packages/generic/release/{}/{}",
            base_url, encoded_project, package_version, filename
        );

        debug!("file uploaded successfully: {}", download_url);
        Ok(download_url)
    }
}

#[derive(Debug, Serialize)]
struct CreateReleaseRequest {
    tag_name: String,
    description: String,
    assets: ReleaseAssets,
}

#[derive(Debug, Serialize)]
struct ReleaseAssets {
    links: Vec<AssetLink>,
}

#[derive(Debug, Serialize)]
struct AssetLink {
    name: String,
    url: String,
    link_type: String,
}


