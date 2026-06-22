#!/usr/bin/env bash
# Restore a Geos Postgres dump created by db-backup.sh.
#
# Usage: scripts/db-restore.sh <path-to-dump>
#
# WARNING: pg_restore with --clean drops and recreates objects in the target
# database. Only run this against a database you intend to overwrite, and after
# taking a fresh backup. See .cursor/rules/data-safety.mdc.
set -euo pipefail
cd "$(dirname "$0")/.."

dump="${1:-}"
if [ -z "$dump" ] || [ ! -f "$dump" ]; then
  echo "Usage: scripts/db-restore.sh <path-to-dump>" >&2
  exit 1
fi

read -r -p "This will overwrite database '${POSTGRES_DB:-geos}'. Type the DB name to confirm: " confirm
if [ "$confirm" != "${POSTGRES_DB:-geos}" ]; then
  echo "Confirmation did not match. Aborting." >&2
  exit 1
fi

docker compose exec -T postgres pg_restore \
  -U "${POSTGRES_USER:-geos}" -d "${POSTGRES_DB:-geos}" --clean --if-exists < "$dump"

echo "Restore complete from $dump"
