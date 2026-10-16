#!/usr/bin/env bash
# Checks the host-resource helpers of config.sh (snapshot line, /dev/shm gate, stale /tmp clean-up)
# in a temp dir; touches nothing outside it.
#
#   tools/nightly-smoke/test_resources.sh
set -uo pipefail
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

mkdir -p "$TMP/tmp" "$TMP/shm"
export NIGHTLY_REPO=/nonexistent NIGHTLY_TMP_ROOT="$TMP/tmp" NIGHTLY_SHM_ROOT="$TMP/shm"
# shellcheck source=config.sh
source "$HERE/config.sh"

echo "== snapshot"
line=$(resource_snapshot)
check_true "snapshot has memory, swap, load, tmp, shm and process counts" \
  bash -c "echo '$line' | grep -Eq 'mem_avail=[0-9]+MB/[0-9]+MB shmem=[0-9]+MB swap_used=[0-9]+MB/[0-9]+MB load=.* tmp_used=[0-9]+MB/[0-9]+MB tmp_free=[0-9]+MB shm_used=[0-9]+MB/[0-9]+MB browsers=[0-9]+ servers=[0-9]+'"
check "snapshot is one line" "$(echo "$line" | wc -l)" 1

echo "== /dev/shm gate"
SHM_MIN_MB=1
check_true "a shm dir with room passes" dev_shm_ok
SHM_MIN_MB=999999999
if dev_shm_ok; then checks=$((checks + 1)); echo "FAIL  too small shm dir should not pass"; failures=$((failures + 1)); else checks=$((checks + 1)); echo "ok    too small shm dir does not pass"; fi
SHM_ROOT="$TMP/missing"
if dev_shm_ok; then checks=$((checks + 1)); echo "FAIL  a missing shm dir should not pass"; failures=$((failures + 1)); else checks=$((checks + 1)); echo "ok    a missing shm dir does not pass"; fi

echo "== stale /tmp clean-up"
STALE_TMP_MINUTES=90
mkdir -p "$TMP/tmp/ti4-playwright-games-111" "$TMP/tmp/ti4-playwright-games-222" "$TMP/tmp/ti4-playwright-games-333" \
  "$TMP/tmp/playwright_chromiumdev_profile-old" "$TMP/tmp/other-dir"
echo x > "$TMP/tmp/ti4-playwright-games-111/f"
touch -d '3 hours ago' "$TMP/tmp/ti4-playwright-games-111" "$TMP/tmp/ti4-playwright-games-222" \
  "$TMP/tmp/ti4-playwright-games-333" "$TMP/tmp/playwright_chromiumdev_profile-old" "$TMP/tmp/other-dir"
# A live process that carries the path of 333 in its environment keeps that dir.
TI4_DATA_DIR="$TMP/tmp/ti4-playwright-games-333" sleep 60 &
sleep 0.3
mkdir -p "$TMP/tmp/ti4-playwright-games-444" # fresh
clean_stale_tmp > "$TMP/clean.out"
check "old game dir removed" "$([ -e "$TMP/tmp/ti4-playwright-games-111" ] && echo kept || echo gone)" gone
check "old empty game dir removed" "$([ -e "$TMP/tmp/ti4-playwright-games-222" ] && echo kept || echo gone)" gone
check "old browser profile removed" "$([ -e "$TMP/tmp/playwright_chromiumdev_profile-old" ] && echo kept || echo gone)" gone
check "dir used by a live process kept" "$([ -e "$TMP/tmp/ti4-playwright-games-333" ] && echo kept || echo gone)" kept
check "fresh dir kept" "$([ -e "$TMP/tmp/ti4-playwright-games-444" ] && echo kept || echo gone)" kept
check "unrelated dir kept" "$([ -e "$TMP/tmp/other-dir" ] && echo kept || echo gone)" kept
check "clean-up reports what it removed" "$(grep -c '^removed stale' "$TMP/clean.out")" 3

echo "== tmp_top"
head -c 2000000 /dev/zero > "$TMP/tmp/big"
check "tmp_top lists the biggest entry first" "$(tmp_top 3 | head -1 | awk '{ print $2 }')" "$TMP/tmp/big"

echo
echo "$checks checks, $failures failures"
[ "$failures" -eq 0 ]
