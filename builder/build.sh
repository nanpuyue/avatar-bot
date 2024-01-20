#!/bin/bash -ex

RLOTTIE_VERSION="d400087"
FFMPEG_VERSION="6.1.1"
LIBVPX_VERSION="1.14.0"

RLOTTIE_SRC="rlottie-${RLOTTIE_VERSION}.zip"
LIBVPX_SRC="libvpx-${LIBVPX_VERSION}.tar.gz"
FFMPEG_SRC="ffmpeg-${FFMPEG_VERSION}.tar.xz"

[ -d /build ] || mkdir -p /build
wget "https://codeload.github.com/Samsung/rlottie/zip/${RLOTTIE_VERSION}" -O "/build/${RLOTTIE_SRC}"
wget "https://github.com/webmproject/libvpx/archive/refs/tags/v${LIBVPX_VERSION}.tar.gz" -O "/build/${LIBVPX_SRC}"
wget "https://ffmpeg.org/releases/${FFMPEG_SRC}" -O "/build/${FFMPEG_SRC}"

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
