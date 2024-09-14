---
title: Configuration 🔧
description: How to configure Rlsr.
---

Rlsr is configured using a configuration file in the root of your project. This file defines the release process and versioning strategy for your project.

## Supported Formats

Rlsr supports the following configuration file formats:

- YAML (`.yml` or `.yaml`)
- TOML (`.toml`)
- JSON (`.json`)

The default filename is `rlsr.yml`, but you can use any of the supported formats with the appropriate extension.

### Sample Configuration

Here's a sample configuration in YAML format:

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
        artifact: "target/x86_64-unknown-linux-gnu/release/rlsr"
        archive_name: "rlsr-{tag}-linux-x86_64"
        name: "Linux build"
      - command: "just build-macos"
        artifact: "target/aarch64-apple-darwin/release/rlsr"
        archive_name: "rlsr-{tag}-macos-arm64"
        name: "MacOS build"
      - command: "just build-windows"
        artifact: "target/x86_64-pc-windows-gnu/release/rlsr.exe"
        archive_name: "rlsr-{tag}-windows-x86_64"
        name: "Windows build"
      - command: "just build-freebsd"
        artifact: "target/x86_64-unknown-freebsd/release/rlsr"
        archive_name: "rlsr-{tag}-freebsd-x86_64"
        name: "FreeBSD build"
      - command: "just build-linux-arm64"
        artifact: "target/aarch64-unknown-linux-musl/release/rlsr"
        archive_name: "rlsr-{tag}-linux-arm64"
        name: "Linux ARM64 build"
changelog:
  format: "github"
```

## Configuration Structure

The configuration file consists of two main sections:

1. `releases`: Defines the release process
2. `changelog`: Specifies how the changelog is generated

Let's explore each section in detail.

## Releases Section

The `releases` section is an array that can contain one or more release configurations. Each release configuration defines a specific release process and can include the following components:

### General Options

- `name`: A descriptive name for the release process.
- `dist_folder`: The directory where built artifacts will be stored.

### Targets

The `targets` subsection specifies where the releases will be published. Rlsr supports multiple target types, including GitHub and Docker.

For detailed information on configuring targets, please refer to the [Release Targets Configuration](./targets) page.

### Checksum

The `checksum` section allows you to specify the algorithm used for generating checksums of your artifacts:
- `algorithm`: The checksum algorithm (e.g., "sha256").

### Additional Files

You can specify a list of extra files to include with all builds:
- `additional_files`: An array of file paths relative to your project root.

### Environment Variables

Define environment variables for the build process:
- `env`: An array of environment variables in the format "KEY=value".
### Hooks

Specify commands to run at certain points in the release process:
- `hooks`:
  - `before`: An array of commands to run before any build starts.

### Builds

The `builds` section is an array that defines one or more build configurations. Each build configuration can include:

- `command`: The command to execute for building.
- `bin_name`: (Optional) The name of the binary produced.
- `artifact`: The path to the built artifact.
- `archive_name`: The name of the archive containing the artifact.
- `no_archive`: If true, the artifact won't be archived.
- `prehook`: (Optional) A script to run before this specific build.
- `posthook`: (Optional) A script to run after this specific build.
- `additional_files`: Build-specific additional files to include.

## Changelog Section

The `changelog` section configures how the changelog is generated for your releases:

- `format`: Specifies the format of the changelog (e.g., "github").