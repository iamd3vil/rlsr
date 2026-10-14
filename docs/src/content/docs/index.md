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
        --skip-build
    -V, --version            Print version information
```

If `--skip-publish` flag is given, `rlsr` will skip publishing. `rm-dist` flag cleans the dist folder before building the release again.

`--skip-build` skips each build's command, `prehook` and `posthook`, and packages the files already at the configured `artifact` paths. Use it when the binaries were built elsewhere, such as on [parallel CI runners](/github-actions/#build-on-parallel-runners). Buildx builds aren't supported with `--skip-build`.

## Configuration 🔧

[Configuration](/config/config/) can be found here.

## Project Examples

[Project Examples](/examples/) includes full Rust and Go `rlsr.yml` samples.

## Templating

[Templating](/templating/) documents available template variables and filters.
