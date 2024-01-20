#!/bin/bash -ex

RUSTARCH="$(uname -m)"
[ "$RUSTARCH" = arm64 ] && RUSTARCH=aarch64

[ -d "$CARGO_HOME" ] || CARGO_HOME="$HOME/.cargo"
WORKDIR="/build/avatar-bot"
REGISTRY="/usr/local/cargo/registry"

docker run -it --rm -v "$CARGO_HOME/registry":"$REGISTRY"\
 -v "$PWD":"$WORKDIR" --workdir "$WORKDIR"\
 ghcr.io/nanpuyue/avatar-bot-builder:latest\
 cargo build --release --target "$RUSTARCH-unknown-linux-musl"
