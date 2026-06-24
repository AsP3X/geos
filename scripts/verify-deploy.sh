#!/usr/bin/env bash
# Verify a Docker + NPM split-host deployment before debugging CORS in the browser.
#
# A 502 from NPM produces "No Access-Control-Allow-Origin" in the browser even
# when GEOS_CORS_ORIGINS is correct — fix connectivity first.
#
# Usage:
#   ./scripts/verify-deploy.sh
#   PUBLIC_API=https://api.example.com PUBLIC_ORIGIN=https://example.com ./scripts/verify-deploy.sh
set -euo pipefail
cd "$(dirname "$0")/.."

PUBLIC_API="${PUBLIC_API:-https://api.geos.corespace.de}"
PUBLIC_ORIGIN="${PUBLIC_ORIGIN:-https://geos.corespace.de}"
API_CONTAINER="${API_CONTAINER:-geos-api}"
PROXY_NETWORK="${PROXY_NETWORK:-proxy-network}"

fail=0
ok() { printf '  OK  %s\n' "$1"; }
bad() { printf '  FAIL %s\n' "$1"; fail=$((fail + 1)); }

echo "==> Geos deploy verification"
echo "    Public API:    ${PUBLIC_API}"
echo "    Public origin: ${PUBLIC_ORIGIN}"
echo

echo "==> Docker containers"
if docker ps --format '{{.Names}}\t{{.Status}}' | grep -q "^${API_CONTAINER}"; then
  ok "${API_CONTAINER} is running"
else
  bad "${API_CONTAINER} is not running (docker compose up -d api)"
fi

echo
echo "==> ${PROXY_NETWORK} membership"
if docker network inspect "$PROXY_NETWORK" >/dev/null 2>&1; then
  members="$(docker network inspect "$PROXY_NETWORK" --format '{{range .Containers}}{{.Name}} {{end}}' 2>/dev/null || true)"
  echo "    members: ${members:-<none>}"
  if [[ "$members" == *"${API_CONTAINER}"* ]]; then
    ok "${API_CONTAINER} is on ${PROXY_NETWORK}"
  else
    bad "${API_CONTAINER} is NOT on ${PROXY_NETWORK} — NPM cannot reach it by container name"
    echo "        Fix: use base docker-compose.yml (not docker-compose.local.yml)"
    echo "        Then: docker compose up -d api"
  fi
else
  bad "network ${PROXY_NETWORK} does not exist (docker network create ${PROXY_NETWORK})"
fi

echo
echo "==> API health (bypass NPM)"
local_code="$(curl -s -o /dev/null -w '%{http_code}' http://127.0.0.1:8080/health 2>/dev/null || echo 000)"
if [[ "$local_code" == "200" ]]; then
  ok "http://127.0.0.1:8080/health -> ${local_code}"
else
  bad "http://127.0.0.1:8080/health -> ${local_code} (check: docker logs ${API_CONTAINER})"
fi

if docker network inspect "$PROXY_NETWORK" >/dev/null 2>&1; then
  net_code="$(docker run --rm --network "$PROXY_NETWORK" curlimages/curl:8.5.0 \
    curl -s -o /dev/null -w '%{http_code}' "http://${API_CONTAINER}:8080/health" 2>/dev/null || echo 000)"
  if [[ "$net_code" == "200" ]]; then
    ok "http://${API_CONTAINER}:8080/health via ${PROXY_NETWORK} -> ${net_code}"
  else
    bad "http://${API_CONTAINER}:8080/health via ${PROXY_NETWORK} -> ${net_code}"
    echo "        NPM forward host should be: ${API_CONTAINER} port 8080 (HTTP)"
  fi
fi

echo
echo "==> Public API (through NPM)"
pub_code="$(curl -s -o /dev/null -w '%{http_code}' "${PUBLIC_API}/health" 2>/dev/null || echo 000)"
if [[ "$pub_code" == "200" ]]; then
  ok "${PUBLIC_API}/health -> ${pub_code}"
else
  bad "${PUBLIC_API}/health -> ${pub_code} (NPM cannot reach backend — fix proxy host before CORS)"
fi

echo
echo "==> CORS preflight (only meaningful when public health is 200)"
if [[ "$pub_code" == "200" ]]; then
  cors_header="$(curl -s -i -X OPTIONS "${PUBLIC_API}/api/v1/auth/login" \
    -H "Origin: ${PUBLIC_ORIGIN}" \
    -H "Access-Control-Request-Method: POST" \
    -H "Access-Control-Request-Headers: content-type" 2>/dev/null \
    | tr -d '\r' | grep -i '^access-control-allow-origin:' | head -1 || true)"
  if [[ -n "$cors_header" ]]; then
    ok "preflight: ${cors_header}"
  else
    bad "preflight missing Access-Control-Allow-Origin (set GEOS_CORS_ORIGINS=${PUBLIC_ORIGIN} and restart api)"
  fi
else
  echo "    skipped (fix 502 first)"
fi

echo
if [[ "$fail" -eq 0 ]]; then
  echo "All checks passed."
  exit 0
fi
echo "${fail} check(s) failed."
exit 1
