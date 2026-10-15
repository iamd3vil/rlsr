---
title: GitHub Actions
description: Use the composite action to run rlsr in CI.
---

## Overview

Rlsr ships a composite GitHub Action so you can run releases as a single step
alongside other steps (like installing Rust or running tests). It installs a
pinned `rlsr` release, runs your `rlsr.yml`, and optionally publishes artifacts.

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
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - name: Checkout
        uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable
      - name: Release
        uses: iamd3vil/rlsr/.github/actions/rlsr@v0.9.0
        with:
          config: rlsr.yml
          publish: true
          rm_dist: true
          github_token: ${{ secrets.GITHUB_TOKEN }}
```

Build only (no publish):

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: iamd3vil/rlsr/.github/actions/rlsr@v0.9.0
        with:
          publish: false
```

## Inputs

| Name | Type | Default | Description |
| --- | --- | --- | --- |
| `config` | string | `rlsr.yml` | Path to the rlsr config file. |
| `publish` | string | `false` | Publish artifacts (false passes `--skip-publish`). |
| `rm_dist` | string | `false` | Remove the dist directory before building. |
| `rlsr_version` | string | `latest` | Version tag to install (example: `v0.9.0`). |
| `args` | string | `""` | Extra args for `rlsr` (space-separated). |
| `working_directory` | string | `.` | Working directory to run `rlsr` in. |
| `github_token` | string | `""` | Token for GitHub API operations. Falls back to `GITHUB_TOKEN`. |

## Notes and behavior

- Publishing needs a tag and a clean repo; otherwise `rlsr` skips publish.
- Use `actions/checkout` with `fetch-depth: 0` so tags are available for changelogs.
- Supported runners: Linux x86_64 and macOS arm64.
- Use `rlsr_version` to pin a known version instead of `latest`.

## Advanced examples

Monorepo working directory:

```yaml
jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: iamd3vil/rlsr/.github/actions/rlsr@v0.9.0
        with:
          working_directory: ./crates/my-tool
          config: rlsr.yml
          publish: true
```

Custom runner:

```yaml
jobs:
  release:
    runs-on: ubuntu-22.04
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: iamd3vil/rlsr/.github/actions/rlsr@v0.9.0
        with:
          publish: false
```

### Build on parallel runners

By default rlsr runs every build on one runner. To give each target its own
runner, run rlsr with `--split --targets <target>` in a build matrix. Each
runner builds and packages its targets and writes a manifest to `dist/`. A
final job downloads every runner's `dist/` and runs `rlsr --merge`, which
checksums and publishes all the archives together. `rlsr.yml` stays the same
as for local releases.

`--targets` matches each build's `target`: the triple for `type: rust` builds,
`os/arch` for `type: go` builds, or the `target` field you set on a custom
build.

```yaml
permissions:
  contents: read

jobs:
  build:
    strategy:
      matrix:
        include:
          - name: linux
            target: x86_64-unknown-linux-musl
          - name: windows
            target: x86_64-pc-windows-gnu
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      # Install toolchains here (Rust, zig and cargo-zigbuild for type: rust).
      - uses: iamd3vil/rlsr/.github/actions/rlsr@v0.9.0
        with:
          args: --split --targets ${{ matrix.target }}
      - uses: actions/upload-artifact@v4
        with:
          name: dist-${{ matrix.name }}
          path: dist/
          if-no-files-found: error

  release:
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: actions/download-artifact@v4
        with:
          pattern: dist-*
          path: dist/
          merge-multiple: true
      - uses: iamd3vil/rlsr/.github/actions/rlsr@v0.9.0
        with:
          publish: true
          args: --merge
          github_token: ${{ secrets.GITHUB_TOKEN }}
```

`--split` and `--merge` need an rlsr release newer than v0.9.0. The action installs the latest release by default.

Don't pass `rm_dist: true` to the merge job, because it would delete the
downloaded archives. rlsr refuses `--merge` together with `--rm-dist`.

If you build binaries outside rlsr, you can still put them at each build's
`artifact` path and run rlsr with `--skip-build` in a single final job instead.

## Local testing with act

Use your normal workflow file and run it with:

```bash
act workflow_dispatch -W .github/workflows/your-workflow.yml -s GITHUB_TOKEN=dummy
```
