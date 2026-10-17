#!/usr/bin/env bash
# Builds the offline bundle (the wasm engine plus the web app, `vite build`) of a checkout and prints
# the directory of the result. Nothing is rebuilt when web/ and crates/ are unchanged: the result is
# cached under $OFFLINE_CACHE keyed by the git tree of web/ (and the engine file by the tree of
# crates/, Cargo.toml and the content of Cargo.lock), so a night's batches only build after a fixer merge.
#
#   offline_build.sh [<checkout>]       prints <cache>/dist-<key>; exit 1 with the reason on stderr
#
#   TI4_WASM_FILE=<file>   use this prebuilt engine instead of `cargo build -p ti4-wasm` (the key
#                          then includes its sha-256 prefix)
#   NIGHTLY_OFFLINE_CACHE, NIGHTLY_OFFLINE_TARGET   see config.sh
#
# The checkout is read but never changed, except for web/src/offline/local/engine/ti4_wasm.wasm,
# the git-ignored copy `scripts/build-wasm.mjs` always makes.
set -uo pipefail
source "$(dirname "$0")/config.sh"

repo="${1:-$GAME_REPO}"
mkdir -p "$OFFLINE_CACHE"
exec 7>"$OFFLINE_CACHE/.build.lock"
flock 7

# The git tree of the working tree (uncommitted and untracked files included, ignored ones not),
# limited to what the bundle depends on, through a throwaway index.
tree_of() {
  local index tree
  index=$(mktemp "$OFFLINE_CACHE/.index.XXXXXX")
  rm -f "$index"
  tree=$(cd "$repo" && GIT_INDEX_FILE="$index" bash -c 'git add -A -- web crates Cargo.toml 2>/dev/null && git write-tree')
  rm -f "$index"
  printf '%s' "$tree"
}
tree=$(tree_of)
[ -n "$tree" ] || { echo "offline_build: cannot read the git tree of $repo" >&2; exit 1; }
sub() { git -C "$repo" rev-parse "$tree:$1" 2>/dev/null || echo none; }

engine_key=$(printf '%s %s %s' "$(sub crates)" "$(sub Cargo.toml)" "$(sha1sum < "$repo/Cargo.lock" 2>/dev/null | cut -c1-40)" | sha1sum | cut -c1-12)
wasm="${TI4_WASM_FILE:-}"
if [ -z "$wasm" ]; then
  wasm="$OFFLINE_CACHE/engine-$engine_key.wasm"
else
  engine_key="file-$(sha256sum "$wasm" | cut -c1-12)"
fi
key="$(printf '%s' "$tree" | cut -c1-12)-$(printf '%s' "$engine_key" | cut -c1-12)"
dist="$OFFLINE_CACHE/dist-$key"
if [ -f "$dist/.ok" ]; then echo "$dist"; exit 0; fi

if [ ! -s "$wasm" ]; then
  echo "offline_build: building the engine (cargo, target $OFFLINE_TARGET)" >&2
  (cd "$repo" && CARGO_TARGET_DIR="$OFFLINE_TARGET" cargo build --quiet -p ti4-wasm --target wasm32-unknown-unknown --profile wasm-release) >&2 \
    || { echo "offline_build: cargo build of ti4-wasm failed" >&2; exit 1; }
  cp "$OFFLINE_TARGET/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm" "$wasm.tmp" && mv "$wasm.tmp" "$wasm" \
    || { echo "offline_build: the engine file is missing after the build" >&2; exit 1; }
fi
echo "offline_build: bundling $key" >&2
(cd "$repo/web" && TI4_REQUIRE_WASM=1 TI4_WASM_FILE="$wasm" node scripts/build-wasm.mjs \
  && npx vite build --outDir "$dist" --emptyOutDir) >&2 \
  || { echo "offline_build: vite build failed" >&2; rm -rf "$dist"; exit 1; }
ls "$dist"/assets/*.wasm > /dev/null 2>&1 || { echo "offline_build: the bundle has no engine file" >&2; rm -rf "$dist"; exit 1; }
touch "$dist/.ok"
# Keep the three newest bundles and engine files.
ls -1dt "$OFFLINE_CACHE"/dist-* 2>/dev/null | tail -n +4 | xargs -r rm -rf
ls -1t "$OFFLINE_CACHE"/engine-*.wasm 2>/dev/null | tail -n +4 | xargs -r rm -f
echo "$dist"
