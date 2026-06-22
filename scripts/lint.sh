#!/usr/bin/env bash
# Run all Geos lint/format/type checks (mirrors lefthook + CI gates).
set -euo pipefail
cd "$(dirname "$0")/.."

echo "==> cargo fmt --check"
cargo fmt --all -- --check

echo "==> cargo clippy (-D warnings)"
cargo clippy --all-targets --all-features -- -D warnings

echo "==> frontend eslint"
pnpm --filter geos-frontend lint

echo "==> frontend tsc --noEmit"
pnpm --filter geos-frontend typecheck

echo "All checks passed."
