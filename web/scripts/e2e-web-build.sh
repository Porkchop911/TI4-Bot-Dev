#!/usr/bin/env bash
# Builds the production bundle for the smoke harness (TI4_E2E_WEB=build) and prints its directory.
# The output is cached per commit + uncommitted web/ changes + heartbeat setting, so repeated runs
# (and the nightly's slots) build once. The cache lives in web/.e2e-dist (gitignored), or in
# $TI4_E2E_WEB_CACHE. A build races safely: it goes to a temp dir and is renamed into place.
set -euo pipefail
cd "$(dirname "$0")/.."
hb="${VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS:-}"
key=$( { git rev-parse HEAD 2>/dev/null || echo nogit; git diff HEAD -- . 2>/dev/null; echo "hb=$hb"; } | sha1sum | cut -c1-16)
cache="${TI4_E2E_WEB_CACHE:-$PWD/.e2e-dist}"
out="$cache/$key"
if [ -f "$out/index.html" ]; then
  echo "== web build cached: $out" >&2
else
  mkdir -p "$cache"
  tmp="$cache/.building-$key-$$"
  start=$(date +%s)
  VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS="$hb" npx vite build --outDir "$tmp" --emptyOutDir --sourcemap >&2
  echo "web build: $(( $(date +%s) - start )) s" >&2
  if [ -f "$out/index.html" ]; then rm -rf "$tmp"; else rm -rf "$out"; mv "$tmp" "$out"; fi
  # Keep the cache small: drop builds older than a day.
  find "$cache" -mindepth 1 -maxdepth 1 -mtime +1 -exec rm -rf {} + 2>/dev/null || true
fi
echo "$out"
