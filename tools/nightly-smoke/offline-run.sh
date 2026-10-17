#!/usr/bin/env bash
# The offline sweep: many full games of the offline single-player build, N browser workers side by
# side, no server and (by default) no model. Used by `nightly.sh loop` when NIGHTLY_MODE is offline or
# both, and by hand for a dry run or a reproduction.
#
#   offline-run.sh [--night-dir DIR] [--workers N] [--games N] [--until EPOCH] [--request-fix]
#                  [--repo DIR]
#
#   --night-dir DIR  where runs/ and report.md go (default: <reports>/offline-manual-<time>/)
#   --workers N      browsers playing side by side (default NIGHTLY_OFFLINE_WORKERS)
#   --games N        stop after starting N games in total (default 0: until --until)
#   --until EPOCH    start no game that could not end by then (default 0: no limit; needs --games)
#   --request-fix    ask for a fix round (request_fix.sh) at the first blocker or error of a night
#   --repo DIR       the checkout to build and play (default: the live checkout, GAME_REPO)
#
# Per game: <night-dir>/runs/oK-NN-HHMM/ (K = worker) with the evidence offline_game.sh writes and a
# report entry in the proctors' shape appended to <night-dir>/report.md (one write under
# .report.lock, the same lock nightly.sh uses). Stops cleanly on SIGTERM/SIGINT/SIGHUP: running games
# are killed and get an entry marked "terminated mid-run".
set -uo pipefail
source "$(dirname "$0")/config.sh"
for var in $(env | grep -o '^CLAUDE[A-Z_]*' || true); do unset "$var"; done
unset AI_AGENT

out="" workers="$OFFLINE_WORKERS" games=0 until_epoch=0 request_fix=0
while [ $# -gt 0 ]; do
  case "$1" in
    --night-dir) out="$2"; shift 2 ;;
    --workers) workers="$2"; shift 2 ;;
    --games) games="$2"; shift 2 ;;
    --until) until_epoch="$2"; shift 2 ;;
    --request-fix) request_fix=1; shift ;;
    --repo) GAME_REPO="$2"; export NIGHTLY_GAME_REPO="$2"; shift 2 ;;
    *) echo "unknown argument $1 (see the header of $0)" >&2; exit 2 ;;
  esac
done
case "$workers" in ''|*[!0-9]*|0) workers=1 ;; esac
[ "$games" -gt 0 ] || [ "$until_epoch" -gt 0 ] || { echo "offline-run.sh: give --games N or --until EPOCH" >&2; exit 2; }
[ -n "$out" ] || out="$REPORT_ROOT/offline-manual-$(date +%Y%m%d-%H%M%S)"
mkdir -p "$out/runs"
report="$out/report.md"
lock="$out/.report.lock"
export NIGHT_DIR="$out"

report_append() { # stdin is appended as a whole, under the lock nightly.sh uses
  local text
  text=$(cat; echo x)
  ( flock 5; printf '%s' "${text%x}" >> "$report" ) 5>> "$lock"
}
log() { echo "[$(TZ="$NIGHTLY_TZ" date '+%F %T %Z')] offline: $*"; }
[ -f "$report" ] || printf '# Offline sweep — %s\n\n%s workers, up to %s games, until %s. Runs are named oK-NN-HHMM (K = worker).\n\n' \
  "$(basename "$out")" "$workers" "$games" "$([ "$until_epoch" -gt 0 ] && date -d "@$until_epoch" '+%F %T' || echo 'the game limit')" | report_append

# --- the bundle and the static server -----------------------------------------------------------
# NIGHTLY_OFFLINE_BUILD_SCRIPT, NIGHTLY_OFFLINE_GAME_SCRIPT and NIGHTLY_OFFLINE_NO_PREVIEW=1 are test hooks.
build_script="${NIGHTLY_OFFLINE_BUILD_SCRIPT:-$NIGHTLY_DIR/offline_build.sh}"
game_script="${NIGHTLY_OFFLINE_GAME_SCRIPT:-$NIGHTLY_DIR/offline_game.sh}"
dist=$("$build_script" "$GAME_REPO" 2> "$out/offline-build.log")
if [ -z "$dist" ] || [ ! -f "$dist/.ok" ]; then
  { echo "## Offline build failed — no offline runs in this batch"; echo '```'; tail -n 40 "$out/offline-build.log"; echo '```'; echo; } | report_append
  log "build failed"; exit 1
fi
port=$(python3 - <<'PY'
import socket
s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()
PY
)
if [ "${NIGHTLY_OFFLINE_NO_PREVIEW:-}" = 1 ]; then
  preview_pid=$$
else
  (cd "$GAME_REPO/web" && TI4_OFFLINE_PREVIEW="$port" exec setsid npx vite preview --outDir "$dist" --host 127.0.0.1 --port "$port" --strictPort) \
    > "$out/offline-preview-$port.log" 2>&1 &
  preview_pid=$!
  for _ in $(seq 1 60); do curl -sf -o /dev/null "http://127.0.0.1:$port/offline" && break; sleep 0.5; done
  curl -sf -o /dev/null "http://127.0.0.1:$port/offline" || {
    { echo "## Offline preview server did not start — no offline runs in this batch"; echo '```'; tail -n 20 "$out/offline-preview-$port.log"; echo '```'; echo; } | report_append
    kill "$preview_pid" 2>/dev/null; log "preview server failed"; exit 1
  }
fi
url="http://127.0.0.1:$port"
log "bundle $(basename "$dist"), preview on $url, $workers workers (games ${games:-0}, until ${until_epoch})"

# --- workers --------------------------------------------------------------------------------------
highest_number() { # highest_number <prefix>
  local d name max=0 num
  for d in "$out"/runs/"$1"*/; do
    [ -d "$d" ] || continue
    name=$(basename "$d")
    [[ "$name" =~ ^$1([0-9]+)-[0-9]{4}$ ]] || continue
    num=$((10#${BASH_REMATCH[1]}))
    [ "$num" -gt "$max" ] && max=$num
  done
  echo "$max"
}
claim_game() { # one more game may start (total limit), counted under a lock
  [ "$games" -gt 0 ] || return 0
  ( flock 4
    started=$(cat "$out/.offline-started" 2>/dev/null || echo 0)
    [ "$started" -lt "$games" ] || exit 1
    echo $((started + 1)) > "$out/.offline-started"
  ) 4> "$out/.offline-started.lock"
}
unclaim_game() {
  [ "$games" -gt 0 ] || return 0
  ( flock 4; started=$(cat "$out/.offline-started" 2>/dev/null || echo 0); echo $((started > 0 ? started - 1 : 0)) > "$out/.offline-started" ) 4> "$out/.offline-started.lock"
}
now() { now_epoch; }
sample_hit() { [ "$1" -gt 0 ] && [ "$(shuf -i 1-100 -n 1)" -le "$1" ]; }

proctor_review() { # proctor_review <run-dir> <name>: Sonnet reviews the evidence; its entry replaces the machine one
  local run_dir="$1" prompt
  prompt=$(sed -e "s#{{RUN_DIR}}#$run_dir#g" -e "s#{{ENTRY}}#$run_dir/entry.md#g" "$NIGHTLY_DIR/prompts/offline_proctor.md")
  (cd "$GAME_REPO" && timeout --kill-after=30 "${NIGHTLY_OFFLINE_PROCTOR_SECONDS:-900}" \
    "$CLAUDE_BIN" -p --model "$PROCTOR_MODEL" --no-session-persistence --permission-mode auto \
    --tools Read Grep Glob --allowedTools "Read" "Grep" "Glob" -- "$prompt" > "$run_dir/proctor-entry.md" 2> "$run_dir/proctor.err" < /dev/null)
  [ -s "$run_dir/proctor-entry.md" ] && grep -q '^## Run ' "$run_dir/proctor-entry.md"
}

worker() { # worker <k>
  local k="$1" prefix="o$1-" n zero=0 name run_dir remaining sev fixmsg run_id
  n=$(highest_number "$prefix")
  trap 'exit 0' TERM INT HUP
  while true; do
    if [ "$until_epoch" -gt 0 ]; then
      remaining=$(( until_epoch - $(now) ))
      [ "$remaining" -gt 120 ] || break
    else
      remaining=$OFFLINE_MAX_SECONDS
    fi
    if ! disk_ok; then log "worker $k: low disk ($(free_gb) GB), stopping"; note="## Offline worker $k stopped: only $(free_gb) GB free, runs need $MIN_FREE_GB GB"; echo "$note" | report_append; break; fi
    claim_game || break
    n=$((n + 1))
    name="$prefix$(printf '%02d' "$n")-$(TZ="$NIGHTLY_TZ" date -d "@$(now_epoch)" +%H%M)"
    run_dir="$out/runs/$name"
    run_id="$name-$$-$(date +%s)"
    mkdir -p "$run_dir"
    printf '%s\n%s\n%s\n' "$run_dir" "$name" "$run_id" > "$out/.active-o$k"
    [ "$remaining" -lt "$OFFLINE_MAX_SECONDS" ] || remaining=$OFFLINE_MAX_SECONDS
    log "run $name: starting"
    OFFLINE_RUN_ID="$run_id" NIGHTLY_OFFLINE_MAX_SECONDS="$remaining" NIGHTLY_RUN_NAME="$name" \
      "$game_script" "$run_dir" "$url" > /dev/null 2>&1
    sev=$(python3 "$NIGHTLY_DIR/offline_entry.py" "$run_dir" --max-severity 2> /dev/null || echo none)
    # A blocker or an error is what a proctor would ask a fix round for.
    if [ "$request_fix" = 1 ] && case "$sev" in blocker*|high*) true ;; *) false ;; esac; then
      fixmsg=$(NIGHTLY_RUN_NAME="$name" "$NIGHTLY_DIR/request_fix.sh" "[offline] run $name: ${sev:0:400} (repro in the entry; evidence $run_dir)" 2>&1) || true
      printf -- '- `request_fix.sh`: %s\n' "$fixmsg" > "$run_dir/fix-request.txt"
    fi
    python3 "$NIGHTLY_DIR/offline_entry.py" "$run_dir" > /dev/null 2>&1
    if [ -s "$run_dir/entry.md" ]; then
      if sample_hit "$OFFLINE_PROCTOR_PERCENT" && proctor_review "$run_dir"; then
        { cat "$run_dir/proctor-entry.md"; echo; echo "_Raw evidence: \`$run_dir\`_"; echo; } | report_append
      else
        { cat "$run_dir/entry.md"; echo "_Raw evidence: \`$run_dir\`_"; echo; } | report_append
      fi
    else
      { echo "## Run $name — offline game harness failed, no entry (exit $(cat "$run_dir/exit_code" 2>/dev/null))"; echo; tail -n 15 "$run_dir/run.log" 2>/dev/null | sed 's/^/    /'; echo; } | report_append
    fi
    rm -f "$out/.active-o$k"
    log "run $name: $(python3 -c "import json,sys; r=json.load(open(sys.argv[1])); print(r['outcome'], str(r['decisions'])+' decisions', str(r['seconds'])+' s', 'seed', r['gameSeed'], str(r['players'])+'p')" "$run_dir/trace/report.json" 2>/dev/null || echo 'no report')"
    # Runs that never reach a decision mean the environment is broken; a fixer cannot help.
    if python3 -c "import json,sys; sys.exit(0 if json.load(open(sys.argv[1])).get('decisions',0)>0 else 1)" "$run_dir/trace/report.json" 2>/dev/null; then zero=0; else zero=$((zero + 1)); fi
    if [ "$zero" -ge "${MAX_ZERO_DECISION_RUNS:-3}" ]; then
      echo "## Offline worker $k stopped: $zero consecutive runs reached no decision (broken environment?)" | report_append
      log "worker $k: $zero zero-decision runs, stopping"; break
    fi
  done
}

rm -f "$out/.offline-started"
pids=()
terminate() { # terminate <reason>: kill the games, file an entry for each, stop the server
  trap '' TERM INT HUP
  local pid f k run_dir name run_id
  for pid in "${pids[@]}"; do kill -TERM "$pid" 2>/dev/null; done
  for f in "$out"/.active-o*; do
    [ -f "$f" ] || continue
    case "$f" in *.lock|*.tmp) continue ;; esac
    { read -r run_dir; read -r name; read -r run_id; } < "$f"
    rm -f "$f"
    kill_processes_with_env TI4_OFFLINE_RUN_ID "$run_id"
    timeout 60 python3 "$NIGHTLY_DIR/offline_entry.py" "$run_dir" --terminated "$1" > /dev/null 2>&1
    if [ -s "$run_dir/entry.md" ]; then { cat "$run_dir/entry.md"; echo "_Raw evidence: \`$run_dir\`_"; echo; } | report_append; fi
    log "run $name: terminated mid-run ($1)"
  done
  kill_processes_with_env TI4_OFFLINE_PREVIEW "$port"
  [ "$preview_pid" = "$$" ] || kill "$preview_pid" 2>/dev/null
}
trap 'terminate SIGTERM; exit 143' TERM
trap 'terminate SIGINT; exit 130' INT
trap 'terminate SIGHUP; exit 129' HUP
# Stale marker files of a sweep that died without cleaning up: close those runs off first.
for f in "$out"/.active-o*; do
  [ -f "$f" ] || continue
  { read -r run_dir; read -r name; read -r run_id; } < "$f"
  rm -f "$f"
  kill_processes_with_env TI4_OFFLINE_RUN_ID "$run_id"
  timeout 60 python3 "$NIGHTLY_DIR/offline_entry.py" "$run_dir" --terminated "earlier sweep died" > /dev/null 2>&1
  [ -s "$run_dir/entry.md" ] && { cat "$run_dir/entry.md"; echo; } | report_append
done

for k in $(seq 1 "$workers"); do
  # workers start a little apart so the first page loads do not all collide
  ( sleep $(( (k - 1) * ${NIGHTLY_OFFLINE_STAGGER_SECONDS:-5} )); worker "$k" ) &
  pids+=("$!")
done
for pid in "${pids[@]}"; do wait "$pid"; done
if [ "$preview_pid" != "$$" ]; then
  kill "$preview_pid" 2>/dev/null
  kill_processes_with_env TI4_OFFLINE_PREVIEW "$port"
  wait "$preview_pid" 2>/dev/null
fi
log "batch finished"
exit 0
