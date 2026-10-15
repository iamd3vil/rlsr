//! Split and merge: `--split` runs (for example one per CI runner) build and
//! package their builds and record what they made in a manifest in the dist
//! folder. A `--merge` run reads every manifest in the dist folder, then
//! checksums and publishes everything together.

use camino::Utf8Path;
use color_eyre::eyre::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::config::Release;

const PREFIX: &str = "rlsr-split-";

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
struct Manifest {
    release: String,
    archives: Vec<String>,
    image_tags: Vec<String>,
}

/// Records what a split run produced for a release.
pub async fn write_manifest(
    release: &Release,
    targets: &[String],
    archives: &[String],
    image_tags: &[String],
) -> Result<String> {
    let manifest = Manifest {
        release: release.name.clone(),
        archives: archives.to_vec(),
        image_tags: image_tags.to_vec(),
    };
    let path = Utf8Path::new(&release.dist_folder).join(manifest_file_name(&release.name, targets));
    fs::write(&path, serde_yaml::to_string(&manifest)?)
        .await
        .with_context(|| format!("error writing split manifest {}", path))?;
    Ok(path.to_string())
}

/// Collects the archives and image tags from every split manifest for the
/// release in its dist folder.
pub async fn read_manifests(release: &Release) -> Result<(Vec<String>, Vec<String>)> {
    let mut paths = Vec::new();
    let mut entries = fs::read_dir(&release.dist_folder)
        .await
        .with_context(|| format!("error reading dist folder {}", release.dist_folder))?;
    while let Some(entry) = entries.next_entry().await? {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with(PREFIX) && name.ends_with(".yml") {
            paths.push(entry.path());
        }
    }
    paths.sort();

    let mut merged = Manifest::default();
    for path in &paths {
        let contents = fs::read_to_string(path).await?;
        let manifest: Manifest = serde_yaml::from_str(&contents)
            .with_context(|| format!("error parsing split manifest {}", path.display()))?;
        // Releases can share a dist folder.
        if manifest.release != release.name {
            continue;
        }
        merge_into(&mut merged, manifest);
    }

    if merged.archives.is_empty() && merged.image_tags.is_empty() {
        bail!(
            "no split manifests for release '{}' in {}; run rlsr --split first and put its dist folder here",
            release.name,
            release.dist_folder
        );
    }
    for archive in &merged.archives {
        if fs::metadata(archive).await.is_err() {
            bail!("archive {} from a split manifest is missing", archive);
        }
    }

    Ok((merged.archives, merged.image_tags))
}

fn merge_into(merged: &mut Manifest, manifest: Manifest) {
    for archive in manifest.archives {
        if !merged.archives.contains(&archive) {
            merged.archives.push(archive);
        }
    }
    for tag in manifest.image_tags {
        if !merged.image_tags.contains(&tag) {
            merged.image_tags.push(tag);
        }
    }
}

/// Names the manifest after the release and targets so runners building
/// different targets don't overwrite each other's manifests.
fn manifest_file_name(release_name: &str, targets: &[String]) -> String {
    let targets = if targets.is_empty() {
        "all".to_string()
    } else {
        targets.join("+")
    };
    format!("{}{}-{}.yml", PREFIX, slug(release_name), slug(&targets))
}

fn slug(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '+' | '.') {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ReleaseTargets;

    fn release(name: &str, dist: &str) -> Release {
        Release {
            name: name.to_string(),
            dist_folder: dist.to_string(),
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

    #[test]
    fn test_manifest_file_name() {
        assert_eq!(
            manifest_file_name("Release to GitHub", &[]),
            "rlsr-split-release_to_github-all.yml"
        );
        assert_eq!(
            manifest_file_name(
                "app",
                &[
                    "linux/arm/7".to_string(),
                    "x86_64-pc-windows-gnu".to_string()
                ]
            ),
            "rlsr-split-app-linux_arm_7+x86_64-pc-windows-gnu.yml"
        );
    }

    #[tokio::test]
    async fn test_manifests_round_trip_and_merge() {
        let dir = tempfile::tempdir().unwrap();
        let dist = dir.path().to_str().unwrap();
        let app = release("app", dist);
        let other = release("other", dist);

        let linux = format!("{}/app-linux.zip", dist);
        let mac = format!("{}/app-mac.zip", dist);
        std::fs::write(&linux, "linux").unwrap();
        std::fs::write(&mac, "mac").unwrap();

        write_manifest(
            &app,
            &["linux".to_string()],
            std::slice::from_ref(&linux),
            &[],
        )
        .await
        .unwrap();
        write_manifest(
            &app,
            &["mac".to_string()],
            &[mac.clone(), linux.clone()],
            &["img:v1".to_string()],
        )
        .await
        .unwrap();
        // Another release sharing the dist folder isn't merged in.
        write_manifest(&other, &[], &[format!("{}/other.zip", dist)], &[])
            .await
            .unwrap();

        let (archives, image_tags) = read_manifests(&app).await.unwrap();
        assert_eq!(archives, vec![linux, mac]);
        assert_eq!(image_tags, vec!["img:v1".to_string()]);
    }

    #[tokio::test]
    async fn test_read_manifests_errors() {
        let dir = tempfile::tempdir().unwrap();
        let dist = dir.path().to_str().unwrap();
        let app = release("app", dist);

        // No manifests yet.
        assert!(read_manifests(&app).await.is_err());

        // Manifest pointing at an archive that wasn't copied over.
        write_manifest(&app, &[], &[format!("{}/missing.zip", dist)], &[])
            .await
            .unwrap();
        assert!(read_manifests(&app).await.is_err());
    }
}
