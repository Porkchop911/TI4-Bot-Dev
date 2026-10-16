#!/usr/bin/env bash
# A proctor's request for an Opus fix round before its scheduled time. Call it as soon as a real
# error shows up, do not wait for it to repeat. See README.md and prompts/proctor.md.
#
#   request_fix.sh <reason...>
#
# Round 1 (scheduled for FIX1_HHMM): the first request of a night writes $NIGHT_DIR/fix-requested
# and nightly.sh decide_night treats round 1 as due.
# Once round 1 has started, the first request after that writes $NIGHT_DIR/fix-requested-2 instead
# (the reason reaches the round 2 prompt); round 2 then starts early as soon as round 1 has
# finished, while the sweep keeps running. Further requests are refused, as are all requests once
# round 2 has started or when no round is enabled: such a problem goes into the report entry.
set -uo pipefail
source "$(dirname "$0")/config.sh"

[ $# -ge 1 ] || { echo "usage: $0 <reason...>  (run, steps, error text, seed, evidence path)" >&2; exit 2; }
reason="$*"

read -r night _ _ < <(current_window)
NIGHT_DIR="$REPORT_ROOT/$night"
fixer_on() { case " $FIXERS " in *" $1 "*) return 0 ;; esac; return 1; }
if ! fixer_on 1 && ! fixer_on 2; then
  echo "request_fix: no fix round is enabled (FIXERS='$FIXERS'); nothing requested" >&2
  exit 1
fi
mkdir -p "$NIGHT_DIR"
# Which round does this request ask for? Round 1 until it has started; then round 2 (which is also
# the only round when round 1 is disabled).
target=1 file=fix-requested
if [ -f "$NIGHT_DIR/.fixer-1-started" ] || ! fixer_on 1; then
  target=2 file=fix-requested-2
  if ! fixer_on 2; then
    echo "request_fix: fix round 1 has already started for night $night and round 2 is disabled; put the problem in your report entry" >&2
    exit 1
  fi
  if [ -f "$NIGHT_DIR/.fixer-2-started" ]; then
    echo "request_fix: fix round 2 has already started for night $night; put the problem in your report entry" >&2
    exit 1
  fi
fi
# A 0-byte marker (written while the disk was full) is no request; it must not block a real one.
[ -e "$NIGHT_DIR/$file" ] && [ ! -s "$NIGHT_DIR/$file" ] && rm -f "$NIGHT_DIR/$file"
# Parallel slots: the proctor's slot comes from its environment, else from its working directory
# (.../slot-K/repo); the run is the one active on that slot. The reason then carries the slot, so
# the fixer knows which of the games running side by side hit the problem.
slot="${NIGHTLY_SLOT:-}"
if [ -z "$slot" ]; then
  case "$PWD" in
    */slot-[0-9]*/repo|*/slot-[0-9]*/repo/*) slot=${PWD#*/slot-}; slot=${slot%%/*} ;;
  esac
fi
run="${NIGHTLY_RUN_NAME:-}"
if [ -z "$run" ] && [ -n "$slot" ] && [ -s "$NIGHT_DIR/.active-s$slot" ]; then run=$(sed -n 2p "$NIGHT_DIR/.active-s$slot"); fi
if [ -z "$run" ] && [ -d "$NIGHT_DIR/runs" ]; then
  if [ -n "$slot" ]; then run=$(ls -1t "$NIGHT_DIR/runs" 2>/dev/null | head -n 1)
  else run=$(ls -1 "$NIGHT_DIR/runs" 2>/dev/null | tail -n 1); fi
fi
[ -z "$slot" ] || reason="[slot $slot] $reason"
if ( set -o noclobber
     { echo "time: $(TZ="$NIGHTLY_TZ" date -d "@$(now_epoch)" '+%F %T %Z')"
       echo "run: ${run:-unknown}"
       echo "reason: $reason"; } > "$NIGHT_DIR/$file" ) 2>/dev/null; then
  if [ "$target" = 1 ]; then
    echo "request_fix: recorded for night $night; fix round 1 starts at the next tick (needs at least one report entry)"
  else
    echo "request_fix: fix round 1 already started; recorded for round 2, which starts at the next tick once round 1 has finished (needs at least one report entry)"
  fi
else
  echo "request_fix: already requested for night $night (only one request per round counts); put this problem in your report entry:"
  sed 's/^/  /' "$NIGHT_DIR/$file"
fi
