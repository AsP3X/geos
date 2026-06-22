#!/usr/bin/env bash
# Back up the Geos Postgres database to a timestamped dump (non-destructive).
set -euo pipefail
cd "$(dirname "$0")/.."

mkdir -p backups
stamp="$(date +%Y%m%d-%H%M%S)"
out="backups/geos-${stamp}.dump"

docker compose exec -T postgres pg_dump \
  -U "${POSTGRES_USER:-geos}" -d "${POSTGRES_DB:-geos}" -Fc > "$out"

echo "Backup written to $out"
