#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
cargo build --locked
while IFS= read -r chip; do
    target/debug/cargo-embassy check --chip "$chip"
done < <(target/debug/cargo-embassy list-chips esp32)
