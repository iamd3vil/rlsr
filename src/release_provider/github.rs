use crate::config::{Changelog, Github as GithubConfig, Release};
use crate::release_provider::ReleaseProvider;
use crate::utils::{get_all_git_log, get_all_tags, get_changelog};
use crate::{build_template_meta, templating};
use async_trait::async_trait;
use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, Result};
use log::{debug, error, info};
use reqwest::{Body, Client};
use std::sync::Arc;
use tokio::fs;
use tokio_util::codec::{BytesCodec, FramedRead};

const MEDIA_TYPE: &str = "application/vnd.github.v3+json";

#[async_trait]
impl ReleaseProvider for Github {
    async fn publish(
        &self,
        release: &Release,
        all_archives: Vec<String>,
        _image_tags: Vec<String>,
        latest_tag: String,
    ) -> Result<()> {
        self.publish_build(release, all_archives, self.ghtoken.clone(), latest_tag)
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

        let gh_release = match Self::find_release(&ghclient, gh, &latest_tag).await? {
            Some(existing) => {
                info!("release for {} already exists, uploading to it", latest_tag);
                existing
            }
            None => self.create_release(&ghclient, gh, &latest_tag).await?,
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
            let route = format!(
                "/repos/{}/{}/releases/assets/{}",
                gh.owner, gh.repo, asset.id
            );
            let res = ghclient
                ._delete(route, None::<&()>)
                .await
                .context("error deleting existing asset")?;
            octocrab::map_github_error(res)
                .await
                .with_context(|| format!("error deleting existing asset {}", asset.name))?;
        }

        Self::upload_archives(
            files,
            gh_release.id.0,
            gh.owner.clone(),
            gh.repo.clone(),
            ghtoken,
        )
        .await
        .context("uploading archives")?;

        info!("release created");
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
        tag: &str,
    ) -> Result<octocrab::models::repos::Release> {
        let tags = get_all_tags().await?;
        let changelog = if tags.len() == 1 {
            get_all_git_log().await?
        } else {
            get_changelog(&self.changelog).await?
        };

        // Tags like v1.0.0-rc.1 shouldn't become the repo's latest release.
        let prerelease = !crate::parse_version_meta(tag).prerelease.is_empty();
        let name = match &gh.name {
            Some(name) => {
                let meta = build_template_meta(tag.to_string()).await?;
                templating::render_template(name, &meta)
            }
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
        let meta = fs::metadata(&filepath)
            .await
            .context("error getting file metadata")?;
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
