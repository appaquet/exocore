#!/usr/bin/env bash
set -ex
CUR_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

cd "$CUR_DIR/.."

export EXOCORE_ROOT="$CUR_DIR/../"

echo "Formatting..."
./format.sh

echo "Cargo checking code, tests and benches"
cargo check --workspace --tests --benches --all-features

echo "Running tests..."
cargo test --workspace --all-features

echo "Running clippy..."
./clippy.sh

echo "Validating web compilation for exocore-client-web"
cd $EXOCORE_ROOT/clients/web
cargo clippy --target "wasm32-unknown-unknown"

echo "Validating exo compilation"
cd $EXOCORE_ROOT/exo
cargo check
