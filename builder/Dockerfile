# syntax=docker/dockerfile:1.4

FROM rust:alpine
RUN apk add bash diffutils cmake clang clang-dev gcc g++ linux-headers make openssl-dev openssl-libs-static perl pkgconfig yasm zlib-dev zlib-static
RUN <<eot
#!/bin/bash -ex

cat > /usr/local/bin/rustc_wrapper <<"EOF"
#!/bin/sh
RUSTC="$1"; shift
if [ "${CARGO_CRATE_NAME#build_script_}" != "$CARGO_CRATE_NAME" ]; then
    exec "$RUSTC" "$@" -C target-feature=-crt-static
else
    exec "$RUSTC" "$@"
fi
EOF
chmod +x /usr/local/bin/rustc_wrapper

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
cd /usr/local/lib/opencv4/3rdparty/
for i in liblib*.a; do
    mv -v "$i" "${i#lib}"
done

rm -rf /build
eot

SHELL ["/bin/bash", "-c"]

ENV OPENSSL_STATIC="1"
ENV OPENCV4_STATIC="1"
ENV FFMPEG_PKG_CONFIG_PATH="/usr/local/lib/pkgconfig"
ENV RUSTC_WRAPPER="/usr/local/bin/rustc_wrapper"
