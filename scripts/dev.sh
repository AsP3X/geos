#!/usr/bin/env bash
# Start the Geos dev stack: data services in Docker, app services locally.
#
# Brings up postgres + meilisearch + object-storage via Docker Compose, then
# runs the API, workers, and frontend on the host for fast iteration.
set -euo pipefail
cd "$(dirname "$0")/.."

if [ ! -f .env ]; then
  echo "No .env found — run ./scripts/gen-env.sh first." >&2
  exit 1
fi

echo "==> Starting data services (postgres, meilisearch, object-storage)"
docker compose up -d postgres meilisearch object-storage

echo "==> Launching app services (Ctrl-C to stop)"
( cargo run --bin geos-api ) &
api_pid=$!
( cargo run --bin geos-workers ) &
workers_pid=$!
( pnpm --filter geos-frontend dev ) &
fe_pid=$!

trap 'kill "$api_pid" "$workers_pid" "$fe_pid" 2>/dev/null || true' INT TERM
wait
