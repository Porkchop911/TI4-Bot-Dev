#!/usr/bin/env bash
# Scripted tests for the nightly schedule: window maths (incl. daylight saving), what `tick` would
# start, and a dry run of a whole night with stub `claude` binaries on a fake clock. Needs only
# bash, git, flock and python3: no real proctors, fixers, games or builds, and it never touches
# the live checkout or nightly-reports (everything runs in a temp dir).
#
#   tools/nightly-smoke/test_schedule.sh
set -uo pipefail
export NIGHTLY_PREFLIGHT=0 # the fake repos have no node_modules
export MAX_ZERO_DECISION_RUNS=0 # the stub games write no harness report
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; rm -rf "$TMP"' EXIT

checks=0
failures=0
check() { # check <name> <actual> <expected>
  checks=$((checks + 1))
  if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: expected [$3], got [$2]"; failures=$((failures + 1)); fi
}
check_true() { # check_true <name> <command...>
  local name="$1"; shift
  checks=$((checks + 1))
  if "$@"; then echo "ok    $name"; else echo "FAIL  $name"; failures=$((failures + 1)); fi
}
epoch() { TZ=Europe/Berlin date -d "$1" +%s; }

# ---------------------------------------------------------------------------------------------
echo "== window maths (20:30 -> 06:00 Berlin, crossing midnight)"
window_at() { # window_at "<Berlin date time>" -> "<night> <start> <end>"
  NIGHTLY_NOW=$(epoch "$1") NIGHTLY_REPO=/nonexistent bash -c "source '$HERE/config.sh'; current_window"
}
expect_window() { # expect_window "<time>" <night> <next-day>
  check "window at $1 is night $2" "$(window_at "$1")" "$2 $(epoch "$2 20:30") $(epoch "$3 06:00")"
}
expect_window "2026-10-07 08:59" 2026-10-06 2026-10-07
expect_window "2026-10-07 20:29" 2026-10-06 2026-10-07
expect_window "2026-10-07 20:30" 2026-10-07 2026-10-08
expect_window "2026-10-07 23:30" 2026-10-07 2026-10-08
expect_window "2026-10-08 00:30" 2026-10-07 2026-10-08
expect_window "2026-10-08 05:59" 2026-10-07 2026-10-08
expect_window "2026-10-08 06:00" 2026-10-07 2026-10-08
expect_window "2026-10-08 06:05" 2026-10-07 2026-10-08
expect_window "2026-10-08 12:00" 2026-10-07 2026-10-08
# Daylight saving: clocks go back on 2026-10-25 (the window is an hour longer) and forward on
# 2026-03-29 (an hour shorter); the window must still start at 20:30 and end at 06:00 local time.
expect_window "2026-10-25 05:59" 2026-10-24 2026-10-25
expect_window "2026-10-25 06:05" 2026-10-24 2026-10-25
expect_window "2026-10-25 20:30" 2026-10-25 2026-10-26
expect_window "2026-03-29 05:59" 2026-03-28 2026-03-29
expect_window "2026-03-29 06:05" 2026-03-28 2026-03-29
expect_window "2026-03-29 20:30" 2026-03-29 2026-03-30
len() { NIGHTLY_REPO=/nonexistent bash -c "source '$HERE/config.sh'; read -r s e < <(window_for_night $1); echo \$((e - s))"; }
check "window 2026-10-24 is 10.5 h (fall back)" "$(len 2026-10-24)" 37800
check "window 2026-03-28 is 8.5 h (spring forward)" "$(len 2026-03-28)" 30600
check "window 2026-10-07 is 9.5 h" "$(len 2026-10-07)" 34200

# ---------------------------------------------------------------------------------------------
echo "== tick decisions"
RR="$TMP/decide"
decide() { # decide "<Berlin time>" [ENV=VALUE...]
  local when="$1"; shift
  env NIGHTLY_NOW="$(epoch "$when")" NIGHTLY_REPORT_ROOT="$RR" NIGHTLY_REPO=/nonexistent "$@" \
    bash "$HERE/nightly.sh" tick-decide | tr '\n' '|' | sed 's/|$//'
}
hold() { ( exec 9>"$1"; flock -n 9 || exit 1; exec sleep 120 ) & HOLD=$!; sleep 0.3; }
release() { kill "$HOLD" 2>/dev/null; wait "$HOLD" 2>/dev/null; }
reset_night() { rm -rf "$RR"; mkdir -p "$RR/$1"; }
entry() { printf '## Run 01-2030 — clean\n' > "$RR/$1/report.md"; }
N=2026-10-07

reset_night 2026-10-05
check "before NOT_BEFORE: nothing at 2026-10-05 21:00" "$(decide '2026-10-05 21:00')" ""
check "before NOT_BEFORE: nothing at 2026-10-06 07:00 (old night 10-05)" "$(decide '2026-10-06 07:00')" ""
check "NOT_BEFORE can be lowered" "$(decide '2026-10-05 20:32' NIGHTLY_NOT_BEFORE=2026-10-05)" "sweep 2026-10-05"
reset_night 2026-10-06
check "default NOT_BEFORE: tonight's sweep (2026-10-06) starts at 20:32" "$(decide '2026-10-06 20:32')" "sweep 2026-10-06"
check "default NOT_BEFORE: not before 20:30" "$(decide '2026-10-06 20:29')" ""

reset_night $N
check "20:29: not yet" "$(decide '2026-10-07 20:29')" ""
check "09:00: not yet" "$(decide '2026-10-07 09:00')" ""
check "20:32: sweep starts" "$(decide '2026-10-07 20:32')" "sweep $N"
touch "$RR/$N/sweep.done"
check "no second sweep once sweep.done exists; round 2 skipped, no entries" "$(decide '2026-10-07 20:32')" "skip-fix $N 2"
check "no second sweep once sweep.done exists (fixers disabled)" "$(decide '2026-10-07 20:32' NIGHTLY_FIXERS=)" ""

reset_night $N; mkdir -p "$RR/$N"; touch "$RR/$N/.loop.lock"; hold "$RR/$N/.loop.lock"
check "sweep running, no entries: round 1 waits at 23:59" "$(decide '2026-10-07 23:59')" ""
entry $N
check "sweep running, entry: 23:58 nothing" "$(decide '2026-10-07 23:58')" ""
check "sweep running, entry: 23:59 round 1" "$(decide '2026-10-07 23:59')" "fix $N 1"
check "round 1 stays due until it has started (01:00)" "$(decide '2026-10-08 01:00')" "fix $N 1"
touch "$RR/$N/.fixer-1-started"
check "round 1 not started twice" "$(decide '2026-10-08 00:05')" ""
rm "${RR:?}/${N:?}/.fixer-1-started"
check "FIXERS=2 disables round 1" "$(decide '2026-10-07 23:59' NIGHTLY_FIXERS=2)" ""
check "FIXERS empty disables round 1" "$(decide '2026-10-07 23:59' NIGHTLY_FIXERS=)" ""
check "FIX1 time is configurable" "$(decide '2026-10-07 21:30' NIGHTLY_FIX1=22:00)" ""
check "FIX1 time is configurable (due)" "$(decide '2026-10-07 22:00' NIGHTLY_FIX1=22:00)" "fix $N 1"
check "FIX1 after midnight belongs to the next day" "$(decide '2026-10-07 23:59' NIGHTLY_FIX1=02:00)" ""
check "FIX1 after midnight (due)" "$(decide '2026-10-08 02:00' NIGHTLY_FIX1=02:00)" "fix $N 1"
check "round 1 still due inside the window (05:50)" "$(decide '2026-10-08 05:50')" "fix $N 1"
touch "$RR/$N/.fixer-1-started"
check "sweep running at 05:30: nothing" "$(decide '2026-10-08 05:30')" ""
check "sweep running past END: nothing (no summary, no round 2)" "$(decide '2026-10-08 06:05')" ""
release

# A proctor's early request (request_fix.sh) makes round 1 due before 23:59.
echo "== early fix request"
request() { # request "<Berlin time>" <reason...>; prints the script's output; REQ_ENV adds env
  local when="$1"; shift
  env NIGHTLY_NOW="$(epoch "$when")" NIGHTLY_REPORT_ROOT="$RR" NIGHTLY_REPO=/nonexistent $REQ_ENV \
    bash "$HERE/request_fix.sh" "$@" 2>&1
}
REQ_ENV=""
reset_night $N; touch "$RR/$N/.loop.lock"; hold "$RR/$N/.loop.lock"
mkdir -p "$RR/$N/runs/03-2100"
entry $N
check "21:00 without request: round 1 not due" "$(decide '2026-10-07 21:00')" ""
out=$(request '2026-10-07 21:00' "every run dies in round 1 & the server panics" "(decision 3, evidence runs/03-2100/run.log)"); rc=$?
check "request accepted (exit status)" "$rc" 0
check_true "marker written with reason, run name and time" bash -c "grep -q '^reason: every run dies in round 1 & the server panics (decision 3' '$RR/$N/fix-requested' && grep -q '^run: 03-2100' '$RR/$N/fix-requested' && grep -q '^time: 2026-10-07 21:00' '$RR/$N/fix-requested'"
check "21:00 with request: round 1 due" "$(decide '2026-10-07 21:00')" "fix $N 1"
out=$(request '2026-10-07 21:05' "a second, different reason"); rc=$?
check "second request is a no-op (exit status)" "$rc" 0
check "first request's reason is kept" "$(grep -c 'second, different' "$RR/$N/fix-requested")" 0
check "still due once only" "$(decide '2026-10-07 21:10')" "fix $N 1"
check "request does not bypass FIXERS=2" "$(decide '2026-10-07 21:00' NIGHTLY_FIXERS=2)" ""
check "request does not bypass FIXERS empty" "$(decide '2026-10-07 21:00' NIGHTLY_FIXERS=)" ""
touch "$RR/$N/sweep.done"
check "request with the sweep finished: no round 1" "$(decide '2026-10-07 21:00')" ""
rm "${RR:?}/${N:?}/sweep.done"
check "request after END: nothing" "$(decide '2026-10-08 06:05')" ""
touch "$RR/$N/.fixer-1-started"
check "round 1 started: not due again" "$(decide '2026-10-07 21:15')" ""
rm -f "${RR:?}/${N:?}/fix-requested"
# Once round 1 has started a request is kept for round 2 (second early round), not refused.
out=$(request '2026-10-07 21:20' "second problem: reaction dialog stuck (seed 5)"); rc=$?
check "request after round 1 started is accepted for round 2 (exit status)" "$rc" 0
check_true "message says it is recorded for round 2" bash -c "echo '$out' | grep -q 'recorded for round 2'"
check "round 1 marker is not rewritten" "$([ -e "$RR/$N/fix-requested" ] && echo yes || echo no)" no
check_true "round 2 marker holds the reason and the run" bash -c "grep -q '^reason: second problem: reaction dialog stuck (seed 5)' '$RR/$N/fix-requested-2' && grep -q '^run: 03-2100' '$RR/$N/fix-requested-2'"
check "round 1 still running: round 2 waits" "$(decide '2026-10-07 21:25')" ""
touch "$RR/$N/.fixer-1.done"
check "round 1 finished: round 2 is due early (sweep still running)" "$(decide '2026-10-07 21:30')" "fix $N 2"
check "round 2 early request does not bypass FIXERS=1" "$(decide '2026-10-07 21:30' NIGHTLY_FIXERS=1)" ""
check "round 2 early request: nothing after END" "$(decide '2026-10-08 06:05')" ""
touch "$RR/$N/.fixer-2-started"
check "round 2 started: not due again" "$(decide '2026-10-07 21:35')" ""
out=$(request '2026-10-07 21:40' "third problem"); rc=$?
check "request refused after round 2 started (exit status)" "$rc" 1
check_true "refusal says round 2 already started" bash -c "echo '$out' | grep -q 'round 2 has already started'"
rm "${RR:?}/${N:?}/.fixer-2-started" "${RR:?}/${N:?}/.fixer-1.done"
out=$(request '2026-10-07 21:41' "another"); rc=$?
check "a second round 2 request is a no-op (exit status)" "$rc" 0
check "the first round 2 reason is kept" "$(grep -c 'another' "$RR/$N/fix-requested-2")" 0
check_true "a no-op request says to use the report" bash -c "echo '$out' | grep -q 'report entry'"
rm -f "${RR:?}/${N:?}/fix-requested-2"
REQ_ENV="NIGHTLY_FIXERS=1"
out=$(request '2026-10-07 21:42' "round 2 disabled"); rc=$?
check "after round 1 started and round 2 disabled: refused (exit status)" "$rc" 1
check "no round 2 marker when round 2 is disabled" "$([ -e "$RR/$N/fix-requested-2" ] && echo yes || echo no)" no
rm "${RR:?}/${N:?}/.fixer-1-started"
REQ_ENV="NIGHTLY_FIXERS=2"
out=$(request '2026-10-07 21:43' "only round 2 is enabled"); rc=$?
check "FIXERS=2: a request goes to round 2 (exit status)" "$rc" 0
check_true "FIXERS=2: marker written for round 2" test -s "$RR/$N/fix-requested-2"
check "FIXERS=2: round 2 due at once" "$(decide '2026-10-07 21:45' NIGHTLY_FIXERS=2)" "fix $N 2"
rm -f "${RR:?}/${N:?}/fix-requested-2"
REQ_ENV="NIGHTLY_FIXERS="
out=$(request '2026-10-07 21:20' "disabled"); rc=$?
check "request refused when fixers are disabled (exit status)" "$rc" 1
check_true "refusal says disabled" bash -c "echo '$out' | grep -q 'no fix round is enabled'"
check "no marker when disabled" "$([ -e "$RR/$N/fix-requested" ] && echo yes || echo no)" no
REQ_ENV=""
out=$(request '2026-10-07 21:20'); rc=$?
check "request without a reason is a usage error" "$rc" 2
release
reset_night $N; touch "$RR/$N/.loop.lock"; hold "$RR/$N/.loop.lock"
request '2026-10-07 21:00' "no entries yet" > /dev/null
check "request but no report entry yet: round 1 waits" "$(decide '2026-10-07 21:00')" ""
release

# Round 2 and the summary.
reset_night $N; entry $N; touch "$RR/$N/sweep.done" "$RR/$N/.fixer-1-started" "$RR/$N/.fixer-1.done"
check "round 2 starts after sweep.done (06:00)" "$(decide '2026-10-08 06:00')" "fix $N 2"
check "round 2 starts after sweep.done (even early)" "$(decide '2026-10-08 05:40')" "fix $N 2"
check "round 2 disabled: summary at 06:00" "$(decide '2026-10-08 06:00' NIGHTLY_FIXERS=1)" "summary $N"
check "round 2 disabled: no summary before END" "$(decide '2026-10-08 05:40' NIGHTLY_FIXERS=1)" ""
touch "$RR/$N/.fixer-2-started"
check "round 2 running: summary waits" "$(decide '2026-10-08 06:30')" ""
check "round 2 hopelessly late: summary goes ahead" "$(decide '2026-10-08 09:40')" "summary $N"
touch "$RR/$N/.fixer-2.done"
check "round 2 done: summary" "$(decide '2026-10-08 06:30')" "summary $N"
touch "$RR/$N/.summary-started"
check "summary not started twice" "$(decide '2026-10-08 06:35')" ""
reset_night $N; touch "$RR/$N/sweep.done"
check "sweep ended without any run entry: round 2 is skipped" "$(decide '2026-10-08 06:00')" "skip-fix $N 2"

# An empty fix-requested (written while the disk was full) is no request; a failed round is
# retried after a delay, not at once.
echo "== empty request marker and failed fixer rounds"
reset_night $N; touch "$RR/$N/.loop.lock"; hold "$RR/$N/.loop.lock"
entry $N; : > "$RR/$N/fix-requested"
check "empty fix-requested does not make round 1 due (21:00)" "$(decide '2026-10-07 21:00')" ""
check "empty fix-requested: round 1 is still due at 23:59" "$(decide '2026-10-07 23:59')" "fix $N 1"
REQ_ENV=""
out=$(request '2026-10-07 21:00' "a real reason"); rc=$?
check "a real request replaces the empty marker (exit status)" "$rc" 0
check_true "marker now holds the reason" grep -q '^reason: a real reason' "$RR/$N/fix-requested"
check "and makes round 1 due" "$(decide '2026-10-07 21:00')" "fix $N 1"
release
reset_night $N; entry $N; touch "$RR/$N/.loop.lock"; hold "$RR/$N/.loop.lock"
echo "1 $(epoch '2026-10-07 23:59')" > "$RR/$N/.fixer-1.failed"
check "failed round 1 waits for the retry delay (00:10)" "$(decide '2026-10-08 00:10')" ""
check "failed round 1 is due again after the delay (00:30)" "$(decide '2026-10-08 00:30')" "fix $N 1"
release
reset_night $N; entry $N; touch "$RR/$N/sweep.done"
echo "1 $(epoch '2026-10-08 06:00')" > "$RR/$N/.fixer-2.failed"
check "failed round 2 waits for the retry delay (06:10)" "$(decide '2026-10-08 06:10')" ""
check "failed round 2 is due again after the delay (06:40)" "$(decide '2026-10-08 06:40')" "fix $N 2"

# The 06:00-20:30 gap: round 2 and the summary of the old night come first, the next sweep waits.
P=2026-10-07; C=2026-10-08
reset_night $P; entry $P; touch "$RR/$P/sweep.done" "$RR/$P/.fixer-1-started" "$RR/$P/.fixer-1.done"
check "20:35: old night's round 2 due, new sweep waits" "$(decide '2026-10-08 20:35')" "fix $P 2"
touch "$RR/$P/.fixer-2-started" "$RR/$P/.fixer-2.done"
check "09:05: old night's summary due" "$(decide '2026-10-08 09:05')" "summary $P"
touch "$RR/$P/.summary-started" "$RR/$P/summary.md"
check "20:35: old night finished, new sweep starts" "$(decide '2026-10-08 20:35')" "sweep $C"
rm "$RR/$P/summary.md"; touch "$RR/$P/.loop.lock"; hold "$RR/$P/.fixer-2.lock"
check "20:35: old round 2 running, new sweep waits" "$(decide '2026-10-08 20:35')" ""
check "22:35: waited long enough, new sweep starts anyway" "$(decide '2026-10-08 22:35')" "sweep $C"
release

# ---------------------------------------------------------------------------------------------
echo "== dry run of a whole night with stub claude binaries"
setup_env() { # setup_env <dir>: temp repo with the scripts under test, stubs, fake clock
  E="$1"; mkdir -p "$E"
  export NIGHTLY_MIN_FREE_GB=0 NIGHTLY_REPO="$E/repo" NIGHTLY_REPORT_ROOT="$E/reports" NIGHTLY_NOW_FILE="$E/clock" \
    NIGHTLY_CLAUDE="$E/claude" NIGHTLY_BUILD_CMD="${BUILD_CMD:-true}" NIGHTLY_NOT_BEFORE=2026-10-07 \
    STUB_LOG="$E/stub.log" STUB_DIR="$E" NIGHTLY_SH="$E/repo/tools/nightly-smoke/nightly.sh" \
    GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
  git init -q -b main "$NIGHTLY_REPO"
  echo hi > "$NIGHTLY_REPO/README"
  mkdir -p "$NIGHTLY_REPO/tools"; cp -r "$HERE" "$NIGHTLY_REPO/tools/nightly-smoke"
  git -C "$NIGHTLY_REPO" add -A; git -C "$NIGHTLY_REPO" commit -q -m init
  echo wip > "$NIGHTLY_REPO/wip.txt" # uncommitted work: must be saved by the snapshot
  : > "$STUB_LOG"
  cat > "$NIGHTLY_CLAUDE" <<'STUB'
#!/usr/bin/env bash
model=""; prompt=""
while [ $# -gt 0 ]; do
  case "$1" in --model) model="$2"; shift 2 ;; --) shift; prompt="$1"; break ;; *) shift ;; esac
done
now=$(cat "$NIGHTLY_NOW_FILE")
case "$prompt" in
  *"nightly fixer"*)
    round=$(printf '%s' "$prompt" | sed -n 's/.*(round \([0-9]*\)).*/\1/p' | head -1)
    echo "fixer$round model=$model clock=$now cwd=$(pwd)" >> "$STUB_LOG"
    printf '%s' "$prompt" > "$STUB_DIR/prompt-fixer$round.txt"
    echo "fix $round" > "fix$round.txt"; git add "fix$round.txt"; git commit -q -m "stub fix round $round"
    if [ -n "${STUB_BREAK_ROUND:-}" ] && [ "$round" = "$STUB_BREAK_ROUND" ]; then
      echo x > broken.flag; git add broken.flag; git commit -q -m "stub: breaks the build"
    fi
    printf '## Fixer round %s\n### Fixed\n- stub\n' "$round" ;;
  *"You are a **proctor**"*)
    rd=$(printf '%s' "$prompt" | sed -n 's/^Run directory: //p' | head -1)
    mkdir -p "$rd"; echo '{"preset":"stub"}' > "$rd/meta.json"
    present=no; [ -f fix1.txt ] && present=yes
    [ -n "${STUB_REQUEST:-}" ] && "$(dirname "$NIGHTLY_SH")/request_fix.sh" "$STUB_REQUEST" > /dev/null 2>&1
    echo "proctor model=$model clock=$now fix1_present=$present" >> "$STUB_LOG"
    new=$((now + ${STUB_RUN_SECONDS:-3000})); echo "$new" > "$NIGHTLY_NOW_FILE"
    # what cron would do every five minutes while the game runs
    "$NIGHTLY_SH" tick > /dev/null 2>&1
    for _ in $(seq 1 100); do # let a fixer started by that tick finish before the proctor ends
      pending=no
      for f in "$STUB_DIR"/reports/*/.fixer-*-started; do
        [ -e "$f" ] || continue
        [ -e "${f%-started}.done" ] || pending=yes
      done
      [ "$pending" = no ] && break
      sleep 0.2
    done
    printf '## Run %s — clean (stub)\n' "$(basename "$rd")" ;;
  *)
    printf '%s' "$prompt" > "$STUB_DIR/prompt-summary.txt"
    echo "summary model=$model clock=$now" >> "$STUB_LOG"
    printf '# Summary stub\n' ;;
esac
STUB
  chmod +x "$NIGHTLY_CLAUDE"
}
wait_for() { # wait_for <file> [seconds]
  local i; for i in $(seq 1 $((${2:-30} * 5))); do [ -e "$1" ] && return 0; sleep 0.2; done; return 1
}
ns() { bash "$NIGHTLY_SH" "$@"; }
repo() { git -C "$NIGHTLY_REPO" "$@"; }

BUILD_CMD=true setup_env "$TMP/night"
echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
check "tick at 20:30 decides to start the sweep" "$(ns tick-decide)" "sweep $NIGHT"
timeout 600 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/night/loop.out" 2>&1
check "loop finished and wrote sweep.done" "$([ -e "$ND/sweep.done" ] && echo yes || echo no)" yes
check "checkout is on the night branch" "$(repo branch --show-current)" "nightly-fixes-$NIGHT"
check "orig branch recorded" "$(cat "$ND/orig_branch")" main
check "snapshot saved the uncommitted work" "$(repo show nightly-fixes-$NIGHT:wip.txt 2>/dev/null | head -1)" wip
check_true "snapshot commit exists" bash -c "git -C '$NIGHTLY_REPO' log --format=%s | grep -q 'snapshot of the working tree'"
check "proctor ran on sonnet" "$(grep '^proctor' "$STUB_LOG" | grep -vc 'model=claude-sonnet-5-5')" 0
check "round 1 fixer started exactly once" "$(grep -c '^fixer1' "$STUB_LOG")" 1
check "round 1 ran on opus" "$(grep '^fixer1' "$STUB_LOG" | grep -c 'model=claude-opus-5-5')" 1
check_true "round 1 worked in its own worktree" bash -c "grep '^fixer1' '$STUB_LOG' | grep -q 'cwd=$ND/fixer-1'"
fix1_clock=$(grep '^fixer1' "$STUB_LOG" | sed 's/.*clock=\([0-9]*\).*/\1/')
check_true "round 1 started at or after 23:59 and before 01:30" \
  bash -c "[ $fix1_clock -ge $(epoch '2026-10-07 23:59') ] && [ $fix1_clock -lt $(epoch '2026-10-08 01:30') ]"
check "scheduled round 1: prompt has no early-request line" "$(grep -c 'asked for this round early' "$TMP/night/prompt-fixer1.txt")" 0
check "scheduled round 1: report has no early note" "$(grep -c 'started early' "$ND/report.md")" 0
# The commit arrives between games: no proctor before the fixer sees it, the next one does.
first_after=$(awk '/^fixer1/{f=1;next} f && /^proctor/{print; exit}' "$STUB_LOG")
check "the proctor right after round 1 already has its commit" "$(echo "$first_after" | grep -c 'fix1_present=yes')" 1
check "no proctor before round 1 had it" "$(awk '/^fixer1/{exit} /^proctor/' "$STUB_LOG" | grep -c 'fix1_present=yes')" 0
check "all later proctors have it" "$(awk '/^fixer1/{f=1;next} f && /^proctor/' "$STUB_LOG" | grep -vc 'fix1_present=yes')" 0
check_true "merge note sits between two run entries in the report" \
  awk '/^## Run /{if(m)a=1; else b=1} /^## Fixer round 1: merged/{m=1} END{exit !(a&&b&&m)}' "$ND/report.md"
check "round 1 marked merged" "$([ -e "$ND/.fixer-1.merged" ] && echo yes || echo no)" yes
check "round 2 not started by the sweep's ticks" "$(grep -c '^fixer2' "$STUB_LOG")" 0


# Night is over: the sweep ended at about 06:00. Round 2 is due; the summary is not.
echo "$(epoch '2026-10-08 06:00')" > "$NIGHTLY_NOW_FILE"
check "after sweep.done: round 2 is due" "$(ns tick-decide)" "fix $NIGHT 2"
check "summary not started before round 2" "$([ -e "$ND/.summary-started" ] && echo yes || echo no)" no
ns tick > /dev/null 2>&1
check_true "round 2 finished" wait_for "$ND/.fixer-2.done"
check "round 2 ran on opus in its own worktree" "$(grep '^fixer2' "$STUB_LOG" | grep -c "model=claude-opus-5-5 .*cwd=$ND/fixer-2")" 1
check "round 2 commit merged into the night branch" "$(repo show nightly-fixes-$NIGHT:fix2.txt 2>/dev/null | head -1)" "fix 2"
check "round 1 commit still there" "$(repo show nightly-fixes-$NIGHT:fix1.txt 2>/dev/null | head -1)" "fix 1"
check "round 2 marked merged" "$([ -e "$ND/.fixer-2.merged" ] && echo yes || echo no)" yes
check "checkout still on the night branch, clean" "$(repo branch --show-current) $(repo status --porcelain | wc -l)" "nightly-fixes-$NIGHT 0"
check "summary only now due" "$(ns tick-decide)" "summary $NIGHT"
check "summary has not run yet" "$(grep -c '^summary' "$STUB_LOG")" 0
ns tick > /dev/null 2>&1
check_true "summary written" wait_for "$ND/summary.md"
check "summary ran on opus" "$(grep -c '^summary model=claude-opus-5-5' "$STUB_LOG")" 1
check "summary ran after round 2" "$(awk '/^fixer2/{f=1} f && /^summary/{print "after"; exit}' "$STUB_LOG")" after
check_true "summary prompt names both fixer branches" bash -c "grep -q 'nightly-opus-$NIGHT-r1' '$TMP/night/prompt-summary.txt' && grep -q 'nightly-opus-$NIGHT-r2' '$TMP/night/prompt-summary.txt'"
check "nothing left to do" "$(ns tick-decide)" ""

# A fixer merge that breaks the build is dropped before the game runs, and marked rejected.
echo "== merge that breaks the build is dropped"
export STUB_BREAK_ROUND=1
BUILD_CMD='test ! -f broken.flag' setup_env "$TMP/broken"
echo "$(epoch '2026-10-07 11:00')" > "$NIGHTLY_NOW_FILE"
NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
export NIGHTLY_END=14:00 NIGHTLY_START=09:00 STUB_RUN_SECONDS=1800 NIGHTLY_MIN_RUN_SECONDS=600
# Two runs fit before the fix round is due; the fixer's commit then breaks the build.
export NIGHTLY_FIX1=11:50
timeout 300 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/broken/loop.out" 2>&1
check "broken merge rejected" "$([ -e "$ND/.fixer-1.rejected" ] && echo yes || echo no)" yes
check "broken file is not in the checkout" "$([ -e "$NIGHTLY_REPO/broken.flag" ] && echo yes || echo no)" no
check_true "report says the merge broke the build" grep -q "broke the build" "$ND/report.md"
check "the fixer's good commit was dropped with it (branch kept)" "$(repo show nightly-opus-$NIGHT-r1:fix1.txt 2>/dev/null | head -1)" "fix 1"
unset STUB_BREAK_ROUND NIGHTLY_END NIGHTLY_START STUB_RUN_SECONDS NIGHTLY_MIN_RUN_SECONDS NIGHTLY_FIX1

# A merge conflict is aborted and leaves the tree clean.
echo "== merge conflict is aborted"
BUILD_CMD=true setup_env "$TMP/conflict"
echo "$(epoch '2026-10-07 09:00')" > "$NIGHTLY_NOW_FILE"
NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
repo checkout -q -b evil; echo "from the fixer" > "$NIGHTLY_REPO/README"; repo commit -q -am evil; repo checkout -q main
echo "from the checkout" > "$NIGHTLY_REPO/README"
mkdir -p "$ND"; echo evil > "$ND/fixer-1.ready"
export NIGHTLY_END=11:00 NIGHTLY_START=09:00 STUB_RUN_SECONDS=1800 NIGHTLY_MIN_RUN_SECONDS=600 NIGHTLY_FIXERS=
timeout 300 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/conflict/loop.out" 2>&1
check "conflicting merge marked conflict" "$([ -e "$ND/.fixer-1.conflict" ] && echo yes || echo no)" yes
check "tree clean after the aborted merge" "$(repo status --porcelain | wc -l)" 0
check "checkout content kept" "$(head -1 "$NIGHTLY_REPO/README")" "from the checkout"
unset NIGHTLY_END NIGHTLY_START STUB_RUN_SECONDS NIGHTLY_MIN_RUN_SECONDS NIGHTLY_FIXERS

# A proctor asks for round 1 early: it starts at the next tick after the first entry, not at 23:59.
echo "== dry run with an early fix request from a proctor"
export STUB_REQUEST="the game cannot start & every run dies (decision 1, runs/01/run.log)"
BUILD_CMD=true setup_env "$TMP/early"
echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
timeout 600 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/early/loop.out" 2>&1
check "early: round 1 fixer started exactly once" "$(grep -c '^fixer1' "$STUB_LOG")" 1
e_clock=$(grep '^fixer1' "$STUB_LOG" | sed 's/.*clock=\([0-9]*\).*/\1/')
check_true "early: round 1 started before 23:59 (and after 20:30)" \
  bash -c "[ $e_clock -lt $(epoch '2026-10-07 23:59') ] && [ $e_clock -gt $(epoch '2026-10-07 20:30') ]"
check "early: the reason reaches the fixer prompt" "$(grep -c 'asked for this round early because: the game cannot start & every run dies (decision 1, runs/01/run.log)' "$TMP/early/prompt-fixer1.txt")" 1
check "early: no unreplaced placeholder in the prompt" "$(grep -c '{{' "$TMP/early/prompt-fixer1.txt")" 0
check_true "early: the report notes the early start with the reason" grep -q "^## Fixer round 1 started early on a proctor's request (run 01-2030): the game cannot start & every run dies" "$ND/report.md"
check "early: round 1 merged as usual" "$([ -e "$ND/.fixer-1.merged" ] && echo yes || echo no)" yes
unset STUB_REQUEST
# A request made once round 1 had started is kept for round 2 and reaches its prompt and the report.
printf 'time: x\nrun: 02-2200\nreason: second early problem (seed 9)\n' > "$ND/fix-requested-2"
echo "$(epoch '2026-10-08 06:00')" > "$NIGHTLY_NOW_FILE"
bash "$NIGHTLY_SH" fix 2 "$NIGHT" > "$TMP/early/fix2.out" 2>&1
check "early round 2: the reason reaches the fixer prompt" "$(grep -c 'asked for this round because: second early problem (seed 9)' "$TMP/early/prompt-fixer2.txt")" 1
check_true "early round 2: the report notes the request" grep -q "^## Fixer round 2 started on a proctor's request (run 02-2200): second early problem (seed 9)" "$ND/report.md"

# No free disk: no run directory, no phantom run, a report line; fixer rounds fail visibly and are
# retried once, then given up.
echo "== low disk"
BUILD_CMD=true setup_env "$TMP/lowdisk"
cat > "$TMP/lowdisk/df" <<'DF'
#!/usr/bin/env bash
echo "Filesystem 1024-blocks Used Available Capacity Mounted on"
echo "/dev/x 104857600 99614720 $(cat "$(dirname "$0")/df.kb") 95% /"
DF
chmod +x "$TMP/lowdisk/df"
echo 1048576 > "$TMP/lowdisk/df.kb" # 1 GB free, 10 GB needed
export NIGHTLY_MIN_FREE_GB=10 NIGHTLY_DF_CMD="$TMP/lowdisk/df" NIGHTLY_DISK_WAIT_SECONDS=0 NIGHTLY_MAX_DISK_WAITS=3 NIGHTLY_FIXERS=
echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/lowdisk/loop.out" 2>&1
check "low disk: no run directory was created" "$(find "$ND/runs" -mindepth 1 2>/dev/null | wc -l)" 0
check "low disk: no proctor was launched" "$(grep -c '^proctor' "$STUB_LOG")" 0
check_true "low disk: the report says the sweep waited for space" grep -q '^## Waiting for disk space: 1 GB free' "$ND/report.md"
check_true "low disk: the report says the sweep gave up" grep -q '^## Sweep stopped: the disk stayed below 10 GB' "$ND/report.md"
check "low disk: the sweep still finished (sweep.done)" "$([ -e "$ND/sweep.done" ] && echo yes || echo no)" yes
echo 52428800 > "$TMP/lowdisk/df.kb" # 50 GB free: plenty
export NIGHTLY_MAX_DISK_WAITS=30 STUB_RUN_SECONDS=34000 # ends the window: exactly one run
rm -f "$ND/sweep.done"
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/lowdisk/loop2.out" 2>&1
check "enough disk: a run starts again" "$(grep -c '^proctor' "$STUB_LOG")" 1
unset STUB_RUN_SECONDS

# fixer round: low disk fails visibly, is retried once, then given up (summary not blocked)
export NIGHTLY_FIXERS="1 2" NIGHTLY_FIX_RETRY_SECONDS=1800
echo 1048576 > "$TMP/lowdisk/df.kb"
echo "$(epoch '2026-10-08 00:00')" > "$NIGHTLY_NOW_FILE"
bash "$NIGHTLY_SH" fix 1 "$NIGHT" > "$TMP/lowdisk/fix1.out" 2>&1
check_true "fixer low disk: attempt 1 is in the report" grep -q '^## Fixer round 1 failed (attempt 1 of 2, retrying in 1800s): only 1 GB free' "$ND/report.md"
check "fixer low disk: not marked done after the first failure" "$([ -e "$ND/.fixer-1.done" ] && echo yes || echo no)" no
check "fixer low disk: started marker removed so the round can be offered again" "$([ -e "$ND/.fixer-1-started" ] && echo yes || echo no)" no
check "fixer low disk: no worktree was created" "$([ -e "$ND/fixer-1" ] && echo yes || echo no)" no
bash "$NIGHTLY_SH" fix 1 "$NIGHT" > "$TMP/lowdisk/fix1b.out" 2>&1
check_true "fixer low disk: attempt 2 gives up in the report" grep -q '^## Fixer round 1 failed (attempt 2 of 2, giving up)' "$ND/report.md"
check "fixer low disk: given up counts as done" "$([ -e "$ND/.fixer-1.done" ] && echo yes || echo no)" yes
check "fixer low disk: no fixer was ever run" "$(grep -c '^fixer' "$STUB_LOG")" 0

# a failed worktree add is reported and retried too, instead of silently ending the round
echo 52428800 > "$TMP/lowdisk/df.kb"
rm -f "$ND/.fixer-2-started" "$ND/.fixer-2.done" "$ND/.fixer-2.failed"
echo "not a directory" > "$ND/fixer-2" # `git worktree add` refuses an existing path
bash "$NIGHTLY_SH" fix 2 "$NIGHT" > "$TMP/lowdisk/fix2.out" 2>&1
check_true "worktree failure: reported with the reason" grep -q '^## Fixer round 2 failed (attempt 1 of 2, retrying in 1800s): cannot create worktree' "$ND/report.md"
check "worktree failure: retry is not blocked by a done marker" "$([ -e "$ND/.fixer-2.done" ] && echo yes || echo no)" no
rm -f "$ND/fixer-2"
echo "$(epoch '2026-10-08 06:00')" > "$NIGHTLY_NOW_FILE"
echo "1 $(epoch '2026-10-08 06:00')" > "$ND/.fixer-2.failed"
touch "$ND/sweep.done"
check "worktree failure: not offered again before the delay" "$(ns tick-decide)" ""
echo "$(epoch '2026-10-08 06:31')" > "$NIGHTLY_NOW_FILE"
check "worktree failure: offered again after the delay" "$(ns tick-decide)" "fix $NIGHT 2"
bash "$NIGHTLY_SH" fix 2 "$NIGHT" > "$TMP/lowdisk/fix2b.out" 2>&1
check "worktree retry ran the fixer" "$(grep -c '^fixer2' "$STUB_LOG")" 1
check "worktree retry finished: done marker" "$([ -e "$ND/.fixer-2.done" ] && echo yes || echo no)" yes
unset NIGHTLY_MIN_FREE_GB NIGHTLY_DF_CMD NIGHTLY_DISK_WAIT_SECONDS NIGHTLY_MAX_DISK_WAITS NIGHTLY_FIXERS NIGHTLY_FIX_RETRY_SECONDS

# A sweep killed mid-run: the trap stops the proctor and the game, files a digest-only entry marked
# as terminated, leaves no process behind; a restart continues the numbering after the highest
# existing run (gaps and foreign directories ignored).
echo "== sweep terminated mid-run, numbering after a restart"
BUILD_CMD=true setup_env "$TMP/term"
cat > "$NIGHTLY_CLAUDE" <<'STUB'
#!/usr/bin/env bash
prompt=""; while [ $# -gt 0 ]; do case "$1" in --) shift; prompt="$1"; break ;; *) shift ;; esac; done
rd=$(printf '%s' "$prompt" | sed -n 's/^Run directory: //p' | head -1)
mkdir -p "$rd/trace"
echo "proctor $(basename "$rd")" >> "$STUB_LOG"
if [ -n "${STUB_HANG:-}" ]; then
  port=34567
  echo "{\"backend_port\": $port, \"preset\": \"stub\", \"players\": 3, \"policy\": \"steer\"}" > "$rd/meta.json"
  echo '{"decision": 41, "round": 3, "phase": "action", "player": "p1", "subtype": "x"}' > "$rd/trace/trace.jsonl"
  setsid env TI4_E2E_BACKEND_PORT=$port sleep 300 < /dev/null > /dev/null 2>&1 &
  echo $! > "$rd/run.pid"; echo $! > "$STUB_DIR/fake-game.pid"
  sleep 300 & echo $! > "$STUB_DIR/proctor-child.pid"
  echo started > "$STUB_DIR/hang.ready"
  wait
  exit 0
fi
echo '{"preset":"stub"}' > "$rd/meta.json"
echo $(( $(cat "$NIGHTLY_NOW_FILE") + ${STUB_RUN_SECONDS:-3000} )) > "$NIGHTLY_NOW_FILE"
printf '## Run %s — clean (stub)\n' "$(basename "$rd")"
STUB
NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"
echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
mkdir -p "$ND/runs/01-2030" "$ND/runs/05-2100" "$ND/runs/notes" "$ND/runs/xx-1234" "$ND/runs/03-99"
export NIGHTLY_FIXERS= STUB_HANG=1
bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/term/loop.out" 2>&1 &
LOOP=$!
check_true "term: the proctor and its game started" wait_for "$TMP/term/hang.ready"
check "term: first run continues after the highest existing number (gaps, foreign dirs ignored)" "$(ls "$ND/runs" | grep -c '^06-2030$')" 1
gamepid=$(cat "$TMP/term/fake-game.pid"); childpid=$(cat "$TMP/term/proctor-child.pid")
check_true "term: game and proctor child are alive before the signal" bash -c "kill -0 $gamepid && kill -0 $childpid"
kill -TERM "$LOOP"
t0=$SECONDS
wait "$LOOP"; rc=$?
check "term: the loop exits with 143" "$rc" 143
check_true "term: the cleanup did not hang (under 30 s)" test $((SECONDS - t0)) -lt 30
check_true "term: the game process is gone" bash -c "! kill -0 $gamepid 2>/dev/null"
check_true "term: the proctor's child is gone" bash -c "! kill -0 $childpid 2>/dev/null"
check "term: no process carries the run's port" "$(grep -lsz '^TI4_E2E_BACKEND_PORT=34567$' /proc/[0-9]*/environ 2>/dev/null | wc -l)" 0
check "term: exit_code says killed" "$(cat "$ND/runs/06-2030/exit_code" 2>/dev/null)" killed
check_true "term: digest.md was written" test -s "$ND/runs/06-2030/digest.md"
check "term: one terminated-mid-run entry in the report" "$(grep -c '^## Run 06-2030 — sweep terminated mid-run (SIGTERM)' "$ND/report.md")" 1
check_true "term: the entry names the last decision and the evidence path" bash -c "grep -q 'Last decision: #41, round 3. Evidence: .$ND/runs/06-2030.' '$ND/report.md'"
check "term: no sweep.done (cron restarts the sweep)" "$([ -e "$ND/sweep.done" ] && echo yes || echo no)" no
check_true "term: the log shows the exit" grep -q 'sweep exited on signal or exit' "$TMP/term/loop.out"
check "term: the EXIT trap after the signal added no second entry" "$(grep -c 'sweep terminated mid-run' "$ND/report.md")" 1
# Restart: the next run is 07, names stay unique.
unset STUB_HANG
export STUB_RUN_SECONDS=34000
timeout 120 bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/term/loop2.out" 2>&1
check "restart: the next run is 07" "$(ls "$ND/runs" | grep -c '^07-')" 1
check "restart: no second 01- or 06- run was made" "$(ls "$ND/runs" | grep -c '^0[16]-')" 2
check "restart: the loop finished normally" "$([ -e "$ND/sweep.done" ] && echo yes || echo no)" yes
# A signal while no run is active (waiting for disk) adds no entry and does not hang.
rm -f "$ND/sweep.done"
echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
export NIGHTLY_MIN_FREE_GB=999999 NIGHTLY_DISK_WAIT_SECONDS=100 NIGHTLY_MAX_DISK_WAITS=5
before=$(grep -c '^## ' "$ND/report.md")
bash "$NIGHTLY_SH" loop "$NIGHT" > "$TMP/term/loop3.out" 2>&1 &
LOOP=$!
sleep 2
kill -TERM "$LOOP"; t0=$SECONDS; wait "$LOOP"; rc=$?
check "idle signal: exits with 143 promptly" "$rc $([ $((SECONDS - t0)) -lt 10 ] && echo fast)" "143 fast"
check "idle signal: only the disk note was added (no run entry)" "$(( $(grep -c '^## ' "$ND/report.md") - before ))" 1
unset NIGHTLY_MIN_FREE_GB NIGHTLY_DISK_WAIT_SECONDS NIGHTLY_MAX_DISK_WAITS STUB_RUN_SECONDS NIGHTLY_FIXERS

# the numbering rule on its own
mk="$TMP/numbering"
mkdir -p "$mk/empty" "$mk/gaps/02-2100" "$mk/gaps/11-0100" "$mk/gaps/7-0100" "$mk/gaps/09-0010" "$mk/gaps/readme" "$mk/gaps/10-12345" "$mk/hundred/99-0100" "$mk/hundred/100-0200"
touch "$mk/gaps/99-0000" # a file, not a run directory
hr() { NIGHTLY_REPO=/nonexistent bash "$HERE/nightly.sh" highest-run-number "$1"; }
check "numbering: no runs directory" "$(hr "$mk/missing")" 0
check "numbering: empty runs directory" "$(hr "$mk/empty")" 0
check "numbering: gaps, one-digit, long suffix, files and foreign names" "$(hr "$mk/gaps")" 11
check "numbering: past 99" "$(hr "$mk/hundred")" 100


echo
if [ "$failures" -eq 0 ]; then echo "all $checks checks passed"; else echo "$failures of $checks checks FAILED"; fi
[ "$failures" -eq 0 ]
