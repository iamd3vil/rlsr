use camino::Utf8Path;
use color_eyre::eyre::{bail, Result};

use crate::config::{Build, Release};
use crate::utils;

/// Expands a `type: go` build into one build per os/arch target, with the
/// command, artifact path, GOOS/GOARCH envs and platform fields filled in.
pub fn expand(build: &Build, release: &Release) -> Result<Vec<Build>> {
    let Some(cfg) = &build.go else {
        bail!("build '{}' has type go but no go config", build.name);
    };
    if cfg.targets.is_empty() {
        bail!("build '{}' must list at least one go target", build.name);
    }

    let main = cfg.main.as_deref().unwrap_or(".");
    let binary = cfg
        .binary
        .clone()
        .or_else(utils::get_go_module_name)
        .unwrap_or_else(utils::get_project_name);

    let mut go_build = vec!["go build".to_string()];
    go_build.extend(cfg.flags.iter().flatten().cloned());
    if let Some(ldflags) = cfg.ldflags.as_ref().filter(|l| !l.is_empty()) {
        // Double quotes work in both sh and cmd.
        go_build.push(format!("-ldflags \"{}\"", ldflags.join(" ")));
    }
    let go_build = go_build.join(" ");

    let mut builds = Vec::with_capacity(cfg.targets.len());
    for target in &cfg.targets {
        let (os, arch, variant) = parse_target(target)?;
        let mut expanded = build.clone();

        if expanded.artifact.trim().is_empty() {
            let ext = if os == "windows" { ".exe" } else { "" };
            expanded.artifact = Utf8Path::new(&release.dist_folder)
                .join("go")
                .join(target.replace('/', "_"))
                .join(format!("{}{}", binary, ext))
                .to_string();
        }
        expanded.command = Some(format!(
            "{} -o \"{}\" {}",
            go_build, expanded.artifact, main
        ));

        let envs = expanded.env.get_or_insert_with(Vec::new);
        envs.push(format!("GOOS={}", os));
        envs.push(format!("GOARCH={}", arch));
        if !variant.is_empty() {
            let key = match arch {
                "arm" => "GOARM",
                "arm64" => "GOARM64",
                "amd64" => "GOAMD64",
                "386" => "GO386",
                _ => bail!(
                    "go target '{}' has a variant, which is only supported for arm, arm64, amd64 and 386",
                    target
                ),
            };
            envs.push(format!("{}={}", key, variant));
        }

        expanded.os = Some(os.to_string());
        expanded.arch = Some(arch.to_string());
        expanded.arm = Some(if arch == "arm" { variant } else { "" }.to_string());
        expanded.target = Some(target.clone());
        builds.push(expanded);
    }

    Ok(builds)
}

fn parse_target(target: &str) -> Result<(&str, &str, &str)> {
    match target.split('/').collect::<Vec<_>>().as_slice() {
        [os, arch] => Ok((os, arch, "")),
        [os, arch, variant] => Ok((os, arch, variant)),
        _ => bail!(
            "invalid go target '{}': expected os/arch or os/arch/variant",
            target
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArchiveFormat, BuildType, GoConfig, ReleaseTargets};

    fn release() -> Release {
        Release {
            name: "Test".to_string(),
            dist_folder: "dist".to_string(),
            builds: vec![],
            targets: ReleaseTargets {
                github: None,
                gitlab: None,
                docker: None,
            },
            checksum: None,
            env: None,
            builds_sequential: false,
            additional_files: None,
            hooks: None,
        }
    }

    fn go_build(go: GoConfig) -> Build {
        Build {
            build_type: BuildType::Go,
            command: None,
            buildx: None,
            rust: None,
            go: Some(go),
            artifact: String::new(),
            bin_name: None,
            archive_name: "app-{{ meta.os }}-{{ meta.arch }}".to_string(),
            name: "Go".to_string(),
            os: None,
            arch: None,
            arm: None,
            target: None,
            matrix: None,
            env: Some(vec!["CGO_ENABLED=0".to_string()]),
            prehook: None,
            posthook: None,
            no_archive: None,
            additional_files: None,
            archive_format: ArchiveFormat::default(),
        }
    }

    fn targets(targets: &[&str]) -> GoConfig {
        GoConfig {
            targets: targets.iter().map(|t| t.to_string()).collect(),
            binary: Some("app".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_expand_defaults() {
        let builds = expand(
            &go_build(targets(&["linux/amd64", "windows/arm64"])),
            &release(),
        )
        .unwrap();

        assert_eq!(builds.len(), 2);
        assert_eq!(builds[0].artifact, "dist/go/linux_amd64/app");
        assert_eq!(
            builds[0].command.as_deref(),
            Some("go build -o \"dist/go/linux_amd64/app\" .")
        );
        assert_eq!(
            builds[0].env.as_deref(),
            Some(
                &[
                    "CGO_ENABLED=0".to_string(),
                    "GOOS=linux".to_string(),
                    "GOARCH=amd64".to_string()
                ][..]
            )
        );
        assert_eq!(builds[0].os.as_deref(), Some("linux"));
        assert_eq!(builds[0].arch.as_deref(), Some("amd64"));
        assert_eq!(builds[0].target.as_deref(), Some("linux/amd64"));
        assert_eq!(builds[1].artifact, "dist/go/windows_arm64/app.exe");
    }

    #[test]
    fn test_expand_flags_ldflags_and_main() {
        let mut cfg = targets(&["darwin/arm64"]);
        cfg.main = Some("./cmd/app".to_string());
        cfg.flags = Some(vec!["-trimpath".to_string()]);
        cfg.ldflags = Some(vec![
            "-s -w".to_string(),
            "-X main.version={{ meta.version }}".to_string(),
        ]);
        let builds = expand(&go_build(cfg), &release()).unwrap();

        assert_eq!(
            builds[0].command.as_deref(),
            Some(
                "go build -trimpath -ldflags \"-s -w -X main.version={{ meta.version }}\" \
                 -o \"dist/go/darwin_arm64/app\" ./cmd/app"
            )
        );
    }

    #[test]
    fn test_expand_variants() {
        let builds = expand(
            &go_build(targets(&["linux/arm/7", "linux/amd64/v3"])),
            &release(),
        )
        .unwrap();

        assert_eq!(builds[0].arm.as_deref(), Some("7"));
        assert!(builds[0]
            .env
            .as_ref()
            .unwrap()
            .contains(&"GOARM=7".to_string()));
        assert_eq!(builds[0].artifact, "dist/go/linux_arm_7/app");
        assert_eq!(builds[1].arm.as_deref(), Some(""));
        assert!(builds[1]
            .env
            .as_ref()
            .unwrap()
            .contains(&"GOAMD64=v3".to_string()));
    }

    #[test]
    fn test_expand_rejects_bad_targets() {
        let release = release();
        assert!(expand(&go_build(targets(&["linux"])), &release).is_err());
        assert!(expand(&go_build(targets(&["linux/riscv64/x"])), &release).is_err());
        assert!(expand(&go_build(targets(&[])), &release).is_err());
    }
}
