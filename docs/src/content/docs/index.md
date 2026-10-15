---
title: Rlsr Documentation
description: A guide in my new Starlight docs site.
---

Rlsr is a tool to create & manage releases for your projects.

Currently `rlsr` supports github releases and docker registry.

## Installation 🚀

Installation instructions can be found [here](/installation/).

## Usage

```
USAGE:
    rlsr [OPTIONS]

OPTIONS:
    -c, --config <CONFIG>    [default: rlsr.yml]
    -h, --help               Print help information
        --rm-dist
    -s, --skip-publish
        --snapshot
        --nightly
        --skip-build
        --split
        --targets <TARGETS>
        --merge
    -V, --version            Print version information
```

A release needs a clean working tree and a tag on HEAD; rlsr stops with an error otherwise. If `--skip-publish` flag is given, `rlsr` will skip publishing. `rm-dist` flag cleans the dist folder before building the release again.

`--snapshot` builds without a tag, for testing a release locally or in CI. Uncommitted changes are allowed, nothing is published, and the version comes from [`snapshot.version_template`](/config/config/#snapshot), `1.2.4-snapshot+abc1234` by default after `v1.2.3`.

`--nightly` builds HEAD and publishes it as a rolling prerelease under the `nightly` tag, replacing the previous nightly. See [Nightly](/config/config/#nightly).

`--skip-build` skips each build's command, `prehook` and `posthook`, and packages the files already at the configured `artifact` paths. Use it when the binaries were built elsewhere, such as on [parallel CI runners](/github-actions/#build-on-parallel-runners). Release-level `hooks` still run. Buildx builds aren't supported with `--skip-build`.

`--split` builds and packages without checksumming or publishing, and writes a manifest (`rlsr-split-*.yml`) listing the archives to the dist folder. `--targets` limits a split run to builds whose `target` is in the comma-separated list, such as `--targets x86_64-unknown-linux-musl,linux/amd64`. `--merge` skips hooks and builds, reads every manifest in the dist folder and checksums and publishes all the archives together. Use these to build each target on its own [CI runner](/github-actions/#build-on-parallel-runners).

## Configuration 🔧

[Configuration](/config/config/) can be found here.

## Project Examples

[Project Examples](/examples/) includes full Rust and Go `rlsr.yml` samples.

## Templating

[Templating](/templating/) documents available template variables and filters.
