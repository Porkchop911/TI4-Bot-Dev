#!/usr/bin/env bash
# Rule-bot self-play: games with the rule bot on every seat, no server, no browser, no libtorch.
# See docs/RULE_BOT_OPTIMISATION.md.
#
#   tools/rulebot/selfplay.sh smoke            10 games, about a minute
#   tools/rulebot/selfplay.sh bench            the golden set (200 games, starting-technology choices on), checked against baseline.json
#   tools/rulebot/selfplay.sh bench-v4         the same golden set as bot version 4 played it, checked against baseline-v4.json
#   tools/rulebot/selfplay.sh bench-v1         the same seeds as bot version 1 played them, checked against baseline-v1.json
#   tools/rulebot/selfplay.sh bench-v2         the same seeds as bot version 2 played them (no choice), checked against baseline-v2.json
#   tools/rulebot/selfplay.sh bench-v3         the same seeds as bot version 3 played them (no choice), checked against baseline-v3.json
#   tools/rulebot/selfplay.sh ab k=v[,k=v] [N] candidate bot against the shipped bot on N tables (default 200)
#   tools/rulebot/selfplay.sh complete         time to completion, 3..8 players, several maps, no cutoff
#   tools/rulebot/selfplay.sh replay ARGS...   one game: --seed S --players N [--map M --factions ...] --text
#   tools/rulebot/selfplay.sh knobs            the bot's tunable numbers
#   tools/rulebot/selfplay.sh run ARGS...      anything `selfplay run` takes
#   tools/rulebot/selfplay.sh baseline         rewrite tools/rulebot/baseline.json (do this on purpose, in its own commit)
#
# Environment: SELFPLAY_TARGET (cargo target dir; default <repo>/target), SELFPLAY_OUT (default
# <target>/selfplay-out), WORKERS (default cores-1), and CARGO_TARGET_DIR is respected.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo="$(cd "$here/../.." && pwd)"
export CARGO_TARGET_DIR="${SELFPLAY_TARGET:-${CARGO_TARGET_DIR:-$repo/target}}"
out="${SELFPLAY_OUT:-$CARGO_TARGET_DIR/selfplay-out}"
# Every run/compare keeps its games (with per-round timelines) here and is never played twice;
# see tools/rulebot/README.md. SELFPLAY_RESULTS=off disables.
export SELFPLAY_RESULTS="${SELFPLAY_RESULTS:-$repo/tools/rulebot/results}"
mkdir -p "$out"
bin="$CARGO_TARGET_DIR/release/selfplay"
workers=()
[[ -n "${WORKERS:-}" ]] && workers=(--workers "$WORKERS")

build() {
  (cd "$repo" && nice cargo build -q -p ti4-selfplay --release)
}

cmd="${1:-smoke}"
[[ $# -gt 0 ]] && shift
case "$cmd" in
  build) build ;;
  smoke)
    build
    "$bin" run --players 4,6 --games 5 --seed-start 100 "${workers[@]}" --out "$out/smoke.json" "$@"
    ;;
  bench)
    build
    # Hashes of the shipped bot on the golden seeds: a difference means the engine or the bot
    # decides differently from the recorded baseline (expected only after a deliberate change).
    "$bin" run --config "$here/golden.json" --no-cache "${workers[@]}" --out "$out/bench.json" \
      --verify "$here/baseline.json" "$@" || { rc=$?; [[ $rc -eq 2 ]] && echo "bench: hashes differ from baseline.json (see above)"; exit $rc; }
    ;;
  bench-v4)
    build
    # Version 4 on the golden set (starting-technology choices on): identical to baseline-v4.json
    # proves the engine and the version-4 bot still play the old games.
    "$bin" run --config "$here/golden-v4.json" --bot bot_version=4 --no-cache "${workers[@]}" --out "$out/bench-v4.json" \
      --verify "$here/baseline-v4.json" "$@" || { rc=$?; [[ $rc -eq 2 ]] && echo "bench-v4: hashes differ from baseline-v4.json (see above)"; exit $rc; }
    ;;
  bench-v1)
    build
    # Version 1 on the same seeds and rules version as version 2 (no starting-technology choice): identical to baseline-v1.json.
    "$bin" run --config "$here/golden-v2.json" --bot bot_version=1 "${workers[@]}" --out "$out/bench-v1.json" \
      --verify "$here/baseline-v1.json" "$@" || { rc=$?; [[ $rc -eq 2 ]] && echo "bench-v1: hashes differ from baseline-v1.json (see above)"; exit $rc; }
    ;;
  bench-v2)
    build
    # The shipped bot of version 2 on the golden seeds without the starting-technology choice:
    # identical to baseline-v2.json proves the engine and the old bot still play the old games.
    "$bin" run --config "$here/golden-v2.json" --bot bot_version=2 "${workers[@]}" --out "$out/bench-v2.json" \
      --verify "$here/baseline-v2.json" "$@" || { rc=$?; [[ $rc -eq 2 ]] && echo "bench-v2: hashes differ from baseline-v2.json (see above)"; exit $rc; }
    ;;
  bench-v3)
    build
    # Version 3 on the same seeds (no starting-technology choice): identical to baseline-v3.json.
    "$bin" run --config "$here/golden-v2.json" --bot bot_version=3 "${workers[@]}" --out "$out/bench-v3.json" \
      --verify "$here/baseline-v3.json" "$@" || { rc=$?; [[ $rc -eq 2 ]] && echo "bench-v3: hashes differ from baseline-v3.json (see above)"; exit $rc; }
    ;;
  ab)
    build
    cand="${1:?usage: selfplay.sh ab key=val[,key=val] [tables]}"
    tables="${2:-100}"
    [[ $# -gt 0 ]] && shift
    [[ $# -gt 0 ]] && shift
    # N tables of each size, each played twice (candidate on the even seats, then on the odd).
    "$bin" compare --players 4,6 --games "$tables" --seed-start 20000 --candidate "$cand" \
      "${workers[@]}" --out "$out/ab.json" "$@"
    ;;
  complete)
    build
    "$bin" run --config "$here/completion.json" --workers "${WORKERS:-1}" --out "$out/completion.json" "$@"
    ;;
  replay)
    build
    "$bin" replay "$@"
    ;;
  knobs)
    build
    "$bin" knobs
    ;;
  run)
    build
    "$bin" run "${workers[@]}" "$@"
    ;;
  baseline)
    build
    "$bin" run --config "$here/golden.json" --no-cache "${workers[@]}" --out "$here/baseline.json" --quiet "$@"
    ;;
  *)
    sed -n '2,16p' "${BASH_SOURCE[0]}"
    exit 1
    ;;
esac
