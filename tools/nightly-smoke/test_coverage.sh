#!/usr/bin/env bash
# Checks coverage.py on a tiny fake repo and fake reports.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail() { echo "FAIL: $*" >&2; exit 1; }

mkdir -p "$tmp/repo/crates/demo/src" "$tmp/reports/a" "$tmp/reports/b"
cat > "$tmp/repo/crates/demo/src/lib.rs" <<'EOF'
fn one() { DecisionContext::new(player.clone(), DecisionSource::Content("x".to_owned()), "seen_always", p, r); }
fn two() {
    DecisionContext::new(
        a,
        DecisionSource::Rule(format!("8.{}", 1)),
        "seen_once",
        state.phase, state.round)
}
fn three() { DecisionContext::new(a, b, "never_offered", c, d) }
fn four() { DecisionContext::new(a, b, subtype_var, c, d) }
EOF
echo '{"subtypes": {"seen_always": 3, "seen_once": 1, "not_in_source": 2}}' > "$tmp/reports/a/report.json"
echo '{"subtypes": {"seen_always": 1}}' > "$tmp/reports/b/report.json"
echo 'not json' > "$tmp/reports/b/other.json"

# worktrees a night leaves in the reports dir (parallel slots, fixers) hold whole checkouts: not runs
mkdir -p "$tmp/reports/slot-1/repo/e2e" "$tmp/reports/fixer-1/e2e" "$tmp/reports/target-slot-1"
for d in slot-1/repo/e2e fixer-1/e2e target-slot-1; do echo '{"subtypes": {"from_a_worktree": 9}}' > "$tmp/reports/$d/report.json"; done
out=$(python3 "$here/coverage.py" --repo "$tmp/repo" "$tmp/reports")
echo "$out" | grep -q "over 2 runs" || fail "should count two reports: $out"
echo "$out" | grep -q "3 literal engine subtypes; 1 never offered, 1 offered in fewer than 2 runs" || fail "counts: $out"
echo "$out" | grep -A2 "## Never offered" | grep -q '`never_offered`' || fail "never_offered should be listed: $out"
echo "$out" | grep -A2 "fewer than 2 runs$" | grep -q '`seen_once` (1)' || fail "seen_once should be rare: $out"
echo "$out" | grep -q 'subtype_var' && fail "a variable subtype is not a literal"
echo "$out" | grep -q "from_a_worktree" && fail "reports inside worktrees must be skipped: $out"
echo "ok: coverage"

# The agenda-phase counters behind run_game.sh's pick and the morning summary line.
mkdir -p "$tmp/night/runs/a/trace" "$tmp/night/runs/b/trace" "$tmp/night/runs/c/trace"
echo '{"subtypes": {"cast_vote": 4}}' > "$tmp/night/runs/a/trace/report.json"
echo '{"subtypes": {"vote_tiebreak": 1}}' > "$tmp/night/runs/b/trace/report.json"
echo '{"subtypes": {"play_card": 4, "politics_place_agenda": 1}}' > "$tmp/night/runs/c/trace/report.json"
[ "$(python3 "$here/coverage.py" --agenda-runs "$tmp/night")" = "2 3" ] || fail "agenda runs should be '2 3'"
[ "$(python3 "$here/coverage.py" --agenda-line "$tmp/night")" = "agenda phase: 2 of 3 runs voted" ] || fail "agenda line"
rm "$tmp/night/runs/a/trace/report.json" "$tmp/night/runs/b/trace/report.json"
python3 "$here/coverage.py" --agenda-line "$tmp/night" | grep -q "NO AGENDA PHASE HAPPENED TONIGHT: 0 of 1 runs" || fail "zero votes must be loud"
echo "ok: agenda coverage line"
