use crate::config::Build;
use crate::templating::{self, TemplateContext};
use crate::utils;
use color_eyre::eyre::{bail, eyre, Context, Result};
use std::collections::BTreeMap;

#[derive(Debug)]
pub(crate) struct BuildxCommand {
    pub(crate) command: String,
    pub(crate) tags: Vec<String>,
    pub(crate) builder: Option<String>,
}

fn render_optional_string<T: TemplateContext>(value: Option<&String>, meta: &T) -> Option<String> {
    value.map(|val| templating::render_template(val, meta))
}

fn render_list<T: TemplateContext>(values: Option<&Vec<String>>, meta: &T) -> Vec<String> {
    values
        .map(|items| {
            items
                .iter()
                .map(|item| templating::render_template(item, meta))
                .collect::<Vec<String>>()
        })
        .unwrap_or_default()
}

fn render_map<T: TemplateContext>(
    values: Option<&BTreeMap<String, String>>,
    meta: &T,
) -> BTreeMap<String, String> {
    values
        .map(|items| {
            items
                .iter()
                .map(|(key, value)| {
                    (
                        templating::render_template(key, meta),
                        templating::render_template(value, meta),
                    )
                })
                .collect::<BTreeMap<String, String>>()
        })
        .unwrap_or_default()
}

pub(crate) fn build_buildx_command<T: TemplateContext>(
    build: &Build,
    meta: &T,
    build_name: &str,
) -> Result<BuildxCommand> {
    // Render buildx config values with templates before assembling CLI args.
    let buildx = build
        .buildx
        .as_ref()
        .ok_or_else(|| eyre!("missing buildx config for build '{}'", build_name))?;

    let context =
        render_optional_string(buildx.context.as_ref(), meta).unwrap_or_else(|| ".".to_string());
    let dockerfile = render_optional_string(buildx.dockerfile.as_ref(), meta)
        .unwrap_or_else(|| "Dockerfile".to_string());
    let tags = render_list(buildx.tags.as_ref(), meta);
    let outputs = render_list(buildx.outputs.as_ref(), meta);
    let load = buildx.load.unwrap_or(false);

    // Buildx forbids combining --load with explicit outputs.
    if load && !outputs.is_empty() {
        bail!(
            "buildx build '{}' cannot set both load and outputs",
            build_name
        );
    }

    // Require tags for --load so the resulting image is addressable.
    if load && tags.is_empty() {
        bail!(
            "buildx build '{}' must set tags when load is true",
            build_name
        );
    }

    let builder = render_optional_string(buildx.builder.as_ref(), meta);
    let platforms = render_list(buildx.platforms.as_ref(), meta);
    let build_args = render_map(buildx.build_args.as_ref(), meta);
    let labels = render_map(buildx.labels.as_ref(), meta);
    let cache_from = render_list(buildx.cache_from.as_ref(), meta);
    let cache_to = render_list(buildx.cache_to.as_ref(), meta);
    let target = render_optional_string(buildx.target.as_ref(), meta);
    let secrets = render_list(buildx.secrets.as_ref(), meta);
    let ssh = render_list(buildx.ssh.as_ref(), meta);
    let annotations = render_map(buildx.annotations.as_ref(), meta);

    let mut args = Vec::new();
    args.push("docker".to_string());
    args.push("buildx".to_string());
    args.push("build".to_string());

    if let Some(builder_name) = builder.as_ref() {
        args.push("--builder".to_string());
        args.push(builder_name.clone());
    }

    args.push("--file".to_string());
    args.push(dockerfile);

    if !platforms.is_empty() {
        args.push("--platform".to_string());
        args.push(platforms.join(","));
    }

    for tag in &tags {
        args.push("--tag".to_string());
        args.push(tag.clone());
    }

    if load {
        args.push("--load".to_string());
    }

    for (key, value) in build_args {
        args.push("--build-arg".to_string());
        args.push(format!("{}={}", key, value));
    }

    for (key, value) in labels {
        args.push("--label".to_string());
        args.push(format!("{}={}", key, value));
    }

    for item in cache_from {
        args.push("--cache-from".to_string());
        args.push(item);
    }

    for item in cache_to {
        args.push("--cache-to".to_string());
        args.push(item);
    }

    if let Some(target) = target {
        args.push("--target".to_string());
        args.push(target);
    }

    for output in outputs {
        args.push("--output".to_string());
        args.push(output);
    }

    if let Some(provenance) = buildx.provenance {
        args.push(format!("--provenance={}", provenance));
    }

    if let Some(sbom) = buildx.sbom {
        args.push(format!("--sbom={}", sbom));
    }

    for secret in secrets {
        args.push("--secret".to_string());
        args.push(secret);
    }

    for item in ssh {
        args.push("--ssh".to_string());
        args.push(item);
    }

    for (key, value) in annotations {
        args.push("--annotation".to_string());
        args.push(format!("{}={}", key, value));
    }

    args.push(context);

    Ok(BuildxCommand {
        command: args.join(" "),
        tags,
        builder,
    })
}

pub(crate) fn buildx_builder_exists_error(stdout: &str, stderr: &str) -> bool {
    let combined = format!("{} {}", stdout, stderr).to_lowercase();
    combined.contains("already exists")
        || combined.contains("existing builder")
        || combined.contains("existing instance")
        || (combined.contains("exists") && combined.contains("builder"))
}

pub(crate) async fn ensure_buildx_builder(
    builder: &str,
    envs: &Option<Vec<String>>,
    build_name: &str,
) -> Result<()> {
    let create_cmd = format!("docker buildx create --name {} --use", builder);
    let output = utils::execute_command(&create_cmd, envs)
        .await
        .with_context(|| {
            format!(
                "failed to run buildx create for build '{}' (builder '{}')",
                build_name, builder
            )
        })?;

    if output.status.success() {
        return Ok(());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    // If the builder already exists, switch to it instead of failing.
    if buildx_builder_exists_error(&stdout, &stderr) {
        let use_cmd = format!("docker buildx use {}", builder);

        let use_output = utils::execute_command(&use_cmd, envs)
            .await
            .with_context(|| {
                format!(
                    "failed to run buildx use for build '{}' (builder '{}')",
                    build_name, builder
                )
            })?;

        if use_output.status.success() {
            return Ok(());
        }

        let use_stdout = String::from_utf8_lossy(&use_output.stdout);
        let use_stderr = String::from_utf8_lossy(&use_output.stderr);
        bail!(
            "buildx builder '{}' for build '{}' failed to activate: stdout: {} stderr: {}",
            builder,
            build_name,
            use_stdout.trim(),
            use_stderr.trim()
        );
    }

    bail!(
        "buildx builder '{}' for build '{}' failed to create: stdout: {} stderr: {}",
        builder,
        build_name,
        stdout.trim(),
        stderr.trim()
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BuildType, BuildxConfig};
    use serde::Serialize;
    use std::collections::HashMap;

    #[derive(Debug, Serialize)]
    struct TestMeta {
        build_name: String,
        tag: String,
        version: String,
        env: HashMap<String, String>,
        date: String,
        timestamp: String,
        now: String,
    }

    impl TemplateContext for TestMeta {
        fn env(&self) -> &HashMap<String, String> {
            &self.env
        }

        fn date(&self) -> &str {
            &self.date
        }

        fn timestamp(&self) -> &str {
            &self.timestamp
        }

        fn now(&self) -> &str {
            &self.now
        }
    }

    fn test_meta() -> TestMeta {
        TestMeta {
            build_name: "Linux build".to_string(),
            tag: "v1.2.3".to_string(),
            version: "1.2.3".to_string(),
            env: HashMap::new(),
            date: "2025-01-25".to_string(),
            timestamp: "1706180400".to_string(),
            now: "2025-01-25T10:30:00Z".to_string(),
        }
    }

    fn base_build() -> Build {
        Build {
            build_type: BuildType::Buildx,
            command: None,
            buildx: None,
            artifact: "./bin/rlsr".to_string(),
            bin_name: None,
            archive_name: "rlsr.tar.gz".to_string(),
            name: "Linux build".to_string(),
            os: None,
            arch: None,
            arm: None,
            target: None,
            matrix: None,
            env: None,
            prehook: None,
            posthook: None,
            no_archive: None,
            additional_files: None,
        }
    }

    fn buildx_build(buildx: BuildxConfig) -> Build {
        let mut build = base_build();
        build.buildx = Some(buildx);
        build
    }

    #[test]
    fn test_buildx_command_renders_templates_and_defaults() {
        let mut build_args = BTreeMap::new();
        build_args.insert("RUST_VERSION".to_string(), "{{ meta.version }}".to_string());

        let mut labels = BTreeMap::new();
        labels.insert(
            "org.opencontainers.image.version".to_string(),
            "{{ meta.tag }}".to_string(),
        );

        let mut annotations = BTreeMap::new();
        annotations.insert(
            "org.opencontainers.image.description".to_string(),
            "{{ meta.build_name }}".to_string(),
        );

        let buildx = BuildxConfig {
            tags: Some(vec!["example/rlsr:{{ meta.tag }}".to_string()]),
            platforms: Some(vec!["linux/amd64".to_string(), "linux/arm64".to_string()]),
            builder: Some("rlsr-builder".to_string()),
            load: Some(true),
            build_args: Some(build_args),
            labels: Some(labels),
            cache_from: Some(vec!["type=registry,ref=cache".to_string()]),
            cache_to: Some(vec!["type=inline".to_string()]),
            target: Some("release".to_string()),
            provenance: Some(true),
            sbom: Some(false),
            secrets: Some(vec!["id=token,src=./token".to_string()]),
            ssh: Some(vec!["default".to_string()]),
            annotations: Some(annotations),
            ..BuildxConfig::default()
        };

        let build = buildx_build(buildx);
        let meta = test_meta();
        let command = build_buildx_command(&build, &meta, &build.name).unwrap();

        assert_eq!(command.tags, vec!["example/rlsr:v1.2.3".to_string()]);
        assert_eq!(
            command.command,
            "docker buildx build --builder rlsr-builder --file Dockerfile --platform linux/amd64,linux/arm64 --tag example/rlsr:v1.2.3 --load --build-arg RUST_VERSION=1.2.3 --label org.opencontainers.image.version=v1.2.3 --cache-from type=registry,ref=cache --cache-to type=inline --target release --provenance=true --sbom=false --secret id=token,src=./token --ssh default --annotation org.opencontainers.image.description=Linux build ."
        );
    }

    #[test]
    fn test_buildx_validation_rejects_load_with_outputs() {
        let buildx = BuildxConfig {
            tags: Some(vec!["example/rlsr:{{ meta.tag }}".to_string()]),
            load: Some(true),
            outputs: Some(vec!["type=registry".to_string()]),
            ..BuildxConfig::default()
        };

        let build = buildx_build(buildx);
        let meta = test_meta();
        let err = build_buildx_command(&build, &meta, &build.name).unwrap_err();

        assert!(err.to_string().contains("cannot set both load and outputs"));
    }

    #[test]
    fn test_buildx_validation_requires_tags_when_load_true() {
        let buildx = BuildxConfig {
            load: Some(true),
            ..BuildxConfig::default()
        };

        let build = buildx_build(buildx);
        let meta = test_meta();
        let err = build_buildx_command(&build, &meta, &build.name).unwrap_err();

        assert!(err.to_string().contains("must set tags when load is true"));
    }

    #[test]
    fn test_buildx_builder_exists_error_detection() {
        assert!(buildx_builder_exists_error(
            "error: builder rlsr-builder already exists",
            ""
        ));
        assert!(buildx_builder_exists_error(
            "",
            "Error: existing builder instance rlsr-builder"
        ));
        assert!(buildx_builder_exists_error(
            "",
            "ERROR: existing instance for \"rlsr-multiarch\" but no append mode"
        ));
        assert!(!buildx_builder_exists_error(
            "Error: failed to connect",
            "timeout reached"
        ));
    }
}
