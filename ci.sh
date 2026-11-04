#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo fmt --check
cargo test --locked
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo build --locked
