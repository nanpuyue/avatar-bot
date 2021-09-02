FROM rust:latest as builder
ADD build.sh /
RUN /build.sh

FROM rust:latest
COPY --from=builder /opt/musl /opt/musl
RUN apt update && apt install -y musl musl-dev musl-tools libclang-dev && apt clean &&\
 rustup target add x86_64-unknown-linux-musl

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
ENV BINDGEN_EXTRA_CLANG_ARGS="-I/usr/include/x86_64-linux-musl"

CMD ["/bin/bash"]
