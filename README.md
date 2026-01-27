<div align="right">
  <a href="https://zerodha.tech">
    <img src="https://zerodha.tech/static/images/github-badge.svg" width="140" />
  </a>
</div>

# Rlsr (Releaser)

Rlsr is a release automation CLI for building, packaging, checksumming, and publishing release artifacts for your projects.

Documentation: [https://rlsr.sarat.dev/](https://rlsr.sarat.dev/)

## Features

- Build multiple release artifacts with configurable commands and hooks.
- Publish to GitHub Releases and Docker registries.
- Generate checksums with configurable algorithms.
- Templated configuration with release/build metadata.
- Build matrix expansion to generate builds across OS/arch combinations.
- Customizable changelog formatting.

## Installation

Download pre-built binaries from the GitHub releases page: [https://github.com/iamd3vil/rlsr/releases](https://github.com/iamd3vil/rlsr/releases)

## Usage

```
USAGE:
    rlsr [OPTIONS]

OPTIONS:
    -c, --config <CONFIG>     [default: rlsr.yml]
    -h, --help                Print help information
    -p, --skip-publish         Skip publishing release artifacts
        --rm-dist             Remove dist folder before building
    -V, --version             Print version information
```

If `--skip-publish` is set, `rlsr` will build but not publish. `--rm-dist` cleans the dist folder before building the release again.

## Configuration

Rlsr uses `rlsr.yml` (or another supported format) to define releases, builds, hooks, and targets. A simple example:

```yaml
releases:
  - name: "Release to GitHub"
    dist_folder: "./dist"
    targets:
      github:
        owner: "iamd3vil"
        repo: "rlsr"
    builds:
      - name: "Linux build"
        command: "just build-linux"
        artifact: "target/x86_64-unknown-linux-musl/release/rlsr"
        archive_name: "rlsr-{{ meta.tag }}-linux-x86_64"
```

### Build matrix example

```yaml
builds:
  - name: "Go build"
    matrix:
      - os: ["linux", "darwin", "windows"]
        arch: ["amd64"]
    command: "GOOS={{ meta.matrix.os }} GOARCH={{ meta.matrix.arch }} go build -o target/{{ meta.matrix.os }}/{{ meta.matrix.arch }}/app"
    artifact: "target/{{ meta.matrix.os }}/{{ meta.matrix.arch }}/app"
    archive_name: "app-{{ meta.matrix.os }}-{{ meta.matrix.arch }}"
```

See the configuration docs for all options: [https://rlsr.sarat.dev/config/config/](https://rlsr.sarat.dev/config/config/)

## License

Rlsr is licensed under the GNU General Public License v3.0. See [LICENSE](LICENSE).
