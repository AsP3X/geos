#!/usr/bin/env bash
# Export the canonical Event JSON Schema from geos-core and generate the
# matching TypeScript types in frontend/src/types/.
#
# Single source of truth: crates/core (Rust). Run this whenever the Event model
# changes (see .cursor/rules/canonical-event-schema.mdc). CI re-runs it and
# fails if the working tree changes (schema/TS drift).
set -euo pipefail
cd "$(dirname "$0")/.."

root="$(pwd)"
schema="$root/crates/core/schema/event.schema.json"
ts_out="$root/frontend/src/types/event.ts"

echo "==> Exporting Event JSON Schema -> $schema"
cargo run -q -p geos-core --bin export_schema > "$schema"

echo "==> Generating TypeScript types -> $ts_out"
mkdir -p "$(dirname "$ts_out")"
pnpm --filter geos-frontend exec json2ts \
  --input "$schema" \
  --output "$ts_out" \
  --bannerComment "/* AUTO-GENERATED from crates/core/schema/event.schema.json by scripts/gen-types.sh. DO NOT EDIT. */"

echo "Done. Event schema + TypeScript types regenerated."
