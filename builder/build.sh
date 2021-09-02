#!/bin/bash -e

export CC="musl-gcc"
export MUSL_DIR="/opt/musl"
export CFLAGS="-I${MUSL_DIR}/include"
export LDFLAGS="-L${MUSL_DIR}/lib"

ZLIB_VERSION="1.2.11"
OPENSSL_VERSION="1.1.1l"
FFMPEG_VERSION="4.4"

ZLIB_SRC="zlib-${ZLIB_VERSION}.tar.xz"
OPENSSL_SRC="openssl-${OPENSSL_VERSION}.tar.gz"
FFMPEG_SRC="ffmpeg-${FFMPEG_VERSION}.tar.xz"

[ -d /build ] || mkdir -p /build
wget "https://zlib.net/${ZLIB_SRC}" -O "/build/${ZLIB_SRC}"
wget "https://www.openssl.org/source/${OPENSSL_SRC}" -O "/build/${OPENSSL_SRC}"
wget "https://ffmpeg.org/releases/${FFMPEG_SRC}" -O "/build/${FFMPEG_SRC}"

apt update
apt install -y musl musl-dev musl-tools zstd nasm

wget "https://archlinux.org/packages/community/x86_64/kernel-headers-musl/download" -O "/build/kernel-headers-musl.tar.zst"
[ -d "${MUSL_DIR}" ] || mkdir -p "${MUSL_DIR}"
tar -C "${MUSL_DIR}" -xf "/build/kernel-headers-musl.tar.zst" --transform 's|usr/lib/musl||' usr/lib/musl

tar -C /build -xf "/build/${ZLIB_SRC}"
cd "/build/zlib-${ZLIB_VERSION}"
./configure --prefix="${MUSL_DIR}" --static --64
make -j$(nproc) install

tar -C /build -xf "/build/${OPENSSL_SRC}"
cd "/build/openssl-${OPENSSL_VERSION}"
./Configure --prefix="${MUSL_DIR}" --openssldir=/etc/ssl --libdir=lib linux-x86_64
make -j$(nproc) install_dev

tar -C /build -xf "/build/${FFMPEG_SRC}"
cd "/build/ffmpeg-${FFMPEG_VERSION}"
./configure --cc="${CC}" --prefix="${MUSL_DIR}" --enable-gpl --enable-nonfree --enable-zlib --disable-programs
make -j$(nproc) install
