#!/usr/bin/env bash
# Plays ONE offline game to its end (or its cap) and writes the run's evidence. Used by
# offline-run.sh (one call per game, any number side by side).
#
#   offline_game.sh <run-dir> <base-url>
#
# <base-url> is a `vite preview` of the offline bundle (offline_build.sh). Parameters are picked at
# random from the NIGHTLY_OFFLINE_* lists in config.sh (seed, table size, policy, viewport) and can
# be forced for a reproduction:
#   OFFLINE_FORCE_SEED, OFFLINE_FORCE_CLICK_SEED, OFFLINE_FORCE_PLAYERS, OFFLINE_FORCE_POLICY,
#   OFFLINE_FORCE_VIEWPORT (desktop|phone), OFFLINE_FORCE_SEAT, OFFLINE_FORCE_FACTION, OFFLINE_FORCE_RESUME_AT
#   (empty value: no resume check)
# Artifacts in <run-dir> (the layout of a server run where it makes sense):
#   meta.json  run.log  exit_code  digest.md  entry.md  resources.log
#   trace/  game.json trace.jsonl events.jsonl report.json save.json failure.txt browser-errors.json shots/
set -uo pipefail
source "$(dirname "$0")/config.sh"

run_dir="${1:?usage: offline_game.sh <run-dir> <base-url>}"
base_url="${2:?usage: offline_game.sh <run-dir> <base-url>}"
mkdir -p "$run_dir/trace"

pick() { shuf -e $1 -n 1; }
game_seed="${OFFLINE_FORCE_SEED:-$(shuf -i 1-4000000000 -n 1)}"
click_seed="${OFFLINE_FORCE_CLICK_SEED:-$(shuf -i 1-2000000000 -n 1)}"
players="${OFFLINE_FORCE_PLAYERS:-$(pick "$OFFLINE_PLAYER_COUNTS")}"
policy="${OFFLINE_FORCE_POLICY:-$(pick "$OFFLINE_POLICIES")}"
viewport="${OFFLINE_FORCE_VIEWPORT:-desktop}"
if [ -z "${OFFLINE_FORCE_VIEWPORT:-}" ] && [ "$(shuf -i 1-100 -n 1)" -le "$OFFLINE_PHONE_PERCENT" ]; then viewport=phone; fi
seat="${OFFLINE_FORCE_SEAT:-random}"
faction="${OFFLINE_FORCE_FACTION:-random}"
resume_at="${OFFLINE_FORCE_RESUME_AT:-}"
if [ -z "${OFFLINE_FORCE_RESUME_AT+x}" ] && [ "$(shuf -i 1-100 -n 1)" -le "$OFFLINE_RESUME_PERCENT" ]; then resume_at=$(shuf -i 8-160 -n 1); fi
run_id="${OFFLINE_RUN_ID:-$(basename "$run_dir")-$$-$(date +%s)}"
commit=$(git -C "$GAME_REPO" rev-parse HEAD 2>/dev/null || echo unknown)
dirty_files=$(git -C "$GAME_REPO" status --porcelain 2>/dev/null | wc -l)
repro="cd web && TI4_OFFLINE_URL=<vite preview of the offline bundle> TI4_OFFLINE_GAME_SEED=$game_seed TI4_OFFLINE_CLICK_SEED=$click_seed TI4_OFFLINE_PLAYERS=$players TI4_OFFLINE_POLICY=$policy TI4_OFFLINE_VIEWPORT=$viewport TI4_OFFLINE_SEAT=$seat TI4_OFFLINE_FACTION=$faction${resume_at:+ TI4_OFFLINE_RESUME_AT=$resume_at} npm run test:e2e:offline-sweep"
cat > "$run_dir/meta.json" <<JSON
{
  "mode": "offline",
  "game_seed": $game_seed,
  "click_seed": $click_seed,
  "players": $players,
  "policy": "$policy",
  "viewport": "$viewport",
  "seat": "$seat",
  "faction": "$faction",
  "resume_at": "$resume_at",
  "preset": "",
  "card_set": "",
  "web": "offline",
  "commit": "$commit",
  "dirty_files": $dirty_files,
  "started_at": "$(TZ="$NIGHTLY_TZ" date '+%F %T %Z')",
  "base_url": "$base_url",
  "max_decisions": $OFFLINE_MAX_DECISIONS,
  "max_seconds": $OFFLINE_MAX_SECONDS,
  "repro": "$repro",
  "replay": "cargo run -p ti4-wasm --example replay_save -- $run_dir/trace/save.json"
}
JSON

cd "$GAME_REPO/web" || { echo 1 > "$run_dir/exit_code"; exit 1; }
export TI4_OFFLINE_RUN_ID="$run_id"
export TI4_OFFLINE_URL="$base_url" TI4_OFFLINE_TRACE_DIR="$run_dir/trace" TI4_OFFLINE_PW_OUTPUT="$run_dir/playwright" \
  TI4_OFFLINE_GAME_SEED="$game_seed" TI4_OFFLINE_CLICK_SEED="$click_seed" TI4_OFFLINE_PLAYERS="$players" \
  TI4_OFFLINE_POLICY="$policy" TI4_OFFLINE_VIEWPORT="$viewport" TI4_OFFLINE_SEAT="$seat" TI4_OFFLINE_FACTION="$faction" TI4_OFFLINE_RESUME_AT="$resume_at" \
  TI4_OFFLINE_MAX_DECISIONS="$OFFLINE_MAX_DECISIONS" TI4_OFFLINE_MAX_SECONDS="$OFFLINE_MAX_SECONDS" \
  TI4_OFFLINE_STALL_SECONDS="$OFFLINE_STALL_SECONDS" TI4_OFFLINE_SHOT_CAP="$OFFLINE_SHOT_CAP"
dev_shm_ok && export TI4_E2E_DEV_SHM=1
{ echo "== at start"; resource_snapshot; } > "$run_dir/resources.log" 2>&1
( while sleep "$RESOURCE_LOG_SECONDS"; do resource_snapshot >> "$run_dir/resources.log"; done ) &
sampler=$!
# Peak memory of the run's own processes (browser, worker, node), sampled with the resource log.
( while sleep 10; do
    kb=$(grep -lsz "^TI4_OFFLINE_RUN_ID=$run_id\$" /proc/[0-9]*/environ 2>/dev/null | cut -d/ -f3 \
      | xargs -r -I{} awk '/^Pss:/ { s += $2 } END { print s + 0 }' /proc/{}/smaps_rollup 2>/dev/null | awk '{ t += $1 } END { print t + 0 }')
    echo "$kb" >> "$run_dir/rss_kb.log"
  done ) &
mem_sampler=$!
timeout --kill-after=30 $(( OFFLINE_MAX_SECONDS + 300 )) \
  npx playwright test -c e2e/offline-sweep.config.ts --reporter=line > "$run_dir/run.log" 2>&1
code=$?
kill "$sampler" "$mem_sampler" 2>/dev/null
{ echo "== at end"; resource_snapshot; } >> "$run_dir/resources.log" 2>&1
kill_processes_with_env TI4_OFFLINE_RUN_ID "$run_id"
echo "$code" > "$run_dir/exit_code"
python3 "$NIGHTLY_DIR/offline_entry.py" "$run_dir" > /dev/null 2>> "$run_dir/run.log" || true
exit 0
