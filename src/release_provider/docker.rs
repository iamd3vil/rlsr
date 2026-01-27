use crate::config::{Docker as DockerConfig, Release};
use crate::release_provider::ReleaseProvider;
use crate::{build_template_meta, templating, TemplateMeta};
use async_trait::async_trait;
use color_eyre::eyre::{bail, Context, Result};
use log::info;
use tokio::process::Command;

pub struct Docker {}

impl Docker {
    pub fn new() -> Self {
        Docker {}
    }

    async fn build_image(
        docker: &DockerConfig,
        latest_tag: &str,
        template_meta: &TemplateMeta,
    ) -> Result<String> {
        let dockerfile = match docker.dockerfile.as_deref() {
            Some(dockerfile) => dockerfile,
            None => {
                bail!("docker target requires dockerfile when building images")
            }
        };
        let image_template = match docker.image.as_deref() {
            Some(image) => image,
            None => {
                bail!("docker target requires image when building images")
            }
        };
        let context = match docker.context.as_deref() {
            Some(context) => context,
            None => {
                bail!("docker target requires context when building images")
            }
        };
        let image = render_docker_image_template(image_template, latest_tag, template_meta);
        let mut cmd = Command::new("docker");
        let args: Vec<&str> = vec!["build", context, "-t", &image, "-f", dockerfile];
        cmd.args(&args);

        info!(
            "executing docker build with command: docker {}",
            args.join(" ")
        );

        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;
        if !output.status.success() {
            bail!(
                "error executing docker build: {}",
                String::from_utf8_lossy(&output.stdout).to_string()
            );
        }

        Ok(image)
    }

    async fn push_image(image: &str) -> Result<()> {
        let mut cmd = Command::new("docker");
        let args: Vec<&str> = vec!["push", image];
        cmd.args(&args);

        info!(
            "executing docker push with command: docker {}",
            args.join(" ")
        );

        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;
        if !output.status.success() {
            bail!(
                "error executing docker push: {}",
                String::from_utf8_lossy(&output.stdout).to_string()
            );
        }

        Ok(())
    }
}

fn render_docker_image_template(image: &str, latest_tag: &str, meta: &TemplateMeta) -> String {
    let rendered = templating::render_template(image, meta);
    normalize_image_tag(&rendered, latest_tag)
}

fn normalize_image_tag(image: &str, latest_tag: &str) -> String {
    // Ensure images without tag/digest are still publishable by appending the release tag.
    if image_has_tag_or_digest(image) {
        image.to_string()
    } else {
        format!("{}:{}", image, latest_tag)
    }
}

fn image_has_tag_or_digest(image: &str) -> bool {
    let name = image.rsplit('/').next().unwrap_or(image);
    name.contains(':') || name.contains('@')
}

#[async_trait]
impl ReleaseProvider for Docker {
    async fn publish(
        &self,
        release: &Release,
        _all_archives: Vec<String>,
        image_tags: Vec<String>,
        latest_tag: String,
    ) -> Result<()> {
        let docker = match release.targets.docker.as_ref() {
            Some(docker) => docker,
            None => {
                bail!("docker config can't be empty")
            }
        };
        let template_meta = build_template_meta(latest_tag.to_string()).await?;
        let should_push = docker.push;

        // Prefer explicit build configuration, then explicit images, then buildx tags.
        if docker.dockerfile.is_some() || docker.image.is_some() || docker.context.is_some() {
            let image = Self::build_image(docker, &latest_tag, &template_meta)
                .await
                .wrap_err_with(|| "error building docker image")?;
            if should_push {
                Self::push_image(&image).await?;
            } else {
                info!("docker push disabled; skipping push for image '{}'", image);
            }
            return Ok(());
        }

        if let Some(images) = docker.images.as_ref() {
            if should_push {
                let rendered_images = images
                    .iter()
                    .map(|image| render_docker_image_template(image, &latest_tag, &template_meta))
                    .collect::<Vec<_>>();
                for image in rendered_images {
                    Self::push_image(&image).await?;
                }
            } else {
                info!("docker push disabled; skipping push for configured images");
            }
            return Ok(());
        }

        if !image_tags.is_empty() {
            if should_push {
                for image in image_tags
                    .iter()
                    .map(|image| normalize_image_tag(image, &latest_tag))
                {
                    Self::push_image(&image).await?;
                }
            } else {
                info!("docker push disabled; skipping push for buildx images");
            }
            return Ok(());
        }

        bail!("docker target requires dockerfile/image/context, images, or buildx tags to publish")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn test_template_meta() -> TemplateMeta {
        TemplateMeta {
            tag: "v1.2.3".to_string(),
            version: "1.2.3".to_string(),
            major: 1,
            minor: 2,
            patch: 3,
            prerelease: String::new(),
            commit: "abcdef1234567890".to_string(),
            short_commit: "abcdef1".to_string(),
            branch: "main".to_string(),
            previous_tag: "v1.2.2".to_string(),
            project_name: "rlsr".to_string(),
            release_url: "https://github.com/owner/repo/releases/tag/v1.2.3".to_string(),
            is_snapshot: false,
            is_prerelease: false,
            is_dirty: false,
            env: HashMap::new(),
            date: "2025-01-25".to_string(),
            timestamp: "1706180400".to_string(),
            now: "2025-01-25T10:30:00Z".to_string(),
        }
    }

    #[test]
    fn test_render_docker_image_with_tag_template() {
        let meta = test_template_meta();
        let docker = DockerConfig {
            dockerfile: Some("Dockerfile".to_string()),
            image: Some("example/rlsr:{{ meta.tag }}".to_string()),
            context: Some(".".to_string()),
            images: None,
            push: true,
        };

        let rendered = render_docker_image_template(
            docker.image.as_deref().expect("image should exist"),
            "v1.2.3",
            &meta,
        );
        assert_eq!(rendered, "example/rlsr:v1.2.3");
    }

    #[test]
    fn test_render_docker_image_appends_tag_when_missing() {
        let meta = test_template_meta();
        let docker = DockerConfig {
            dockerfile: Some("Dockerfile".to_string()),
            image: Some("example/rlsr".to_string()),
            context: Some(".".to_string()),
            images: None,
            push: true,
        };

        let rendered = render_docker_image_template(
            docker.image.as_deref().expect("image should exist"),
            "v1.2.3",
            &meta,
        );
        assert_eq!(rendered, "example/rlsr:v1.2.3");
    }
}
