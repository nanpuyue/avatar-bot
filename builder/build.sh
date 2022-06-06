#!/bin/bash -e

export CC="musl-gcc"
export MUSL_DIR="/opt/musl"
export CFLAGS="-I${MUSL_DIR}/include"
export LDFLAGS="-L${MUSL_DIR}/lib"

ZLIB_VERSION="1.2.12"
OPENSSL_VERSION="1.1.1l"
FFMPEG_VERSION="4.4"
VPX_VERSION="1.11.0"

ZLIB_SRC="zlib-${ZLIB_VERSION}.tar.xz"
OPENSSL_SRC="openssl-${OPENSSL_VERSION}.tar.gz"
FFMPEG_SRC="ffmpeg-${FFMPEG_VERSION}.tar.xz"
VPX_SRC="libvpx-${VPX_VERSION}.tar.gz"

apt update
apt install -y --no-install-recommends g++ make musl{,-dev,-tools} nasm pkg-config wget xz-utils zstd

[ -d /build ] || mkdir -p /build
wget "https://zlib.net/${ZLIB_SRC}" -O "/build/${ZLIB_SRC}"
wget "https://www.openssl.org/source/${OPENSSL_SRC}" -O "/build/${OPENSSL_SRC}"
wget "https://ffmpeg.org/releases/${FFMPEG_SRC}" -O "/build/${FFMPEG_SRC}"
wget "https://github.com/webmproject/libvpx/archive/refs/tags/v${VPX_VERSION}.tar.gz" -O "/build/${VPX_SRC}"

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

tar -C /build -xf "/build/${VPX_SRC}"
cd "/build/libvpx-${VPX_VERSION}"
./configure --prefix="${MUSL_DIR}" --disable-unit-tests
make -j$(nproc)
make install

tar -C /build -xf "/build/${FFMPEG_SRC}"
cd "/build/ffmpeg-${FFMPEG_VERSION}"
./configure --cc="${CC}" --prefix="${MUSL_DIR}" --enable-gpl --enable-nonfree --enable-zlib\
 --enable-libvpx --disable-programs
make -j$(nproc) install
