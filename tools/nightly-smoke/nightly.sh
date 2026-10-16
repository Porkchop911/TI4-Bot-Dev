#!/usr/bin/env bash
# Nightly random-UI smoke sweep: Sonnet proctors play games from 20:30 to 06:00 Berlin, Opus fixes
# the first real findings at 23:59 (round 1, earlier when a proctor asks via request_fix.sh) and again after the sweep (round 2), and an Opus
# morning summary follows round 2. See README.md.
#
#   nightly.sh tick              called by cron every 5 minutes; starts whatever is due
#   nightly.sh tick-decide       prints what tick would start, without starting it (tests)
#   nightly.sh loop [night]      the sweep itself: one proctored run after another until END
#                                (NIGHTLY_SLOTS=N: N such sequences side by side, see README.md)
#   nightly.sh fix <round> [night]   an Opus fix round, working in its own git worktree
#   nightly.sh summary [night]   the Opus morning summary (read-only)
#
# Output: $REPORT_ROOT/<night>/report.md (one entry per run, appended by the proctors),
#         fixer-<round>.md (fix reports), summary.md (morning summary), runs/<NN-HHMM>/ (evidence).
set -uo pipefail
source "$(dirname "$0")/config.sh"

# A nested `claude -p` started from inside a Claude Code session must not inherit its session.
for var in $(env | grep -o '^CLAUDE[A-Z_]*' || true); do unset "$var"; done
unset AI_AGENT

set_night() {
  NIGHT="$1"
  read -r START_EPOCH END_EPOCH < <(window_for_night "$NIGHT")
  NIGHT_DIR="$REPORT_ROOT/$NIGHT"
  LOCK="$NIGHT_DIR/.loop.lock"
  FIX_BRANCH="nightly-fixes-$NIGHT"
}
default_night() { current_window | cut -d' ' -f1; }

render() { # render <template> KEY=VALUE...
  local text
  text=$(cat "$1")
  shift
  for pair in "$@"; do text=${text//"{{${pair%%=*}}}"/"${pair#*=}"}; done
  printf '%s' "$text"
}

lock_held() { [ -e "$1" ] && ! flock -n "$1" true 2>/dev/null; }
sweep_running() { lock_held "$LOCK"; }
fixer_enabled() { case " $FIXERS " in *" $1 "*) return 0 ;; esac; return 1; }
report_entries() {
  local n=0
  [ -f "$NIGHT_DIR/report.md" ] && n=$(grep -c '^## Run ' "$NIGHT_DIR/report.md" || true)
  echo "$n"
}
fix1_epoch() {
  local e
  e=$(day_epoch "$NIGHT" "$FIX1_HHMM")
  [ "$e" -lt "$START_EPOCH" ] && e=$(day_epoch "$(shift_day "$NIGHT" +1)" "$FIX1_HHMM")
  echo "$e"
}
# The morning summary waits for fix round 2 (done, skipped, disabled, or hopelessly late).
fixer2_gate_open() {
  fixer_enabled 2 || return 0
  [ -f "$NIGHT_DIR/.fixer-2.done" ] && return 0
  [ "$(now_epoch)" -ge $(( END_EPOCH + FIX2_MAX_SECONDS + 1800 )) ] && return 0
  return 1
}
# Round 2 and the summary of the previous night use the same checkout as the next sweep, so the
# sweep waits while either is running or is already due (decide_night prints an action for it).
previous_pipeline_busy() (
  set_night "$(shift_day "$NIGHT" -1)"
  lock_held "$NIGHT_DIR/.fixer-1.lock" || lock_held "$NIGHT_DIR/.fixer-2.lock" \
    || lock_held "$NIGHT_DIR/.summary.lock" || [ -n "$(decide_night "$NIGHT" 0)" ]
)

# ---------------------------------------------------------------------------------------------
# Hand-off of fixer branches into the night branch. The fixers work in their own worktrees; their
# commits arrive in $REPO only here, between games, so no game ever runs on changing sources.
# ---------------------------------------------------------------------------------------------
# Everything that goes into report.md is one append under a lock, so entries of parallel slots
# (and the notes of the fix rounds) never interleave.
report_append() { # report_append: stdin is appended as a whole
  local text
  text=$(cat; echo x)
  ( flock 5; printf '%s' "${text%x}" >> "$NIGHT_DIR/report.md" ) 5>> "$NIGHT_DIR/.report.lock"
}
note_report() { printf '%s\n\n' "$*" | report_append; }
note_once() { # note_once <round> <text>
  [ -f "$NIGHT_DIR/.merge-note-$1" ] && return 0
  touch "$NIGHT_DIR/.merge-note-$1"
  note_report "## Fixer round $1 not merged yet: $2"
  log "fixer round $1: $2"
}
MERGED_NOW=""
merge_pending() { # merge_pending <verify: 1 builds after each merge and drops a merge that breaks it>
  local verify="$1" r branch pre count
  for r in 1 2; do
    [ -f "$NIGHT_DIR/fixer-$r.ready" ] || continue
    branch=$(cat "$NIGHT_DIR/fixer-$r.ready")
    if [ "$(git -C "$REPO" branch --show-current)" != "$FIX_BRANCH" ]; then
      note_once "$r" "the checkout is on '$(git -C "$REPO" branch --show-current)', not on $FIX_BRANCH; branch $branch is kept for the user"
      continue
    fi
    pre=$(git -C "$REPO" rev-parse HEAD)
    count=$(git -C "$REPO" rev-list --count "$pre..$branch")
    if git -C "$REPO" merge --no-edit -q "$branch" >> "$NIGHT_DIR/merge.log" 2>&1; then
      mv "$NIGHT_DIR/fixer-$r.ready" "$NIGHT_DIR/.fixer-$r.merged"
      MERGED_NOW="$MERGED_NOW $r"
      note_report "## Fixer round $r: merged $branch ($count commits) into $FIX_BRANCH"
      log "fixer round $r: merged $branch ($count commits)"
      if [ "$verify" = 1 ] && ! (cd "$REPO" && bash -c "$BUILD_CMD") >> "$NIGHT_DIR/merge.log" 2>&1; then
        git -C "$REPO" reset -q --hard "$pre" >> "$NIGHT_DIR/merge.log" 2>&1
        mv "$NIGHT_DIR/.fixer-$r.merged" "$NIGHT_DIR/.fixer-$r.rejected"
        note_report "## Fixer round $r: the merge broke the build and was dropped (branch $branch is kept)"
        log "fixer round $r: merge broke the build, dropped"
      fi
    else
      git -C "$REPO" merge --abort >> "$NIGHT_DIR/merge.log" 2>&1 || true
      mv "$NIGHT_DIR/fixer-$r.ready" "$NIGHT_DIR/.fixer-$r.conflict"
      note_report "## Fixer round $r: merge of $branch conflicted and was aborted (branch is kept for the user)"
      log "fixer round $r: merge conflict, aborted"
    fi
  done
}

# ---------------------------------------------------------------------------------------------
# The sweep
# ---------------------------------------------------------------------------------------------
# Highest NN of the existing runs/<prefix>NN-HHMM directories, 0 when there are none. A restarted
# sweep (cron after a kill) continues from it, so run names stay unique within a night. Gaps and
# directories that do not match the pattern are ignored. The prefix is empty for the classic
# single-slot sweep and "sK-" for slot K of a parallel sweep (every slot counts on its own).
highest_run_number() { # highest_run_number <runs-dir> [<prefix>]
  local d name max=0 num prefix="${2:-}"
  for d in "$1"/*/; do
    [ -d "$d" ] || continue
    name=$(basename "$d")
    [[ "$name" =~ ^${prefix}([0-9]+)-[0-9]{4}$ ]] || continue
    num=$((10#${BASH_REMATCH[1]}))
    [ "$num" -gt "$max" ] && max=$num
  done
  echo "$max"
}

# sleep that a signal can interrupt at once (a foreground sleep would delay the traps).
# The sleeper does not inherit the sweep lock (fd 9) or the admission lock (fd 6): a killed sweep
# must not stay "running" for as long as its last nap.
nap() { sleep "$1" 9>&- 6>&- & wait $! 2>/dev/null; }
# A process that exists and is not a zombie (a child that ended but was not waited for yet).
alive() { local st; st=$(ps -o stat= -p "$1" 2>/dev/null); [ -n "$st" ] && [[ "$st" != Z* ]]; }

# Kill the proctor's whole session (subshell, timeout, claude and everything claude started).
kill_proctor_session() {
  local pid="$1" sig pids
  [ -n "$pid" ] || return 0
  for sig in TERM KILL; do
    pids=$(ps -eo pid=,sid= | awk -v s="$pid" '$2 == s { print $1 }')
    [ -n "$pids" ] || return 0
    kill -"$sig" $pids 2>/dev/null || true
    [ "$sig" = TERM ] && nap 3
  done
  return 0
}

# ---------------------------------------------------------------------------------------------
# Slots. A slot is one worker that plays proctored games one after another. With NIGHTLY_SLOTS=1
# the single slot is the classic sweep: it works in the live checkout on the night branch, run
# names are NN-HHMM. With N >= 2 the sweep process only supervises N workers (subshells); slot K
# works in its own git worktree $NIGHT_DIR/slot-K/repo on branch <night branch>-sK (so proctors
# never share a working tree, index or branch), builds into its own cargo target dir, names its
# runs sK-NN-HHMM and files its entries into the one shared report.md.
# ---------------------------------------------------------------------------------------------
NSLOTS=1 WORKER_PIDS=() CLEANUP_DONE=0
SLOT=1 SLOT_TAG="" SDIR="" SREPO="" SBRANCH="" STARGET="" SUFFIX="" SACTIVE="" BASE_COMMIT="" MERGED_NOW=""

# The number of slots the sweep of this night uses (written by the sweep; the fix rounds and the
# summary follow the sweep, not their own environment).
night_slots() {
  local n=""
  [ -s "$NIGHT_DIR/slots" ] && read -r n < "$NIGHT_DIR/slots"
  case "${n:-}" in ''|*[!0-9]*|0) n="$SLOTS" ;; esac
  case "$n" in ''|*[!0-9]*|0) n=1 ;; esac
  echo "$n"
}

slot_init() { # slot_init <k>: sets the per-slot globals (no side effects)
  SLOT="$1"
  SACTIVE="$NIGHT_DIR/.active-s$1"
  if [ "$NSLOTS" -le 1 ]; then
    SLOT_TAG="" SDIR="$NIGHT_DIR" SREPO="$REPO" SBRANCH="$FIX_BRANCH" STARGET="" SUFFIX=""
  else
    SLOT_TAG="s$1-" SDIR="$NIGHT_DIR/slot-$1" SREPO="$NIGHT_DIR/slot-$1/repo" SBRANCH="$FIX_BRANCH-s$1" \
      STARGET="$REPORT_ROOT/target-slot-$1" SUFFIX=" (slot $1)"
  fi
}

# Make web/node_modules of the live checkout visible in a worktree (and keep git from seeing it).
link_node_modules() { # link_node_modules <worktree> [copy]
  local wt="$1" exclude
  if [ -d "$REPO/web/node_modules" ] && [ -d "$wt/web" ] && [ ! -e "$wt/web/node_modules" ]; then
    if [ "${2:-}" = copy ]; then
      # A slot plays games here: a real copy (about 160 MB) keeps vite's root and its dependency
      # cache (node_modules/.vite) private to the slot instead of shared with the live checkout, the
      # phone-play instance and the other slots, and keeps the modules inside the vite root.
      cp -a "$REPO/web/node_modules" "$wt/web/node_modules" 2> /dev/null
      # A node_modules/node_modules self-link in the source (a stray one broke two nights) would
      # load Playwright twice in the copy; and a vite cache pre-bundled in the live checkout may
      # hold a second React. The slot starts with neither.
      [ ! -L "$wt/web/node_modules/node_modules" ] || rm -f "$wt/web/node_modules/node_modules"
      rm -rf "$wt/web/node_modules/.vite" 2> /dev/null
    else
      ln -sfnT "$REPO/web/node_modules" "$wt/web/node_modules"
    fi
    exclude=$(git -C "$wt" rev-parse --git-path info/exclude)
    grep -qxF '/web/node_modules' "$exclude" 2>/dev/null || echo '/web/node_modules' >> "$exclude"
  fi
}

# Before a slot plays: Playwright must find the smoke test (a doubled Playwright reports "No tests
# found") and vite must serve the app. NIGHTLY_PREFLIGHT=0 skips it (tests without node).
slot_preflight() {
  [ "${NIGHTLY_PREFLIGHT:-1}" != 0 ] || return 0
  [ -d "$SREPO/web/node_modules" ] || { echo "preflight: $SREPO/web/node_modules is missing" >> "$SDIR/prepare.log"; return 1; }
  local port=$(( 20000 + RANDOM % 20000 )) vpid ok=0 i
  if ! (cd "$SREPO/web" && timeout 120 npx playwright test --list) >> "$SDIR/prepare.log" 2>&1; then
    echo "preflight: playwright test --list failed" >> "$SDIR/prepare.log"; return 1
  fi
  (cd "$SREPO/web" && exec setsid npx vite --port "$port" --strictPort --host 127.0.0.1) >> "$SDIR/prepare.log" 2>&1 < /dev/null &
  vpid=$!
  for i in $(seq 1 30); do
    if curl -sf "http://127.0.0.1:$port/src/main.tsx" > /dev/null 2>&1 && curl -sf "http://127.0.0.1:$port/" > /dev/null 2>&1; then ok=1; break; fi
    sleep 2
  done
  kill -- "-$vpid" 2> /dev/null || kill "$vpid" 2> /dev/null
  [ "$ok" = 1 ] || echo "preflight: vite did not serve the app on port $port" >> "$SDIR/prepare.log"
  [ "$ok" = 1 ]
}

# Create (or, after a restart, refresh) this slot's worktree. A restarted slot drops the half-done
# edits of a killed proctor and takes in anything the night branch gained meanwhile.
slot_prepare() {
  mkdir -p "$SDIR" || return 1
  local attempt
  for attempt in 1 2 3; do
    (
      flock 4
      git -C "$REPO" worktree prune || echo "worktree prune failed ($?)"
      if [ ! -e "$SREPO/.git" ]; then
        git -C "$REPO" worktree add -q -B "$SBRANCH" "$SREPO" "$FIX_BRANCH" || { echo "worktree add failed ($?)"; exit 1; }
      else
        git -C "$SREPO" reset -q --hard HEAD || { echo "reset failed ($?)"; exit 1; }
        if ! git -C "$SREPO" merge-base --is-ancestor "$FIX_BRANCH" HEAD; then
          git -C "$SREPO" merge --no-edit -q "$FIX_BRANCH" || { git -C "$SREPO" merge --abort; echo "merge of $FIX_BRANCH failed"; exit 1; }
        fi
      fi
    ) 4>"$NIGHT_DIR/.repo.lock" >> "$SDIR/prepare.log" 2>&1 && break
    echo "slot_prepare: attempt $attempt failed" >> "$SDIR/prepare.log"
    [ "$attempt" -lt 3 ] || return 1
    sleep 2 # a half-created worktree is taken as it is and refreshed by the next attempt
  done
  link_node_modules "$SREPO" copy
  slot_preflight || return 1
  mkdir -p "$STARGET" || { echo "cannot create $STARGET" >> "$SDIR/prepare.log"; return 1; }
}

# The state of the run in progress is a file, $NIGHT_DIR/.active-sK (run dir, run name, proctor pid
# on three lines). Whoever moves it away first owns the end of that run: the slot after its proctor
# ended, the slot's own signal trap, or the supervisor's cleanup. So every run gets exactly one entry.
set_active() { # set_active <run-dir> <run-name> [<proctor pid>]
  printf '%s\n%s\n%s\n' "$1" "$2" "${3:-}" > "$SACTIVE.tmp" && mv -f "$SACTIVE.tmp" "$SACTIVE"
}
claim_active() { # claim_active: 0 when this process now owns the end of the slot's run
  mv -f "$SACTIVE" "$SACTIVE.claim.$BASHPID" 2>/dev/null && rm -f "$SACTIVE.claim.$BASHPID"
}

# Stop the slot's proctor and game, build the mechanical digest and append an entry marked "sweep
# terminated mid-run". Does nothing when the slot has no run active. Idempotent.
cleanup_slot() { # cleanup_slot <slot> <reason>
  local k="$1" reason="$2" claim run_dir="" name="" pid=""
  slot_init "$k"
  claim="$SACTIVE.claim.$BASHPID"
  mv -f "$SACTIVE" "$claim" 2>/dev/null || return 0
  { read -r run_dir; read -r name; read -r pid; } < "$claim"
  rm -f "$claim"
  kill_proctor_session "$pid"
  [ -n "$run_dir" ] || return 0
  timeout --kill-after=5 60 "$NIGHTLY_DIR/run_game.sh" stop "$run_dir" > /dev/null 2>&1
  timeout --kill-after=5 120 python3 "$NIGHTLY_DIR/digest.py" "$run_dir" > "$run_dir/digest.md" 2>&1
  local last="unknown"
  if [ -s "$run_dir/trace/trace.jsonl" ]; then
    last=$(tail -n 1 "$run_dir/trace/trace.jsonl" | python3 -c \
      'import json,sys; r=json.load(sys.stdin); print("#%s, round %s" % (r.get("decision","?"), r.get("round","?")))' 2>/dev/null) || last="unknown"
    [ -n "$last" ] || last="unknown"
  fi
  {
    echo "## Run $name — sweep terminated mid-run ($reason), proctor failed, mechanical digest only"
    echo
    echo "The sweep process ended while this run was active; the game and the proctor were stopped and there is no proctor entry. Last decision: $last. Evidence: \`$run_dir\`."
    if [ -s "$run_dir/proctor-entry.md" ]; then echo "A partial proctor entry was kept in \`$run_dir/proctor-entry.md\`."; fi
    echo
    cat "$run_dir/digest.md"
    echo
  } | report_append
  log "run $name: sweep terminated mid-run ($reason); game stopped, digest appended"
}

# Runs on SIGTERM/SIGINT/SIGHUP and on every exit of the sweep process. Idempotent
# (CLEANUP_DONE). With parallel slots every worker is told to clean up its own run (they do it
# concurrently); whatever is left afterwards (a worker that died or hangs) is cleaned up here from
# the slots' state files. With no run active nothing happens and no entry is written.
sweep_cleanup() { # sweep_cleanup <reason>
  [ "$CLEANUP_DONE" = 1 ] && return 0
  CLEANUP_DONE=1
  trap '' TERM INT HUP # a second signal must not interrupt the cleanup
  local pid k t0=$SECONDS
  for pid in "${WORKER_PIDS[@]}"; do kill -TERM "$pid" 2>/dev/null; done
  for pid in "${WORKER_PIDS[@]}"; do
    while alive "$pid" && [ $((SECONDS - t0)) -lt 300 ]; do sleep 0.5; done
    alive "$pid" && kill -KILL "$pid" 2>/dev/null
  done
  for k in $(seq 1 "$NSLOTS"); do cleanup_slot "$k" "$1"; done
}
log_loop_exit() { log "sweep exited on signal or exit (wall time: $(TZ="$NIGHTLY_TZ" date -d "@$(now_epoch)" +"%F %T %Z"))"; }

# --- hand-off of fixer branches into one slot's branch (parallel sweeps) -----------------------
# Like merge_pending, but into this slot's own worktree and only between this slot's games. The
# ready marker stays where it is (it is consumed once, into the night branch, after the sweep);
# what a slot did with a branch is recorded next to the slot: $SDIR/.fixer-N.merged|.rejected|.conflict.
merge_pending_slot() {
  local r branch pre count
  MERGED_NOW=""
  for r in 1 2; do
    [ -f "$NIGHT_DIR/fixer-$r.ready" ] || continue
    if [ -e "$SDIR/.fixer-$r.merged" ] || [ -e "$SDIR/.fixer-$r.rejected" ] || [ -e "$SDIR/.fixer-$r.conflict" ]; then continue; fi
    branch=$(cat "$NIGHT_DIR/fixer-$r.ready")
    pre=$(git -C "$SREPO" rev-parse HEAD)
    count=$(git -C "$SREPO" rev-list --count "$pre..$branch")
    if git -C "$SREPO" merge --no-edit -q "$branch" >> "$SDIR/merge.log" 2>&1; then
      touch "$SDIR/.fixer-$r.merged"
      MERGED_NOW="$MERGED_NOW $r"
      note_report "## Fixer round $r: merged $branch ($count commits) into $SBRANCH (slot $SLOT)"
      log "fixer round $r: merged $branch ($count commits) into slot $SLOT"
    else
      git -C "$SREPO" merge --abort >> "$SDIR/merge.log" 2>&1 || true
      touch "$SDIR/.fixer-$r.conflict"
      note_report "## Fixer round $r: merge of $branch into slot $SLOT conflicted (with that slot's own repairs) and was aborted; the slot goes on without it (branch is kept for the user)"
      log "fixer round $r: merge conflict in slot $SLOT, aborted"
    fi
  done
}

# After the sweep (and before a fix round starts) the slots' branches, with the proctors' repairs
# and the fixer merges, come together in the night branch of the live checkout. A conflict between
# two slots' repairs is aborted, noted once per branch tip, and the branch is kept for the user.
integrate_slots() {
  local d k branch count tip
  ls -d "$NIGHT_DIR"/slot-*/repo > /dev/null 2>&1 || return 0
  if [ "$(git -C "$REPO" branch --show-current)" != "$FIX_BRANCH" ]; then
    note_once "slots" "the checkout is on '$(git -C "$REPO" branch --show-current)', not on $FIX_BRANCH; the slot branches $FIX_BRANCH-s* are kept for the user"
    return 0
  fi
  (
    flock 4
    for d in "$NIGHT_DIR"/slot-*/repo; do
      k=${d%/repo}; k=${k##*slot-}
      branch="$FIX_BRANCH-s$k"
      git -C "$REPO" show-ref -q --verify "refs/heads/$branch" || continue
      count=$(git -C "$REPO" rev-list --count "$FIX_BRANCH..$branch")
      [ "$count" -gt 0 ] || continue
      tip=$(git -C "$REPO" rev-parse "$branch")
      [ "$(cat "$NIGHT_DIR/.slot-s$k.conflict" 2>/dev/null)" = "$tip" ] && continue
      if git -C "$REPO" merge --no-edit -q "$branch" >> "$NIGHT_DIR/merge.log" 2>&1; then
        note_report "## Slot $k: merged $branch ($count commits) into $FIX_BRANCH"
        log "slot $k: merged $branch ($count commits) into $FIX_BRANCH"
      else
        git -C "$REPO" merge --abort >> "$NIGHT_DIR/merge.log" 2>&1 || true
        echo "$tip" > "$NIGHT_DIR/.slot-s$k.conflict"
        note_report "## Slot $k: merge of $branch into $FIX_BRANCH conflicted and was aborted (branch is kept for the user)"
        log "slot $k: merge conflict, aborted"
      fi
    done
  ) 4>"$NIGHT_DIR/.repo.lock"
}

# Sentences about the parallel slots for the fixer and summary prompts (empty for one slot).
slots_note_fixer() {
  [ "$(night_slots)" -gt 1 ] || return 0
  printf '%s' " The night ran $(night_slots) games in parallel (slots; run names sK-NN-HHMM, K = slot): the report mixes their entries, and the same bug may show up in two slots at once. Your commits are merged into every slot's branch between that slot's games."
}
slots_note_summary() {
  local n
  n=$(night_slots)
  [ "$n" -gt 1 ] || return 0
  printf '%s' "
The night ran $n games in parallel (slots; run names sK-NN-HHMM with K = slot, entries of all slots are interleaved in the report). Each slot's proctor worked in its own worktree on the branch \`$FIX_BRANCH-sK\`; after the sweep these branches were merged into \`$FIX_BRANCH\` (the report says per slot whether that worked; a conflicting slot branch is kept and its commits are NOT in \`$FIX_BRANCH\`). A fixer round's merge was done per slot between that slot's games (markers \`$NIGHT_DIR/slot-K/.fixer-N.merged|.rejected|.conflict\`). Report lines like \"Slot K waiting for memory\" mean the memory gate delayed a start; list them under Harness / sweep issues."
}

# --- admission: memory ------------------------------------------------------------------------
mem_needed_mb() {
  local p max=0
  for p in $PLAYER_COUNTS; do [ "$p" -gt "$max" ] && max=$p; done
  echo $(( MEM_BASE_MB + MEM_PER_PLAYER_MB * max ))
}
mem_available_mb() { bash -c "$MEM_AVAIL_CMD" 2>/dev/null | head -n 1; }
other_slot_active() {
  local f
  for f in "$NIGHT_DIR"/.active-s*; do
    [ -f "$f" ] || continue
    case "$f" in *.claim.*|*.tmp) continue ;; esac
    [ "$f" = "$SACTIVE" ] || return 0
  done
  return 1
}
time_left() { [ $(( END_EPOCH - $(now_epoch) )) -gt "$MIN_RUN_SECONDS" ]; }
# Parallel sweeps only. Takes the admission lock (fd 6; starts are one at a time) and, while another
# slot has a run active, waits until mem_needed_mb is available. The lock stays held until the new
# game has had SLOT_SETTLE_SECONDS to ramp up (admit_release), so the next check sees its memory.
# Returns 1 when the window ended while waiting.
admit_run() {
  [ "$NSLOTS" -gt 1 ] || return 0
  exec 6>"$NIGHT_DIR/.admit.lock"
  until flock -n 6; do time_left || return 1; nap "$SLOT_POLL_SECONDS"; done
  local need avail noted=0
  need=$(mem_needed_mb)
  while other_slot_active; do
    avail=$(mem_available_mb)
    case "$avail" in ''|*[!0-9]*) log "slot $SLOT: cannot read the available memory, starting anyway"; break ;; esac
    [ "$avail" -ge "$need" ] && break
    if [ "$noted" = 0 ]; then
      noted=1
      note_report "## Slot $SLOT waiting for memory: $avail MB available, a new game needs $need MB and another game is running (checked every ${MEM_WAIT_SECONDS}s)"
    fi
    log "slot $SLOT: low memory: $avail MB available, need $need MB"
    time_left || { flock -u 6; return 1; }
    nap "$MEM_WAIT_SECONDS"
  done
  [ "$noted" = 0 ] || log "slot $SLOT: memory is available now ($avail MB), starting"
  return 0
}
admit_release() { # admit_release <proctor pid>: hold the lock while the new game ramps up
  [ "$NSLOTS" -gt 1 ] || return 0
  local t0=$SECONDS
  while [ $((SECONDS - t0)) -lt "$SLOT_SETTLE_SECONDS" ] && alive "$1"; do nap 2; done
  flock -u 6
}

branch_note() { # the sentence about the branch in the proctor prompt
  if [ "$NSLOTS" -le 1 ]; then
    printf '%s' "That branch is shared by every run tonight: earlier proctors' repairs are
  already in it, and the next run is built from it."
  else
    printf '%s' "This is your own copy of the repository (slot $SLOT of $NSLOTS): another proctor is
  probably playing a game in parallel on another slot, in a copy of its own, on the same machine
  (its CPU, memory and log noise are not your run's, and its repairs are not in your copy). Your
  branch is private to this slot: earlier runs' repairs of this slot are in it, fixer rounds'
  commits are merged into it between this slot's games, and after the sweep all slots' branches
  are merged into the night branch. The next run of this slot is built from it. request_fix.sh
  tags your request with the slot by itself; the report is shared by all slots."
  fi
}

# --- one slot's loop ----------------------------------------------------------------------------
run_made_decisions() { # run_made_decisions <run-dir>: the harness report counted at least one decision
  python3 - "$1/trace/report.json" << 'PY' 2> /dev/null
import json, sys
try:
    sys.exit(0 if int(json.load(open(sys.argv[1])).get("decisions", 0)) > 0 else 1)
except Exception:
    sys.exit(1)
PY
}

slot_loop() {
  local zero_runs=0 n disk_waits=0 run_name run_dir prompt status r proctor_pid proctor_seconds
  n=$(highest_run_number "$NIGHT_DIR/runs" "$SLOT_TAG")
  BASE_COMMIT=$(git -C "$SREPO" rev-parse HEAD)
  while time_left; do
    # No run (and no run directory) without free disk: a full disk used to turn the rest of a night
    # into phantom runs and a truncated report. Wait for space, and give up after MAX_DISK_WAITS.
    if ! disk_ok; then
      disk_waits=$((disk_waits + 1))
      if [ "$disk_waits" -eq 1 ]; then
        note_report "## Waiting for disk space: $(free_gb) GB free, runs need $MIN_FREE_GB GB (checked every ${DISK_WAIT_SECONDS}s)$SUFFIX"
      fi
      log "low disk: $(free_gb) GB free, need $MIN_FREE_GB GB (check $disk_waits of $MAX_DISK_WAITS)$SUFFIX"
      if [ "$disk_waits" -ge "$MAX_DISK_WAITS" ]; then
        note_report "## Sweep stopped: the disk stayed below $MIN_FREE_GB GB for $MAX_DISK_WAITS checks$SUFFIX"
        log "sweep stopped: no disk space$SUFFIX"; break
      fi
      nap "$DISK_WAIT_SECONDS"; continue
    fi
    disk_waits=0
    n=$((n + 1))
    run_name="$SLOT_TAG$(printf '%02d' "$n")-$(TZ="$NIGHTLY_TZ" date -d "@$(now_epoch)" +%H%M)"
    run_dir="$NIGHT_DIR/runs/$run_name"
    if ! mkdir -p "$run_dir"; then
      note_report "## Run $run_name not started: cannot create $run_dir"
      log "run $run_name: cannot create the run directory"
      n=$((n - 1)); nap "$DISK_WAIT_SECONDS"; continue
    fi
    # Fixer branches that are ready come in now, between games, never during one.
    MERGED_NOW=""
    if [ "$NSLOTS" -gt 1 ]; then merge_pending_slot; else merge_pending 0; fi
    # Build before every run so Playwright's backend start stays fast after a repair. A repair or
    # merge that broke the build is dropped (reset to the last good commit), so one bad fix
    # cannot cost the rest of the night.
    if ! (cd "$SREPO" && { [ -z "$STARGET" ] || export CARGO_TARGET_DIR="$STARGET"; } && bash -c "$BUILD_CMD") > "$run_dir/build.log" 2>&1; then
      if [ "$(git -C "$SREPO" rev-parse HEAD)" != "$BASE_COMMIT" ]; then
        # Only proctor commits and fixer merges exist past the last good build; drop them (they
        # stay in the reflog and in the fixer branches).
        git -C "$SREPO" reset -q --hard "$BASE_COMMIT" >> "$run_dir/build.log" 2>&1
        for r in $MERGED_NOW; do
          if [ "$NSLOTS" -gt 1 ]; then mv "$SDIR/.fixer-$r.merged" "$SDIR/.fixer-$r.rejected" 2>/dev/null
          else mv "$NIGHT_DIR/.fixer-$r.merged" "$NIGHT_DIR/.fixer-$r.rejected" 2>/dev/null; fi
        done
        { echo "## Before run $run_name: a proctor repair or fixer merge broke the build; branch reset to the last good commit${MERGED_NOW:+ (fixer round(s)$MERGED_NOW rejected)}"; echo; } | report_append
        log "run $run_name: build broke, reset to last good commit"
        rm -rf "$run_dir"; n=$((n - 1)); continue
      fi
      if [ "$NSLOTS" -gt 1 ]; then
        { echo "## Build failed on slot $SLOT — no more runs on this slot tonight"; echo '```'; tail -n 60 "$run_dir/build.log"; echo '```'; } | report_append
        log "build failed (slot $SLOT)"; break
      fi
      { echo "## Build failed — no runs tonight"; echo '```'; tail -n 60 "$run_dir/build.log"; echo '```'; } | report_append
      log "build failed"; break
    fi
    BASE_COMMIT=$(git -C "$SREPO" rev-parse HEAD)
    # Parallel sweeps: wait for memory and for our turn to start (a no-op with one slot).
    if ! admit_run; then rm -rf "$run_dir"; n=$((n - 1)); break; fi
    log "run $run_name: launching proctor"
    prompt=$(render "$NIGHTLY_DIR/prompts/proctor.md" "RUN_DIR=$run_dir" "TOOLS=$NIGHTLY_DIR" "RUN_NAME=$run_name" \
      "FIX_BRANCH=$SBRANCH" "REPO=$SREPO" "BRANCH_NOTE=$(branch_note)")
    # The game itself stops 5 minutes before END so the proctor can still write its entry.
    export DEADLINE=$(( END_EPOCH - 300 ))
    # Auto mode with edit tools, working in the slot's repo on its branch. Pushing, switching
    # branches and rewriting history stay denied.
    set_active "$run_dir" "$run_name"
    proctor_seconds=$(( END_EPOCH - $(now_epoch) + 600 ))
    [ "$proctor_seconds" -ge 60 ] || proctor_seconds=60 # the window ended while we were waiting for our turn
    # Background job in its own session (setsid) + wait: a signal reaches the traps at once and
    # cleanup_slot can kill claude and everything it started.
    (cd "$SREPO" && { [ "$NSLOTS" -le 1 ] || export NIGHTLY_GAME_REPO="$SREPO" CARGO_TARGET_DIR="$STARGET" NIGHTLY_SLOT="$SLOT" NIGHTLY_RUN_NAME="$run_name"; } \
      && exec setsid timeout --kill-after=30 "$proctor_seconds" \
      "$CLAUDE_BIN" -p --model "$PROCTOR_MODEL" --no-session-persistence \
      --permission-mode auto --tools Bash Read Grep Glob Edit Write \
      --allowedTools "Bash($NIGHTLY_DIR/run_game.sh:*)" "Bash($NIGHTLY_DIR/watch.sh:*)" "Bash($NIGHTLY_DIR/request_fix.sh:*)" \
      "Bash(python3 $NIGHTLY_DIR/digest.py:*)" "Read" "Grep" "Glob" "Edit" "Write" \
      "Bash(git status:*)" "Bash(git diff:*)" "Bash(git add:*)" "Bash(git commit:*)" "Bash(git log:*)" \
      "Bash(cargo check:*)" "Bash(cargo test:*)" "Bash(cargo fmt:*)" "Bash(npm test:*)" "Bash(npx tsc:*)" "Bash(npx vitest:*)" \
      --disallowedTools "NotebookEdit" "Agent" "Bash(git push:*)" "Bash(git checkout:*)" "Bash(git switch:*)" \
      "Bash(git reset:*)" "Bash(git rebase:*)" "Bash(git merge:*)" "Bash(git branch:*)" "Bash(git worktree:*)" \
      "Bash(git stash:*)" "Bash(git clean:*)" "Bash(git revert:*)" "Bash(rm:*)" "Bash(sudo:*)" "Bash(kill:*)" "Bash(pkill:*)" \
      -- "$prompt" > "$run_dir/proctor-entry.md" 2> "$run_dir/proctor.err" < /dev/null 6>&-) &
    proctor_pid=$!
    set_active "$run_dir" "$run_name" "$proctor_pid"
    admit_release "$proctor_pid"
    wait "$proctor_pid"
    status=$?
    # If a signal cleanup took the run over, it has filed the entry and this slot is done.
    claim_active || exit 0
    # Whatever the proctor did, never leave a game running.
    "$NIGHTLY_DIR/run_game.sh" stop "$run_dir" > /dev/null 2>&1
    python3 "$NIGHTLY_DIR/digest.py" "$run_dir" > "$run_dir/digest.md" 2>&1
    if [ -s "$run_dir/proctor-entry.md" ] && [ -f "$run_dir/meta.json" ]; then
      { cat "$run_dir/proctor-entry.md"; echo; echo "_Raw evidence: \`$run_dir\`_"; echo; } | report_append
    else
      {
        echo "## Run $run_name — proctor failed (exit $status), mechanical digest only"
        echo
        sed -n '1,20p' "$run_dir/proctor.err" | sed 's/^/    /'
        echo
        cat "$run_dir/digest.md"
        echo
      } | report_append
    fi
    log "run $run_name: done (proctor exit $status)"
    # Circuit breaker: runs that never reach a decision mean the environment is broken (it burned
    # two nights); a fixer cannot help, so stop instead of spending proctors.
    if run_made_decisions "$run_dir"; then zero_runs=0; else zero_runs=$((zero_runs + 1)); fi
    if [ "${MAX_ZERO_DECISION_RUNS:-3}" -gt 0 ] && [ "$zero_runs" -ge "${MAX_ZERO_DECISION_RUNS:-3}" ]; then
      note_report "## Sweep stopped$SUFFIX: $zero_runs consecutive runs reached no decision (broken environment?)"
      log "sweep stopped: $zero_runs consecutive zero-decision runs$SUFFIX"; break
    fi
    # A run that never started (e.g. the proctor failed immediately) must not spin the loop.
    [ -f "$run_dir/meta.json" ] || nap "${NIGHTLY_FAILED_RUN_SLEEP:-60}"
  done
  if [ "$NSLOTS" -gt 1 ]; then log "slot $SLOT finished after $n runs"; else log "sweep finished after $n runs"; fi
}

slot_signal() { # slot_signal <name> <exit status>
  trap '' TERM INT HUP
  cleanup_slot "$SLOT" "$1"
  exit "$2"
}

# A parallel worker: stagger, set up the slot's worktree, play until the window ends. Runs in a
# subshell of the sweep; its own traps file the entry of its run when the sweep is stopped.
slot_main() { # slot_main <k>
  slot_init "$1"
  # A signal can reach a worker twice (once from the sweep, once as part of a process group kill,
  # e.g. `timeout`): the handler ignores further signals first, or the second one would cut the
  # cleanup short.
  trap 'slot_signal SIGTERM 143' TERM
  trap 'slot_signal SIGINT 130' INT
  trap 'slot_signal SIGHUP 129' HUP
  trap 'cleanup_slot "$SLOT" exit' EXIT
  [ "$SLOT" -le 1 ] || nap $(( (SLOT - 1) * SLOT_STAGGER_SECONDS ))
  if ! slot_prepare; then
    { echo "## Slot $SLOT could not set up its worktree $SREPO on $SBRANCH — no runs on this slot tonight"; echo '```'; tail -n 20 "$SDIR/prepare.log"; echo '```'; } | report_append
    log "slot $SLOT: worktree setup failed"; return 1
  fi
  log "slot $SLOT: working in $SREPO on $SBRANCH (cargo target $STARGET)"
  slot_loop
}

cmd_loop() {
  set_night "${1:-$(default_night)}"
  case "$SLOTS" in ''|*[!0-9]*|0) SLOTS=1 ;; esac
  NSLOTS="$SLOTS"
  mkdir -p "$NIGHT_DIR"
  exec 9>"$LOCK"
  flock -n 9 || { log "sweep already running for $NIGHT"; exit 0; }

  # Log when the loop exits via signal or explicit exit.
  # A signal first stops the active runs and files their entries (sweep_cleanup), then exits, which
  # runs the EXIT trap; the cleanup runs only once. sweep.done is not written: cron restarts the sweep.
  trap 'sweep_cleanup exit; log_loop_exit' EXIT
  trap 'sweep_cleanup SIGTERM; exit 143' TERM
  trap 'sweep_cleanup SIGINT; exit 130' INT
  trap 'sweep_cleanup SIGHUP; exit 129' HUP

  local report="$NIGHT_DIR/report.md"
  [ -f "$report" ] || {
    echo "# Nightly UI smoke sweep — night of $NIGHT"
    echo
    echo "Window $START_HHMM–$END_HHMM $NIGHTLY_TZ, runs until round $STOP_ROUND, proctor $PROCTOR_MODEL, fixer rounds: ${FIXERS:-none} ($FIXER_MODEL)."
    echo
    if [ "$NSLOTS" -gt 1 ]; then
      echo "Parallel slots: $NSLOTS. Runs are named sK-NN-HHMM (K = slot); each slot works in its own worktree on the branch $FIX_BRANCH-sK and is merged into $FIX_BRANCH after the sweep."
      echo
    fi
  } > "$report"
  echo "$NSLOTS" > "$NIGHT_DIR/slots"
  log "sweep for $NIGHT until $(TZ="$NIGHTLY_TZ" date -d "@$END_EPOCH" '+%F %T %Z')"

  # The whole night runs on one branch, nightly-fixes-<night>, checked out in $REPO. Before
  # switching, the current state (uncommitted and untracked work included) is committed onto it,
  # so nothing is lost. Proctors commit their minor repairs here (with parallel slots: on the slot
  # branches, merged into it after the sweep), and the Opus fixers' branches are merged into it
  # between games, so every later run includes the earlier fixes. Nothing is pushed. In the
  # morning: git checkout <orig_branch> and merge/cherry-pick.
  if [ "$(git -C "$REPO" branch --show-current)" != "$FIX_BRANCH" ]; then
    git -C "$REPO" branch --show-current > "$NIGHT_DIR/orig_branch"
    if git -C "$REPO" show-ref -q --verify "refs/heads/$FIX_BRANCH"; then
      git -C "$REPO" checkout -q "$FIX_BRANCH"
    else
      git -C "$REPO" checkout -q -b "$FIX_BRANCH" \
        && git -C "$REPO" add -A \
        && { git -C "$REPO" diff --cached --quiet || git -C "$REPO" commit -q -m "nightly $NIGHT: snapshot of the working tree before the sweep"; }
    fi >> "$NIGHT_DIR/build.log" 2>&1 || {
      { echo "## Could not switch to $FIX_BRANCH — no runs tonight"; echo '```'; tail -n 30 "$NIGHT_DIR/build.log"; echo '```'; } | report_append
      log "branch setup failed"; touch "$NIGHT_DIR/sweep.done"; exit 1
    }
  fi

  local k pid
  # A run left active by a sweep that died without cleaning up (SIGKILL, or killed half way through
  # the cleanup: its claim file is still there) is closed off first.
  local claim
  for claim in "$NIGHT_DIR"/.active-s*.claim.*; do
    [ -f "$claim" ] || continue
    k=${claim##*/.active-s}; k=${k%%.*}
    [ -e "$NIGHT_DIR/.active-s$k" ] && rm -f "$claim" || mv -f "$claim" "$NIGHT_DIR/.active-s$k"
  done
  for k in $(seq 1 "$NSLOTS"); do cleanup_slot "$k" "earlier sweep died"; done
  if [ "$NSLOTS" -le 1 ]; then
    slot_init 1
    slot_loop
  else
    log "parallel sweep: $NSLOTS slots, stagger ${SLOT_STAGGER_SECONDS}s, a new game needs $(mem_needed_mb) MB available while another runs"
    for k in $(seq 1 "$NSLOTS"); do
      ( slot_main "$k" ) &
      WORKER_PIDS+=("$!")
    done
    for pid in "${WORKER_PIDS[@]}"; do wait "$pid"; done
    WORKER_PIDS=()
    integrate_slots
    log "sweep finished (all $NSLOTS slots)"
  fi
  touch "$NIGHT_DIR/sweep.done"
}

# ---------------------------------------------------------------------------------------------
# An Opus fix round
# ---------------------------------------------------------------------------------------------
cmd_fix() {
  local round="${1:?usage: nightly.sh fix <round> [night]}"
  set_night "${2:-$(default_night)}"
  mkdir -p "$NIGHT_DIR"
  exec 8>"$NIGHT_DIR/.fixer-$round.lock"
  flock -n 8 || { log "fixer round $round already running for $NIGHT"; exit 0; }
  touch "$NIGHT_DIR/.fixer-$round-started"
  # The exit trap tells a finished round from one that never got going (no disk, no worktree): the
  # second is reported and retried, see fixer_exit.
  FIX_ROUND="$round" FIX_OK=0 FIX_FAIL_REASON=""
  trap fixer_exit EXIT

  local max="$FIX1_MAX_SECONDS"
  [ "$round" -ge 2 ] && max="$FIX2_MAX_SECONDS"
  local fixer_branch="nightly-opus-$NIGHT-r$round" wt="$NIGHT_DIR/fixer-$round" base status

  # A parallel sweep keeps the proctors' repairs on the slot branches; bring them into the night
  # branch first so this round starts from them (as it would after a single-slot sweep).
  integrate_slots

  # With no sweep running, earlier rounds' ready branches come in first (and are build-checked),
  # so this round builds on them.
  if ! sweep_running; then merge_pending 1; fi

  if git -C "$REPO" show-ref -q --verify "refs/heads/$FIX_BRANCH"; then
    base="$FIX_BRANCH"
  else
    base=$(git -C "$REPO" rev-parse HEAD)
  fi
  if ! disk_ok; then
    FIX_FAIL_REASON="only $(free_gb) GB free, need $MIN_FREE_GB GB"
    log "fixer round $round: $FIX_FAIL_REASON"; exit 1
  fi
  if [ ! -d "$wt" ]; then
    flock "$NIGHT_DIR/.repo.lock" git -C "$REPO" worktree add -q -B "$fixer_branch" "$wt" "$base" >> "$NIGHT_DIR/fixer-$round.err" 2>&1 \
      || { FIX_FAIL_REASON="cannot create worktree $wt ($(tail -n 1 "$NIGHT_DIR/fixer-$round.err" 2>/dev/null))"
           log "fixer round $round: $FIX_FAIL_REASON"; exit 1; }
  fi
  git -C "$wt" rev-parse HEAD > "$NIGHT_DIR/fixer-$round.base"
  link_node_modules "$wt"

  log "fixer round $round: working in $wt on $fixer_branch ($FIXER_MODEL, budget ${max}s)"
  # A round started because a proctor asked (request_fix.sh): say so in the prompt and report.
  # Round 1 reads fix-requested; round 2 reads fix-requested-2 (a request made once round 1 had
  # started), also when it only starts after the sweep.
  local request="" req_reason req_run req_file="$NIGHT_DIR/fix-requested" early=" early"
  [ "$round" -ge 2 ] && { req_file="$NIGHT_DIR/fix-requested-2"; early=""; }
  if [ -s "$req_file" ]; then
    req_reason=$(sed -n '/^reason: /,$p' "$req_file" | sed '1s/^reason: //')
    req_run=$(sed -n 's/^run: //p' "$req_file" | head -1)
    request="A proctor (run ${req_run:-unknown}) asked for this round${early} because: $req_reason"
    note_report "## Fixer round $round started${early} on a proctor's request (run ${req_run:-unknown}): $req_reason"
    log "fixer round $round: started${early} on request: $req_reason"
  fi
  local prompt
  prompt=$(render "$NIGHTLY_DIR/prompts/fixer.md" "ROUND=$round" "REPORT=$NIGHT_DIR/report.md" \
    "RUNS_DIR=$NIGHT_DIR/runs" "FIXER_BRANCH=$fixer_branch" "MAX_FIXES=$MAX_FIXES_PER_ROUND" \
    "BUDGET_MINUTES=$(( max / 60 ))" "REQUEST=$request" "SLOTS_NOTE=$(slots_note_fixer)")
  # Own worktree, own target dir, low priority: the games keep their memory and CPU.
  (cd "$wt" && CARGO_TARGET_DIR="$REPORT_ROOT/target-fixer" CARGO_BUILD_JOBS=2 RUST_TEST_THREADS=2 \
    nice -n 10 timeout --kill-after=60 "$max" \
    "$CLAUDE_BIN" -p --model "$FIXER_MODEL" --no-session-persistence \
    --permission-mode auto --tools Bash Read Grep Glob Edit Write \
    --allowedTools "Read" "Grep" "Glob" "Edit" "Write" \
    "Bash(git status:*)" "Bash(git diff:*)" "Bash(git add:*)" "Bash(git commit:*)" "Bash(git log:*)" \
    "Bash(cargo check:*)" "Bash(cargo test:*)" "Bash(cargo fmt:*)" "Bash(npm test:*)" "Bash(npx tsc:*)" "Bash(npx vitest:*)" \
    --disallowedTools "NotebookEdit" "Agent" "Bash(git push:*)" "Bash(git checkout:*)" "Bash(git switch:*)" \
    "Bash(git reset:*)" "Bash(git rebase:*)" "Bash(git merge:*)" "Bash(git branch:*)" "Bash(git worktree:*)" \
    "Bash(git stash:*)" "Bash(git clean:*)" "Bash(git revert:*)" "Bash(rm:*)" "Bash(sudo:*)" "Bash(kill:*)" "Bash(pkill:*)" \
    -- "$prompt" > "$NIGHT_DIR/fixer-$round.out" 2>> "$NIGHT_DIR/fixer-$round.err" < /dev/null)
  status=$?

  local commits
  commits=$(git -C "$wt" rev-list --count "$(cat "$NIGHT_DIR/fixer-$round.base")..HEAD" 2>/dev/null || echo 0)
  {
    echo "# Fixer round $round — night $NIGHT"
    echo
    echo "Model $FIXER_MODEL, exit $status, $commits commit(s) on \`$fixer_branch\`."
    echo
    if [ -s "$NIGHT_DIR/fixer-$round.out" ]; then
      cat "$NIGHT_DIR/fixer-$round.out"
    else
      echo "The fixer wrote no report (exit $status). Commits:"
      git -C "$wt" log --oneline "$(cat "$NIGHT_DIR/fixer-$round.base")..HEAD"
    fi
  } > "$NIGHT_DIR/fixer-$round.md"
  note_report "## Fixer round $round finished: $commits commit(s) on $fixer_branch, exit $status (report: fixer-$round.md)"
  [ "$commits" -gt 0 ] && echo "$fixer_branch" > "$NIGHT_DIR/fixer-$round.ready"
  # Once the sweep has ended nobody else would merge it; while it runs, the loop does, between games.
  if [ -f "$NIGHT_DIR/sweep.done" ] && ! sweep_running; then merge_pending 1; fi
  FIX_OK=1
}

# Exit trap of a fix round. A round that finished (FIX_OK) is done. One that never started working
# says so in the report and is retried after FIX_RETRY_SECONDS (decide_night), at most
# FIX_MAX_ATTEMPTS times; only then it counts as done so the morning summary is not held up.
fixer_exit() {
  local failed="$NIGHT_DIR/.fixer-$FIX_ROUND.failed" count=1 prev=""
  if [ "$FIX_OK" = 1 ]; then
    touch "$NIGHT_DIR/.fixer-$FIX_ROUND.done"
    log "fixer round $FIX_ROUND finished"
    return 0
  fi
  [ -s "$failed" ] && read -r prev _ < "$failed" && count=$((prev + 1))
  echo "$count $(now_epoch)" > "$failed"
  if [ "$count" -ge "$FIX_MAX_ATTEMPTS" ]; then
    touch "$NIGHT_DIR/.fixer-$FIX_ROUND.done"
    note_report "## Fixer round $FIX_ROUND failed (attempt $count of $FIX_MAX_ATTEMPTS, giving up): ${FIX_FAIL_REASON:-it stopped before doing any work}"
  else
    rm -f "$NIGHT_DIR/.fixer-$FIX_ROUND-started"
    note_report "## Fixer round $FIX_ROUND failed (attempt $count of $FIX_MAX_ATTEMPTS, retrying in ${FIX_RETRY_SECONDS}s): ${FIX_FAIL_REASON:-it stopped before doing any work}"
  fi
  log "fixer round $FIX_ROUND failed (attempt $count): ${FIX_FAIL_REASON:-stopped early}"
}
# A failed round waits FIX_RETRY_SECONDS before it is offered again.
fixer_retry_wait_over() { # fixer_retry_wait_over <round>
  local failed="$NIGHT_DIR/.fixer-$1.failed" count at
  [ -s "$failed" ] || return 0
  read -r count at < "$failed"
  [ "$(now_epoch)" -ge $(( ${at:-0} + FIX_RETRY_SECONDS )) ]
}

# ---------------------------------------------------------------------------------------------
# The morning summary
# ---------------------------------------------------------------------------------------------
cmd_summary() {
  set_night "${1:-$(default_night)}"
  mkdir -p "$NIGHT_DIR"
  exec 7>"$NIGHT_DIR/.summary.lock"
  flock -n 7 || { log "summary already running for $NIGHT"; exit 0; }
  if [ -s "$NIGHT_DIR/summary.md" ] && [ -z "${FORCE:-}" ]; then
    log "summary for $NIGHT already exists"
    exit 0
  fi
  # Let the last proctor finish its entry.
  exec 9>"$LOCK"
  flock -w 1800 9 || log "sweep still running after 30 minutes; summarising what is there"
  [ -f "$NIGHT_DIR/report.md" ] || { log "no report for $NIGHT"; exit 0; }
  log "writing morning summary for $NIGHT"
  local prompt agenda_line
  agenda_line=$(python3 "$NIGHTLY_DIR/coverage.py" --agenda-line "$NIGHT_DIR/runs" 2>/dev/null || echo "agenda phase: unknown")
  prompt=$(render "$NIGHTLY_DIR/prompts/summary.md" "REPORT=$NIGHT_DIR/report.md" "AGENDA_LINE=$agenda_line" \
    "NIGHT_DIR=$NIGHT_DIR/runs" "NIGHT_ROOT=$NIGHT_DIR" "REPO=$REPO" "STOP_ROUND=$STOP_ROUND" \
    "FIX_BRANCH=$FIX_BRANCH" "FIXER1_BRANCH=nightly-opus-$NIGHT-r1" "FIXER2_BRANCH=nightly-opus-$NIGHT-r2" \
    "SLOTS_NOTE=$(slots_note_summary)")
  (cd "$REPO" && timeout 3600 "$CLAUDE_BIN" -p --model "$SUMMARY_MODEL" --no-session-persistence \
    --permission-mode dontAsk --tools Bash Read Grep Glob \
    --allowedTools "Read" "Grep" "Glob" "Bash(git log:*)" "Bash(git show:*)" "Bash(git diff:*)" \
    --disallowedTools "Edit" "Write" "NotebookEdit" "Agent" \
    -- "$prompt" > "$NIGHT_DIR/summary.md.tmp" 2> "$NIGHT_DIR/summary.err" < /dev/null)
  local status=$?
  if [ "$status" -eq 0 ] && [ -s "$NIGHT_DIR/summary.md.tmp" ]; then
    # The agenda line goes on top mechanically, whatever the model wrote.
    { printf '%s\n\n' "$agenda_line"; cat "$NIGHT_DIR/summary.md.tmp"; } > "$NIGHT_DIR/summary.md"
    rm -f "$NIGHT_DIR/summary.md.tmp"
    ln -sfn "$NIGHT_DIR/summary.md" "$REPORT_ROOT/latest-summary.md"
    log "summary written: $NIGHT_DIR/summary.md"
  else
    log "summary failed (exit $status), see $NIGHT_DIR/summary.err"
  fi
}

# ---------------------------------------------------------------------------------------------
# tick: decide what is due. Looks at the previous night too, whose round 2 and summary run after
# the next window has already started (the gap before the next 20:30 start is long).
# ---------------------------------------------------------------------------------------------
decide_night() { # decide_night <night> <current: 1|0>; prints "<action> <night> [round]"
  set_night "$1"
  local current="$2" now
  now=$(now_epoch)
  [[ "$NIGHT" < "$NOT_BEFORE" ]] && return 0

  if [ "$current" = 1 ] && [ "$now" -ge "$START_EPOCH" ] && [ $(( END_EPOCH - now )) -gt "$MIN_RUN_SECONDS" ] \
    && [ ! -f "$NIGHT_DIR/sweep.done" ] && ! sweep_running; then
    if previous_pipeline_busy && [ "$now" -lt $(( START_EPOCH + START_DEFER_SECONDS )) ]; then
      return 0
    fi
    echo "sweep $NIGHT"
    return 0
  fi

  if [ "$current" = 1 ] && fixer_enabled 1 && [ ! -f "$NIGHT_DIR/.fixer-1-started" ] \
    && { [ "$now" -ge "$(fix1_epoch)" ] || [ -s "$NIGHT_DIR/fix-requested" ]; } && [ "$now" -lt "$END_EPOCH" ] \
    && fixer_retry_wait_over 1 \
    && [ ! -f "$NIGHT_DIR/sweep.done" ] && [ "$(report_entries)" -ge 1 ]; then
    echo "fix $NIGHT 1"
    return 0
  fi

  # A second early request (made once round 1 had started): round 2 starts as soon as round 1 has
  # finished, while the sweep keeps running. Its branch is merged between games like round 1's.
  if [ "$current" = 1 ] && fixer_enabled 2 && [ ! -f "$NIGHT_DIR/.fixer-2-started" ] \
    && [ -s "$NIGHT_DIR/fix-requested-2" ] && { ! fixer_enabled 1 || [ -f "$NIGHT_DIR/.fixer-1.done" ]; } \
    && [ "$now" -lt "$END_EPOCH" ] && fixer_retry_wait_over 2 \
    && [ ! -f "$NIGHT_DIR/sweep.done" ] && [ "$(report_entries)" -ge 1 ]; then
    echo "fix $NIGHT 2"
    return 0
  fi

  if fixer_enabled 2 && [ ! -f "$NIGHT_DIR/.fixer-2-started" ] && [ -f "$NIGHT_DIR/sweep.done" ] \
    && fixer_retry_wait_over 2 \
    && ! sweep_running && ! lock_held "$NIGHT_DIR/.fixer-1.lock"; then
    if [ "$(report_entries)" -ge 1 ]; then echo "fix $NIGHT 2"; else echo "skip-fix $NIGHT 2"; fi
    return 0
  fi

  if [ "$now" -ge "$END_EPOCH" ] && [ $(( now - END_EPOCH )) -lt $(( FIX2_MAX_SECONDS + 14400 )) ] \
    && [ ! -s "$NIGHT_DIR/summary.md" ] && [ ! -f "$NIGHT_DIR/.summary-started" ] && fixer2_gate_open; then
    echo "summary $NIGHT"
  fi
}
decide_all() {
  local cur
  cur=$(default_night)
  decide_night "$(shift_day "$cur" -1)" 0
  decide_night "$cur" 1
}

cmd_tick() {
  local action night arg
  while read -r action night arg; do
    [ -n "${action:-}" ] || continue
    set_night "$night"
    mkdir -p "$NIGHT_DIR"
    case "$action" in
      sweep)
        log "tick: starting sweep for $NIGHT"
        setsid "$0" loop "$NIGHT" >> "$NIGHT_DIR/sweep.log" 2>&1 < /dev/null &
        ;;
      fix)
        touch "$NIGHT_DIR/.fixer-$arg-started"
        log "tick: starting fixer round $arg for $NIGHT"
        setsid "$0" fix "$arg" "$NIGHT" >> "$NIGHT_DIR/sweep.log" 2>&1 < /dev/null &
        ;;
      skip-fix)
        touch "$NIGHT_DIR/.fixer-$arg-started" "$NIGHT_DIR/.fixer-$arg.done"
        log "tick: no run entries for $NIGHT, skipping fixer round $arg"
        ;;
      summary)
        touch "$NIGHT_DIR/.summary-started"
        log "tick: starting morning summary for $NIGHT"
        setsid "$0" summary "$NIGHT" >> "$NIGHT_DIR/sweep.log" 2>&1 < /dev/null &
        ;;
    esac
  done < <(decide_all)
}

case "${1:-}" in
  tick) cmd_tick ;;
  tick-decide) decide_all ;;
  loop) cmd_loop "${2:-}" ;;
  highest-run-number) highest_run_number "${2:?usage: nightly.sh highest-run-number <runs-dir>}" ;; # test hook
  mem-needed) mem_needed_mb ;; # test hook
  link-node-modules) link_node_modules "${2:?usage: nightly.sh link-node-modules <worktree> [copy]}" "${3:-}" ;; # test hook
  run-made-decisions) run_made_decisions "${2:?usage: nightly.sh run-made-decisions <run-dir>}" ;; # test hook
  fix) cmd_fix "${2:-}" "${3:-}" ;;
  summary) cmd_summary "${2:-}" ;;
  *) echo "usage: $0 tick|tick-decide|loop [night]|fix <round> [night]|summary [night]" >&2; exit 2 ;;
esac
