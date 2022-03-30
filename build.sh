#!/bin/bash

docker build builder -t avatar-bot-builder

[ -d "$CARGO_HOME" ] || CARGO_HOME="$HOME/.cargo"
WORKDIR="/build/avatar-bot"
REGISTRY="/usr/local/cargo/registry"

docker run -it --rm -v "$PWD":"$WORKDIR" -v "$CARGO_HOME/registry":"$REGISTRY"\
 --workdir "$WORKDIR" avatar-bot-builder\
 cargo build --release --target x86_64-unknown-linux-musl
