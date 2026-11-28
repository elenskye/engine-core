#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
# 测试不碰所有 App 共用的花钱账本（spend）
ENGINE_LEDGER_DIR=$(mktemp -d "${TMPDIR:-/tmp}/engine-ledger-check.XXXXXX")
export ENGINE_LEDGER_DIR
trap 'rm -rf "$ENGINE_LEDGER_DIR"' EXIT
cargo fmt --check
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --features encoding
cargo clippy --all-targets --features encoding -- -D warnings
cargo test --features clean
cargo clippy --all-targets --features clean -- -D warnings
