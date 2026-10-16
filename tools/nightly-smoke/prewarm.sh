#!/usr/bin/env bash
# One-time pre-warm of the cargo target dirs the nightly sweep builds into, so the first game of a
# night does not pay the cold build of the third-party dependencies (they are compiled with
# optimization in the dev profile, see [profile.dev.package."*"] in Cargo.toml: about 2.5 min at -j2
# instead of 1.3 min, once per target dir). Run it before the sweep starts (20:30 Berlin), never
# while one is running:
#
#   tools/nightly-smoke/prewarm.sh                 # target-slot-1 .. target-slot-$NIGHTLY_SLOTS (default 2)
#   NIGHTLY_SLOTS=1 tools/nightly-smoke/prewarm.sh # the classic sweep builds into the checkout's own target/
#   tools/nightly-smoke/prewarm.sh --fixer         # also nightly-reports/target-fixer (fix rounds: cargo test)
#
# It builds exactly what BUILD_CMD builds (`cargo build -p ti4-server --bin server`) from the
# checkout $NIGHTLY_REPO (default the live checkout) into each slot's target dir. The slot worktrees
# do not exist yet; that is fine: third-party artifacts are keyed by registry source, so only the
# workspace crates (about a minute) are rebuilt in a slot worktree. Cargo.lock is respected
# (--locked), nothing in the checkout is modified.
# Knobs: PREWARM_CARGO (test hook), PREWARM_JOBS (cargo -j per build, default 3), PREWARM_NICE (default 10), PREWARM_DRY_RUN=1
# prints the commands only.
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=config.sh
source "$HERE/config.sh"

JOBS="${PREWARM_JOBS:-3}"
NICE="${PREWARM_NICE:-10}"
want_fixer=0
[ "${1:-}" = "--fixer" ] && want_fixer=1

# `tick` processes started by cron exit at once; a sweep (`loop`), fix round or summary does not.
if [ -z "${PREWARM_ALLOW_RUNNING:-}" ] && [ -z "${PREWARM_DRY_RUN:-}" ] \
  && pgrep -f 'nightly-smoke/nightly\.sh (loop|fix|summary)' > /dev/null; then
  echo "prewarm: a nightly sweep/fix round is running; refusing (PREWARM_ALLOW_RUNNING=1 to override)" >&2
  exit 1
fi

targets=()
slots="${NIGHTLY_SLOTS:-2}"
if [ "$slots" -le 1 ]; then
  targets+=("${CARGO_TARGET_DIR:-$REPO/target}")
else
  for ((k = 1; k <= slots; k++)); do targets+=("$REPORT_ROOT/target-slot-$k"); done
fi
[ "$want_fixer" = 1 ] && targets+=("$REPORT_ROOT/target-fixer")

status=0
for target in "${targets[@]}"; do
  if [ "$target" = "$REPORT_ROOT/target-fixer" ]; then
    cmd=("${PREWARM_CARGO:-cargo}" test --locked -j "$JOBS" -p ti4-server -p ti4-engine --no-run)
  else
    cmd=("${PREWARM_CARGO:-cargo}" build --locked -j "$JOBS" -p ti4-server --bin server)
  fi
  echo "prewarm: CARGO_TARGET_DIR=$target (cd $REPO && nice -n $NICE ${cmd[*]})"
  [ -z "${PREWARM_DRY_RUN:-}" ] || continue
  mkdir -p "$target" || { status=1; continue; }
  start=$(date +%s)
  if (cd "$REPO" && CARGO_TARGET_DIR="$target" nice -n "$NICE" "${cmd[@]}"); then
    echo "prewarm: $target done in $(($(date +%s) - start))s"
  else
    echo "prewarm: $target FAILED" >&2
    status=1
  fi
done
exit "$status"
