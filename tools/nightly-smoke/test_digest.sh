#!/usr/bin/env bash
# Checks digest.py on a small synthetic run: history unwrapped once (decisions are read), Mecatol Rex
# found under both planet ids, dev-proxy noise filtered from the suspicious lines, the VP ledger
# line, and the rare-subtype list. Needs only python3; never touches nightly-reports.
#
#   tools/nightly-smoke/test_digest.sh [<real-run-dir>]   (a real run dir is digested as well)
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

checks=0
failures=0
check_has() { # check_has <name> <text> <pattern>
  checks=$((checks + 1))
  if printf '%s\n' "$2" | grep -Eq -- "$3"; then echo "ok    $1"; else echo "FAIL  $1: no match for /$3/"; failures=$((failures + 1)); fi
}
check_lacks() { # check_lacks <name> <text> <pattern>
  checks=$((checks + 1))
  if printf '%s\n' "$2" | grep -Eq -- "$3"; then echo "FAIL  $1: unexpected /$3/"; failures=$((failures + 1)); else echo "ok    $1"; fi
}

# make_run <dir> <mecatol planet id> <activate id>
make_run() {
  python3 - "$1" "$2" "$3" <<'PY'
import json, os, sys
run, planet, activate = sys.argv[1:4]
os.makedirs(f"{run}/trace", exist_ok=True)
os.makedirs(f"{run}/server-data/game_x", exist_ok=True)
json.dump({"game_seed": 1, "click_seed": 2, "players": 2, "policy": "steer", "preset": "combat",
           "repro": "cd web && true", "server_data_dir": f"{run}/server-data"}, open(f"{run}/meta.json", "w"))
open(f"{run}/exit_code", "w").write("124")
json.dump({"gameId": "game_x", "players": ["pa", "pb"]}, open(f"{run}/trace/game.json", "w"))
json.dump({"decisions": 3, "clicks": 5, "rejections": [], "roundStarts": {"1": 0, "2": 1}}, open(f"{run}/trace/report.json", "w"))
players = [{"id": "pa", "faction": "hacan", "victory_points": 3}, {"id": "pb", "faction": "sol", "victory_points": 1}]
json.dump({"state": {"players": players, "custodians_removed": True,
                     "board": {"18": {"planet_control": {planet: "pa"}}},
                     "vp_ledger": [["pa", 1, "custodians"], ["pa", 1, "imperial_primary"], ["pb", 1, "objective"]]}},
          open(f"{run}/trace/final-snapshot.json", "w"))
trace = [
    {"decision": 1, "round": 1, "phase": "action", "player": "pa", "subtype": "activate_system",
     "prompt": "activate a system", "options": [{"id": f"activate|{activate}", "label": "activate"}]},
    {"decision": 2, "round": 1, "phase": "strategy", "player": "pb", "subtype": "draft_strategy_card",
     "prompt": "pick", "options": [{"id": "leadership", "label": "1. Leadership"}]},
    {"decision": 3, "round": 1, "phase": "status", "player": "pa", "subtype": "assign_ground_casualty",
     "prompt": "casualty", "options": [{"id": "x", "label": "x"}]},
]
open(f"{run}/trace/trace.jsonl", "w").write("".join(json.dumps(r) + "\n" for r in trace))
decisions = [
    {"player": "pa", "prompt": "activate a system", "chosen": f"activate|{activate}", "offered": [f"activate|{activate}"], "context": None},
    {"player": "pb", "prompt": "pick", "chosen": "leadership", "offered": ["leadership"], "context": None},
    {"player": "pa", "prompt": "casualty", "chosen": "x", "offered": ["x"], "context": None},
]
wrapped = {"format_version": 1, "payload": {"decisions": decisions, "events": []}, "checksum": "x"}
json.dump(wrapped, open(f"{run}/server-data/game_x/history.json", "w"))
json.dump({"format_version": 1, "payload": {"initial_state": {"players": []}}, "checksum": "x"},
          open(f"{run}/server-data/game_x/init.json", "w"))
open(f"{run}/run.log", "w").write(
    "[frontend] Error: connect ECONNREFUSED 127.0.0.1:8081\n"
    "[frontend] Error: read ECONNRESET\n"
    "[frontend] Error: This socket has been ended by the other party\n"
    "[vite] ws proxy error:\n"
    "thread 'main' panicked at engine.rs:1\n")
PY
}

for planet in mr mrte; do
  run="$TMP/run-$planet"
  make_run "$run" "$planet" 112
  out=$(python3 "$HERE/digest.py" "$run")
  check_has "[$planet] Mecatol Rex is controlled by hacan" "$out" 'controlled by: hacan'
  check_has "[$planet] system 18/112 activation is counted from the history" "$out" 'activated 1×'
  check_has "[$planet] strategy card line is present" "$out" '^- strategy cards: r1: sol→1\. Leadership'
  check_lacks "[$planet] proxy noise is not suspicious" "$out" 'ECONNREFUSED|ECONNRESET|socket has been ended|proxy error'
  check_has "[$planet] a real panic still is" "$out" 'panicked'
  check_has "[$planet] VP ledger line" "$out" 'VP ledger: hacan: custodians \+1, imperial_primary \+1'
  check_has "[$planet] ledger mismatch is flagged" "$out" 'sol: objective \+1$|hacan.*ledger sum 2 != 3 VP'
  check_has "[$planet] exit 124 is labelled a deadline" "$out" 'exit code: \*\*124\*\* \(the deadline was reached'
  check_has "[$planet] ground casualty counts as combat" "$out" 'combat decisions: 1'
done

# Run directories of parallel slots are named sK-NN-HHMM; nothing in the digest depends on the name.
run="$TMP/s2-03-2215"
make_run "$run" mr 112
out=$(python3 "$HERE/digest.py" "$run")
check_has "[slot run name] digest reads a run named s2-03-2215" "$out" 'controlled by: hacan'
check_has "[slot run name] decisions are read" "$out" 'combat decisions: 1'

if [ -n "${1:-}" ]; then
  out=$(python3 "$HERE/digest.py" "$1")
  check_lacks "real run: no ECONNREFUSED in suspicious lines" "$out" 'ECONNREFUSED'
  check_has "real run: Mecatol owner is not 'nobody'" "$out" 'controlled by: [a-z]'
  check_has "real run: system 18 activated at least once" "$out" 'activated [1-9][0-9]*×'
  check_has "real run: strategy cards line" "$out" '^- strategy cards:'
fi

echo
if [ "$failures" -eq 0 ]; then echo "all $checks checks passed"; else echo "$failures of $checks checks FAILED"; fi
[ "$failures" -eq 0 ]
