# M00-011 — Correctness baseline

## Package details
- **ID:** M00-011
- **Title:** Correctness baseline
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-011
- **Dependencies:** M00-003 (Test ledger) ✅

## Objective
Run the complete suite without modifying tracked Python files. Failures and environment limitations are signed off.

## Work packages

### M00-011a — Suite execution
- **Objective:** Run the complete Python test suite and record results.
- **Dependency:** M00-003
- **Evidence output:** `plans/evidence/M00-011a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-011b — Failure analysis
- **Objective:** Analyze all failures and categorize them.
- **Dependency:** M00-011a
- **Evidence output:** `plans/evidence/M00-011b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-011c — Environment limitations
- **Objective:** Document environment limitations (Windows-specific, missing dependencies).
- **Dependency:** M00-011a
- **Evidence output:** `plans/evidence/M00-011c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-011d — Sign-off
- **Objective:** Sign off on the correctness baseline.
- **Dependency:** M00-011b, M00-011c
- **Evidence output:** `plans/evidence/M00-011d.md`
- **Permissions:** P1 (write evidence)

## Compatibility invariants
- No tracked Python files may be modified.
- All failures must be documented and categorized.
- Environment limitations must be documented.

## DoD
- Complete suite run without modifying tracked Python files.
- All failures analyzed and categorized.
- Environment limitations documented.
- Correctness baseline signed off.
