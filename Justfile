default: build-linux

run:
    cargo run --release -- --rm-dist --config rlsr.local.yml

build:
    cargo build

release:
    cargo run --release


build-linux $RUSTFLAGS="-C target-feature=+crt-static":
    cross build --release --target x86_64-unknown-linux-gnu

build-freebsd:
    cross build --release --target x86_64-unknown-freebsd

build-macos $RUSTFLAGS="-C target-feature=+crt-static":
    #!/usr/bin/env sh
    if [ "$(uname)" = "Darwin" ]; then
        # Running on macOS
        cargo build --release --target aarch64-apple-darwin
    else
        # Running on non-macOS (Linux, Windows)
        docker run --rm \
        --volume ${PWD}:/root/src \
        --workdir /root/src \
        joseluisq/rust-linux-darwin-builder:1.79.0 \
        sh -c 'CC=aarch64-apple-darwin22.4-clang CXX=aarch64-apple-darwin22.4-clang++ TARGET_CC=aarch64-apple-darwin22.4-clang TARGET_AR=aarch64-apple-darwin22.4-ar cargo build --release --target aarch64-apple-darwin'
    fi

build-windows:
    cross build --target x86_64-pc-windows-gnu --release
