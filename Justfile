default: build-linux

run:
    cargo run --release -- --rm-dist --config rlsr.local.yml

build:
    cargo build

release:
    cargo run --release -- -c rlsr.yml --rm-dist -p

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

build-macos $RUSTFLAGS="-C target-feature=+crt-static":
    #!/usr/bin/env sh
    if [ "$(uname)" = "Darwin" ]; then
        # Running on macOS
        cargo build --release --target aarch64-apple-darwin
    else
        # Running on non-macOS (Linux, Windows)
        docker run --rm \
        --volume ${PWD}:/io \
        --workdir /io \
        ghcr.io/rust-cross/cargo-zigbuild:latest \
        sh -c 'rustup update stable && rustup target add aarch64-apple-darwin && cargo zigbuild --release --target aarch64-apple-darwin'
    fi

build-windows $RUSTFLAGS="-C target-feature=+crt-static":
    docker run --rm \
    --volume ${PWD}:/io \
    --workdir /io \
    ghcr.io/rust-cross/cargo-zigbuild:latest \
    sh -c 'rustup update stable && rustup target add x86_64-pc-windows-gnu && cargo zigbuild --release --target x86_64-pc-windows-gnu'

docs-serve:
    cd docs && npm run dev && cd ../
