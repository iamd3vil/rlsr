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

    async fn build_image(release: &Release, latest_tag: &str) -> Result<String> {
        let docker = match &release.targets.docker {
            Some(docker) => docker,
            None => {
                bail!("missing docker config in config");
            }
        };
        let template_meta = build_template_meta(latest_tag.to_string()).await?;
        let image = render_docker_image(docker, latest_tag, &template_meta);
        let mut cmd = Command::new("docker");
        let args: Vec<&str> = vec![
            "build",
            &docker.context,
            "-t",
            &image,
            "-f",
            &docker.dockerfile,
        ];
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

fn render_docker_image(docker: &DockerConfig, latest_tag: &str, meta: &TemplateMeta) -> String {
    let rendered = templating::render_template(&docker.image, meta);
    if image_has_tag_or_digest(&rendered) {
        rendered
    } else {
        format!("{}:{}", rendered, latest_tag)
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
        latest_tag: String,
    ) -> Result<()> {
        if release.targets.docker.is_none() {
            bail!("docker config can't be empty")
        }
        let image = Self::build_image(release, &latest_tag)
            .await
            .wrap_err_with(|| "error building docker image")?;

        Self::push_image(&image).await?;

        Ok(())
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
            dockerfile: "Dockerfile".to_string(),
            image: "example/rlsr:{{ meta.tag }}".to_string(),
            context: ".".to_string(),
        };

        let rendered = render_docker_image(&docker, "v1.2.3", &meta);
        assert_eq!(rendered, "example/rlsr:v1.2.3");
    }

    #[test]
    fn test_render_docker_image_appends_tag_when_missing() {
        let meta = test_template_meta();
        let docker = DockerConfig {
            dockerfile: "Dockerfile".to_string(),
            image: "example/rlsr".to_string(),
            context: ".".to_string(),
        };

        let rendered = render_docker_image(&docker, "v1.2.3", &meta);
        assert_eq!(rendered, "example/rlsr:v1.2.3");
    }
}
