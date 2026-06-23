#!/bin/sh
# Inject runtime frontend config before nginx starts so the API base URL can be
# set via GEOS_API_BASE_URL without rebuilding the SPA (e.g. behind NPM).
set -eu

API_URL="${GEOS_API_BASE_URL:-}"
# Escape for embedding in a JSON string value.
escaped=$(printf '%s' "$API_URL" | sed 's/\\/\\\\/g; s/"/\\"/g')

cat > /usr/share/nginx/html/config.js <<EOF
window.__GEOS_CONFIG__ = { apiBaseUrl: "${escaped}" };
EOF

exec nginx -g 'daemon off;'
