# Rlsr (Releaser)

Rlsr is a tool to create & manage releases for your projects.

Documentation can be found at: [Docs](https://rlsr.sarat.dev/)

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

More details can be found at: [Configuration Docs](https://rlsr.sarat.dev/config/config/)
