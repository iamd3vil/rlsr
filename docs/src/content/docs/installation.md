---
title: Installation 🚀
description: How to install Rlsr.
---

Rlsr is available on multiple platforms. You can install it using the following methods:

## Github Releases

You can download the pre-built binaries from the [releases page](https://github.com/iamd3vil/rlsr/releases) on GitHub. Choose the appropriate binary for your platform and architecture.

## Nightly builds

Every night, the [`nightly` release](https://github.com/iamd3vil/rlsr/releases/tag/nightly) is rebuilt from the latest commit on `master`, if there are new commits. The download links stay the same between nightlies:

```sh
curl -fLO https://github.com/iamd3vil/rlsr/releases/download/nightly/rlsr-nightly-linux-x86_64.zip
```

Replace `linux-x86_64` with `linux-arm64`, `macos-arm64` or `windows-x86_64` for other platforms. Nightlies aren't tested beyond CI, so use the [latest release](https://github.com/iamd3vil/rlsr/releases/latest) unless you need an unreleased change.
