#!/usr/bin/env bash
# Checks prewarm.sh with a stub `cargo` in a temp dir; touches nothing outside it.
#
#   tools/nightly-smoke/test_prewarm.sh
set -uo pipefail
HERE=$(cd "$(dirname "$0")" && pwd)
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

checks=0
failures=0
check() { # check <name> <actual> <expected>
  checks=$((checks + 1))
  if [ "$2" = "$3" ]; then echo "ok    $1"; else echo "FAIL  $1: expected [$3], got [$2]"; failures=$((failures + 1)); fi
}

mkdir -p "$TMP/repo" "$TMP/bin" "$TMP/reports"
cat > "$TMP/bin/cargo" << 'EOF'
#!/usr/bin/env bash
echo "$PWD|$CARGO_TARGET_DIR|$*" >> "$STUB_LOG"
[ -z "${STUB_FAIL:-}" ]
EOF
chmod +x "$TMP/bin/cargo"
export STUB_LOG="$TMP/cargo.log" PREWARM_CARGO="$TMP/bin/cargo" NIGHTLY_REPO="$TMP/repo" NIGHTLY_REPORT_ROOT="$TMP/reports"

echo "== two slots (the cron default)"
: > "$STUB_LOG"
NIGHTLY_SLOTS=2 "$HERE/prewarm.sh" > "$TMP/out.log" 2>&1
check "exit code" "$?" 0
check "one build per slot" "$(wc -l < "$STUB_LOG")" 2
check "slot 1 builds the server bin into target-slot-1 from the checkout" \
  "$(sed -n 1p "$STUB_LOG")" "$TMP/repo|$TMP/reports/target-slot-1|build --locked -j 3 -p ti4-server --bin server"
check "slot 2 builds into target-slot-2" \
  "$(sed -n 2p "$STUB_LOG" | cut -d'|' -f2)" "$TMP/reports/target-slot-2"
check "the target dirs were created" "$(ls "$TMP/reports" | tr '\n' ' ')" "target-slot-1 target-slot-2 "

echo "== one slot, fixer, jobs"
: > "$STUB_LOG"
NIGHTLY_SLOTS=1 PREWARM_JOBS=5 "$HERE/prewarm.sh" --fixer > "$TMP/out.log" 2>&1
check "one slot + fixer = two builds" "$(wc -l < "$STUB_LOG")" 2
check "the classic sweep builds into the checkout's target/" "$(sed -n 1p "$STUB_LOG" | cut -d'|' -f2)" "$TMP/repo/target"
check "the fixer target gets the test build with -j5" "$(sed -n 2p "$STUB_LOG" | cut -d'|' -f2-)" \
  "$TMP/reports/target-fixer|test --locked -j 5 -p ti4-server -p ti4-engine --no-run"

echo "== failure and dry run"
: > "$STUB_LOG"
STUB_FAIL=1 NIGHTLY_SLOTS=2 "$HERE/prewarm.sh" > "$TMP/out.log" 2>&1
check "a failing build fails the script" "$?" 1
check "but the other slot is still tried" "$(wc -l < "$STUB_LOG")" 2
: > "$STUB_LOG"
PREWARM_DRY_RUN=1 NIGHTLY_SLOTS=2 "$HERE/prewarm.sh" > "$TMP/out.log" 2>&1
check "dry run runs no cargo" "$(wc -l < "$STUB_LOG")" 0
check "dry run prints the commands" "$(grep -c '^prewarm: CARGO_TARGET_DIR=' "$TMP/out.log")" 2

echo "== $checks checks, $failures failures"
[ "$failures" = 0 ]
