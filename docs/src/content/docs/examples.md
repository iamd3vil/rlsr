---
title: Project Examples
description: Example rlsr.yml files for Rust and Go projects.
---

Use these end-to-end examples as starting points for your own projects.

## Rust (rlsr.yml)

```yaml
releases:
  - name: "Release to github"
    dist_folder: "./dist"
    targets:
      github:
        owner: "iamd3vil"
        repo: "rlsr"
    checksum:
      algorithm: "sha256"
    additional_files:
      - "README.md"
      - "rlsr.sample.yml"
      - "LICENSE"
    builds:
      - command: "just build-linux"
        artifact: "target/x86_64-unknown-linux-musl/release/rlsr"
        bin_name: "rlsr"
        archive_name: "rlsr-{{ meta.tag }}-linux-x86_64"
        name: "Linux build"
      - command: "just build-macos"
        artifact: "target/aarch64-apple-darwin/release/rlsr"
        bin_name: "rlsr"
        archive_name: "rlsr-{{ meta.tag }}-macos-arm64"
        name: "MacOS build"
      - command: "just build-windows"
        artifact: "target/x86_64-pc-windows-gnu/release/rlsr.exe"
        bin_name: "rlsr.exe"
        archive_name: "rlsr-{{ meta.tag }}-windows-x86_64"
        name: "Windows build"
changelog:
  format: "github"
  template: "changelog.tpl"
  exclude:
    - "^doc:"
```

## Rust (native builder)

```yaml
releases:
  - name: "Release to github"
    dist_folder: "./dist"
    targets:
      github:
        owner: "OWNER"
        repo: "myapp"
    checksum:
      algorithm: "sha256"
    builds:
      - name: "myapp"
        type: "rust"
        archive_name: "myapp-{{ meta.tag }}-{{ meta.os }}-{{ meta.arch }}"
        archive_format: tar_gz
        env:
          - "CARGO_TARGET_DIR=target/{{ meta.target }}"
        rust:
          targets:
            - "x86_64-unknown-linux-musl"
            - "aarch64-unknown-linux-musl"
            - "aarch64-apple-darwin"
            - "x86_64-pc-windows-gnu"
```

## Go (native builder)

```yaml
releases:
  - name: "Release to github"
    dist_folder: "./dist"
    targets:
      github:
        owner: "OWNER"
        repo: "myapp"
    checksum:
      algorithm: "sha256"
    env:
      - "CGO_ENABLED=0"
    builds:
      - name: "myapp"
        type: "go"
        archive_name: "myapp-{{ meta.tag }}-{{ meta.os }}-{{ meta.arch }}"
        archive_format: tar_gz
        go:
          main: "./cmd/myapp"
          targets: ["linux/amd64", "linux/arm64", "darwin/arm64", "windows/amd64"]
          flags: ["-trimpath"]
          ldflags: ["-s -w", "-X main.version={{ meta.version }}"]
```

## Go (matrix + buildx)

```yaml
releases:
  - name: "Go hello release"
    dist_folder: "./dist"
    builds_sequential: false
    targets:
      docker:
        push: false
    checksum:
      algorithm: "sha256"
    builds:
      - name: "Go build"
        matrix:
          - os: ["linux", "darwin", "windows"]
            arch: ["amd64"]
        command: "mkdir -p target/{{ meta.matrix.os }}/{{ meta.matrix.arch }} && CGO_ENABLED=0 GOOS={{ meta.matrix.os }} GOARCH={{ meta.matrix.arch }} go build -o target/{{ meta.matrix.os }}/{{ meta.matrix.arch }}/hello{{ '.exe' if meta.matrix.os == 'windows' else '' }}"
        artifact: "./target/{{ meta.matrix.os }}/{{ meta.matrix.arch }}/hello{{ '.exe' if meta.matrix.os == 'windows' else '' }}"
        bin_name: "hello{{ '.exe' if meta.matrix.os == 'windows' else '' }}"
        archive_name: "hello-{{ meta.matrix.os }}-{{ meta.matrix.arch }}"
      - name: "Docker buildx"
        type: "buildx"
        artifact: ""
        archive_name: "buildx"
        buildx:
          context: "."
          dockerfile: "./Dockerfile"
          builder: "rlsr-multiarch"
          platforms:
            - "linux/amd64"
            - "linux/arm64"
          tags:
            - "iamd3vil/go-hello-world:{{ meta.tag }}"
            - "iamd3vil/go-hello-world:latest"
          outputs:
            - "type=registry"
changelog:
  format: "default"
```

## Buildx matrix example

```yaml
releases:
  - name: "Buildx matrix"
    dist_folder: "./dist"
    targets:
      docker:
        push: false
    builds:
      - name: "Docker buildx matrix"
        type: "buildx"
        artifact: ""
        archive_name: "buildx"
        matrix:
          - platforms: ["linux/amd64", "linux/arm64"]
            tags: ["ghcr.io/acme/app:{{ meta.tag }}", "ghcr.io/acme/app:latest"]
            build_args.RUST_VERSION: ["1.78", "1.79"]
            labels.org.opencontainers.image.title: ["acme-app"]
        buildx:
          context: "."
          dockerfile: "./Dockerfile"
          builder: "acme-builder"
          outputs:
            - "type=registry"
changelog:
  format: "default"
```
