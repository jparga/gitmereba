#!/usr/bin/env bash
# Verificación completa del workspace. Falla al primer error.
set -euo pipefail
cd "$(dirname "$0")/.."

cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace --no-tests=pass
cargo deny check
cargo audit
