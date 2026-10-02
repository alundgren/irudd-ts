#!/usr/bin/env bash
set -euo pipefail
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
python3 scripts/licenses.py
cargo run --locked -- check --config archguard.json
