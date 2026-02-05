---
title: GitLab Releases 🦊
description: How to configure GitLab releases in Rlsr.
---

Rlsr can publish releases directly to GitLab by configuring the `gitlab` target in your release configuration.

## Configure the GitLab Target

Add a `gitlab` entry under `targets`:

```yaml
targets:
  gitlab:
    owner: "namespace" # or username
    repo: "project-name"
    url: "https://gitlab.com" # optional, defaults to gitlab.com
```

- `owner`: The GitLab group/namespace or username that owns the project.
- `repo`: The GitLab project name.
- `url`: The GitLab instance URL (optional, defaults to `https://gitlab.com`). Use this for self-hosted GitLab instances.

## Authentication

Set the `GITLAB_TOKEN` environment variable with a GitLab Personal Access Token that has the **api** scope. Rlsr uses this token to create releases and upload assets.

## Example Configuration

```yaml
releases:
  - name: "Release to GitLab"
    dist_folder: "./dist"
    targets:
      gitlab:
        owner: "namespace"
        repo: "project-name"
    builds:
      - command: "cargo build --release"
        artifact: "./target/release/app"
        archive_name: "app-{{ meta.tag }}-linux-x86_64"
        archive_format: tar_gz
```

## Self-Hosted GitLab

For self-hosted GitLab instances, specify the `url` field:

```yaml
targets:
  gitlab:
    owner: "myteam"
    repo: "myproject"
    url: "https://gitlab.example.com"
```

## Asset Handling

Build artifacts are uploaded as GitLab **generic packages** and then attached to the release as asset links. The release page will link to the package files stored in the GitLab Package Registry.
