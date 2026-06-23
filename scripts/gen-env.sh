#!/usr/bin/env bash
# Bootstrap `.env` from `.env.example` and generate random secrets for keys
# that still use placeholder values (32+ chars where required by Geos/nebular-os).
#
# Usage:
#   ./scripts/gen-env.sh           # create .env if missing; fill placeholders only
#   ./scripts/gen-env.sh --force   # regenerate all secret keys (overwrites existing)
set -euo pipefail
cd "$(dirname "$0")/.."

ENV_FILE=".env"
EXAMPLE_FILE=".env.example"
FORCE=false

for arg in "$@"; do
  case "$arg" in
    --force | -f)
      FORCE=true
      ;;
    -h | --help)
      cat <<'EOF'
Usage: ./scripts/gen-env.sh [--force]

Creates .env from .env.example when missing, then fills secret keys with
openssl-generated values (64 hex chars each).

Keys: MEILI_MASTER_KEY, NOS_JWT_SECRET, NOS_SIGNING_SECRET, JWT_SECRET

Without --force, existing non-placeholder values are kept.
With --force, all secret keys are replaced (use when rotating credentials).
EOF
      exit 0
      ;;
    *)
      echo "Unknown option: $arg (try --help)" >&2
      exit 1
      ;;
  esac
done

SECRET_KEYS=(
  MEILI_MASTER_KEY
  NOS_JWT_SECRET
  NOS_SIGNING_SECRET
  JWT_SECRET
)

# Human: Values that mean "not configured yet" and should be replaced by gen-env.
# Agent: MATCHES placeholder literals from .env.example and Compose fallbacks.
is_placeholder() {
  case "$1" in
    "" | change-me-to-a-32+char-random-string | masterKeyChangeMe32CharsMinimum!)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

gen_secret() {
  if ! command -v openssl >/dev/null 2>&1; then
    echo "openssl is required to generate secrets" >&2
    exit 1
  fi
  openssl rand -hex 32
}

# Human: Return the value for KEY= from an env file, or empty when absent.
# Agent: GREP ^KEY=; STRIPS prefix; RETURNS last match or empty.
read_env_key() {
  local key="$1"
  local file="$2"
  local line
  if [[ ! -f "$file" ]]; then
    return 0
  fi
  line="$(grep -E "^${key}=" "$file" 2>/dev/null | tail -1 || true)"
  if [[ -z "$line" ]]; then
    return 0
  fi
  printf '%s' "${line#*=}"
}

# Human: Set or replace KEY=value in an env file, preserving all other lines.
# Agent: REWRITES file via temp; REPLACES first ^KEY= line or APPENDS if missing.
set_env_key() {
  local key="$1"
  local value="$2"
  local file="$3"
  local tmp found=false
  tmp="$(mktemp)"
  if [[ -f "$file" ]]; then
    while IFS= read -r line || [[ -n "$line" ]]; do
      if [[ "$line" == "${key}="* ]]; then
        printf '%s=%s\n' "$key" "$value" >>"$tmp"
        found=true
      else
        printf '%s\n' "$line" >>"$tmp"
      fi
    done <"$file"
  fi
  if [[ "$found" == false ]]; then
    printf '%s=%s\n' "$key" "$value" >>"$tmp"
  fi
  mv "$tmp" "$file"
}

if [[ ! -f "$EXAMPLE_FILE" ]]; then
  echo "Missing ${EXAMPLE_FILE} — cannot bootstrap .env" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  cp "$EXAMPLE_FILE" "$ENV_FILE"
  echo "Created ${ENV_FILE} from ${EXAMPLE_FILE}"
fi

updated=0
for key in "${SECRET_KEYS[@]}"; do
  current="$(read_env_key "$key" "$ENV_FILE")"
  if [[ "$FORCE" == true ]] || is_placeholder "$current"; then
    secret="$(gen_secret)"
    set_env_key "$key" "$secret" "$ENV_FILE"
    echo "Set ${key}"
    updated=$((updated + 1))
  else
    echo "Kept ${key} (already configured)"
  fi
done

if [[ "$updated" -eq 0 ]]; then
  echo "No secret keys updated. Use --force to regenerate all secrets."
else
  echo "Updated ${updated} secret key(s) in ${ENV_FILE}."
fi

echo "Review ${ENV_FILE} for non-secret settings (ports, GEOS_CORS_ORIGINS, GEOS_API_BASE_URL, etc.)."
