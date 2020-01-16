#!/usr/bin/env bash
set -eu
CUR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd $CUR_DIR/../
COVERAGE_DIR="$CUR_DIR/../coverage"
OUTPUT=${1:-Html}

##
## See https://github.com/mozilla/grcov#grcov-with-travis
##

if [[ -d $CUR_DIR/../target ]]; then
  find $CUR_DIR/../target -name "*.gc*" -delete
fi

export RUSTUP_TOOLCHAIN=nightly-2019-11-14 # if changed, change in push-tester.yml
export CARGO_OPTIONS="--all --all-features --exclude=exo --exclude=exocore-client-wasm --exclude=exocore-client-android --exclude=exocore-client-ios"
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Cinline-threshold=0 -Clink-dead-code -Coverflow-checks=off -Zno-landing-pads"
cargo clean -p exocore-schema
cargo build $CARGO_OPTIONS
cargo test $CARGO_OPTIONS

mkdir -p $COVERAGE_DIR
zip -0 $COVERAGE_DIR/ccov.zip `find . \( -name "*exocore*.gc*" \) -print`;

grcov $COVERAGE_DIR/ccov.zip -s . -t lcov --llvm -o $COVERAGE_DIR/lcov.info \
	--ignore-not-existing \
	--ignore "clients/*" \
	--ignore "cli/*" \
	--ignore "/*" \
	--ignore "common/src/protos/*"

if [[ "$OUTPUT" == "Html" ]]; then
  # genhtml is provided by `lcov` package
	genhtml -o $COVERAGE_DIR/ --show-details --highlight --ignore-errors source --legend $COVERAGE_DIR/lcov.info
else
	bash <(curl -s https://codecov.io/bash) -f $COVERAGE_DIR/lcov.info;
fi
