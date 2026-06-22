#!/usr/bin/env bash
# Export the canonical Event JSON Schema from geos-core and generate the
# matching TypeScript types in frontend/src/types/.
#
# SCAFFOLD STUB: the JSON Schema exporter and TS generation are implemented by
# the core-schema work (see .cursor/rules/canonical-event-schema.mdc). This
# script exists so the workflow and CI drift-check have a stable entry point.
set -euo pipefail
cd "$(dirname "$0")/.."

echo "gen-types: not yet implemented — added with the canonical Event schema." >&2
exit 1
