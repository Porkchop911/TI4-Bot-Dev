#!/usr/bin/env bash
# Follow a running playthrough for up to <seconds> (default 540) and print what changed.
#
#   watch.sh <run-dir> [seconds]
#
# Prints progress (round, decision count, current subtype), new suspicious lines from run.log
# (engine panics, server errors, rejections, browser errors) and ends with one status line:
#   STATUS: RUNNING | FINISHED exit=<code> | STALLED (killed) | KILLED
# A run whose trace has not grown for STALL_SECONDS is stopped and reported as STALLED.
set -uo pipefail
source "$(dirname "$0")/config.sh"

run_dir="${1:?usage: watch.sh <run-dir> [seconds]}"
limit="${2:-540}"
[ "$limit" -le 570 ] || limit=570 # stay below the 10-minute tool call limit
trace="$run_dir/trace/trace.jsonl"
log_file="$run_dir/run.log"
seen_file="$run_dir/.watch-log-offset"
# Dev-proxy noise: the /battle advisor is not started for e2e runs, and sockets close at teardown.
harmless='/battle|ECONNREFUSED|ECONNRESET|socket has been ended by the other party|ws proxy error|http proxy error'
suspicious='panicked|thread .* panicked|RUST_BACKTRACE|ERROR|Error:|error\[|rejected:|click failed|pageerror|console:|did not advance|no actionable control|never reached|game idle|Timeout|timed out|failed|✘'

progress() {
  if [ -s "$trace" ]; then
    tail -n 1 "$trace" | python3 -c '
import json, sys
t = json.loads(sys.stdin.read())
print("progress: decision #{} round {} {} seat{} ({}) {}".format(t["decision"], t["round"], t["phase"], t["seat"], t.get("faction"), t["subtype"]))'
  else
    echo "progress: no decision yet (server building or starting)"
  fi
}

new_log_lines() {
  [ -f "$log_file" ] || return
  local offset size
  offset=$(cat "$seen_file" 2>/dev/null || echo 0)
  size=$(stat -c %s "$log_file")
  [ "$size" -gt "$offset" ] || return
  tail -c +"$((offset + 1))" "$log_file" | grep -E "$suspicious" | grep -vE "$harmless" | grep -v "^\s*$" | cut -c1-400 | head -n 40
  echo "$size" > "$seen_file"
}

start=$(now_epoch)
while :; do
  if [ -f "$run_dir/exit_code" ]; then
    progress
    new_log_lines
    code=$(cat "$run_dir/exit_code")
    if [ "$code" = "killed" ]; then
      [ -f "$run_dir/stalled" ] && echo "STATUS: STALLED (killed)" || echo "STATUS: KILLED"
    else
      echo "STATUS: FINISHED exit=$code"
    fi
    exit 0
  fi
  if [ -s "$trace" ]; then
    idle=$(( $(now_epoch) - $(stat -c %Y "$trace") ))
  elif [ -f "$run_dir/run.pid" ]; then
    idle=$(( $(now_epoch) - $(stat -c %Y "$run_dir/run.pid") ))
    # The first build of the server may take a while; allow twice the stall budget.
    idle=$(( idle / 2 ))
  else
    echo "STATUS: NOT STARTED"
    exit 0
  fi
  if [ "$idle" -ge "$STALL_SECONDS" ]; then
    progress
    new_log_lines
    echo "no new decision for ${idle}s, stopping the run"
    touch "$run_dir/stalled"
    "$NIGHTLY_DIR/run_game.sh" stop "$run_dir" >/dev/null
    echo "STATUS: STALLED (killed)"
    exit 0
  fi
  if [ $(( $(now_epoch) - start )) -ge "$limit" ]; then
    progress
    new_log_lines
    echo "STATUS: RUNNING"
    exit 0
  fi
  sleep 15
done
