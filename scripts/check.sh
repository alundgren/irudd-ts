#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
for installation in providers/typescript7 research/t3code/semantic; do
  if [[ ! -d "$installation/node_modules" ]]; then
    printf 'Missing test dependencies. Run npm ci --prefix %s --ignore-scripts --no-audit --no-fund\n' "$installation" >&2
    exit 1
  fi
done
cargo fmt --all -- --check
cargo clippy --locked --all-targets --all-features -- -D warnings
cargo test --locked --all-features
node --test tests/mutator_sdk.test.ts tests/mutator_reporter.test.ts
python3 tests/release_test.py
python3 scripts/licenses.py
python3 scripts/semantic_licenses.py
python3 scripts/semantic_licenses.py --history
python3 scripts/check_docs.py
cargo run --locked -- check --config archguard.json
