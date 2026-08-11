# M00-014 — Oracle integrity guard

## Package details
- **ID:** M00-014
- **Title:** Oracle integrity guard
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-014
- **Dependencies:** M00-001 (Environment record) ✅

## Objective
Hash critical Python source/content and fail migration tests if the oracle changes unexpectedly.

## Work packages

### M00-014a — Critical file selection
- **Objective:** Select files to hash (source, content, config).
- **Dependency:** M00-001
- **Evidence output:** `plans/evidence/M00-014a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-014b — Hash computation
- **Objective:** Compute SHA-256 hashes for all critical files.
- **Dependency:** M00-014a
- **Evidence output:** `plans/evidence/M00-014b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-014c — Integrity guard implementation
- **Objective:** Design the integrity guard tool/check.
- **Dependency:** M00-014b
- **Evidence output:** `plans/evidence/M00-014c.md`
- **Permissions:** P1 (write evidence)

### M00-014d — Migration test integration
- **Objective:** Integrate integrity guard into migration test pipeline.
- **Dependency:** M00-014c
- **Evidence output:** `plans/evidence/M00-014d.md`
- **Permissions:** P1 (write evidence)

## Compatibility invariants
- The oracle repository must remain clean (no modifications).
- Hashes must be committed to the Rust repo for comparison.
- Migration tests must fail if oracle hashes don't match.

## DoD
- Critical files hashed and committed.
- Integrity guard designed and documented.
- Migration test integration documented.
