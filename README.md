# Rlsr (Releaser)

Rlsr is a tool to create & manage releases for your projects.

Currently `rlsr` supports github releases and docker registry.

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

## Configuration

`rlsr` looks for a `rlsr.yml` in your project.

#### Example

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
