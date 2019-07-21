#!/usr/bin/env bash
set -eu
CUR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd $CUR_DIR/../
COVERAGE_DIR="$CUR_DIR/../coverage"
OUTPUT=${1:-Html}

export CARGO_OPTIONS="--all --all-features --exclude=exocore-cli --exclude=exocore-client-wasm --exclude=exocore-client-android"
export CARGO_INCREMENTAL=0
export RUSTFLAGS="-Zprofile -Ccodegen-units=1 -Cinline-threshold=0 -Clink-dead-code -Coverflow-checks=off -Zno-landing-pads"
cargo +nightly build --verbose $CARGO_OPTIONS
cargo +nightly test --verbose $CARGO_OPTIONS

mkdir -p $COVERAGE_DIR
zip -0 $COVERAGE_DIR/ccov.zip `find . \( -name "*exocore*.gc*" \) -print`;

# Create HTML report
grcov $COVERAGE_DIR/ccov.zip -s . -t lcov --llvm --branch -o $COVERAGE_DIR/lcov.info \
	--ignore-not-existing \
	--ignore-dir "clients/*" \
	--ignore-dir "cli/*" \
	--ignore-dir "/*" \
	--ignore-dir "common/src/protos/*"

if [[ "$OUTPUT" == "Html" ]]; then
	genhtml -o $COVERAGE_DIR/ --show-details --highlight --ignore-errors source --legend $COVERAGE_DIR/lcov.info
else
	bash <(curl -s https://codecov.io/bash) -f $COVERAGE_DIR/lcov.info;
fi
