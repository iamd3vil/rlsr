---
title: GitHub Actions
description: Use the reusable workflow to run rlsr in CI.
---

## Overview

Rlsr ships a reusable GitHub Actions workflow so other repositories can run releases
without copying the setup. It installs a pinned `rlsr` release, runs your `rlsr.yml`,
and optionally publishes artifacts.

Use this when you want a single, consistent release step across multiple repos.

## Quick start

Release on tags:

```yaml
name: Release

on:
  push:
    tags:
      - "v*"

jobs:
  release:
    uses: iamd3vil/rlsr/.github/workflows/rlsr.yml@v0.6.1
    with:
      config: rlsr.yml
      publish: true
      rm_dist: true
    secrets:
      github_token: ${{ secrets.GITHUB_TOKEN }}
```

Build only (no publish):

```yaml
jobs:
  build:
    uses: iamd3vil/rlsr/.github/workflows/rlsr.yml@v0.6.1
    with:
      publish: false
```

## Inputs

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `runs_on` | string | `ubuntu-latest` | Runner label to execute on. |
| `config` | string | `rlsr.yml` | Path to the rlsr config file. |
| `publish` | boolean | `false` | Publish artifacts (false passes `--skip-publish`). |
| `rm_dist` | boolean | `false` | Remove the dist directory before building. |
| `rlsr_version` | string | `latest` | Version tag to install (example: `v0.6.1`). |
| `args` | string | `""` | Extra args for `rlsr` (space-separated). |
| `working_directory` | string | `.` | Working directory to run `rlsr` in. |

## Secrets

| Name | Required | Description |
| --- | --- | --- |
| `github_token` | No | Token for GitHub API operations. Falls back to `GITHUB_TOKEN`. |

## Notes and behavior

- Publishing needs a tag and a clean repo; otherwise `rlsr` skips publish.
- The workflow checks out full git history and tags so changelog generation works.
- Supported runners: Linux x86_64 and macOS arm64.
- Use `rlsr_version` to pin a known version instead of `latest`.

## Advanced examples

Monorepo working directory:

```yaml
jobs:
  release:
    uses: iamd3vil/rlsr/.github/workflows/rlsr.yml@v0.6.1
    with:
      working_directory: ./crates/my-tool
      config: rlsr.yml
      publish: true
```

Custom runner:

```yaml
jobs:
  release:
    uses: iamd3vil/rlsr/.github/workflows/rlsr.yml@v0.6.1
    with:
      runs_on: ubuntu-22.04
      publish: false
```

## Local testing with act

Minimal config for a dry run:

```yaml
releases:
  - name: "E2E Test Release"
    dist_folder: "./dist-e2e"
    targets:
      github:
        owner: "example"
        repo: "example"
    checksum:
      algorithm: "sha256"
    builds:
      - name: "hello"
        command: "mkdir -p target/rlsr-e2e && echo 'hello rlsr' > target/rlsr-e2e/hello.txt"
        artifact: "target/rlsr-e2e/hello.txt"
        archive_name: "hello-e2e"
```

Temporary caller workflow:

```yaml
name: rlsr e2e test

on:
  workflow_dispatch:

jobs:
  e2e:
    uses: ./.github/workflows/rlsr.yml
    with:
      rlsr_version: v0.6.1
      config: rlsr.e2e.yml
      publish: false
      rm_dist: true
```

Run it:

```bash
act workflow_dispatch -W .github/workflows/rlsr-e2e-test.yml -s GITHUB_TOKEN=dummy
```
