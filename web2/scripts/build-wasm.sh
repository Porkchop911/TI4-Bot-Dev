#!/usr/bin/env bash
# Builds the engine for local play and puts it where web2 imports it.
#   build-wasm.sh         the release build: small and fast in the page. A change of the engine
#                         builds the engine again from the start (30 s to minutes).
#   build-wasm.sh --dev   not optimized, incremental: seconds after a change. For work on the
#                         page; build the release one before the full e2e and before a deploy.
# Needs: rustup target add wasm32-unknown-unknown
set -euo pipefail
cd "$(dirname "$0")/../.."
profile=wasm-release
case "${1:-}" in
  "") ;;
  --dev) profile=wasm-dev ;;
  *) echo "usage: $0 [--dev]" >&2; exit 2 ;;
esac
cargo build -p ti4-wasm --target wasm32-unknown-unknown --profile "$profile"
cp "target/wasm32-unknown-unknown/$profile/ti4_wasm.wasm" web2/src/session/ti4.wasm
ls -l web2/src/session/ti4.wasm
