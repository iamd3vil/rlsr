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
    builds_sequential: false
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
        archive_name: "rlsr-{{ meta.tag }}-linux-x86_64"
        name: "Linux build"
        env:
          - "BIN_NAME=rlsr.bin"
      - command: "just build-macos"
        artifact: "target/aarch64-apple-darwin/release/rlsr"
        archive_name: "rlsr-{{ meta.tag }}-macos-arm64"
        name: "MacOS build"
      - command: "just build-windows"
        artifact: "target/x86_64-pc-windows-gnu/release/rlsr.exe"
        archive_name: "rlsr-{{ meta.tag }}-windows-x86_64"
        name: "Windows build"
      - command: "just build-freebsd"
        artifact: "target/x86_64-unknown-freebsd/release/rlsr"
        archive_name: "rlsr-{{ meta.tag }}-freebsd-x86_64"
        name: "FreeBSD build"
      - command: "just build-linux-arm64"
        artifact: "target/aarch64-unknown-linux-musl/release/rlsr"
        archive_name: "rlsr-{{ meta.tag }}-linux-arm64"
        name: "Linux ARM64 build"
changelog:
  format: "github"
  exclude:
    - "^doc:"
```

See [Project Examples](/examples/) for full Rust and Go configurations (including build matrices and Buildx).

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
- `builds_sequential`: (Optional) Run builds sequentially instead of in parallel.

### Targets

The `targets` subsection specifies where the releases will be published. Rlsr supports multiple target types, including GitHub and Docker.

For detailed information on configuring targets, please refer to the [Release Targets Configuration](./targets) page.

### Checksum

The `checksum` section allows you to specify the algorithm used for generating checksums of your artifacts:

- `algorithm`: The checksum algorithm.

Supported algorithms: `sha256`, `sha512`, `sha3_256`, `sha3_512`, `blake2b`, `blake2s`, `md5`, `sha1`.

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
  - `after`: An array of commands to run after all builds complete.

### Builds

The `builds` section is an array that defines one or more build configurations. Each build configuration can include:

- `type`: (Optional) Build type. Defaults to `custom`. Use `buildx` for Docker Buildx builds.
- `command`: The command to execute for building (required for `custom` builds).
- `buildx`: (Optional) Buildx configuration when `type` is `buildx`.
- `bin_name`: (Optional) The name of the binary produced.
- `artifact`: The path to the built artifact.
- `archive_name`: The name of the archive containing the artifact.
- `os`: (Optional) The target operating system label.
- `arch`: (Optional) The target architecture label.
- `arm`: (Optional) The ARM version label.
- `target`: (Optional) The target triple (or Buildx target when using `type: "buildx"`).
- `matrix`: (Optional) A build matrix that expands into multiple builds.
- `no_archive`: If true, the artifact won't be archived.
- `prehook`: (Optional) A script to run before this specific build.
- `posthook`: (Optional) A script to run after this specific build.
- `additional_files`: Build-specific additional files to include.
- `env`: Environment variables specific to this build. These will be merged with the global environment variables defined in the `releases` section.

## Build Types

### Custom (default)

Use `type: "custom"` (or omit `type`) to run a shell command. This build type uses the `command` field to produce the artifact.

### Buildx

Use `type: "buildx"` to build Docker images with Buildx. Configure it with the `buildx` block and keep `artifact`/`archive_name` so Rlsr can copy or archive the output. A common pattern is to use `buildx.outputs` with `type=tar` or `type=local` to generate a file or directory that matches the `artifact` path. If you set `buildx.outputs` to `type=registry`, Buildx pushes the multi-arch image during the build step (no local artifact is produced).

Supported `buildx` keys:

- `context`: Build context path.
- `dockerfile`: Dockerfile path.
- `tags`: Image tags to apply.
- `platforms`: Build platforms (for example, `linux/amd64`).
- `builder`: Named Buildx builder to use.
- `load`: Load the image into the local Docker daemon.
- `build_args`: Map of build arguments.
- `labels`: Map of image labels.
- `cache_from`: Cache sources.
- `cache_to`: Cache destinations.
- `target`: Target stage to build.
- `outputs`: Output specs (such as `type=tar`).
- `provenance`: Enable or disable provenance.
- `sbom`: Enable or disable SBOM generation.
- `secrets`: Build secrets.
- `ssh`: SSH agent forward specs.
- `annotations`: Map of image annotations.

## Build Matrix

You can expand a single build into multiple builds by defining a `matrix` on the build. Each matrix entry is a map of keys to lists of values, and Rlsr generates the cartesian product of those values.

Matrix entries can also target Buildx fields. For `type: "buildx"`, `target` updates the Buildx stage. Supported Buildx matrix keys include `platforms`, `tags`, `cache_from`, `cache_to`, `outputs`, `secrets`, `ssh`, `annotations` (use `key=value`), `builder`, `context`, `dockerfile`, `load`, `provenance`, and `sbom`. For maps, use `build_args.KEY`, `labels.KEY`, or `annotations.KEY` entries.

Example:

```yaml
builds:
  - name: "Go build"
    matrix:
      - os: ["linux", "darwin"]
        arch: ["amd64", "arm64"]
    command: "GOOS={{ meta.matrix.os }} GOARCH={{ meta.matrix.arch }} go build -o target/{{ meta.matrix.os }}/{{ meta.matrix.arch }}/app"
    artifact: "target/{{ meta.matrix.os }}/{{ meta.matrix.arch }}/app"
    archive_name: "app-{{ meta.matrix.os }}-{{ meta.matrix.arch }}"
```

## Templating

Rlsr supports templating in various configuration fields, allowing you to dynamically generate values based on release metadata. The templating system uses Minijinja (Jinja2-compatible) with `{{ variable }}` and `{% if %}` syntax.

For a full reference of template variables and filters, see the [Templating](/templating/) page.

### Supported Fields for Templating

Templating can be used in the following configuration fields:

- Release hooks: `hooks.before`, `hooks.after`
- Release env values: `env`
- Build fields: `name`, `command`, `bin_name`, `artifact`, `archive_name`, `prehook`, `posthook`
- Build env values: `env`
- Additional files: release and build `additional_files`
- Docker image: `targets.docker.image`, `targets.docker.images`

### Example Usage

```yaml
builds:
  - command: "cargo build --release --target x86_64-unknown-linux-gnu"
    artifact: "target/x86_64-unknown-linux-gnu/release/myapp"
    archive_name: "myapp-{{ meta.tag }}-linux-x86_64"
    prehook: "echo 'Building version {{ meta.version }} for Linux'"
    posthook: "cp LICENSE dist/{{ meta.tag }}/linux/"
```

In this example:

- The archive will be named with the Git tag (e.g., `myapp-v1.2.3-linux-x86_64`)
- The prehook displays the version being built (e.g., `Building version 1.2.3 for Linux`)
- The posthook copies the LICENSE file to a directory named after the tag

## Changelog Section

The `changelog` section configures how the changelog is generated for your releases:

- `format`: Specifies the format of the changelog (e.g., "github").
- `exclude`: An array of regular expressions to exclude specific entries from the changelog.
- `template`: The template file to use for generating the changelog.

### Templating changelog

Rlsr also supports templating in the changelog section. The following variables are available:

- `meta.tag`: The Git tag for the current release (e.g., `v1.2.3`)
- `commits`: An array of commits since the last release
  - See [Templating](/templating/) for the complete list of changelog fields and filters.

### Example Template

```jinja
## Features:
{% for commit in commits if commit.subject|starts_with("feat:") or commit.subject|starts_with("refactor:") %}
{{ commit.hash }}: {{ commit.subject|trim("feat: ")|trim("refactor: ") }}
{%- endfor %}

## Fixes:
{% for commit in commits if commit.subject|starts_with("fix:") %}
{{ commit.hash }}: {{ commit.subject|trim("fix: ") }}
{%- endfor %}
```

#### Filters

- `starts_with`: Checks if a string starts with a specified prefix.
- `trim`: Removes a specified prefix from a string.
