#!/usr/bin/env bash
# Initialize / update the nebular-os object-storage submodule.
set -euo pipefail
cd "$(dirname "$0")/.."

git submodule update --init --recursive
echo "nebular-os submodule initialized."
