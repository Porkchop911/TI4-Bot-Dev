#!/usr/bin/env bash
# Scripted tests for the offline mode of the nightly sweep (NIGHTLY_MODE=offline|both): the entry
# generator, the runner offline-run.sh (workers, run names, report entries, termination) and the
# integration in `nightly.sh loop`, all with a stub game, stub build, a fake clock and a temp repo.
# No browser, vite, cargo or model is started; the live checkout and nightly-reports are never touched.
#
#   tools/nightly-smoke/test_offline.sh
set -uo pipefail
export NIGHTLY_PREFLIGHT=0 MAX_ZERO_DECISION_RUNS=3
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; [ -n "${KEEP_TMP:-}" ] || rm -rf "$TMP"' EXIT

checks=0 failures=0
check() { checks=$((checks + 1)); if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: expected [$3], got [$2]"; failures=$((failures + 1)); fi; }
check_true() { local name="$1"; shift; checks=$((checks + 1)); if "$@"; then echo "ok    $name"; else echo "FAIL  $name"; failures=$((failures + 1)); fi; }
epoch() { TZ=Europe/Berlin date -d "$1" +%s; }
yesno() { if "$@"; then echo yes; else echo no; fi; }
wait_for() { local i; for i in $(seq 1 $((${2:-30} * 5))); do [ -e "$1" ] && return 0; sleep 0.2; done; return 1; }

# ---- the entry generator ----------------------------------------------------------------------
echo "== offline_entry.py"
mk_run() { # mk_run <dir> <report-json-fields>
  mkdir -p "$1/trace"
  echo '{"game_seed":42,"click_seed":7,"players":4,"policy":"steer","viewport":"desktop","commit":"abcdef012345","repro":"REPRO CMD","replay":"REPLAY CMD"}' > "$1/meta.json"
  echo 0 > "$1/exit_code"
  python3 - "$1/trace/report.json" "$2" <<'PY'
import json, sys
base = {"outcome": "finished", "decisions": 120, "seconds": 90, "subtypes": {"action_menu": 40, "cast_vote": 2}, "roundStarts": {"1": 0, "2": 30},
        "findings": [], "panics": [], "pageErrors": [], "consoleErrors": [], "failedRequests": [], "players": 4, "humanSeat": 2, "humanFaction": "sol",
        "factions": ["sol", "hacan"], "finalStandings": ["sol 10 VP"], "shots": {"names": []}, "engine": {"wasmSha256": "d7c36be8"}}
base.update(json.loads(sys.argv[2]))
json.dump(base, open(sys.argv[1], "w"))
PY
}
mk_run "$TMP/clean" '{}'
python3 "$HERE/offline_entry.py" "$TMP/clean" > "$TMP/clean.out"
check "clean run: heading" "$(sed -n 1p "$TMP/clean.out")" "## Run clean — clean"
check_true "entry has the proctors' sections" bash -c "for s in 'Potential bugs' 'Interesting events' 'Repairs' 'UI observations' 'Fix requests' 'Proctor notes'; do grep -q \"^### \$s\" '$TMP/clean.out' || exit 1; done"
check_true "entry carries repro and replay" bash -c "grep -q 'repro: .REPRO CMD' '$TMP/clean.out' && grep -q 'REPLAY CMD' '$TMP/clean.out'"
check "severity of a clean run" "$(python3 "$HERE/offline_entry.py" "$TMP/clean" --max-severity)" none
check_true "digest.md and entry.md are written" test -s "$TMP/clean/digest.md" -a -s "$TMP/clean/entry.md"
mk_run "$TMP/stuck" '{"outcome":"stuck","findings":["stuck: no actionable control at decision #5 subtype x"]}'
echo "stuck on x at decision #5" > "$TMP/stuck/trace/failure.txt"
python3 "$HERE/offline_entry.py" "$TMP/stuck" > "$TMP/stuck.out"
check_true "stuck run: blocker verdict" bash -c "sed -n 1p '$TMP/stuck.out' | grep -q '^## Run stuck — blocker: stuck game'"
check_true "severity of a stuck run starts with blocker" bash -c "python3 '$HERE/offline_entry.py' '$TMP/stuck' --max-severity | grep -q '^blocker '"
mk_run "$TMP/panic" '{"outcome":"crash","panics":["crash: RuntimeError: unreachable"]}'
check_true "engine trap is a blocker, category engine error" bash -c "python3 '$HERE/offline_entry.py' '$TMP/panic' | grep -q 'blocker\*\* (engine error)'"
mk_run "$TMP/err" '{"consoleErrors":["TypeError: x is undefined (http://h/a.js)"],"findings":["layout: page scrolls sideways"]}'
check_true "console errors are high" bash -c "python3 '$HERE/offline_entry.py' '$TMP/err' --max-severity | grep -q '^high '"
mk_run "$TMP/cap" '{"outcome":"time-cap"}'
check_true "time cap is medium" bash -c "python3 '$HERE/offline_entry.py' '$TMP/cap' --max-severity | grep -q '^medium '"
python3 "$HERE/offline_entry.py" "$TMP/clean" --terminated SIGTERM > "$TMP/term.out"
check_true "terminated entry says so" bash -c "sed -n 1p '$TMP/term.out' | grep -q 'sweep terminated mid-run (SIGTERM)'"
mkdir -p "$TMP/empty"; echo '{}' > "$TMP/empty/meta.json"
check_true "a run without any report still gets an entry" bash -c "python3 '$HERE/offline_entry.py' '$TMP/empty' | grep -q 'wrote no report.json'"

# ---- the runner and the integration ------------------------------------------------------------
write_stub_game() { # write_stub_game <path>: a stand-in for offline_game.sh
  cat > "$1" <<'STUB'
#!/usr/bin/env bash
run_dir="$1"; name=$(basename "$run_dir")
mkdir -p "$run_dir/trace"
echo "start $name url=$2 run_id=${OFFLINE_RUN_ID:-} max=${NIGHTLY_OFFLINE_MAX_SECONDS:-}" >> "$STUB_LOG"
echo '{"game_seed":1,"click_seed":2,"players":3,"policy":"steer","viewport":"desktop","commit":"abc","repro":"R","replay":"X"}' > "$run_dir/meta.json"
if [ -n "${STUB_HANG:-}" ]; then
  mkdir -p "$STUB_DIR/hung"; echo ready > "$STUB_DIR/hung/$name"
  TI4_OFFLINE_RUN_ID="$OFFLINE_RUN_ID" sleep 300 &
  echo $! >> "$STUB_DIR/hung-pids"
  wait
  exit 0
fi
outcome=finished
[ -z "${STUB_BLOCKER:-}" ] || outcome=stuck
python3 - "$run_dir/trace/report.json" "$outcome" <<'PY'
import json, sys
json.dump({"outcome": sys.argv[2], "decisions": 50 if sys.argv[2] == "finished" else 9, "seconds": 5, "gameSeed": 1, "players": 3, "subtypes": {"action_menu": 3},
           "roundStarts": {"1": 0}, "findings": ["stuck: no actionable control"] if sys.argv[2] == "stuck" else [], "panics": [], "pageErrors": [], "consoleErrors": [],
           "failedRequests": [], "shots": {"names": []}}, open(sys.argv[1], "w"))
PY
echo 0 > "$run_dir/exit_code"
python3 "$(dirname "$NIGHTLY_SH")/offline_entry.py" "$run_dir" > /dev/null
if [ -n "${NIGHTLY_NOW_FILE:-}" ]; then ( flock 8; echo $(( $(cat "$NIGHTLY_NOW_FILE") + ${STUB_RUN_SECONDS:-3000} )) > "$NIGHTLY_NOW_FILE" ) 8> "$STUB_DIR/clock.lock"; fi
sleep "${STUB_SLEEP:-0.5}"
echo "end $name" >> "$STUB_LOG"
exit 0
STUB
  chmod +x "$1"
}
write_stub_build() {
  cat > "$1" <<'STUB'
#!/usr/bin/env bash
echo "build $1" >> "$STUB_LOG"
[ -z "${STUB_BUILD_FAIL:-}" ] || { echo "boom" >&2; exit 1; }
mkdir -p "$STUB_DIR/dist"; touch "$STUB_DIR/dist/.ok"; echo "$STUB_DIR/dist"
STUB
  chmod +x "$1"
}
setup_env() { # setup_env <dir>
  E="$1"; mkdir -p "$E"
  export NIGHTLY_MIN_FREE_GB=0 NIGHTLY_REPO="$E/repo" NIGHTLY_REPORT_ROOT="$E/reports" NIGHTLY_NOW_FILE="$E/clock" \
    NIGHTLY_NOT_BEFORE=2026-10-07 NIGHTLY_FIXERS= NIGHTLY_CLAUDE="$E/claude" NIGHTLY_BUILD_CMD="bash '$E/build.sh'" \
    NIGHTLY_OFFLINE_GAME_SCRIPT="$E/game.sh" NIGHTLY_OFFLINE_BUILD_SCRIPT="$E/offline_build.sh" NIGHTLY_OFFLINE_NO_PREVIEW=1 \
    NIGHTLY_OFFLINE_STAGGER_SECONDS=0 NIGHTLY_FAILED_RUN_SLEEP=1 NIGHTLY_OFFLINE_FAILED_BATCH_SLEEP=1 NIGHTLY_OFFLINE_PROCTOR_PERCENT=0 \
    STUB_LOG="$E/stub.log" STUB_DIR="$E" NIGHTLY_SH="$E/repo/tools/nightly-smoke/nightly.sh" \
    GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
  unset NIGHTLY_SLOTS NIGHTLY_END NIGHTLY_START NIGHTLY_MODE NIGHTLY_OFFLINE_WORKERS STUB_HANG STUB_BLOCKER STUB_BUILD_FAIL
  git init -q -b main "$NIGHTLY_REPO"
  echo hi > "$NIGHTLY_REPO/README"
  mkdir -p "$NIGHTLY_REPO/tools" "$NIGHTLY_REPO/web/node_modules"; cp -r "$HERE" "$NIGHTLY_REPO/tools/nightly-smoke"
  echo '{}' > "$NIGHTLY_REPO/web/package.json"
  printf '/web/node_modules\n' > "$NIGHTLY_REPO/.gitignore"
  git -C "$NIGHTLY_REPO" add -A; git -C "$NIGHTLY_REPO" commit -q -m init
  : > "$STUB_LOG"
  write_stub_game "$E/game.sh"; write_stub_build "$E/offline_build.sh"
  # a build.sh for the server build (must not run in offline mode) and a claude that must not start
  printf '#!/usr/bin/env bash\necho server-build >> "$STUB_LOG"\n' > "$E/build.sh"
  printf '#!/usr/bin/env bash\necho claude-started >> "$STUB_LOG"\nexit 0\n' > "$E/claude"; chmod +x "$E/claude"
  echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
  NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
}
RUN="bash $HERE/offline-run.sh"

echo "== offline-run.sh: workers, names, entries"
setup_env "$TMP/a"
(cd "$NIGHTLY_REPO" && NIGHTLY_REPO="$NIGHTLY_REPO" timeout 60 $RUN --night-dir "$E/night" --workers 2 --games 5 > "$E/run.out" 2>&1)
check "five games were played" "$(ls "$E/night/runs" | wc -l)" 5
check "run names are oK-NN-HHMM" "$(ls "$E/night/runs" | grep -vcE '^o[12]-[0-9]{2}-[0-9]{4}$')" 0
check "run names are unique" "$(ls "$E/night/runs" | sort | uniq -d | wc -l)" 0
check "one report entry per run" "$(grep -c '^## Run ' "$E/night/report.md")" 5
check "entries are clean" "$(grep -c '^## Run o.* — clean' "$E/night/report.md")" 5
check_true "report names the raw evidence" bash -c "grep -q '_Raw evidence: .*runs/o1-01-' '$E/night/report.md'"
check_true "both workers played" bash -c "ls '$E/night/runs' | grep -q '^o1-' && ls '$E/night/runs' | grep -q '^o2-'"
check "no model and no server build started" "$(grep -c 'claude-started\|server-build' "$STUB_LOG")" 0
check "no active marker left" "$(ls -a "$E/night" | grep -c '^\.active-o')" 0
(cd "$NIGHTLY_REPO" && NIGHTLY_REPO="$NIGHTLY_REPO" timeout 60 $RUN --night-dir "$E/night" --workers 2 --games 2 > "$E/run2.out" 2>&1)
check "a second batch continues each worker's numbering" "$(ls "$E/night/runs" | grep -c '^o1-0[4-9]-\|^o2-0[3-9]-')" 2
check "--games counts per batch" "$(cat "$E/night/.offline-started")" 2

echo "== offline-run.sh: optional Sonnet review of a sample"
setup_env "$TMP/rv"
cat > "$E/claude" <<'STUB'
#!/usr/bin/env bash
prompt=""; while [ $# -gt 0 ]; do case "$1" in --) shift; prompt="$1"; break ;; *) shift ;; esac; done
echo "review $(printf '%s' "$prompt" | grep -c 'Machine entry')" >> "$STUB_LOG"
rd=$(printf '%s' "$prompt" | sed -n 's/^Run directory: //p' | head -1)
printf '## Run %s — reviewed (stub)\n- facts\n' "$(basename "$rd")"
STUB
chmod +x "$E/claude"
NIGHTLY_OFFLINE_PROCTOR_PERCENT=100 timeout 60 $RUN --night-dir "$E/night" --workers 1 --games 2 > "$E/rv.out" 2>&1
check "both entries are the reviewer's" "$(grep -c '^## Run o1-0[0-9]-[0-9]* — reviewed (stub)' "$E/night/report.md")" 2
check "the reviewer was started once per run" "$(grep -c '^review 1' "$STUB_LOG")" 2
check "no machine entry replaced it" "$(grep -c '^## Run o1-0[0-9]-[0-9]* — clean' "$E/night/report.md")" 0
setup_env "$TMP/rv0"
NIGHTLY_OFFLINE_PROCTOR_PERCENT=0 timeout 60 $RUN --night-dir "$E/night" --workers 1 --games 1 > "$E/rv0.out" 2>&1
check "percent 0 never starts a model" "$(grep -c 'claude-started\|^review' "$STUB_LOG")" 0

echo "== offline-run.sh: a build failure is reported"
STUB_BUILD_FAIL=1 timeout 60 $RUN --night-dir "$E/night-bf" --workers 1 --games 1 > "$E/bf.out" 2>&1
check "exit status 1" "$?" 1
check_true "report says the build failed" grep -q '^## Offline build failed' "$E/night-bf/report.md"

echo "== offline-run.sh: SIGTERM mid-run files an entry and leaves nothing behind"
export STUB_HANG=1
setsid timeout 60 $RUN --night-dir "$E/night-t" --workers 2 --games 2 > "$E/t.out" 2>&1 &
sup=$!
check_true "both games are running" bash -c "for i in \$(seq 1 100); do [ \$(ls '$E/hung' 2>/dev/null | wc -l) -ge 2 ] && exit 0; sleep 0.2; done; exit 1"
kill -TERM "$sup" 2>/dev/null; wait "$sup" 2>/dev/null
sleep 1
check "one entry per killed run" "$(grep -c '^## Run .*sweep terminated mid-run (SIGTERM)' "$E/night-t/report.md")" 2
check "no active marker left" "$(ls -a "$E/night-t" | grep -c '^\.active-o')" 0
check "the hung games were killed" "$(for p in $(cat "$E/hung-pids"); do kill -0 "$p" 2>/dev/null && echo alive; done | wc -l)" 0
unset STUB_HANG

echo "== nightly.sh loop, NIGHTLY_MODE=offline"
setup_env "$TMP/b"
export NIGHTLY_MODE=offline NIGHTLY_OFFLINE_WORKERS=2 NIGHTLY_OFFLINE_BATCH_SECONDS=20000 STUB_RUN_SECONDS=4000
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1
check "sweep.done written" "$(yesno test -e "$ND/sweep.done")" yes
check_true "report header names the mode" grep -q '^Mode: offline\.' "$ND/report.md"
check_true "offline entries are in the night's report" bash -c "grep -q '^## Run o1-01-' '$ND/report.md' && grep -q '^## Run o2-01-' '$ND/report.md'"
check "one entry per run directory" "$(grep -c '^## Run ' "$ND/report.md")" "$(ls "$ND/runs" | wc -l)"
check "the server build and the proctor were not used" "$(grep -c 'claude-started\|server-build' "$STUB_LOG")" 0
check "the offline bundle was built through the offline build script" "$(grep -c '^build ' "$STUB_LOG" | awk '{ print ($1 > 0) ? "yes" : "no" }')" yes
check_true "the night branch is checked out in the live checkout" bash -c "[ \"\$(git -C '$NIGHTLY_REPO' branch --show-current)\" = nightly-fixes-$NIGHT ]"
check_true "report_entries (what tick needs for a fix round) counts them" bash -c "[ \$(grep -c '^## Run ' '$ND/report.md') -ge 2 ]"
check "no games after the window" "$(grep -c '^start ' "$STUB_LOG" | awk '{ print ($1 <= 12) ? "ok" : "too many" }')" ok

echo "== nightly.sh loop, offline blocker asks for a fix round"
setup_env "$TMP/c"
export NIGHTLY_MODE=offline NIGHTLY_OFFLINE_WORKERS=1 NIGHTLY_OFFLINE_BATCH_SECONDS=20000 STUB_RUN_SECONDS=20000 STUB_BLOCKER=1 NIGHTLY_FIXERS="1 2"
echo "$(epoch '2026-10-07 20:31')" > "$NIGHTLY_NOW_FILE"
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1
check_true "fix-requested exists" test -s "$ND/fix-requested"
check_true "the request names the run, offline and the repro path" bash -c "grep -q '^reason: \[offline\] run o1-01-' '$ND/fix-requested'"
check_true "the entry records the request" grep -q 'request_fix.sh' "$ND/runs/o1-01-2031/entry.md"
check "one request recorded" "$(ls "$ND" | grep -c '^fix-requested')" 1

echo "== nightly.sh loop, two failed batches stop the offline sweep"
setup_env "$TMP/d"
export NIGHTLY_MODE=offline NIGHTLY_OFFLINE_WORKERS=1 STUB_BUILD_FAIL=1
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1
check_true "report says the sweep stopped" grep -q '^## Offline sweep stopped' "$ND/report.md"
check "sweep.done is still written" "$(yesno test -e "$ND/sweep.done")" yes

echo "== NIGHTLY_MODE=both"
setup_env "$TMP/e"
cat > "$E/claude" <<'STUB'
#!/usr/bin/env bash
prompt=""; while [ $# -gt 0 ]; do case "$1" in --) shift; prompt="$1"; break ;; *) shift ;; esac; done
rd=$(printf '%s' "$prompt" | sed -n 's/^Run directory: //p' | head -1)
mkdir -p "$rd/trace"; echo '{"preset":"stub"}' > "$rd/meta.json"
echo "proctor $(basename "$rd")" >> "$STUB_LOG"
( flock 8; echo $(( $(cat "$NIGHTLY_NOW_FILE") + 8000 )) > "$NIGHTLY_NOW_FILE" ) 8> "$STUB_DIR/clock.lock"
printf '## Run %s — clean (stub)\n' "$(basename "$rd")"
STUB
chmod +x "$E/claude"
export NIGHTLY_MODE=both NIGHTLY_OFFLINE_WORKERS=1 NIGHTLY_OFFLINE_BATCH_SECONDS=20000 STUB_RUN_SECONDS=8000
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1
check_true "proctored entries exist" grep -q '^## Run 01-[0-9]* — clean (stub)' "$ND/report.md"
check_true "offline entries exist" grep -q '^## Run o1-01-' "$ND/report.md"
check_true "the server build ran for the proctored games" grep -q '^server-build' "$STUB_LOG"
check "sweep.done written once both ended" "$(yesno test -e "$ND/sweep.done")" yes

echo "== default mode is the classic sweep"
setup_env "$TMP/f"
printf '#!/usr/bin/env bash\necho claude-started >> "$STUB_LOG"\nrd=$(printf "%%s" "$4" | sed -n "s/^Run directory: //p" | head -1)\nmkdir -p "$rd"; echo "{}" > "$rd/meta.json"\n( flock 8; echo $(( $(cat "$NIGHTLY_NOW_FILE") + 40000 )) > "$NIGHTLY_NOW_FILE" ) 8> "$STUB_DIR/clock.lock"\necho "## Run x — clean"\n' > "$E/claude"
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1
check "no offline game was started" "$(grep -c '^start ' "$STUB_LOG")" 0
check "no offline header line" "$(grep -c '^Mode:' "$ND/report.md")" 0
check "NIGHTLY_MODE=bogus is refused" "$(NIGHTLY_MODE=bogus bash "$NIGHTLY_SH" tick-decide > /dev/null 2>&1; echo $?)" 2

echo
echo "$((checks - failures)) of $checks checks passed"
[ "$failures" -eq 0 ]
