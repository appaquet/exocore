#!/usr/bin/env bash
set -e
CUR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd $CUR_DIR/../

sudo docker run -it --rm --security-opt seccomp=unconfined -v "$PWD:/volume" xd009642/tarpaulin cargo tarpaulin --exclude-files=3rd --verbose --exclude-files="*_capnp.rs" --all --out Html -- --test-threads=1
