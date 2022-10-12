FROM rust:slim as builder
ARG TARGET_ARCH=x86_64
ADD build.sh /
RUN /build.sh

FROM rust:slim
ARG TARGET_ARCH=x86_64
SHELL ["/bin/bash", "-c"]

COPY --from=builder /opt/musl /opt/musl
RUN apt update &&\
 apt install -y --no-install-recommends make musl{,-dev,-tools} pkg-config libclang-dev &&\
 apt clean &&\
 rustup target add "$TARGET_ARCH"-unknown-linux-musl

ENV CC="musl-gcc"
ENV MUSL_DIR="/opt/musl"
ENV CFLAGS="-I${MUSL_DIR}/include"
ENV LDFLAGS="-L${MUSL_DIR}/lib"
ENV OPENSSL_DIR="${MUSL_DIR}"
ENV OPENSSL_STATIC="1"
ENV TARGET_PKG_CONFIG_PATH="${MUSL_DIR}/lib/pkgconfig"
ENV FFMPEG_PKG_CONFIG_PATH="${TARGET_PKG_CONFIG_PATH}"
ENV TARGET_PKG_CONFIG_ALLOW_CROSS="1"
ENV RUSTFLAGS="-Copt-level=s -Clink-arg=-s"
ENV BINDGEN_EXTRA_CLANG_ARGS="-I/usr/include/${TARGET_ARCH}-linux-musl"

CMD ["/bin/bash"]
