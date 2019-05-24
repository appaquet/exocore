#!/usr/bin/env bash
set -e
CUR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd $CUR_DIR

. ./env.sh

if [[ ! -d "$CUR_DIR/openssl" ]]; then
    echo "Building openssl"
    ./build_openssl.sh
fi

if [[ ! -x "$(command -v cargo-apk)" ]]; then
    echo "cargo-apk needs to be installed"
    exit 1
fi

$ANDROID_SDK/tools/bin/sdkmanager "platform-tools" "platforms;android-21" "build-tools;26.0.1"

export OPENSSL_DIR=$CUR_DIR/openssl/target/arm
cargo-apk build --lib -p exocore-client-android --target arm-linux-androideabi

export OPENSSL_DIR=$CUR_DIR/openssl/target/aarch64
cargo-apk build --lib -p exocore-client-android --target aarch64-linux-android
