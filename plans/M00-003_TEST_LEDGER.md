# M00-003 — Test ledger

## ID and title
M00-003 — Test ledger

## Milestone and dependencies
- Milestone: M00 — Oracle and baseline
- Dependencies: M00-001 (Environment record) ✅

## One-sentence objective
Map all 2,097 collected Python tests to behavior families and future Rust test targets so that every Python test has a corresponding Rust test or reviewed exception.

## Exact Python source references
- Read-only: `D:\Projects\ti4-engine\tests\` — all test files via `find tests/ -name "test_*.py"`

## Exact Python test references
- Oracle test count (machine-verified): **2,097** collected by pytest in 1.24s

## Allowed Rust edit paths
- `plans/M00-003_TEST_LEDGER.md` (this spec file)
- `plans/evidence/M00-003.md` (evidence file)
- `plans/EXECUTION_STATE.md` (execution state update)

## Permission class and scoped access declaration
Permission class: **P1** — Allowed inside the Rust repository.
- Writable paths: `D:\Projects\ti4-engine-rs\plans\`, `D:\Projects\ti4-engine-rs\plans\evidence\`
- Read-only external paths: `D:\Projects\ti4-engine\tests\` (oracle, must remain clean)
- Network access: none required
- Processes/ports: none (pytest runs locally)
- Expected generated artifacts: test ledger markdown (~10–20 KB), evidence file (~5 KB)
- Destructive actions: none
- External-state changes: none

## Inputs and outputs
**Inputs:**
- All 78 test files in `tests/` directory
- Machine-verified count of 2,097 collected tests
- Behavior family taxonomy (game mechanics, rules, bots, ML, bridge, etc.)

**Outputs:**
- Test ledger mapping each test file to behavior families and Rust milestone targets
- Per-file test counts for traceability
- Summary statistics by behavior family and milestone

## Invariants and compatibility class
- **Invariant:** Every collected test must be mapped; zero unclassified tests allowed.
- **Compatibility class:** exact — the mapping is deterministic from test file names and contents to behavior families.

## Explicit non-goals
- Not writing Rust tests (that comes in later milestones).
- Not running the full 2,097-test suite (that is M00-011).
- Not modifying any oracle files.

## Tests to add
No code tests. Verification:
1. `pytest --collect-only` count matches ledger total (2,097).
2. Every test file has at least one behavior family assigned.
3. Summary counts are internally consistent (file totals = grand total).

## Commands to run
```bash
cd D:\Projects\ti4-engine && python -m pytest --collect-only tests/ 2>&1 | grep "tests collected"
# Then classify each test file by examining its name and content patterns
```

## Expected evidence
`plans/evidence/M00-003.md` containing:
- Machine-verified test count (2,097)
- Classification of all 78 test files into behavior families
- Per-file test counts
- Summary by behavior family and milestone target
- Confirmation of zero unclassified tests

## Known traps
- Some test files may cover multiple behavior families — list all relevant ones.
- Fixture-only tests (tests/fixtures/) are not pytest-collected but should be noted separately.
- ML/training tests may have flaky or environment-dependent assertions.
- Bridge/TTS tests may require specific Windows dependencies (pywin32, pywinauto).

## Definition of done
- [x] Machine-verified test count confirmed at 2,097 ✅
- [x] All **106** test files classified into behavior families (CORRECTED: was 78 + 3 missing) ✅
- [x] Each file mapped to Rust milestone target(s) ✅
- [x] Per-file and summary counts are internally consistent (sums to 2,097) ✅
- [x] Evidence file written with full ledger and statistics (CORRECTED version) ✅
- [ ] `plans/EXECUTION_STATE.md` updated to reflect completion and next package
> **NOTE:** Package REOPENED after audit. Original was missing three test modules (`test_transactions.py`: 39 tests, `test_tactical_plans.py`: 8 tests, `test_promotion_confirmation.py`: 7 tests = 54 total). Corrected evidence written but requires independent review before marking complete.
