#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --features encoding
cargo clippy --all-targets --features encoding -- -D warnings
