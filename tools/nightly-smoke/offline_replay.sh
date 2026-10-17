#!/usr/bin/env bash
# Reproduces an offline run's game natively: plays the saved game again from its seed and the human's
# answers with the same host crate the browser runs (crates/ti4-wasm/examples/replay_save.rs), and
# says whether the decisions come back the same (count and hash) and what the human was asked at the
# end. No browser, no server.
#
#   offline_replay.sh <run-dir | save.json> [--snapshot FILE]
#
# A run directory is <night>/runs/oK-NN-HHMM; its trace/save.json is the save blob the page held at
# the last question (or the final state of a finished game). Anything `replay_save` accepts works too:
# a feedback report or a request body with context.local.save. `--snapshot FILE` also writes the
# board as the human saw it at that position. Exit status 1 when the replay differs.
# The engine of the checkout must be the one that played the game (the entry names its wasm hash).
set -uo pipefail
source "$(dirname "$0")/config.sh"
target="${1:?usage: offline_replay.sh <run-dir | save.json> [--snapshot FILE]}"
shift
[ ! -d "$target" ] || target="$target/trace/save.json"
[ -s "$target" ] || { echo "offline_replay: no save at $target" >&2; exit 2; }
cd "$GAME_REPO" && CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$OFFLINE_TARGET}" exec cargo run --quiet -p ti4-wasm --example replay_save -- "$target" "$@"
