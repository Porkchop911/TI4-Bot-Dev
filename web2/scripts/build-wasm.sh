#!/usr/bin/env bash
# Builds the engine for local play and puts it where web2 imports it. The build is the shared one,
# web-shared/scripts/build-wasm.mjs (with TI4_REQUIRE_WASM=1: a failed build is an error).
#   build-wasm.sh         the release build: small and fast in the page. A change of the engine
#                         builds the engine again from the start (30 s to minutes).
#   build-wasm.sh --dev   not optimized, incremental: seconds after a change. For work on the
#                         page; build the release one before the full e2e and before a deploy.
# Needs: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")/../.."
case "${1:-}" in
  "" | --dev) ;;
  *) echo "usage: $0 [--dev]" >&2; exit 2 ;;
esac
TI4_REQUIRE_WASM=1 node web-shared/scripts/build-wasm.mjs web2/src/session/ti4.wasm "$@"
