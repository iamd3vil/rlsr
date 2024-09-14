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
    -p, --publish
        --rm-dist
    -V, --version            Print version information
```

If `publish` flag isn't given, `rlsr` will skip publishing. `rm-dist` flag cleans the dist folder before building the release again.

## Configuration 🔧

[Configuration](/config/config/) can be found here.