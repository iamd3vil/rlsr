use crate::config::{Changelog, Release};
use crate::release_provider::ReleaseProvider;
use crate::utils::{get_all_git_log, get_changelog};
use crate::TemplateMeta;
use async_trait::async_trait;
use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, Result};
use log::{debug, error, info};
use reqwest::{Body, Client};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::fs;
use tokio_util::codec::{BytesCodec, FramedRead};

#[async_trait]
impl ReleaseProvider for Gitlab {
    fn name(&self) -> &'static str {
        "GitLab"
    }

    async fn publish(
        &self,
        release: &Release,
        all_archives: Vec<String>,
        _image_tags: Vec<String>,
        meta: &TemplateMeta,
    ) -> Result<()> {
        self.publish_build(release, all_archives, self.token.clone(), meta)
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
        meta: &TemplateMeta,
    ) -> Result<()> {
        let latest_tag = &meta.tag;
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

        // The first release has no previous tag to diff against.
        let changelog = if meta.previous_tag.is_empty() {
            get_all_git_log().await?
        } else {
            get_changelog(&self.changelog, meta).await?
        };

        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::limited(100))
            .build()?;
        let client = Arc::new(client);

        // A nightly replaces the previous one under the same tag.
        if meta.is_nightly {
            Self::delete_previous_nightly(&client, base_url, &encoded_project, &token, latest_tag)
                .await?;
        }

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
                latest_tag,
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
                latest_tag,
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
        let create_release_url =
            format!("{}/api/v4/projects/{}/releases", base_url, encoded_project);

        let release_request = CreateReleaseRequest {
            tag_name: latest_tag.clone(),
            // Creates the tag at HEAD when it doesn't exist, as for a nightly.
            git_ref: meta.commit.clone(),
            description: changelog,
            assets: ReleaseAssets { links: asset_links },
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
            error!(
                "gitlab release creation failed: {} - {}",
                status, error_body
            );
            bail!(
                "error creating gitlab release, status: {}, error: {}",
                status,
                error_body
            );
        }

        info!("gitlab release created successfully");
        Ok(())
    }

    /// Deletes the previous nightly's release, tag and package files, if they
    /// exist.
    async fn delete_previous_nightly(
        client: &Client,
        base_url: &str,
        encoded_project: &str,
        token: &str,
        tag: &str,
    ) -> Result<()> {
        let api = format!("{}/api/v4/projects/{}", base_url, encoded_project);
        let encoded_tag = urlencoding::encode(tag);
        // Deleting a release leaves its tag behind, so delete both.
        for (what, url) in [
            ("release", format!("{}/releases/{}", api, encoded_tag)),
            ("tag", format!("{}/repository/tags/{}", api, encoded_tag)),
        ] {
            let res = client
                .delete(&url)
                .header("PRIVATE-TOKEN", token)
                .send()
                .await
                .with_context(|| format!("error deleting the previous {} {}", tag, what))?;
            if res.status() == reqwest::StatusCode::NOT_FOUND {
                continue;
            }
            if !res.status().is_success() {
                let status = res.status();
                let body = res.text().await.unwrap_or_default();
                bail!(
                    "error deleting the previous {} {}, status: {}, error: {}",
                    tag,
                    what,
                    status,
                    body
                );
            }
            info!("deleted the previous {} {}", tag, what);
        }

        // Uploading to the same package version adds files next to the old
        // ones, so delete the package. package_name matches loosely, so
        // check the name and version too.
        let version = package_version(tag);
        let packages: Vec<Package> = client
            .get(format!("{}/packages", api))
            .query(&[
                ("package_type", "generic"),
                ("package_name", PACKAGE_NAME),
                ("package_version", version),
            ])
            .header("PRIVATE-TOKEN", token)
            .send()
            .await
            .context("error listing gitlab packages")?
            .error_for_status()
            .context("error listing gitlab packages")?
            .json()
            .await
            .context("error reading gitlab packages")?;
        for package in packages
            .iter()
            .filter(|p| p.name == PACKAGE_NAME && p.version == version)
        {
            client
                .delete(format!("{}/packages/{}", api, package.id))
                .header("PRIVATE-TOKEN", token)
                .send()
                .await
                .context("error deleting gitlab package")?
                .error_for_status()
                .with_context(|| format!("error deleting gitlab package {}", package.id))?;
            info!("deleted the previous {} package files", tag);
        }
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
        let package_version = package_version(version);

        // Upload URL for generic packages
        let upload_url = format!(
            "{}/api/v4/projects/{}/packages/generic/{}/{}/{}",
            base_url, encoded_project, PACKAGE_NAME, package_version, filename
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
            "{}/api/v4/projects/{}/packages/generic/{}/{}/{}",
            base_url, encoded_project, PACKAGE_NAME, package_version, filename
        );

        debug!("file uploaded successfully: {}", download_url);
        Ok(download_url)
    }
}

/// Generic package that release files are uploaded to.
const PACKAGE_NAME: &str = "release";

/// The package version files for a tag are uploaded under.
fn package_version(tag: &str) -> &str {
    tag.strip_prefix('v').unwrap_or(tag)
}

#[derive(Debug, Deserialize)]
struct Package {
    id: u64,
    name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct CreateReleaseRequest {
    tag_name: String,
    #[serde(rename = "ref")]
    git_ref: String,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_version() {
        assert_eq!(package_version("v1.2.3"), "1.2.3");
        assert_eq!(package_version("1.2.3"), "1.2.3");
        assert_eq!(package_version("nightly"), "nightly");
    }

    #[test]
    fn test_create_release_request_sends_ref() {
        let request = CreateReleaseRequest {
            tag_name: "nightly".to_string(),
            git_ref: "abcdef1234567890".to_string(),
            description: "notes".to_string(),
            assets: ReleaseAssets { links: vec![] },
        };
        let json = serde_yaml::to_string(&request).unwrap();
        assert!(json.contains("ref: abcdef1234567890"));
        assert!(!json.contains("git_ref"));
    }
}
