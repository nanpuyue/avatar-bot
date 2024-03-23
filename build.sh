#!/bin/bash -e

if [ "$1" = amd64 -o "$1" = arm64 ]; then
    PLATFORM="linux/$1"
    echo "build for $PLATFORM platform ..."
else
    echo "build for docker host platform ..."
fi

[ -d "$CARGO_HOME" ] || CARGO_HOME="$HOME/.cargo"
WORKDIR="/build/avatar-bot"
REGISTRY="/usr/local/cargo/registry"

docker run ${PLATFORM+--platform=$PLATFORM }-it --rm\
 -v "$CARGO_HOME/registry":"$REGISTRY"\
 -v "$PWD":"$WORKDIR" --workdir "$WORKDIR"\
 -e RUSTFLAGS="-C opt-level=s -C link-arg=-s"\
 ghcr.io/nanpuyue/avatar-bot-builder:latest\
 sh -c 'cargo build --release --target "$(rustc -Vv|grep host:|cut -b7-)"'
