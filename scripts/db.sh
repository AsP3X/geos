#!/usr/bin/env bash
# Manage the local Postgres (PostGIS + pgvector) dev database via Docker Compose.
#
# Usage: scripts/db.sh <up|down|logs|psql>
#   up    Start the postgres service (no data loss).
#   down  Stop the postgres service WITHOUT removing volumes (data preserved).
#   logs  Tail postgres logs.
#   psql  Open an interactive psql shell.
#
# NOTE: This script never removes volumes. Destroying data (e.g. `down -v`) must
# be done explicitly and deliberately — see .cursor/rules/data-safety.mdc.
set -euo pipefail
cd "$(dirname "$0")/.."

cmd="${1:-up}"
case "$cmd" in
  up)   docker compose up -d postgres ;;
  down) docker compose stop postgres ;;
  logs) docker compose logs -f postgres ;;
  psql) docker compose exec postgres psql -U "${POSTGRES_USER:-geos}" -d "${POSTGRES_DB:-geos}" ;;
  *)    echo "Unknown command: $cmd"; echo "Usage: scripts/db.sh <up|down|logs|psql>"; exit 1 ;;
esac
