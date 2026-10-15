default: build-linux

run:
    cargo run --release -- --rm-dist --config rlsr.local.yml

build:
    cargo build

release:
    cargo run --release -- -c rlsr.yml --rm-dist

build-linux $RUSTFLAGS="-C target-feature=+crt-static":
    #!/usr/bin/env sh
    if [ "$(uname)" = "Darwin" ]; then
        cross build --release --target x86_64-unknown-linux-musl
    elif [ "$(uname)" = "Linux" ]; then
        cargo build --release --target x86_64-unknown-linux-musl
    else
        echo "Unsupported platform for build-linux"
        exit 1
    fi

build-linux-arm64 $RUSTFLAGS="-C target-feature=+crt-static":
    cross build --release --target aarch64-unknown-linux-musl

build-freebsd $RUSTFLAGS="-C target-feature=+crt-static":
    cross build --release --target x86_64-unknown-freebsd

# Separate target dirs so parallel builds don't wait on cargo's target dir lock.
build-macos $RUSTFLAGS="-C target-feature=+crt-static" $CARGO_TARGET_DIR="target/macos":
    #!/usr/bin/env sh
    if [ "$(uname)" = "Darwin" ]; then
        # Running on macOS
        cargo build --release --target aarch64-apple-darwin
    else
        # Running on non-macOS (Linux, Windows)
        docker run --rm \
        --volume ${PWD}:/io \
        --workdir /io \
        -e CARGO_TARGET_DIR \
        ghcr.io/rust-cross/cargo-zigbuild:latest \
        sh -c 'rustup update stable && rustup target add aarch64-apple-darwin && cargo zigbuild --release --target aarch64-apple-darwin'
    fi

build-windows $RUSTFLAGS="-C target-feature=+crt-static" $CARGO_TARGET_DIR="target/windows":
    docker run --rm \
    --volume ${PWD}:/io \
    --workdir /io \
    -e CARGO_TARGET_DIR \
    ghcr.io/rust-cross/cargo-zigbuild:latest \
    sh -c 'rustup update stable && rustup target add x86_64-pc-windows-gnu && cargo zigbuild --release --target x86_64-pc-windows-gnu'

# Tool for the macOS build in rlsr.yml, which appends `--target <triple> <flags>`.
# Linking for macOS needs the macOS SDK, so use cargo on a Mac and the
# cargo-zigbuild image, which ships the SDK, everywhere else.
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
