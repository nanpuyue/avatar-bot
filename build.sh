#!/bin/bash -ex

if [ "$1" = x86_64 ] || [ "$1" = aarch64 ]; then
    RUSTARCH="$1"
else
    RUSTARCH="$(uname -m)"
    [ "$RUSTARCH" = arm64 ] && RUSTARCH=aarch64
fi

if [ "$RUSTARCH" = x86_64 ];then
    PLATFORM=linux/amd64
elif [ "$RUSTARCH" = aarch64 ]; then
    PLATFORM=linux/arm64
else
    echo "Unsupported platform!"
    exit 1
fi

[ -d "$CARGO_HOME" ] || CARGO_HOME="$HOME/.cargo"
WORKDIR="/build/avatar-bot"
REGISTRY="/usr/local/cargo/registry"

docker run --platform="$PLATFORM" -it --rm -v "$CARGO_HOME/registry":"$REGISTRY"\
 -v "$PWD":"$WORKDIR" --workdir "$WORKDIR"\
 ghcr.io/nanpuyue/avatar-bot-builder:latest\
 cargo build --release --target "$RUSTARCH-unknown-linux-musl"
