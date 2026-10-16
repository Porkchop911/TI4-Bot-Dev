#!/usr/bin/env bash
# Scripted tests for the parallel slots of the nightly sweep (NIGHTLY_SLOTS=N): two proctors at a
# time with stub `claude` binaries and a fake clock, in a temp dir. Needs only bash, git, flock and
# python3: no real proctors, fixers, games or builds, and it never touches the live checkout or
# nightly-reports. The single-slot behaviour (NIGHTLY_SLOTS=1) is covered by test_schedule.sh,
# which runs unchanged; this file adds a few checks that the single slot is the old sweep.
#
#   tools/nightly-smoke/test_slots.sh
set -uo pipefail
export NIGHTLY_PREFLIGHT=0 # the fake repos have no node_modules
export MAX_ZERO_DECISION_RUNS=0 # the stub games write no harness report
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
trap 'kill $(jobs -p) 2>/dev/null; [ -n "${KEEP_TMP:-}" ] || rm -rf "$TMP"' EXIT

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
# Ports of the fake games. Games are found and killed by the port in their environment (like the
# real ones), so a port shared with another test run (test_schedule.sh, a parallel session) would
# get the fake games killed: pick a random pair.
export STUB_PORT=$((30000 + (RANDOM % 8000) * 2))
wait_for() { # wait_for <file> [seconds]
  local i; for i in $(seq 1 $((${2:-30} * 5))); do [ -e "$1" ] && return 0; sleep 0.2; done; return 1
}
yesno() { if "$@"; then echo yes; else echo no; fi; }

# A universal stub `claude`. Behaviour comes from environment variables:
#   STUB_SLEEP        real seconds a proctor "plays" (default 1)
#   STUB_RUN_SECONDS  fake-clock seconds each run adds when it ends (default 8000)
#   STUB_HANG         the proctor starts a fake game (its port depends on the slot) and hangs
#   STUB_REPAIR       the proctor commits a file repair-<run>.txt in its repo
#   STUB_CONFLICT     the proctor of slot 2 commits a change to README, then marks a fixer branch ready
#   STUB_LATE_FIX     the first run of slot 1 marks fixbr ready while it plays
write_stub() { # write_stub <path>
  cat > "$1" <<'STUB'
#!/usr/bin/env bash
model=""; prompt=""
while [ $# -gt 0 ]; do
  case "$1" in --model) model="$2"; shift 2 ;; --) shift; prompt="$1"; break ;; *) shift ;; esac
done
advance() { ( flock 8; echo $(( $(cat "$NIGHTLY_NOW_FILE") + $1 )) > "$NIGHTLY_NOW_FILE" ) 8> "$STUB_DIR/clock.lock"; }
case "$prompt" in
  *"nightly fixer"*)
    round=$(printf '%s' "$prompt" | sed -n 's/.*(round \([0-9]*\)).*/\1/p' | head -1)
    echo "fixer$round cwd=$(pwd) repair2_present=$([ -f repair-s2-01.txt ] && echo yes || echo no)" >> "$STUB_LOG"
    printf '%s' "$prompt" > "$STUB_DIR/prompt-fixer$round.txt"
    echo "fix $round" > "fix$round.txt"; git add "fix$round.txt"; git commit -q -m "stub fix round $round"
    printf '## Fixer round %s\n### Fixed\n- stub\n' "$round" ;;
  *"You are a **proctor**"*)
    rd=$(printf '%s' "$prompt" | sed -n 's/^Run directory: //p' | head -1)
    name=$(basename "$rd"); slot=${name%%-*}
    mkdir -p "$rd/trace"
    printf '%s' "$prompt" > "$STUB_DIR/prompt-$name.txt"
    echo "start $name $(date +%s.%N) cwd=$(pwd) slot=${NIGHTLY_SLOT:-} run=${NIGHTLY_RUN_NAME:-} game_repo=${NIGHTLY_GAME_REPO:-} target=${CARGO_TARGET_DIR:-} fix1=$([ -f fix1.txt ] && echo yes || echo no)" >> "$STUB_LOG"
    if [ -n "${STUB_HANG:-}" ]; then
      case "$slot" in s2) port=$((STUB_PORT + 1)) ;; *) port=$STUB_PORT ;; esac
      echo "{\"backend_port\": $port, \"preset\": \"stub\", \"players\": 3, \"policy\": \"steer\"}" > "$rd/meta.json"
      echo "{\"decision\": 4${slot#s}, \"round\": 3, \"phase\": \"action\", \"player\": \"p1\", \"subtype\": \"x\"}" > "$rd/trace/trace.jsonl"
      setsid env TI4_E2E_BACKEND_PORT=$port sleep 300 < /dev/null > /dev/null 2>&1 &
      echo $! > "$rd/run.pid"; echo $! > "$STUB_DIR/fake-game-$slot.pid"
      sleep 300 & echo $! > "$STUB_DIR/proctor-child-$slot.pid"
      echo started > "$STUB_DIR/hang-$slot.ready"
      wait
      advance 8000 # whatever ended the wait, the fake clock must move on or the sweep would spin
      exit 0
    fi
    echo '{"preset":"stub"}' > "$rd/meta.json"
    "$(dirname "$NIGHTLY_SH")/nightly.sh" tick-decide >> "$STUB_DIR/tick-decide.log" 2>&1
    if [ -n "${STUB_REPAIR:-}" ]; then
      echo "repair $name" > "repair-$name.txt"; git add "repair-$name.txt"; git commit -q -m "stub repair $name"
    fi
    if [ -n "${STUB_CONFLICT:-}" ] && [ "$slot" = s2 ] && [ ! -f "$STUB_DIR/conflict.done" ]; then
      echo "from slot 2" > README; git commit -q -am "stub: slot 2 edits README"
      touch "$STUB_DIR/conflict.done"
      echo fixbr > "$STUB_DIR/reports/2026-10-07/fixer-1.ready"
    fi
    if [ -n "${STUB_LATE_FIX:-}" ] && [ "$name" = "s1-01-2030" ]; then echo fixbr > "$STUB_DIR/reports/2026-10-07/fixer-1.ready"; fi
    sleep "${STUB_SLEEP:-1}"
    echo "end $name $(date +%s.%N) fix1=$([ -f fix1.txt ] && echo yes || echo no)" >> "$STUB_LOG"
    advance "${STUB_RUN_SECONDS:-8000}"
    printf '## Run %s — clean (stub)\n' "$name" ;;
  *)
    echo "summary" >> "$STUB_LOG"; printf '%s' "$prompt" > "$STUB_DIR/prompt-summary.txt"; printf '# Summary stub\n' ;;
esac
STUB
  chmod +x "$1"
}

setup_env() { # setup_env <dir>: temp repo with the scripts under test, stubs, fake clock, slot knobs
  E="$1"; mkdir -p "$E"
  export NIGHTLY_MIN_FREE_GB=0 NIGHTLY_REPO="$E/repo" NIGHTLY_REPORT_ROOT="$E/reports" NIGHTLY_NOW_FILE="$E/clock" \
    NIGHTLY_CLAUDE="$E/claude" NIGHTLY_NOT_BEFORE=2026-10-07 NIGHTLY_FIXERS= \
    NIGHTLY_BUILD_CMD="bash '$E/build.sh'" \
    NIGHTLY_SLOT_STAGGER_SECONDS=0 NIGHTLY_SLOT_SETTLE_SECONDS=0 NIGHTLY_SLOT_POLL_SECONDS=1 NIGHTLY_MEM_WAIT_SECONDS=1 \
    NIGHTLY_MEM_AVAIL_CMD="cat '$E/mem'" NIGHTLY_FAILED_RUN_SLEEP=1 \
    STUB_LOG="$E/stub.log" STUB_DIR="$E" NIGHTLY_SH="$E/repo/tools/nightly-smoke/nightly.sh" \
    GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
  unset NIGHTLY_SLOTS NIGHTLY_END NIGHTLY_START
  echo 99999 > "$E/mem"
  git init -q -b main "$NIGHTLY_REPO"
  echo hi > "$NIGHTLY_REPO/README"
  # The stand-in for BUILD_CMD: logs where it ran and which cargo target it was given; fails in the
  # worktrees listed in STUB_BUILD_FAIL (e.g. "slot-2").
  cat > "$E/build.sh" <<'BUILD'
#!/usr/bin/env bash
echo "$PWD target=${CARGO_TARGET_DIR:-none}" >> "$STUB_DIR/builds.log"
for s in ${STUB_BUILD_FAIL:-}; do case "$PWD" in */$s/repo) echo "boom" >&2; exit 1 ;; esac; done
exit 0
BUILD
  mkdir -p "$NIGHTLY_REPO/tools" "$NIGHTLY_REPO/web/node_modules"; cp -r "$HERE" "$NIGHTLY_REPO/tools/nightly-smoke"
  echo '{}' > "$NIGHTLY_REPO/web/package.json"
  echo "pkg" > "$NIGHTLY_REPO/web/node_modules/pkg.txt"
  printf '/web/node_modules\n' > "$NIGHTLY_REPO/.gitignore"
  git -C "$NIGHTLY_REPO" add -A; git -C "$NIGHTLY_REPO" commit -q -m init
  echo wip > "$NIGHTLY_REPO/wip.txt" # uncommitted work: must be in the night branch (snapshot)
  : > "$STUB_LOG"
  write_stub "$NIGHTLY_CLAUDE"
  echo "$(epoch '2026-10-07 20:30')" > "$NIGHTLY_NOW_FILE"
  NIGHT=2026-10-07; ND="$NIGHTLY_REPORT_ROOT/$NIGHT"; NB="nightly-fixes-$NIGHT"
}
repo() { git -C "$NIGHTLY_REPO" "$@"; }
run_loop() { # run_loop <label> [timeout]
  timeout "${2:-120}" bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/$1.out" 2>&1
}
stray() { pgrep -f "$E/claude" | wc -l; }

# ---------------------------------------------------------------------------------------------
echo "== two slots side by side"
setup_env "$TMP/two"
export NIGHTLY_SLOTS=2 STUB_RUN_SECONDS=8000 STUB_REPAIR=1
run_loop loop
check "loop finished and wrote sweep.done" "$(yesno test -e "$ND/sweep.done")" yes
check "slots file records 2" "$(cat "$ND/slots")" 2
check "the live checkout is on the night branch" "$(repo branch --show-current)" "$NB"
runs=$(ls "$ND/runs")
check "every run name is sK-NN-HHMM" "$(echo "$runs" | grep -vcE '^s[12]-[0-9]{2}-[0-9]{4}$')" 0
check "run names are unique" "$(echo "$runs" | sort | uniq -d | wc -l)" 0
check_true "both slots played" bash -c "echo '$runs' | grep -q '^s1-' && echo '$runs' | grep -q '^s2-'"
check_true "each slot counts from 01 without gaps" bash -c "for s in s1 s2; do n=\$(echo '$runs' | grep -c \"^\$s-\"); for i in \$(seq 1 \$n); do echo '$runs' | grep -q \"^\$s-\$(printf %02d \$i)-\" || exit 1; done; done"
check "one report entry per run" "$(grep -c '^## Run ' "$ND/report.md")" "$(echo "$runs" | wc -l)"
check "all entries are the proctors' own" "$(grep -c '^## Run .* — clean (stub)' "$ND/report.md")" "$(echo "$runs" | wc -l)"
check_true "report header names the parallel slots" grep -q '^Parallel slots: 2\.' "$ND/report.md"
check "no memory wait with plenty of memory" "$(grep -c 'waiting for memory' "$ND/report.md")" 0
check_true "the two slots' games overlapped in time" python3 - "$E/stub.log" <<'PY'
import sys
iv = {}
for line in open(sys.argv[1]):
    p = line.split()
    if p[0] == "start": iv.setdefault(p[1], [0, 0])[0] = float(p[2])
    elif p[0] == "end": iv.setdefault(p[1], [0, 0])[1] = float(p[2])
s1 = [v for k, v in iv.items() if k.startswith("s1-")]
s2 = [v for k, v in iv.items() if k.startswith("s2-")]
ok = any(a[0] < b[1] and b[0] < a[1] for a in s1 for b in s2)
sys.exit(0 if ok else 1)
PY
check "proctors of slot 1 ran in slot-1/repo" "$(grep '^start s1-' "$STUB_LOG" | grep -vc "cwd=$ND/slot-1/repo slot=1 ")" 0
check "proctors of slot 2 ran in slot-2/repo" "$(grep '^start s2-' "$STUB_LOG" | grep -vc "cwd=$ND/slot-2/repo slot=2 ")" 0
check "the proctor environment names its game repo and run" "$(grep '^start s2-' "$STUB_LOG" | grep -vc "game_repo=$ND/slot-2/repo ")" 0
check_true "the proctor environment carries the run name" bash -c "grep -q 'start s1-01-2030 .* run=s1-01-2030 ' '$STUB_LOG'"
check "each slot has its own cargo target dir" "$(grep '^start s1-' "$STUB_LOG" | grep -vc "target=$NIGHTLY_REPORT_ROOT/target-slot-1 ")" 0
check "slot 2 target dir differs" "$(grep '^start s2-' "$STUB_LOG" | grep -vc "target=$NIGHTLY_REPORT_ROOT/target-slot-2 ")" 0
check "the build ran in both slot worktrees" "$(sort -u "$E/builds.log" | grep -c "slot-[12]/repo target=$NIGHTLY_REPORT_ROOT/target-slot-[12]$")" 2
check "slot 1's build uses slot 1's target" "$(grep -c "slot-1/repo target=$NIGHTLY_REPORT_ROOT/target-slot-2" "$E/builds.log")" 0
check "slot branches exist" "$(repo branch --list "$NB-s1" "$NB-s2" | wc -l)" 2
check "slot worktree is a real worktree with the snapshot" "$(git -C "$ND/slot-1/repo" show HEAD:wip.txt | head -1)" wip
check "web/node_modules is a private copy in the slot worktrees (not a link)" "$(cat "$ND/slot-2/repo/web/node_modules/pkg.txt") $(test -L "$ND/slot-2/repo/web/node_modules" && echo link || echo dir)" "pkg dir"
check "slot worktree stays clean (node_modules excluded)" "$(git -C "$ND/slot-1/repo" status --porcelain | wc -l)" 0
check "proctor prompt of slot 2 names its repo and branch" "$(grep -c "game runs from \`$ND/slot-2/repo\`" "$E/prompt-s2-01-2030.txt")" 1
check_true "prompt explains the parallel slot" grep -q 'slot 2 of 2' "$E/prompt-s2-01-2030.txt"
check "no unreplaced placeholder in the proctor prompts" "$(cat "$E"/prompt-s*.txt | grep -c '{{')" 0
check_true "repairs of slot 1 are on the night branch afterwards" bash -c "git -C '$NIGHTLY_REPO' show '$NB:repair-s1-01-2030.txt' > /dev/null 2>&1"
check_true "repairs of slot 2 are on the night branch afterwards" bash -c "git -C '$NIGHTLY_REPO' show '$NB:repair-s2-01-2030.txt' > /dev/null 2>&1"
check "slot 1's branch does not hold slot 2's repair" "$(git -C "$ND/slot-1/repo" ls-tree -r --name-only HEAD | grep -c 'repair-s2')" 0
check_true "report says both slot branches were merged" bash -c "grep -q '^## Slot 1: merged $NB-s1' '$ND/report.md' && grep -q '^## Slot 2: merged $NB-s2' '$ND/report.md'"
check "tick never wanted a second sweep while the slots ran" "$(grep -c '^sweep' "$E/tick-decide.log")" 0
check "no stray processes" "$(stray)" 0
check "no run left marked active" "$(ls -a "$ND" | grep -c '^\.active-s')" 0

# ---------------------------------------------------------------------------------------------
echo "== NIGHTLY_SLOTS=1 is the old sweep"
setup_env "$TMP/one"
export NIGHTLY_SLOTS=1 STUB_RUN_SECONDS=12000
unset STUB_REPAIR
run_loop loop
runs=$(ls "$ND/runs")
check "run names are NN-HHMM" "$(echo "$runs" | grep -vcE '^[0-9]{2}-[0-9]{4}$')" 0
check "numbering starts at 01 and has no gaps" "$(echo "$runs" | head -1)-$(echo "$runs" | wc -l)" "01-2030-3"
check "no slot directories, no slot branches, no slot cargo targets" "$(ls "$ND" "$NIGHTLY_REPORT_ROOT" | grep -c 'slot-')" 0
check "no slot branches" "$(repo branch --list '*-s1' '*-s2' | wc -l)" 0
check "proctors ran in the live checkout" "$(grep '^start' "$STUB_LOG" | grep -vc "cwd=$NIGHTLY_REPO slot= run= game_repo= target= ")" 0
check "builds ran in the live checkout without a target override" "$(grep -vc "^$NIGHTLY_REPO target=none$" "$E/builds.log")" 0
check "report header has no slot line" "$(grep -c 'Parallel slots' "$ND/report.md")" 0
check_true "prompt keeps the old branch sentence" grep -q "That branch is shared by every run tonight" "$E/prompt-01-2030.txt"
check "prompt points at the live checkout" "$(grep -c "game runs from \`$NIGHTLY_REPO\`" "$E/prompt-01-2030.txt")" 1
check "no unreplaced placeholder in the single-slot prompt" "$(grep -c '{{' "$E/prompt-01-2030.txt")" 0
check "entries match runs" "$(grep -c '^## Run ' "$ND/report.md")" 3
check "sweep.done" "$(yesno test -e "$ND/sweep.done")" yes
unset STUB_RUN_SECONDS

# ---------------------------------------------------------------------------------------------
echo "== a fixer branch reaches every slot between that slot's games"
setup_env "$TMP/fixer"
repo checkout -q -b fixbr; echo "fix 1" > "$NIGHTLY_REPO/fix1.txt"; git -C "$NIGHTLY_REPO" add fix1.txt; repo commit -q -m "fixer commit"; repo checkout -q main
export NIGHTLY_SLOTS=2 STUB_RUN_SECONDS=8000 STUB_LATE_FIX=1
run_loop loop
check "the ready marker appeared mid-run and is still there for the night branch merge" "$(cat "$ND/fixer-1.ready")" fixbr
check "no game saw its sources change (start = end state)" "$(awk '$1=="start"{split($0,a,"fix1=");s[$2]=a[2]} $1=="end"{split($0,a,"fix1=");if(s[$2]!=a[2])bad++} END{print bad+0}' "$STUB_LOG")" 0
check "the first runs started without the fixer's commit" "$(grep '^start s1-01-' "$STUB_LOG" | grep -c 'fix1=no')" 1
check_true "runs after both first games have it" bash -c "grep '^start s[12]-0[2-9]-' '$STUB_LOG' | grep -q 'fix1=yes' && ! grep '^start s[12]-0[2-9]-' '$STUB_LOG' | grep -q 'fix1=no'"
check "slot 1 merged round 1" "$(yesno test -e "$ND/slot-1/.fixer-1.merged")" yes
check "slot 2 merged round 1" "$(yesno test -e "$ND/slot-2/.fixer-1.merged")" yes
check "each slot merged it exactly once" "$(grep -c '^## Fixer round 1: merged fixbr' "$ND/report.md")" 2
check_true "the report names the slot branch each time" bash -c "grep -q 'merged fixbr (1 commits) into $NB-s1 (slot 1)' '$ND/report.md' && grep -q 'merged fixbr (1 commits) into $NB-s2 (slot 2)' '$ND/report.md'"
check "the night branch has the fixer's commit through the slot branches" "$(repo show "$NB:fix1.txt" | head -1)" "fix 1"
check "the global ready marker is not consumed by the slots" "$(yesno test -e "$ND/.fixer-1.merged")" no

echo "== a fixer merge that conflicts in one slot only"
setup_env "$TMP/fixconf"
repo checkout -q -b fixbr; echo "from the fixer" > "$NIGHTLY_REPO/README"; repo commit -q -am "fixer edits README"; repo checkout -q main
export NIGHTLY_SLOTS=2 STUB_RUN_SECONDS=8000 STUB_CONFLICT=1
unset STUB_LATE_FIX
run_loop loop
check "slot 1 merged the fixer branch" "$(yesno test -e "$ND/slot-1/.fixer-1.merged")" yes
check "slot 2 marked it conflicted" "$(yesno test -e "$ND/slot-2/.fixer-1.conflict")" yes
check "slot 2 did not mark it merged" "$(yesno test -e "$ND/slot-2/.fixer-1.merged")" no
check_true "report says slot 2's merge conflicted and was aborted" grep -q 'merge of fixbr into slot 2 conflicted .* and was aborted' "$ND/report.md"
check "slot 2's worktree is clean after the abort" "$(git -C "$ND/slot-2/repo" status --porcelain | wc -l)" 0
check "slot 2 kept its own repair" "$(git -C "$ND/slot-2/repo" show HEAD:README | head -1)" "from slot 2"
check_true "slot 2 played after the conflict" bash -c "[ \$(ls '$ND/runs' | grep -c '^s2-') -ge 2 ]"
check_true "the integration conflict between the slot branches is reported" grep -q "^## Slot 2: merge of $NB-s2 into $NB conflicted and was aborted" "$ND/report.md"
check "the checkout is clean after the aborted integration" "$(repo status --porcelain | wc -l)" 0
check "the night branch has slot 1's side (the fixer's README)" "$(repo show "$NB:README" | head -1)" "from the fixer"
check_true "the conflicting slot branch is kept" bash -c "git -C '$NIGHTLY_REPO' show-ref -q --verify refs/heads/$NB-s2"
unset STUB_CONFLICT

# ---------------------------------------------------------------------------------------------
echo "== a slot that cannot build does not stop the other"
setup_env "$TMP/buildfail"; export STUB_BUILD_FAIL=slot-2
echo 100 > "$E/mem" # no memory for a second game: slot 1 is alone, so the gate must not apply
export NIGHTLY_SLOTS=2 STUB_RUN_SECONDS=8000
run_loop loop
check_true "report says the build failed on slot 2" grep -q '^## Build failed on slot 2 — no more runs on this slot tonight' "$ND/report.md"
check "slot 2 played nothing (no report entry)" "$(grep -c '^## Run s2-' "$ND/report.md")" 0
check_true "slot 1 played anyway (and not gated by the low memory)" bash -c "[ \$(ls '$ND/runs' | grep -c '^s1-') -ge 2 ]"
check "the failed slot left only its build log, no run" "$(ls "$ND"/runs/s2-*/ | tr '\n' ' ')" "build.log "
check "sweep.done after both slots ended" "$(yesno test -e "$ND/sweep.done")" yes
check "a broken build is not reported for slot 1" "$(grep -c 'Build failed on slot 1' "$ND/report.md")" 0
unset STUB_BUILD_FAIL

# ---------------------------------------------------------------------------------------------
echo "== memory gate delays a start while another game runs"
setup_env "$TMP/mem"
echo 1000 > "$E/mem" # a new game needs 1500 + 1200 * 4 = 6300 MB
export NIGHTLY_SLOTS=2 STUB_RUN_SECONDS=34000 STUB_SLEEP=8
( for _ in $(seq 1 100); do
    grep -q 'waiting for memory' "$ND/report.md" 2>/dev/null && break; sleep 0.2
  done
  sleep 3; date +%s.%N > "$E/raised"; echo 99999 > "$E/mem" ) &
run_loop loop
wait
check "exactly one slot waited for memory" "$(grep -c '^## Slot [12] waiting for memory: 1000 MB available, a new game needs 8700 MB' "$ND/report.md")" 1
check "both slots played once" "$(ls "$ND/runs" | wc -l)" 2
check_true "the waiting slot started only after the memory came back, while the other game still ran" python3 - "$STUB_LOG" "$E/raised" <<'PY'
import sys
raised = float(open(sys.argv[2]).read())
iv = {}
for line in open(sys.argv[1]):
    p = line.split()
    if p[0] == "start": iv.setdefault(p[1], [0, 0])[0] = float(p[2])
    elif p[0] == "end": iv.setdefault(p[1], [0, 0])[1] = float(p[2])
first, second = sorted(iv.values())
sys.exit(0 if second[0] >= raised and second[0] < first[1] else 1)
PY
check "memory need: base + per-player share for the largest table" "$(bash "$NIGHTLY_SH" mem-needed) $(NIGHTLY_PLAYER_COUNTS="3 5" NIGHTLY_MEM_BASE_MB=1000 NIGHTLY_MEM_PER_PLAYER_MB=100 bash "$NIGHTLY_SH" mem-needed)" "8700 1500"
check "no stray processes after the memory test" "$(stray)" 0
unset STUB_SLEEP STUB_RUN_SECONDS

# ---------------------------------------------------------------------------------------------
echo "== SIGTERM mid-run cleans up both runs"
setup_env "$TMP/term"
export NIGHTLY_SLOTS=2 STUB_HANG=1
mkdir -p "$ND/runs/s1-02-2000" "$ND/runs/s2-01-2000" "$ND/runs/s2-05-2100" "$ND/runs/09-2000" "$ND/runs/s3-xx"
bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1 &
LOOP=$!
check_true "term: both proctors and their games started" bash -c "for i in \$(seq 1 150); do [ -e '$E/hang-s1.ready' ] && [ -e '$E/hang-s2.ready' ] && exit 0; sleep 0.2; done; exit 1"
check "term: slot 1 continues after its own highest number (other slots' and foreign runs ignored)" "$(ls "$ND/runs" | grep -c '^s1-03-2030$')" 1
check "term: slot 2 continues after its own highest number" "$(ls "$ND/runs" | grep -c '^s2-06-2030$')" 1
g1=$(cat "$E/fake-game-s1.pid"); g2=$(cat "$E/fake-game-s2.pid"); c1=$(cat "$E/proctor-child-s1.pid"); c2=$(cat "$E/proctor-child-s2.pid")
check_true "term: both games and proctor children are alive before the signal" bash -c "kill -0 $g1 && kill -0 $g2 && kill -0 $c1 && kill -0 $c2"
kill -TERM "$LOOP"
t0=$SECONDS
wait "$LOOP"; rc=$?
check "term: the sweep exits with 143" "$rc" 143
check_true "term: the cleanup of two runs did not hang (under 40 s)" test $((SECONDS - t0)) -lt 40
check_true "term: both game processes are gone" bash -c "! kill -0 $g1 2>/dev/null && ! kill -0 $g2 2>/dev/null"
check_true "term: both proctor children are gone" bash -c "! kill -0 $c1 2>/dev/null && ! kill -0 $c2 2>/dev/null"
check "term: no process carries either run's port" "$(grep -lsz -e "^TI4_E2E_BACKEND_PORT=$STUB_PORT\$" -e "^TI4_E2E_BACKEND_PORT=$((STUB_PORT + 1))\$" /proc/[0-9]*/environ 2>/dev/null | wc -l)" 0
check "term: both exit_code files say killed" "$(cat "$ND/runs/s1-03-2030/exit_code" "$ND/runs/s2-06-2030/exit_code" 2>/dev/null | tr '\n' ' ')" "killed killed "
check_true "term: both digests were written" bash -c "[ -s '$ND/runs/s1-03-2030/digest.md' ] && [ -s '$ND/runs/s2-06-2030/digest.md' ]"
check "term: exactly two entries, one per slot" "$(grep -c 'sweep terminated mid-run (SIGTERM)' "$ND/report.md")" 2
check_true "term: slot 1's entry names its run and last decision" bash -c "grep -q '^## Run s1-03-2030 — sweep terminated mid-run (SIGTERM)' '$ND/report.md' && grep -q 'Last decision: #41, round 3. Evidence: .$ND/runs/s1-03-2030.' '$ND/report.md'"
check_true "term: slot 2's entry names its run and last decision" bash -c "grep -q '^## Run s2-06-2030 — sweep terminated mid-run (SIGTERM)' '$ND/report.md' && grep -q 'Last decision: #42, round 3. Evidence: .$ND/runs/s2-06-2030.' '$ND/report.md'"
check "term: no sweep.done (cron restarts the sweep)" "$(yesno test -e "$ND/sweep.done")" no
check "term: no run left marked active" "$(ls -a "$ND" | grep -c '^\.active-s')" 0
check "term: no stray stub processes" "$(stray)" 0
check_true "term: the log shows the exit" grep -q 'sweep exited on signal or exit' "$E/loop.out"
# Restart: both slots continue their own numbering.
unset STUB_HANG
export STUB_RUN_SECONDS=34000
run_loop loop2
check "restart: slot 1 continues at 04" "$(ls "$ND/runs" | grep -c '^s1-04-')" 1
check "restart: slot 2 continues at 07" "$(ls "$ND/runs" | grep -c '^s2-07-')" 1
check "restart: no run name was reused" "$(ls "$ND/runs" | sort | uniq -d | wc -l)" 0
check "restart: the worktrees were reused (one worktree per slot)" "$(repo worktree list | grep -c 'slot-')" 2
check "restart: finished normally" "$(yesno test -e "$ND/sweep.done")" yes
check "restart: the old entries are still the only terminated ones" "$(grep -c 'sweep terminated mid-run' "$ND/report.md")" 2
unset STUB_RUN_SECONDS

echo "== SIGTERM to the whole process group (as timeout(1) does) still cleans up each run once"
setup_env "$TMP/group"
export NIGHTLY_SLOTS=2 STUB_HANG=1
setsid bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1 &
LOOP=$!
check_true "group: both games started" bash -c "for i in \$(seq 1 150); do [ -e '$E/hang-s1.ready' ] && [ -e '$E/hang-s2.ready' ] && exit 0; sleep 0.2; done; exit 1"
g1=$(cat "$E/fake-game-s1.pid"); g2=$(cat "$E/fake-game-s2.pid")
kill -TERM -- "-$LOOP"
t0=$SECONDS
wait "$LOOP"; rc=$?
check "group: the sweep exits with 143" "$rc" 143
check_true "group: the cleanup did not hang (under 40 s)" test $((SECONDS - t0)) -lt 40
check_true "group: both games are gone" bash -c "! kill -0 $g1 2>/dev/null && ! kill -0 $g2 2>/dev/null"
check "group: exactly one entry per run" "$(grep -c '^## Run s[12]-01-2030 — sweep terminated mid-run (SIGTERM)' "$ND/report.md")" 2
check_true "group: both digests exist" bash -c "[ -s '$ND/runs/s1-01-2030/digest.md' ] && [ -s '$ND/runs/s2-01-2030/digest.md' ]"
check "group: no active or claim files are left" "$(ls -a "$ND" | grep -c '^\.active-s')" 0
check "group: no stray processes" "$(stray)" 0
# A cleanup that was killed half way leaves its claim file; the next sweep closes that run off.
mkdir -p "$ND/runs/s1-05-2100"; echo '{"preset":"stub"}' > "$ND/runs/s1-05-2100/meta.json"
printf '%s\n%s\n%s\n' "$ND/runs/s1-05-2100" s1-05-2100 "" > "$ND/.active-s1.claim.12345"
unset STUB_HANG
export STUB_RUN_SECONDS=34000
run_loop loop2
check_true "claim: the half-cleaned run got its entry at the next start" grep -q '^## Run s1-05-2100 — sweep terminated mid-run (earlier sweep died)' "$ND/report.md"
check "claim: the next run of slot 1 is 06" "$(ls "$ND/runs" | grep -c '^s1-06-')" 1
unset STUB_RUN_SECONDS

echo "== a signal while no slot has a run adds no entry; a stale active file is closed off at the next start"
setup_env "$TMP/idle"
export NIGHTLY_SLOTS=2 NIGHTLY_MIN_FREE_GB=999999 NIGHTLY_DISK_WAIT_SECONDS=100 NIGHTLY_MAX_DISK_WAITS=5
bash "$NIGHTLY_SH" loop "$NIGHT" > "$E/loop.out" 2>&1 &
LOOP=$!
sleep 3
kill -TERM "$LOOP"; t0=$SECONDS; wait "$LOOP"; rc=$?
check "idle: exits with 143 promptly" "$rc $([ $((SECONDS - t0)) -lt 15 ] && echo fast)" "143 fast"
check "idle: no run entry" "$(grep -c '^## Run ' "$ND/report.md")" 0
check_true "idle: both slots reported the disk wait" bash -c "[ \$(grep -c 'Waiting for disk space' '$ND/report.md') -eq 2 ]"
unset NIGHTLY_MIN_FREE_GB NIGHTLY_DISK_WAIT_SECONDS NIGHTLY_MAX_DISK_WAITS
export NIGHTLY_MIN_FREE_GB=0
mkdir -p "$ND/runs/s2-01-2030"
printf '%s\n%s\n%s\n' "$ND/runs/s2-01-2030" s2-01-2030 "" > "$ND/.active-s2"
echo '{"preset":"stub"}' > "$ND/runs/s2-01-2030/meta.json"
export STUB_RUN_SECONDS=34000
run_loop loop2
check_true "stale: the leftover run got a terminated entry" grep -q '^## Run s2-01-2030 — sweep terminated mid-run (earlier sweep died)' "$ND/report.md"
check "stale: the next run of slot 2 is 02" "$(ls "$ND/runs" | grep -c '^s2-02-')" 1
unset STUB_RUN_SECONDS NIGHTLY_MIN_FREE_GB

# ---------------------------------------------------------------------------------------------
echo "== early fix request from a slot"
setup_env "$TMP/req"
export NIGHTLY_FIXERS="1 2"
mkdir -p "$ND/runs/s1-02-2200" "$ND/runs/s2-03-2215" "$ND/slot-2/repo/web"
printf '## Run s1-01-2030 — clean\n' > "$ND/report.md"
printf '%s\n%s\n%s\n' "$ND/runs/s2-03-2215" s2-03-2215 "" > "$ND/.active-s2"
touch "$ND/.loop.lock"; ( exec 9>"$ND/.loop.lock"; flock -n 9 || exit 1; exec sleep 100 ) & HOLD=$!; sleep 0.3 # a sweep is running
request() { # request <dir to run in> <reason...>
  local dir="$1"; shift
  ( cd "$dir" && bash "$HERE/request_fix.sh" "$@" 2>&1 )
}
out=$(NIGHTLY_SLOT=2 NIGHTLY_RUN_NAME=s2-03-2215 request "$TMP" "env route: panic at decision 7")
check_true "env: marker carries run and slot" bash -c "grep -q '^run: s2-03-2215' '$ND/fix-requested' && grep -q '^reason: \[slot 2\] env route: panic at decision 7' '$ND/fix-requested'"
rm -f "$ND/fix-requested"
out=$(request "$ND/slot-2/repo/web" "cwd route: stuck game")
check_true "cwd: the slot comes from the working directory, the run from its active file" bash -c "grep -q '^run: s2-03-2215' '$ND/fix-requested' && grep -q '^reason: \[slot 2\] cwd route: stuck game' '$ND/fix-requested'"
check "the request makes round 1 due as before" "$(bash "$NIGHTLY_SH" tick-decide | tr '\n' '|')" "fix $NIGHT 1|"
rm -f "$ND/fix-requested"
out=$(request "$TMP" "outside any worktree")
check_true "outside a slot the reason is untouched" bash -c "grep -q '^reason: outside any worktree' '$ND/fix-requested' && ! grep -q '\[slot' '$ND/fix-requested'"
kill "$HOLD" 2>/dev/null; wait "$HOLD" 2>/dev/null
unset NIGHTLY_FIXERS

# ---------------------------------------------------------------------------------------------
echo "== a fix round during/after a parallel sweep starts from the slots' repairs"
setup_env "$TMP/fixround"
export NIGHTLY_SLOTS=2 STUB_RUN_SECONDS=34000 STUB_REPAIR=1
run_loop loop
# A repair made on slot 2 after the last integration (as if a proctor committed mid-night).
git -C "$ND/slot-2/repo" commit -q --allow-empty -m "late slot 2 repair"
echo late > "$ND/slot-2/repo/repair-s2-01.txt"; git -C "$ND/slot-2/repo" add repair-s2-01.txt; git -C "$ND/slot-2/repo" commit -q -m "late file"
rm -f "$ND/sweep.done"; touch "$ND/.loop.lock"
( exec 9>"$ND/.loop.lock"; flock -n 9 || exit 1; exec sleep 60 ) & HOLD=$!; sleep 0.3 # a sweep is "running"
export NIGHTLY_FIXERS="1 2"
echo "$(epoch '2026-10-08 00:05')" > "$NIGHTLY_NOW_FILE"
bash "$NIGHTLY_SH" fix 1 "$NIGHT" > "$E/fix1.out" 2>&1
kill "$HOLD" 2>/dev/null; wait "$HOLD" 2>/dev/null
check "fixer round 1 worktree has the slots' repairs (integrated before the round started)" "$(grep '^fixer1' "$STUB_LOG" | grep -c 'repair2_present=yes')" 1
check_true "the fixer prompt tells about the parallel slots" grep -q 'games in parallel' "$E/prompt-fixer1.txt"
check "no unreplaced placeholder in the fixer prompt" "$(grep -c '{{' "$E/prompt-fixer1.txt")" 0
echo "$(epoch '2026-10-08 06:00')" > "$NIGHTLY_NOW_FILE"
touch "$ND/sweep.done"
bash "$NIGHTLY_SH" fix 2 "$NIGHT" > "$E/fix2.out" 2>&1
check "round 2 commit merged into the night branch after the sweep" "$(repo show "$NB:fix2.txt" | head -1)" "fix 2"
check "round 1 commit merged too (it was ready when round 2 started)" "$(repo show "$NB:fix1.txt" | head -1)" "fix 1"
bash "$NIGHTLY_SH" summary "$NIGHT" > "$E/summary.out" 2>&1
check_true "summary prompt describes the slots and has no placeholder left" bash -c "grep -q 'games in parallel' '$E/prompt-summary.txt' && ! grep -q '{{' '$E/prompt-summary.txt'"
unset NIGHTLY_FIXERS STUB_REPAIR STUB_RUN_SECONDS

echo "== three slots, staggered starts"
setup_env "$TMP/three"
export NIGHTLY_SLOTS=3 NIGHTLY_SLOT_STAGGER_SECONDS=2 STUB_RUN_SECONDS=34000 STUB_SLEEP=8
run_loop loop
check "one run per slot" "$(ls "$ND/runs" | sort | tr '\n' ' ')" "s1-01-2030 s2-01-2030 s3-01-2030 "
check "slot worktrees for all three" "$(ls -d "$ND"/slot-*/repo | wc -l)" 3
check_true "slot K started (K-1) x stagger after slot 1" python3 - "$STUB_LOG" <<'PY'
import sys
t = {}
for line in open(sys.argv[1]):
    p = line.split()
    if p[0] == "start": t[p[1][:2]] = float(p[2])
sys.exit(0 if t["s2"] - t["s1"] >= 1.8 and t["s3"] - t["s1"] >= 3.8 else 1)
PY
check "report has three entries" "$(grep -c '^## Run s[123]-01-2030' "$ND/report.md")" 3
check "no stray processes" "$(stray)" 0
unset NIGHTLY_SLOT_STAGGER_SECONDS STUB_RUN_SECONDS STUB_SLEEP

echo
if [ "$failures" -eq 0 ]; then echo "all $checks checks passed"; else echo "$failures of $checks checks FAILED"; fi
[ "$failures" -eq 0 ]
