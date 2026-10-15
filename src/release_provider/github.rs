use crate::config::{Changelog, Github as GithubConfig, Release};
use crate::release_provider::ReleaseProvider;
use crate::utils::{get_all_git_log, get_changelog};
use crate::{templating, TemplateMeta};
use async_trait::async_trait;
use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, ContextCompat, Result};
use log::{debug, error, info, warn};
use reqwest::{Body, Client};
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio_util::codec::{BytesCodec, FramedRead};

const MEDIA_TYPE: &str = "application/vnd.github.v3+json";
const UPLOAD_ATTEMPTS: u64 = 3;

#[async_trait]
impl ReleaseProvider for Github {
    fn name(&self) -> &'static str {
        "GitHub"
    }

    async fn publish(
        &self,
        release: &Release,
        all_archives: Vec<String>,
        _image_tags: Vec<String>,
        meta: &TemplateMeta,
    ) -> Result<()> {
        self.publish_build(release, all_archives, self.ghtoken.clone(), meta)
            .await
            .with_context(|| {
                format!(
                    "error publishing release to github for release: {}",
                    release.name
                )
            })?;
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
        all_archives: Vec<String>,
        ghtoken: String,
        meta: &TemplateMeta,
    ) -> Result<()> {
        let latest_tag = &meta.tag;
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

        let gh_release = match Self::find_release(&ghclient, gh, latest_tag).await? {
            Some(existing) => {
                info!("release for {} already exists, uploading to it", latest_tag);
                existing
            }
            None => self.create_release(&ghclient, gh, meta).await?,
        };

        let mut files = all_archives;
        if release.checksum.is_some() {
            files.push(
                Utf8Path::new(&release.dist_folder)
                    .join("checksums.txt")
                    .to_string(),
            );
        }

        // Settle name clashes with existing assets before uploading anything.
        for asset in &gh_release.assets {
            if !uploads_file_named(&files, &asset.name) {
                continue;
            }
            if !gh.replace_existing_assets {
                bail!(
                    "release {} already has an asset named {}; set replace_existing_assets: true to replace it",
                    latest_tag,
                    asset.name
                );
            }
            info!("replacing existing asset {}", asset.name);
            delete_asset(&ghclient, &gh.owner, &gh.repo, asset.id.0)
                .await
                .with_context(|| format!("error deleting existing asset {}", asset.name))?;
        }

        let uploader = Arc::new(Uploader {
            ghclient,
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::limited(100))
                .build()?,
            owner: gh.owner.clone(),
            repo: gh.repo.clone(),
            release_id: gh_release.id.0,
            ghtoken,
        });
        let num_files = files.len();
        uploader
            .upload_all(files)
            .await
            .context("uploading archives")?;

        info!("uploaded {} files to release {}", num_files, latest_tag);
        Ok(())
    }

    /// Finds the release for a tag, including drafts, which have no tag yet
    /// and so can't be looked up by tag directly.
    async fn find_release(
        ghclient: &octocrab::Octocrab,
        gh: &GithubConfig,
        tag: &str,
    ) -> Result<Option<octocrab::models::repos::Release>> {
        let releases = ghclient
            .repos(&gh.owner, &gh.repo)
            .releases()
            .list()
            .per_page(100)
            .send()
            .await
            .context("error listing releases in github")?;
        Ok(releases.items.into_iter().find(|r| r.tag_name == tag))
    }

    async fn create_release(
        &self,
        ghclient: &octocrab::Octocrab,
        gh: &GithubConfig,
        meta: &TemplateMeta,
    ) -> Result<octocrab::models::repos::Release> {
        let tag = meta.tag.as_str();
        // The first release has no previous tag to diff against.
        let changelog = if meta.previous_tag.is_empty() {
            get_all_git_log().await?
        } else {
            get_changelog(&self.changelog, meta).await?
        };

        // Tags like v1.0.0-rc.1 shouldn't become the repo's latest release.
        let prerelease = meta.is_prerelease;
        let name = match &gh.name {
            Some(name) => templating::render_template(name, meta),
            None => tag.to_string(),
        };

        let created = ghclient
            .repos(&gh.owner, &gh.repo)
            .releases()
            .create(tag)
            .name(&name)
            .body(&changelog)
            .draft(gh.draft)
            .prerelease(prerelease)
            .send()
            .await
            .context("error creating release in github")?;
        info!("release created");
        Ok(created)
    }
}

/// Uploads files to one release. Shared by the upload tasks.
struct Uploader {
    ghclient: octocrab::Octocrab,
    http: Client,
    owner: String,
    repo: String,
    release_id: u64,
    ghtoken: String,
}

impl Uploader {
    /// Uploads all files in parallel and reports every failure.
    async fn upload_all(self: Arc<Self>, files: Vec<String>) -> Result<()> {
        let total = files.len();
        let tasks = files.into_iter().map(|file| {
            let uploader = self.clone();
            tokio::spawn(async move {
                uploader
                    .upload_with_retry(&file)
                    .await
                    .with_context(|| format!("error uploading {}", file))
            })
        });

        let mut failures = Vec::new();
        for result in futures::future::join_all(tasks).await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => {
                    error!("{:?}", err);
                    failures.push(format!("{:?}", err));
                }
                Err(err) => failures.push(format!("upload task panicked: {}", err)),
            }
        }
        if !failures.is_empty() {
            bail!(
                "{} of {} uploads failed:\n{}",
                failures.len(),
                total,
                failures.join("\n")
            );
        }
        Ok(())
    }

    async fn upload_with_retry(&self, file: &str) -> Result<()> {
        let name = Utf8Path::new(file)
            .file_name()
            .with_context(|| format!("no file name in {}", file))?;
        let mut attempt = 1;
        loop {
            let err = match self.upload_file(file, name).await {
                Ok(()) => return Ok(()),
                Err(err) => err,
            };
            if attempt == UPLOAD_ATTEMPTS {
                return Err(err);
            }
            warn!(
                "uploading {} failed (attempt {} of {}), retrying: {:?}",
                name, attempt, UPLOAD_ATTEMPTS, err
            );
            tokio::time::sleep(Duration::from_secs(2 * attempt)).await;
            // A failed upload can leave a broken asset with this name behind,
            // which would make the retry fail.
            self.delete_asset_named(name).await?;
            attempt += 1;
        }
    }

    async fn delete_asset_named(&self, name: &str) -> Result<()> {
        let assets = self
            .ghclient
            .repos(&self.owner, &self.repo)
            .releases()
            .assets(self.release_id)
            .per_page(100)
            .send()
            .await
            .context("error listing release assets")?;
        if let Some(asset) = assets.items.into_iter().find(|a| a.name == name) {
            debug!("deleting leftover asset {}", name);
            delete_asset(&self.ghclient, &self.owner, &self.repo, asset.id.0).await?;
        }
        Ok(())
    }

    async fn upload_file(&self, filepath: &str, name: &str) -> Result<()> {
        let url = format!(
            "https://uploads.github.com/repos/{}/{}/releases/{}/assets?name={}",
            self.owner, self.repo, self.release_id, name
        );
        debug!("uploading to url: {}", url);

        // Stat the file to get the size of the file.
        let meta = fs::metadata(filepath)
            .await
            .context("error getting file metadata")?;
        let size = meta.len();

        let mime_type = match infer::get_from_path(filepath)? {
            Some(mime_type) => mime_type.to_string(),
            None => String::from("application/octet-stream"),
        };

        let f = tokio::fs::File::open(filepath).await?;
        let res = self
            .http
            .post(url)
            .basic_auth(&self.owner, Some(&self.ghtoken))
            .body(file_to_body(f))
            .header("Content-Length", size)
            .header("Content-Type", mime_type)
            .header("Accept", MEDIA_TYPE)
            .send()
            .await
            .context("error uploading file")?;
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

async fn delete_asset(
    ghclient: &octocrab::Octocrab,
    owner: &str,
    repo: &str,
    asset_id: u64,
) -> Result<()> {
    let route = format!("/repos/{}/{}/releases/assets/{}", owner, repo, asset_id);
    let res = ghclient
        ._delete(route, None::<&()>)
        .await
        .context("error deleting asset")?;
    octocrab::map_github_error(res)
        .await
        .context("error deleting asset")?;
    Ok(())
}

fn uploads_file_named(files: &[String], name: &str) -> bool {
    files
        .iter()
        .any(|file| Utf8Path::new(file).file_name() == Some(name))
}

fn file_to_body(file: tokio::fs::File) -> Body {
    let stream = FramedRead::new(file, BytesCodec::new());
    Body::wrap_stream(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uploads_file_named_matches_file_names() {
        let files = vec![
            "dist/app-linux-amd64.zip".to_string(),
            "dist/checksums.txt".to_string(),
        ];
        assert!(uploads_file_named(&files, "checksums.txt"));
        assert!(uploads_file_named(&files, "app-linux-amd64.zip"));
        assert!(!uploads_file_named(&files, "dist"));
        assert!(!uploads_file_named(&files, "app-darwin-arm64.zip"));
    }
}
