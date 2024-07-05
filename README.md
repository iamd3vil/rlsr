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
    # The dist folder where the builds will be stored.
    dist_folder: "./dist"
    # The targets where the builds will be released.
    targets:
      github:
        owner: "iamd3vil"
        repo: "rlsr"
      docker:
        image: "localhost:5000/rlsr"
        dockerfile: "./Dockerfile"
        context: "."
    # The checksum algorithm to use.
    checksum:
      algorithm: "sha256"
    # These additional files will be included with all the builds.
    additional_files:
      - "README.md"
      - "LICENSE"
    builds:
      - command: "cargo build --release"
        bin_name: "rlsr" # Optional, defaults to the archive name.
        artifact: "./target/release/rlsr" # The artifact to archive and release.
        archive_name: "rlsr-linux-x86_64" # Archive name.
        no_archive: false # If turned true, will not archive the artifact.

        # Build specific additional files.
        additional_files:
          - "README.md"
          - "LICENSE"
changelog:
  format: "github"
```
