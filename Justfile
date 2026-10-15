default: build

run:
    cargo run --release -- --rm-dist --config rlsr.local.yml

build:
    cargo build

release:
    cargo run --release -- -c rlsr.yml --rm-dist

build-freebsd $RUSTFLAGS="-C target-feature=+crt-static":
    cross build --release --target x86_64-unknown-freebsd

# rlsr.yml's macOS build runs this with `--target <triple> <flags>` appended.
# Linking for macOS needs the macOS SDK, which the cargo-zigbuild image ships.
# Build a macOS target: cargo on a Mac, the cargo-zigbuild image elsewhere.
cargo-macos *ARGS:
    #!/usr/bin/env sh
    set -eu
    if [ "$(uname)" = "Darwin" ]; then
        exec cargo build {{ARGS}}
    fi
    # Run as the current user so target/ doesn't end up owned by root. The
    # image already has the macOS targets installed.
    exec docker run --rm \
        --user "$(id -u):$(id -g)" \
        --volume "$PWD":/io \
        --workdir /io \
        -e HOME=/tmp \
        -e CARGO_HOME=/io/target/docker-cargo-home \
        -e CARGO_TARGET_DIR \
        ghcr.io/rust-cross/cargo-zigbuild:latest \
        cargo zigbuild {{ARGS}}

docs-serve:
    cd docs && npm run dev && cd ../
