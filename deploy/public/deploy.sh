#!/usr/bin/env bash
# Build and release the public playtest server. Run as a normal user with sudo rights on the
# server, from a checkout of the repo (it is NOT run by CI and nothing here is installed for you).
#
#   deploy/public/deploy.sh                          # build origin/main, check, release, restart
#   deploy/public/deploy.sh --ref <ref>              # build any git ref (branch, tag, sha)
#   deploy/public/deploy.sh --ref <ref> --allow-failed <game_id>   # ... knowing this saved game
#                                                      cannot be loaded by the new build (repeatable)
#   deploy/public/deploy.sh --ref <ref> --preflight-only   # build + persistence check, stop there
#   deploy/public/deploy.sh rollback                 # point `current` back at the previous release
#
# The ref is built in a TEMPORARY git worktree. The checkout you run this from is never switched,
# pulled or reset (the nightly sweep and other agents use it).
#
# Before anything is switched, the new server binary is started on a COPY of the live data dir and
# must recover every saved game (recovery-check.sh). After the restart the recovery line of the
# live service is read again; if it shows failures the preflight did not allow, the previous
# release is restored automatically.
#
# The MLP models: when out/models/index.json exists in the checkout this runs from (written by
# scripts/pack-models.sh; TI4_PUBLIC_MODELS names another directory), the pre-flight loads them and
# they replace the live ones in TI4_MODEL_DIR of the env file (default /var/lib/ti4-public/models).
# Without it the live models stay.
#
# Layout:  /opt/ti4-public/releases/<UTC timestamp>/{server,dist/,COMMIT}
#          /opt/ti4-public/current  -> releases/<newest>        (atomic symlink swap)
#          /opt/ti4-public/previous -> releases/<one before>    (rollback target)
#          /var/lib/ti4-public/models{,.previous}               (the models of the last two deploys)
set -euo pipefail

ROOT="${TI4_PUBLIC_ROOT:-/opt/ti4-public}"
SERVICE="${TI4_PUBLIC_SERVICE:-ti4-public}"
BRANCH="${TI4_PUBLIC_BRANCH:-main}"
KEEP="${TI4_PUBLIC_KEEP_RELEASES:-4}"
ENV_FILE="${TI4_PUBLIC_ENV_FILE:-/etc/ti4-public/env}"
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
HERE="$REPO/deploy/public"
BUILD_BASE="${TI4_PUBLIC_BUILD_DIR:-$HOME/.cache/ti4-public-build}"
MODELS_SRC="$(realpath -m "${TI4_PUBLIC_MODELS:-$REPO/out/models}")"
# Own target dir so this never fights with agent builds in the shared checkout.
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$HOME/.cache/ti4-public-target}"

REF=""
PREFLIGHT_ONLY=0
ALLOW_FAILED=()
MODE=deploy
while [[ $# -gt 0 ]]; do
  case "$1" in
    rollback) MODE=rollback; shift ;;
    --ref) REF="${2:?--ref needs a git ref}"; shift 2 ;;
    --allow-failed) ALLOW_FAILED+=("${2:?--allow-failed needs a game id}"); shift 2 ;;
    --preflight-only) PREFLIGHT_ONLY=1; shift ;;
    -h|--help) sed -n '2,30p' "${BASH_SOURCE[0]}"; exit 0 ;;
    *) echo "unknown argument: $1 (see --help)" >&2; exit 2 ;;
  esac
done

say() { printf '\n== %s\n' "$*"; }

swap_current() { # $1 = release dir
  local new="$1"
  sudo ln -sfn "$new" "$ROOT/current.new"
  sudo mv -T "$ROOT/current.new" "$ROOT/current"
}

env_value() { # $1 = key, $2 = default; reads the service env file (root:ti4, mode 640)
  local v
  v="$(sudo grep -E "^$1=" "$ENV_FILE" 2>/dev/null | tail -1 | cut -d= -f2- || true)"
  echo "${v:-$2}"
}

health_check() {
  local port
  port="$(env_value PORT 18200)"
  for _ in $(seq 1 90); do
    if curl -fsS "http://127.0.0.1:${port}/health" >/dev/null 2>&1; then
      curl -fsS "http://127.0.0.1:${port}/health"; echo
      return 0
    fi
    sleep 2
  done
  return 1
}

MODELS_SWAPPED=0
LIVE_MODELS=""
restore_models() { # the models of the deploy before, when this one replaced them
  if (( MODELS_SWAPPED )) && sudo test -d "$LIVE_MODELS.previous"; then
    sudo systemctl stop "$SERVICE"
    sudo rm -rf "$LIVE_MODELS"
    sudo mv -T "$LIVE_MODELS.previous" "$LIVE_MODELS"
    MODELS_SWAPPED=0
    echo "the previous models are back in $LIVE_MODELS"
  fi
}

restore_previous() { # $1 = release dir to go back to
  restore_models
  if [[ -n "$1" ]]; then
    swap_current "$1"
    sudo systemctl restart "$SERVICE"
    health_check && echo "previous release is live again" \
      || echo "health check FAILED after the automatic rollback: look at journalctl -u $SERVICE" >&2
  else
    echo "no previous release to go back to" >&2
  fi
}

if [[ "$MODE" == "rollback" ]]; then
  [[ -L "$ROOT/previous" ]] || { echo "no previous release to roll back to" >&2; exit 1; }
  target="$(readlink -f "$ROOT/previous")"
  say "Rolling back to $target"
  current="$(readlink -f "$ROOT/current")"
  swap_current "$target"
  sudo ln -sfn "$current" "$ROOT/previous"
  sudo systemctl restart "$SERVICE"
  health_check && echo "rolled back" || { echo "health check failed after rollback" >&2; exit 1; }
  exit 0
fi

# ---- 1. Resolve the ref (the checkout we run from is not touched) ------------------------------
cd "$REPO"
if [[ -z "$REF" ]]; then
  say "Fetching origin/$BRANCH"
  git fetch origin "$BRANCH"
  REF="origin/$BRANCH"
elif ! git rev-parse --verify --quiet "$REF^{commit}" >/dev/null; then
  say "Fetching $REF from origin"
  git fetch origin "$REF"
  REF="FETCH_HEAD"
fi
FULL_COMMIT="$(git rev-parse --verify "$REF^{commit}")"
COMMIT="${FULL_COMMIT:0:8}"
export TI4_BUILD_COMMIT="$COMMIT"
STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
RELEASE="$ROOT/releases/${STAMP}-${COMMIT}"
echo "building $REF = $FULL_COMMIT"

# ---- 2. Temporary worktree --------------------------------------------------------------------
mkdir -p "$BUILD_BASE"
WT="$(mktemp -d "$BUILD_BASE/wt-${COMMIT}.XXXXXX")"
cleanup() {
  cd "$REPO" 2>/dev/null || true
  git worktree remove --force "$WT" >/dev/null 2>&1 || rm -rf "$WT"
  git worktree prune >/dev/null 2>&1 || true
}
trap cleanup EXIT
git worktree add --detach "$WT" "$FULL_COMMIT" >/dev/null
cd "$WT"

# ---- 3. Build: wasm engine, web client, server ------------------------------------------------
say "Building the web client ($COMMIT)"
if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-unknown-unknown   # rust-toolchain.toml lists it; this makes it explicit
fi
# web/scripts/build-wasm.mjs (`npm run build:wasm`) runs the shared build of both apps,
# web-shared/scripts/build-wasm.mjs: it builds crates/ti4-wasm and copies the engine to
# web/src/offline/local/engine/, where vite finds it. `npm run build` runs it as its `prebuild`
# hook. TI4_REQUIRE_WASM=1 turns "cargo or the wasm32 target is missing" into an error: without it
# the script only warns and the release would silently have no "Play offline".
HAS_WASM_SCRIPT=0
if node -e 'process.exit(require("./web/package.json").scripts?.["build:wasm"] ? 0 : 1)'; then
  HAS_WASM_SCRIPT=1
  export TI4_REQUIRE_WASM=1
fi
(
  cd web
  npm ci
  # A ref that has the script but does not run it from `build` (no prebuild hook): run it first.
  if node -e 'const s=require("./package.json").scripts||{}; process.exit(s["build:wasm"] && !s.prebuild && !String(s.build).includes("build:wasm") ? 0 : 1)'; then
    npm run build:wasm
  fi
  npm run build
)

say "Compressing wasm for the server (.br, .gz next to each file)"
mapfile -t WASM_FILES < <(find web/dist -name '*.wasm')
if (( ${#WASM_FILES[@]} == 0 )); then
  if (( HAS_WASM_SCRIPT )); then
    echo "ERROR: this ref builds the offline engine (web/scripts/build-wasm.mjs) but web/dist holds no .wasm;" >&2
    echo "       'Play offline' would not work in this release. Nothing was installed." >&2
    exit 1
  elif [[ -d web/src/offline ]]; then
    echo "WARNING: this ref has offline play (web/src/offline) but no wasm build script;" >&2
    echo "         'Play offline' will not work in this release." >&2
  else
    echo "no wasm in this build"
  fi
fi
for f in "${WASM_FILES[@]}"; do
  gzip -9 -k -f "$f"
  node -e 'const z=require("zlib"),fs=require("fs");const f=process.argv[1];
    fs.writeFileSync(f+".br", z.brotliCompressSync(fs.readFileSync(f),{params:{[z.constants.BROTLI_PARAM_QUALITY]:11}}))' "$f"
  ls -l "$f" "$f.gz" "$f.br"
done

say "Building the release server"
cargo build --release -p ti4-server --bin server

# ---- 3b. The MLP models -----------------------------------------------------------------------
say "MLP models"
LIVE_MODELS="$(env_value TI4_MODEL_DIR /var/lib/ti4-public/models)"
NEW_MODELS=""   # the directory to install; empty: the live models stay
PRE_MODELS=""   # the directory the pre-flight loads
if [[ -f "$MODELS_SRC/index.json" ]]; then
  NEW_MODELS="$MODELS_SRC"
  PRE_MODELS="$MODELS_SRC"
  echo "the models of $MODELS_SRC replace the live ones:"
  ls -l "$MODELS_SRC"
  if ! cmp -s "$MODELS_SRC/index.json" "$WT/models/index.json"; then
    echo "WARNING: $MODELS_SRC/index.json is not models/index.json of $COMMIT;" >&2
    echo "         run scripts/pack-models.sh in a checkout of that commit to deploy its models." >&2
  fi
elif sudo test -f "$LIVE_MODELS/index.json"; then
  echo "no $MODELS_SRC/index.json (scripts/pack-models.sh writes it): the models of $LIVE_MODELS stay"
  PRE_MODELS="$WT/out/live-models"
  mkdir -p "$WT/out"
  sudo cp -a "$LIVE_MODELS" "$PRE_MODELS"
  sudo chown -R "$(id -u):$(id -g)" "$PRE_MODELS"
else
  echo "no models (scripts/pack-models.sh writes out/models): the release offers no MLP bot"
fi
if [[ -n "$PRE_MODELS" && -z "$(env_value TI4_MODEL_DIR '')" ]]; then
  echo "WARNING: $ENV_FILE has no TI4_MODEL_DIR, so the service loads no model." >&2
  echo "         Add TI4_MODEL_DIR=$LIVE_MODELS (deploy/public/env.example)." >&2
fi

# ---- 4. Persistence pre-flight on a copy of the live data -------------------------------------
say "Pre-flight: can the new build load every saved game?"
DATA_DIR="$(env_value TI4_DATA_DIR /var/lib/ti4-public)"
pre_args=(--bin "$CARGO_TARGET_DIR/release/server" --web-dir "$WT/web/dist" --data-dir "$DATA_DIR")
if [[ -n "$PRE_MODELS" ]]; then pre_args+=(--model-dir "$PRE_MODELS"); fi
for id in "${ALLOW_FAILED[@]}"; do pre_args+=(--allow-failed "$id"); done
if ! "$HERE/recovery-check.sh" preflight "${pre_args[@]}"; then
  echo "pre-flight FAILED: nothing was installed or restarted. Fix the build, or name a game you accept to lose with --allow-failed <id>." >&2
  exit 1
fi
ALLOWED_CSV="$(IFS=,; echo "${ALLOW_FAILED[*]:-}")"
if (( PREFLIGHT_ONLY )); then
  echo "--preflight-only: stopping before install"
  exit 0
fi

# ---- 4b. Is the live Caddy config the one this release expects? ---------------------------------
# This script installs the app, never the proxy config. A release can need new proxy headers (the
# offline game needs `wasm-unsafe-eval` in the CSP): compare the live Content-Security-Policy with
# the one in this ref's Caddyfile and warn loudly. Only a warning: the live file is edited by hand.
csp_line() { grep -h 'Content-Security-Policy' "$1" 2>/dev/null | sed 's/^[[:space:]]*//' | head -1; }
LIVE_CADDYFILE="${TI4_LIVE_CADDYFILE:-/etc/caddy/Caddyfile}"
if [[ -r "$LIVE_CADDYFILE" && -f "$WT/deploy/public/Caddyfile" ]]; then
  if [[ "$(csp_line "$LIVE_CADDYFILE")" != "$(csp_line "$WT/deploy/public/Caddyfile")" ]]; then
    say "WARNING: the live Caddy CSP differs from deploy/public/Caddyfile in this ref"
    echo "  live:  $(csp_line "$LIVE_CADDYFILE")"
    echo "  ref:   $(csp_line "$WT/deploy/public/Caddyfile")"
    echo "  Offline play needs 'wasm-unsafe-eval' in script-src and worker-src 'self'. Update"
    echo "  $LIVE_CADDYFILE by hand (keep your domain), run 'caddy validate', then 'systemctl reload caddy'."
  fi
fi

# ---- 5. Install, switch, verify, roll back on failure -----------------------------------------
say "Installing $RELEASE"
sudo install -d -m 755 "$RELEASE"
sudo install -m 755 "$CARGO_TARGET_DIR/release/server" "$RELEASE/server"
sudo cp -r web/dist "$RELEASE/dist"
echo "$COMMIT" | sudo tee "$RELEASE/COMMIT" >/dev/null
sudo chown -R root:root "$RELEASE"

previous=""
[[ -L "$ROOT/current" ]] && previous="$(readlink -f "$ROOT/current")"

say "Switching to the new release and restarting"
SINCE="$(date '+%Y-%m-%d %H:%M:%S')"
if [[ -n "$NEW_MODELS" ]]; then
  # The old server serves the files of its models by path: it is stopped before they change, so
  # that it never sends a new file at the address (and with the immutable cache) of an old one.
  echo "installing the models to $LIVE_MODELS (the ones before: $LIVE_MODELS.previous)"
  sudo rm -rf "$LIVE_MODELS.new"
  sudo install -d -m 750 -o root -g ti4 "$LIVE_MODELS.new"
  sudo install -m 640 -o root -g ti4 "$NEW_MODELS"/* "$LIVE_MODELS.new"/
  sudo systemctl stop "$SERVICE"
  sudo rm -rf "$LIVE_MODELS.previous"
  if sudo test -d "$LIVE_MODELS"; then sudo mv -T "$LIVE_MODELS" "$LIVE_MODELS.previous"; fi
  sudo mv -T "$LIVE_MODELS.new" "$LIVE_MODELS"
  MODELS_SWAPPED=1
fi
swap_current "$RELEASE"
[[ -n "$previous" ]] && sudo ln -sfn "$previous" "$ROOT/previous"
sudo systemctl restart "$SERVICE"

say "Health check"
if health_check; then
  echo "release $COMMIT answers /health"
else
  echo "health check FAILED; rolling back" >&2
  restore_previous "$previous"
  exit 1
fi

say "Post-restart check: saved games recovered by the live service"
summary=""
for _ in $(seq 1 15); do
  if summary="$(sudo journalctl -u "$SERVICE" --since "$SINCE" --no-pager -o cat 2>/dev/null | "$HERE/recovery-check.sh" parse)"; then
    break
  fi
  summary=""
  sleep 2
done
if [[ -z "$summary" ]]; then
  echo "no recovery summary in the journal since $SINCE; cannot verify the saved games; rolling back" >&2
  restore_previous "$previous"
  exit 1
fi
echo "$summary"
live_ids="${summary##* ids=}"
if ! "$HERE/recovery-check.sh" compare "$ALLOWED_CSV" "$live_ids"; then
  echo "the live service lost games the pre-flight did not predict; rolling back" >&2
  restore_previous "$previous"
  exit 1
fi
echo "release $COMMIT is live"

say "Pruning old releases (keeping $KEEP)"
# shellcheck disable=SC2012
ls -1dt "$ROOT"/releases/*/ | tail -n +"$((KEEP + 1))" | while read -r old; do
  case "$(readlink -f "$old")" in
    "$(readlink -f "$ROOT/current")"|"$(readlink -f "$ROOT/previous" 2>/dev/null || true)") ;;
    *) sudo rm -rf "$old" ;;
  esac
done
