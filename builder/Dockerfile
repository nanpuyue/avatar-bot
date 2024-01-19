FROM alpine AS alpine-amd64
ARG RUSTARCH=x86_64

FROM rust:slim AS rust-amd64
ARG RUSTARCH=x86_64

FROM alpine AS alpine-arm64
ARG RUSTARCH=aarch64

FROM rust:slim AS rust-arm64
ARG RUSTARCH=aarch64

FROM alpine-$TARGETARCH AS builder
RUN apk add bash diffutils cmake gcc g++ make openssl-dev openssl-libs-static perl pkgconfig yasm zlib-dev zlib-static
ADD build.sh /
RUN /build.sh && rm -rf /build

FROM rust-$TARGETARCH
SHELL ["/bin/bash", "-c"]
COPY --from=builder / /opt/musl
RUN apt update &&\
 apt install -y --no-install-recommends make musl{,-dev,-tools} pkg-config libclang-dev &&\
 apt clean &&\
 rustup target add "$RUSTARCH"-unknown-linux-musl

ENV CC="musl-gcc"
ENV MUSL_DIR="/opt/musl"
ENV CFLAGS="-I${MUSL_DIR}/usr/include -I${MUSL_DIR}/usr/local/include"
ENV LDFLAGS="-L${MUSL_DIR}/lib -L${MUSL_DIR}/usr/lib -L${MUSL_DIR}/usr/local/lib"
ENV OPENSSL_DIR="${MUSL_DIR}/usr"
ENV OPENSSL_STATIC="1"
ENV TARGET_PKG_CONFIG_SYSROOT_DIR="${MUSL_DIR}"
ENV TARGET_PKG_CONFIG_PATH="${MUSL_DIR}/usr/lib/pkgconfig:${MUSL_DIR}/usr/local/lib/pkgconfig"
ENV FFMPEG_PKG_CONFIG_PATH="${MUSL_DIR}/usr/local/lib/pkgconfig"
ENV TARGET_PKG_CONFIG_ALLOW_CROSS="1"
ENV RUSTFLAGS="-Copt-level=s -Clink-arg=-s"
ENV BINDGEN_EXTRA_CLANG_ARGS="-I/usr/include/${RUSTARCH}-linux-musl ${CFLAGS}"
