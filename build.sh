#!/bin/bash

docker build builder -t avatar-bot-builder
docker run -it --rm -v "$PWD":/build/avatar-bot --workdir /build/avatar-bot avatar-bot-builder\
 cargo build --release --target x86_64-unknown-linux-musl
