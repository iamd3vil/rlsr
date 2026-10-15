use color_eyre::eyre::{bail, Result};
use log::{debug, info};
use tokio::sync::Mutex;

use crate::config::{Build, Release};
use crate::utils;

const DEFAULT_TOOL: &str = "cargo zigbuild";

// rustup can corrupt a toolchain when two installs run at once, and parallel
// builds would otherwise install their targets at the same time.
static RUSTUP_LOCK: Mutex<()> = Mutex::const_new(());

/// Expands a `type: rust` build into one build per target triple, with the
/// command, artifact path and platform fields filled in.
pub fn expand(build: &Build, release: &Release) -> Result<Vec<Build>> {
    let Some(cfg) = &build.rust else {
        bail!("build '{}' has type rust but no rust config", build.name);
    };
    if cfg.targets.is_empty() {
        bail!("build '{}' must list at least one rust target", build.name);
    }

    let tool = cfg.tool.as_deref().unwrap_or(DEFAULT_TOOL);
    let flags = cfg
        .flags
        .clone()
        .unwrap_or_else(|| vec!["--release".to_string()]);
    let binary = cfg.binary.clone().unwrap_or_else(utils::get_project_name);
    let target_dir = cargo_target_dir(release, build);
    let profile = profile_dir(&flags);
    let flags = flags.join(" ");

    let builds = cfg
        .targets
        .iter()
        .map(|target| {
            let (os, arch, arm) = parse_triple(target);
            let mut expanded = build.clone();

            let cargo = format!("{} --target {} {}", tool, target, flags);
            expanded.command = Some(cargo.trim_end().to_string());

            if expanded.artifact.trim().is_empty() {
                let ext = if os == "windows" { ".exe" } else { "" };
                expanded.artifact =
                    format!("{}/{}/{}/{}{}", target_dir, target, profile, binary, ext);
            }

            expanded.os = Some(os);
            expanded.arch = Some(arch);
            expanded.arm = Some(arm);
            expanded.target = Some(target.clone());
            expanded
        })
        .collect();

    Ok(builds)
}

/// Installs the build's target with rustup if it isn't installed yet. Skipped
/// for cross, which brings its own toolchains.
pub async fn ensure_target(build: &Build, envs: &Option<Vec<String>>) -> Result<()> {
    let tool = build
        .rust
        .as_ref()
        .and_then(|cfg| cfg.tool.as_deref())
        .unwrap_or(DEFAULT_TOOL);
    let Some(target) = build.target.as_deref() else {
        return Ok(());
    };
    if tool.starts_with("cross") {
        return Ok(());
    }

    let _guard = RUSTUP_LOCK.lock().await;
    let output = utils::execute_command("rustup target list --installed", envs).await?;
    if !output.status.success() {
        bail!(
            "rustup target list failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    if is_installed(&String::from_utf8_lossy(&output.stdout), target) {
        debug!("rust target {} is already installed", target);
        return Ok(());
    }

    info!("installing rust target {}", target);
    let output = utils::execute_command(&format!("rustup target add {}", target), envs).await?;
    if !output.status.success() {
        bail!(
            "rustup target add {} failed: {}",
            target,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}

fn is_installed(installed: &str, target: &str) -> bool {
    installed.lines().any(|line| line.trim() == target)
}

/// Finds the cargo target dir the build will use. Build envs are applied after
/// release envs, so the last CARGO_TARGET_DIR wins.
fn cargo_target_dir(release: &Release, build: &Build) -> String {
    release
        .env
        .iter()
        .chain(build.env.iter())
        .flatten()
        .filter_map(|env| env.strip_prefix("CARGO_TARGET_DIR="))
        .next_back()
        .map(str::to_string)
        .or_else(|| std::env::var("CARGO_TARGET_DIR").ok())
        .unwrap_or_else(|| "target".to_string())
        .trim_end_matches('/')
        .to_string()
}

/// Returns the directory under `<target>/` that cargo writes to for these flags.
fn profile_dir(flags: &[String]) -> String {
    let args: Vec<&str> = flags.iter().flat_map(|f| f.split_whitespace()).collect();
    let mut profile = "debug";
    for (i, arg) in args.iter().enumerate() {
        if *arg == "--release" || *arg == "-r" {
            profile = "release";
        } else if let Some(name) = arg.strip_prefix("--profile=") {
            profile = name;
        } else if *arg == "--profile" {
            if let Some(name) = args.get(i + 1) {
                profile = name;
            }
        }
    }
    // Cargo writes the dev profile to target/debug.
    if profile == "dev" { "debug" } else { profile }.to_string()
}

const OSES: [&str; 11] = [
    "linux",
    "windows",
    "darwin",
    "freebsd",
    "netbsd",
    "openbsd",
    "dragonfly",
    "illumos",
    "solaris",
    "ios",
    "wasi",
];

/// Maps a Rust target triple to Go-style os, arch and arm version, so archive
/// name templates work the same across build types.
fn parse_triple(triple: &str) -> (String, String, String) {
    let mut parts = triple.split('-');
    let arch_part = parts.next().unwrap_or_default();
    let rest: Vec<&str> = parts.collect();

    let (arch, arm) = match arch_part {
        "x86_64" => ("amd64".to_string(), String::new()),
        "aarch64" => ("arm64".to_string(), String::new()),
        "i586" | "i686" => ("386".to_string(), String::new()),
        "riscv64gc" => ("riscv64".to_string(), String::new()),
        "powerpc64le" => ("ppc64le".to_string(), String::new()),
        "powerpc64" => ("ppc64".to_string(), String::new()),
        "loongarch64" => ("loong64".to_string(), String::new()),
        other => match other.strip_prefix("armv") {
            Some(version) => (
                "arm".to_string(),
                version.chars().take_while(char::is_ascii_digit).collect(),
            ),
            None => (other.to_string(), String::new()),
        },
    };

    // Android triples also contain "linux", so check for it first.
    let os = if rest.contains(&"android") {
        "android"
    } else {
        rest.iter()
            .copied()
            .find(|part| OSES.contains(part))
            .unwrap_or_default()
    };

    (os.to_string(), arch, arm)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ArchiveFormat, BuildType, ReleaseTargets, RustConfig};

    fn release(env: Option<Vec<String>>) -> Release {
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
            env,
            builds_sequential: false,
            additional_files: None,
            hooks: None,
        }
    }

    fn rust_build(rust: RustConfig) -> Build {
        Build {
            build_type: BuildType::Rust,
            command: None,
            buildx: None,
            rust: Some(rust),
            go: None,
            artifact: String::new(),
            bin_name: None,
            archive_name: "app-{{ meta.os }}-{{ meta.arch }}".to_string(),
            name: "Rust".to_string(),
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
            archive_format: ArchiveFormat::default(),
        }
    }

    fn targets(targets: &[&str]) -> RustConfig {
        RustConfig {
            targets: targets.iter().map(|t| t.to_string()).collect(),
            binary: Some("app".to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn test_expand_defaults() {
        let build = rust_build(targets(&[
            "x86_64-unknown-linux-musl",
            "x86_64-pc-windows-gnu",
        ]));
        let builds = expand(&build, &release(None)).unwrap();

        assert_eq!(builds.len(), 2);
        assert_eq!(
            builds[0].command.as_deref(),
            Some("cargo zigbuild --target x86_64-unknown-linux-musl --release")
        );
        assert_eq!(
            builds[0].artifact,
            "target/x86_64-unknown-linux-musl/release/app"
        );
        assert_eq!(builds[0].os.as_deref(), Some("linux"));
        assert_eq!(builds[0].arch.as_deref(), Some("amd64"));
        assert_eq!(
            builds[0].target.as_deref(),
            Some("x86_64-unknown-linux-musl")
        );
        assert_eq!(
            builds[1].artifact,
            "target/x86_64-pc-windows-gnu/release/app.exe"
        );
        assert_eq!(builds[1].os.as_deref(), Some("windows"));
    }

    #[test]
    fn test_expand_with_tool_and_flags() {
        let mut cfg = targets(&["aarch64-unknown-linux-musl"]);
        cfg.tool = Some("cross build".to_string());
        cfg.flags = Some(vec!["--profile dist".to_string(), "--locked".to_string()]);
        let builds = expand(&rust_build(cfg), &release(None)).unwrap();

        assert_eq!(
            builds[0].command.as_deref(),
            Some("cross build --target aarch64-unknown-linux-musl --profile dist --locked")
        );
        assert_eq!(
            builds[0].artifact,
            "target/aarch64-unknown-linux-musl/dist/app"
        );
    }

    #[test]
    fn test_expand_keeps_explicit_artifact() {
        let mut build = rust_build(targets(&["aarch64-apple-darwin"]));
        build.artifact = "out/{{ meta.target }}/app".to_string();
        let builds = expand(&build, &release(None)).unwrap();
        assert_eq!(builds[0].artifact, "out/{{ meta.target }}/app");
    }

    #[test]
    fn test_expand_requires_targets() {
        let build = rust_build(targets(&[]));
        assert!(expand(&build, &release(None)).is_err());
    }

    #[test]
    fn test_cargo_target_dir_prefers_build_env() {
        let mut build = rust_build(targets(&["x86_64-unknown-linux-gnu"]));
        build.env = Some(vec![
            "CARGO_TARGET_DIR=target/{{ meta.target }}/".to_string()
        ]);
        let release = release(Some(vec!["CARGO_TARGET_DIR=release-target".to_string()]));
        assert_eq!(
            cargo_target_dir(&release, &build),
            "target/{{ meta.target }}"
        );
    }

    #[test]
    fn test_is_installed() {
        let installed = "x86_64-unknown-linux-gnu\nx86_64-unknown-linux-musl\n";
        assert!(is_installed(installed, "x86_64-unknown-linux-musl"));
        assert!(!is_installed(installed, "x86_64-unknown-linux"));
        assert!(!is_installed(installed, "aarch64-apple-darwin"));
    }

    #[test]
    fn test_profile_dir() {
        let flags = |f: &[&str]| f.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(profile_dir(&flags(&[])), "debug");
        assert_eq!(profile_dir(&flags(&["--release"])), "release");
        assert_eq!(profile_dir(&flags(&["--profile=dev"])), "debug");
        assert_eq!(profile_dir(&flags(&["--profile", "dist"])), "dist");
    }

    #[test]
    fn test_parse_triple() {
        let cases = [
            ("x86_64-unknown-linux-musl", ("linux", "amd64", "")),
            ("aarch64-apple-darwin", ("darwin", "arm64", "")),
            ("i686-pc-windows-msvc", ("windows", "386", "")),
            ("armv7-unknown-linux-gnueabihf", ("linux", "arm", "7")),
            ("aarch64-linux-android", ("android", "arm64", "")),
            ("riscv64gc-unknown-linux-gnu", ("linux", "riscv64", "")),
            ("x86_64-unknown-freebsd", ("freebsd", "amd64", "")),
        ];
        for (triple, (os, arch, arm)) in cases {
            assert_eq!(
                parse_triple(triple),
                (os.to_string(), arch.to_string(), arm.to_string()),
                "{}",
                triple
            );
        }
    }
}
