#!/usr/bin/env bash
set -e
CUR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

. ./env.sh

OPENSSL_VERSION="1.1.1b"
TARGET_DIR=$CUR_DIR/openssl/target

mkdir -p $CUR_DIR/openssl
cd $CUR_DIR/openssl

if [[ ! -d "openssl-$OPENSSL_VERSION" ]]; then
    curl https://www.openssl.org/source/openssl-$OPENSSL_VERSION.tar.gz | tar xz || exit 1
fi

cd $CUR_DIR/openssl/openssl-$OPENSSL_VERSION/
export INITIAL_PATH=$PATH

#
# See https://github.com/openssl/openssl/blob/master/NOTES.ANDROID
#

# Build for ARM
export PREFIX=$TARGET_DIR/arm
if [[ ! -d "$PREFIX" ]]; then
    mkdir -p $PREFIX
    export PATH=$ANDROID_NDK/toolchains/arm-linux-androideabi-4.9/prebuilt/linux-x86_64/bin:$INITIAL_PATH
    ./Configure android-arm -D__ANDROID_API__=14 --prefix=$PREFIX
    make -j12 clean
    make -j12
    make -j12 install
fi

# Build for ARM64
export PREFIX=$TARGET_DIR/aarch64
if [[ ! -d "$PREFIX" ]]; then
    mkdir -p $PREFIX
    export PATH=$ANDROID_NDK/toolchains/aarch64-linux-android-4.9/prebuilt/linux-x86_64/bin:$INITIAL_PATH
    ./Configure android-arm64 -D__ANDROID_API__=21 --prefix=$PREFIX
    make -j12 clean
    make -j12
    make -j12 install
fi
