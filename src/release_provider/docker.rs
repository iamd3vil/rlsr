use crate::config::Release;
use crate::release_provider::ReleaseProvider;
use async_trait::async_trait;
use color_eyre::eyre::{bail, Context, Result};
use log::info;
use std::sync::Arc;
use tokio::{process::Command, sync::Mutex};

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
        let mut cmd = Command::new("docker");
        let image = format!("{}:{}", &docker.image, latest_tag);
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

#[allow(clippy::needless_arbitrary_self_type)]
#[async_trait]
impl ReleaseProvider for Docker {
    async fn publish(
        self: &Self,
        release: &Release,
        _all_archives: Arc<Mutex<Vec<String>>>,
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
