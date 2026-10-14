#!/usr/bin/env bash
# Started by run_game.sh under setsid; runs one smoke playthrough and records its exit code.
source "$(dirname "$0")/config.sh"
run_dir="$1" players="$2" game_seed="$3" click_seed="$4" policy="$5" port="$6" budget="$7" preset="${8:-}" card_set="${9:-te}" exercise_env="${10:-}" map_template="${11:-}" web="${12:-dev}"
cd "$GAME_REPO/web" || exit 1
[ "$web" != build ] || export TI4_E2E_WEB=build
# Optional exercises: "TI4_SMOKE_UI_TOUR=1 TI4_SMOKE_RECAP=1 ..." from run_game.sh.
for assignment in $exercise_env; do export "$assignment"; done
export TI4_SMOKE_SHOT_CAP="$SHOT_CAP" TI4_SMOKE_PREP_PROBABILITY="$PREP_PROBABILITY" \
  TI4_SMOKE_PREP_AUTO_PROBABILITY="$PREP_AUTO_PROBABILITY"
[ "$PREP_PROBABILITY" != 0 ] || export TI4_SMOKE_PREP=0
export TI4_SMOKE=1 TI4_SMOKE_PLAYERS="$players" TI4_SMOKE_GAME_SEED="$game_seed" \
  TI4_SMOKE_CLICK_SEED="$click_seed" TI4_SMOKE_POLICY="$policy" \
  TI4_SMOKE_ROUND="$STOP_ROUND" TI4_SMOKE_DECISIONS="$MAX_DECISIONS" \
  TI4_SMOKE_TRACE_DIR="$run_dir/trace" TI4_SMOKE_PRESET="$preset" TI4_SMOKE_CARD_SET="$card_set" TI4_SMOKE_MAP_TEMPLATE="$map_template" \
  TI4_E2E_BACKEND_PORT="$port" TI4_E2E_FRONTEND_PORT="$((port + 1))"
# Host resources (see config.sh): clear /tmp leftovers of finished runs, keep the browser's shared
# memory off a full /tmp, and leave a time series next to the run so a refused-resources failure
# shows what the host looked like.
{
  clean_stale_tmp
  echo "== at start"; resource_snapshot
  echo "== largest entries of $TMP_ROOT (MB)"; tmp_top 8
} > "$run_dir/resources.log" 2>&1
if dev_shm_ok; then
  export TI4_E2E_DEV_SHM=1
  echo "== browser shared memory: $SHM_ROOT (TI4_E2E_DEV_SHM=1)" >> "$run_dir/resources.log"
else
  echo "== browser shared memory: $TMP_ROOT (less than ${SHM_MIN_MB} MB free in $SHM_ROOT)" >> "$run_dir/resources.log"
fi
( while sleep "$RESOURCE_LOG_SECONDS"; do resource_snapshot >> "$run_dir/resources.log"; done ) &
sampler=$!
timeout --kill-after=30 "$budget" npx playwright test e2e/smoke_playthrough.spec.ts \
  --global-timeout=0 --reporter=line --output="$run_dir/playwright" > "$run_dir/run.log" 2>&1
code=$?
kill "$sampler" 2>/dev/null
{ echo "== at end"; resource_snapshot; } >> "$run_dir/resources.log" 2>&1
# Nothing of this run may outlive it (a deadline kill can orphan browsers and web servers).
kill_run_processes "$port"
echo "$code" > "$run_dir/exit_code"
# Keep the server's own record of the game next to the trace.
# A copy that succeeded replaces the (RAM-backed) original; a failed copy leaves it for the next start's clean-up.
cp -r "$TMP_ROOT/ti4-playwright-games-$port" "$run_dir/server-data" 2>/dev/null \
  && rm -rf "${TMP_ROOT:?}/ti4-playwright-games-$port"
exit 0
