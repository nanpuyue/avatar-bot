#!/bin/bash -ex

TARGET_ARCH="$(uname -m)"

docker build builder --build-arg TARGET_ARCH="$TARGET_ARCH" -t avatar-bot-builder

[ -d "$CARGO_HOME" ] || CARGO_HOME="$HOME/.cargo"
WORKDIR="/build/avatar-bot"
REGISTRY="/usr/local/cargo/registry"

if [ "$TARGET_ARCH" = "aarch64" ]; then
  EXTRA_CFLAGS="-mno-outline-atomics"
fi

docker run -it --rm -v "$PWD":"$WORKDIR" -v "$CARGO_HOME/registry":"$REGISTRY"\
 ${EXTRA_CFLAGS+-e EXTRA_CFLAGS=$EXTRA_CFLAGS }--workdir "$WORKDIR" avatar-bot-builder\
 cargo build --release --target "$TARGET_ARCH"-unknown-linux-musl
