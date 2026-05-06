#!/usr/bin/env bash
# Clean Nuxt dev caches and restart the dev server.
#
# Usage:
#   ./scripts/clean-frontend.sh             # clean + start (foreground)
#   ./scripts/clean-frontend.sh --no-start  # clean only
#
# Root-cause fix for the `Failed to resolve import "#app-manifest"` error
# landed via nuxt.config.ts `experimental.appManifest = false`. This script
# exists as an emergency workaround for any future virtual-module cache
# staleness (e.g. after upgrading Nuxt or adding a new module).

set -euo pipefail

cd "$(dirname "$0")/.."
FRONTEND_DIR="frontend"

if [[ ! -d "$FRONTEND_DIR" ]]; then
  echo "error: no frontend/ dir at $(pwd)"
  exit 1
fi

echo "==> Stopping any running Nuxt dev servers (port 3003 / 3000)"
# Match the specific Nuxt entrypoint to avoid killing unrelated Node procs.
pkill -f "nuxt-nitro.*dev" 2>/dev/null || true
pkill -f "nuxi.*dev" 2>/dev/null || true
# Give the OS a moment to release TCP ports.
sleep 1

echo "==> Removing Nuxt caches"
rm -rf \
  "$FRONTEND_DIR/.nuxt" \
  "$FRONTEND_DIR/.output" \
  "$FRONTEND_DIR/node_modules/.vite" \
  "$FRONTEND_DIR/node_modules/.cache"

if [[ "${1:-}" == "--no-start" ]]; then
  echo "==> Caches cleaned. Skipping start (--no-start)."
  exit 0
fi

echo "==> Starting Nuxt dev server"
cd "$FRONTEND_DIR"
exec npm run dev
