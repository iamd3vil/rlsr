default: build-linux

build:
    cargo build

release:
    cargo run --release

build-linux:
    RUSTFLAGS='-C target-feature=+crt-static' cargo build --release --target x86_64-unknown-linux-gnu

build-macos:
    docker run --rm \
    --volume ${PWD}:/root/src \
    --workdir /root/src \
    joseluisq/rust-linux-darwin-builder:1.72.1 \
    sh -c 'CC=aarch64-apple-darwin22.4-clang CXX=aarch64-apple-darwin22.4-clang++ TARGET_CC=aarch64-apple-darwin22.4-clang TARGET_AR=aarch64-apple-darwin22.4-ar cargo build --release --target aarch64-apple-darwin'

build-windows:
    cross build --target x86_64-pc-windows-gnu --release