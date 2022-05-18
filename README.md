# Rlsr (Releaser)

Rlsr is a tool to create & manage releases for your projects.

Currently `rlsr` supports github releases.

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
  - name: "Github release"
    # Dist folder is where the builds will exist.
    dist_folder: "./dist"
    # Github repo details.
    github:
      owner: "iamd3vil"
      repo: "rlsr"
    # Builds to execute.
    builds:
      # Command is the command to create a release build.
      - command: "cargo build --release"
        # Binary name.
        bin_name: "rlsr"
        # Path to the built binary after running the given command.
        artifact: "./target/release/rlsr"
        # Name of the archive that will be created with the built binary.
        # The archive will be attached with the github release.
        name: "rlsr-linux-x86_64"
```

- `dist_folder` is where
