#!/usr/bin/env bash
# Start one random UI smoke playthrough in the background.
#
#   run_game.sh start <run-dir>     pick random seeds, launch, print the run's parameters
#   run_game.sh stop  <run-dir>     kill the run (all of its processes)
#
# The run plays every seat through the browser until STOP_ROUND, game over, the first failure
# or DEADLINE (epoch seconds, from the proctor loop). Artifacts land in <run-dir>:
#   meta.json  run.log (Playwright + backend stderr: engine panics show up here)
#   trace/     game.json trace.jsonl report.json final-snapshot.json browser-errors.json failure.txt
#   exit_code  written when the run ends
set -euo pipefail
source "$(dirname "$0")/config.sh"

cmd="${1:-}"
run_dir="${2:-}"
[ -n "$cmd" ] && [ -n "$run_dir" ] || { echo "usage: $0 start|stop <run-dir>" >&2; exit 2; }

case "$cmd" in
start)
  if [ -f "$run_dir/run.pid" ]; then
    echo "run already started in $run_dir (pid $(cat "$run_dir/run.pid"))"
    cat "$run_dir/meta.json"
    exit 0
  fi
  mkdir -p "$run_dir/trace"
  game_seed=$(shuf -i 1-2000000000 -n 1)
  click_seed=$(shuf -i 1-2000000000 -n 1)
  players=$(shuf -e $PLAYER_COUNTS -n 1)
  policy=$(shuf -e $POLICIES -n 1)
  preset=""
  if [ "$(shuf -i 1-100 -n 1)" -le "$PRESET_PROBABILITY" ]; then
    preset=$(shuf -e $PRESET_NAME -n 1)
    preset_base="$preset"
    if [ "$preset" != leaders ] && [ "$(shuf -i 1-100 -n 1)" -le "$PRESET_ROTATE_PERCENT" ]; then
      preset="$preset+rot"
    fi
    # Roster factions replace the rotation of the six (and `world` always uses them).
    if [ "$preset_base" != world ] && [ "$preset" = "$preset_base" ] \
      && [ "$(shuf -i 1-100 -n 1)" -le "$PRESET_FACTIONS_PERCENT" ]; then
      preset="$preset+fac"
    fi
    case " $SHORT_PRESETS " in
      *" $preset_base "*)
        [ "$(shuf -i 1-100 -n 1)" -le "$PRESET_SHORT_PERCENT" ] && preset="$preset+short" ;;
    esac
    case " $AGENDA_EXCLUDE " in
      *" $preset_base "*) ;;
      *)
        agenda_percent=$PRESET_AGENDA_PERCENT
        # Coverage-aware: a night (or its predecessor) without a single agenda phase pushes harder.
        # Only inside a night's <night>/runs/<run> layout (a stray run dir has no history to read).
        night_dir="$(dirname "$(dirname "$run_dir")")"
        cov="$(dirname "$0")/coverage.py"
        voted=1 total=0
        if [ "$(basename "$(dirname "$run_dir")")" = runs ]; then
          read -r voted total < <(python3 "$cov" --agenda-runs "$night_dir" 2>/dev/null || echo "0 0")
        fi
        if [ "${total:-0}" -ge "$AGENDA_NIGHT_MIN" ] && [ "${voted:-0}" -eq 0 ]; then
          agenda_percent=100
        else
          prev=""
          [ "$(basename "$(dirname "$run_dir")")" != runs ] \
            || prev=$(ls -1d "$(dirname "$night_dir")"/20??-??-?? 2>/dev/null | awk -v me="$night_dir" '$0 < me' | tail -n 1) || prev=""
          if [ -n "$prev" ]; then
            read -r voted total < <(python3 "$cov" --agenda-runs "$prev" 2>/dev/null || echo "0 0")
            if [ "${total:-0}" -ge 5 ] && [ "${voted:-0}" -eq 0 ] && [ "$AGENDA_BOOST_PERCENT" -gt "$agenda_percent" ]; then
              agenda_percent=$AGENDA_BOOST_PERCENT
            fi
          fi
        fi
        [ "$(shuf -i 1-100 -n 1)" -le "$agenda_percent" ] && preset="$preset+agenda" ;;
    esac
  fi
  preset_env=""
  [ -z "$preset" ] || preset_env="TI4_SMOKE_PRESET=$preset "
  map_template=""
  if [ "${preset%%+*}" = explore ] && [ "$players" = 3 ] && [ "$(shuf -i 1-100 -n 1)" -le "$EXPLORE_MAP_PERCENT" ]; then
    map_template="$EXPLORE_MAP_TEMPLATE"
  fi
  map_env=""
  [ -z "$map_template" ] || map_env="TI4_SMOKE_MAP_TEMPLATE=$map_template "
  card_set=te
  [ "$(shuf -i 1-100 -n 1)" -le "$POK_PROBABILITY" ] && card_set=pok
  card_env=""
  [ "$card_set" = te ] || card_env="TI4_SMOKE_CARD_SET=$card_set "
  exercise_env=""
  [ "$(shuf -i 1-100 -n 1)" -le "$UI_TOUR_PROBABILITY" ] && exercise_env="${exercise_env}TI4_SMOKE_UI_TOUR=1 "
  [ "$(shuf -i 1-100 -n 1)" -le "$RECAP_PROBABILITY" ] && exercise_env="${exercise_env}TI4_SMOKE_RECAP=1 "
  [ "$(shuf -i 1-100 -n 1)" -le "$REDO_PROBABILITY" ] && exercise_env="${exercise_env}TI4_SMOKE_REDO=1 "
  exercises=$(echo "$exercise_env" | sed -e 's/TI4_SMOKE_//g' -e 's/=1//g' | tr 'A-Z_' 'a-z-' | xargs)
  web=dev
  [ "$WEB_BUILD_SHARE" -gt 0 ] && [ "$(shuf -i 1-100 -n 1)" -le "$WEB_BUILD_SHARE" ] && web=build
  web_env=""
  [ "$web" = dev ] || web_env="TI4_E2E_WEB=build "
  # The backend takes <port>, the frontend <port>+1. Skip ports that are bound right now or that a
  # run still in progress (another slot, possibly still building) has picked.
  port=""
  for _ in $(seq 1 50); do
    candidate=$(shuf -i "$PORT_MIN-$PORT_MAX" -n 1)
    if python3 - "$candidate" "$(dirname "$run_dir")" <<'PY'
import glob, json, os, socket, sys
port, runs = int(sys.argv[1]), sys.argv[2]
for meta in glob.glob(os.path.join(runs, "*", "meta.json")):
    if os.path.exists(os.path.join(os.path.dirname(meta), "exit_code")):
        continue
    try:
        other = int(json.load(open(meta))["backend_port"])
    except Exception:
        continue
    if abs(other - port) <= 1:
        sys.exit(1)
for number in (port, port + 1):
    sock = socket.socket()
    try:
        sock.bind(("127.0.0.1", number))
    except OSError:
        sys.exit(1)
    finally:
        sock.close()
PY
    then port=$candidate; break; fi
  done
  [ -n "$port" ] || port=$(shuf -i "$PORT_MIN-$PORT_MAX" -n 1)
  deadline="${DEADLINE:-$(( $(now_epoch) + 6 * 3600 ))}"
  # The code under test: nothing keeps interactive commits off the night branch while a game plays,
  # so each run records the commit and how many files differ from it.
  commit=$(git -C "$GAME_REPO" rev-parse HEAD 2>/dev/null || echo unknown)
  dirty_files=$(git -C "$GAME_REPO" status --porcelain 2>/dev/null | wc -l)
  budget=$(( deadline - $(now_epoch) ))
  [ "$budget" -gt 60 ] || { echo "no time left before the deadline" >&2; exit 1; }
  cat > "$run_dir/meta.json" <<EOF
{
  "game_seed": $game_seed,
  "click_seed": $click_seed,
  "players": $players,
  "policy": "$policy",
  "preset": "$preset",
  "card_set": "$card_set",
  "web": "$web",
  "map_template": "$map_template",
  "exercises": "$exercises",
  "prep_probability": "$PREP_PROBABILITY",
  "prep_auto_probability": "$PREP_AUTO_PROBABILITY",
  "commit": "$commit",
  "dirty_files": $dirty_files,
  "stop_round": $STOP_ROUND,
  "backend_port": $port,
  "server_data_dir": "/tmp/ti4-playwright-games-$port",
  "started_at": "$(TZ="$NIGHTLY_TZ" date '+%F %T %Z')",
  "deadline": "$(TZ="$NIGHTLY_TZ" date -d "@$deadline" '+%F %T %Z')",
  "repro": "cd web && ${web_env}TI4_SMOKE=1 ${preset_env}${card_env}${map_env}${exercise_env}TI4_SMOKE_PREP_PROBABILITY=$PREP_PROBABILITY TI4_SMOKE_PREP_AUTO_PROBABILITY=$PREP_AUTO_PROBABILITY TI4_SMOKE_PLAYERS=$players TI4_SMOKE_GAME_SEED=$game_seed TI4_SMOKE_CLICK_SEED=$click_seed TI4_SMOKE_POLICY=$policy TI4_SMOKE_ROUND=$STOP_ROUND TI4_SMOKE_DECISIONS=$MAX_DECISIONS npm run test:e2e:smoke"
}
EOF
  # A new session makes the run its own process group, so `stop` can kill browsers and the
  # Playwright web servers (cargo/vite) together.
  setsid "$NIGHTLY_DIR/_run_inner.sh" "$run_dir" "$players" "$game_seed" "$click_seed" \
    "$policy" "$port" "$budget" "$preset" "$card_set" "$exercise_env" "$map_template" "$web" </dev/null >/dev/null 2>&1 &
  echo $! > "$run_dir/run.pid"
  echo "started run in $run_dir"
  cat "$run_dir/meta.json"
  ;;
stop)
  if [ -f "$run_dir/run.pid" ] && [ ! -f "$run_dir/exit_code" ]; then
    pid=$(cat "$run_dir/run.pid")
    # Playwright starts its web servers and browsers in new process groups/sessions, so a group
    # kill misses them. Collect everything in the run's session plus all descendants first
    # (children are re-parented to init once their parent dies), then kill the whole set.
    descendants() { local child; for child in $(ps -o pid= --ppid "$1"); do echo "$child"; descendants "$child"; done; }
    procs=$( { echo "$pid"
      for p in $(ps -eo pid=,sid= | awk -v s="$pid" '$2 == s { print $1 }'); do echo "$p"; descendants "$p"; done
    } | sort -u)
    kill -TERM $procs 2>/dev/null || true
    port=$(python3 -c "import json,sys; print(json.load(open(sys.argv[1]))['backend_port'])" "$run_dir/meta.json")
    kill_run_processes "$port"
    kill -KILL $procs 2>/dev/null || true
    [ -f "$run_dir/exit_code" ] || echo "killed" > "$run_dir/exit_code"
    echo "stopped run in $run_dir"
  else
    echo "run in $run_dir is not running"
  fi
  ;;
*)
  echo "usage: $0 start|stop <run-dir>" >&2
  exit 2
  ;;
esac
