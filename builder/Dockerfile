# syntax=docker/dockerfile:1.4

FROM alpine AS alpine-amd64
ARG RUSTARCH=x86_64

FROM rust:slim AS rust-amd64
ARG RUSTARCH=x86_64

FROM alpine AS alpine-arm64
ARG RUSTARCH=aarch64

FROM rust:slim AS rust-arm64
ARG RUSTARCH=aarch64

FROM alpine-$TARGETARCH AS builder
RUN apk add bash diffutils cmake gcc g++ linux-headers make openssl-dev openssl-libs-static perl pkgconfig yasm zlib-dev zlib-static
RUN <<eot
#!/bin/bash -ex

RLOTTIE_VERSION="d400087"
FFMPEG_VERSION="6.1.1"
LIBVPX_VERSION="1.14.0"
OPENCV_VERSION="4.9.0"

RLOTTIE_SRC="rlottie-${RLOTTIE_VERSION}.zip"
LIBVPX_SRC="libvpx-${LIBVPX_VERSION}.tar.gz"
FFMPEG_SRC="ffmpeg-${FFMPEG_VERSION}.tar.xz"
OPENCV_SRC="opencv-${OPENCV_VERSION}.tar.gz"

[ -d /build ] || mkdir -p /build
wget "https://codeload.github.com/Samsung/rlottie/zip/${RLOTTIE_VERSION}" -O "/build/${RLOTTIE_SRC}"
wget "https://github.com/webmproject/libvpx/archive/refs/tags/v${LIBVPX_VERSION}.tar.gz" -O "/build/${LIBVPX_SRC}"
wget "https://ffmpeg.org/releases/${FFMPEG_SRC}" -O "/build/${FFMPEG_SRC}"
wget "https://github.com/opencv/opencv/archive/refs/tags/${OPENCV_VERSION}.tar.gz" -O "/build/${OPENCV_SRC}"

unzip -d /build -o "/build/${RLOTTIE_SRC}"
cd "/build/rlottie-${RLOTTIE_VERSION}"
sed -ri 's/(-lrlottie)/\1 -lstdc++/' rlottie.pc.in
mkdir -p build && cd build
cmake -DBUILD_SHARED_LIBS=OFF -DCMAKE_INSTALL_PREFIX=/usr/local -DLIB_INSTALL_DIR="/usr/local/lib" ..
make -j$(nproc) install

tar -C /build -xf "/build/${LIBVPX_SRC}"
cd "/build/libvpx-${LIBVPX_VERSION}"
./configure --prefix=/usr/local --disable-unit-tests
make -j$(nproc) install

tar -C /build -xf "/build/${FFMPEG_SRC}"
cd "/build/ffmpeg-${FFMPEG_VERSION}"
./configure --prefix=/usr/local --enable-gpl --enable-nonfree --enable-zlib\
 --enable-libvpx --disable-programs
make -j$(nproc) install

tar -C /build -xf "/build/${OPENCV_SRC}"
cd "/build/opencv-${OPENCV_VERSION}"
mkdir -p build && cd build
cmake -DCMAKE_INSTALL_PREFIX=/usr/local\
 -DBUILD_SHARED_LIBS=OFF\
 -DBUILD_opencv_apps=OFF\
 -DBUILD_opencv_python2=OFF\
 -DBUILD_ITT=OFF\
 -DWITH_ITT=OFF\
 -DWITH_FFMPEG=ON\
 -DBUILD_LIST=core,imgcodecs,imgproc,objdetect\
 -DOPENCV_GENERATE_PKGCONFIG=ON ..
make -j$(nproc) install

rm -rf /build
eot

FROM rust-$TARGETARCH
SHELL ["/bin/bash", "-c"]
COPY --from=builder / /musl
RUN <<eot
#!/bin/bash -ex

apt update
apt install -y --no-install-recommends make musl{,-dev,-tools} pkg-config clang libclang-dev

apt clean
rustup target add "$RUSTARCH-unknown-linux-musl"

cat >/usr/bin/musl-g++ <<EOF
#!/bin/sh
export LD_LIBRARY_PATH=/musl/lib:/musl/usr/lib
exec /musl/usr/bin/$RUSTARCH-alpine-linux-musl-g++ "\$@" --sysroot=/musl
EOF
chmod +x /usr/bin/musl-g++
eot

ENV CC="musl-gcc"
ENV CXX="musl-g++"
ENV MUSL_DIR="/musl"
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
ENV OPENCV_LINK_LIBS="opencv_objdetect,opencv_imgcodecs,opencv_imgproc,opencv_core,IlmImf,liblibwebp,liblibtiff,liblibjpeg-turbo,liblibpng,liblibopenjp2,tegra_hal"
ENV OPENCV_LINK_PATHS="${MUSL_DIR}/usr/local/lib,${MUSL_DIR}/usr/local/lib/opencv4/3rdparty"
