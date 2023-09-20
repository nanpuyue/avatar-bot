#!/bin/bash -ex

RUSTARCH="$(uname -m)"

[ -d "$CARGO_HOME" ] || CARGO_HOME="$HOME/.cargo"
WORKDIR="/build/avatar-bot"
REGISTRY="/usr/local/cargo/registry"

if [ "$RUSTARCH" = "aarch64" ]; then
  EXTRA_CFLAGS="-mno-outline-atomics"
fi

docker run -it --rm -v "$PWD":"$WORKDIR" -v "$CARGO_HOME/registry":"$REGISTRY"\
 ${EXTRA_CFLAGS+-e EXTRA_CFLAGS=$EXTRA_CFLAGS }--workdir "$WORKDIR"\
 ghcr.io/nanpuyue/avatar-bot-builder:latest\
 cargo build --release --target "$RUSTARCH"-unknown-linux-musl
