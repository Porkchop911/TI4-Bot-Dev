# M00-010 — Entropy/replay corpus

## Package details
- **ID:** M00-010
- **Title:** Entropy/replay corpus
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-010
- **Dependencies:** M00-008 (Fixture selection), M00-009 (Oracle exporter) ✅

## Objective
Capture explicit dice/deck/random decisions for legacy scenarios. 100 scenarios must replay identically.

## Work packages

### M00-010a — Entropy capture design
- **Objective:** Design the entropy capture mechanism (dice, deck, random).
- **Dependency:** M00-008, M00-009
- **Evidence output:** `plans/evidence/M00-010a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-010b — Dice entropy capture
- **Objective:** Capture dice roll entropy for legacy scenarios.
- **Dependency:** M00-010a
- **Evidence output:** `plans/evidence/M00-010b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-010c — Deck entropy capture
- **Objective:** Capture deck shuffle entropy for legacy scenarios.
- **Dependency:** M00-010a
- **Evidence output:** `plans/evidence/M00-010c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-010d — Random decision capture
- **Objective:** Capture random decision entropy (ability resolution, event ordering).
- **Dependency:** M00-010a
- **Evidence output:** `plans/evidence/M00-010d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-010e — Replay verification
- **Objective:** Verify 100 scenarios replay identically with captured entropy.
- **Dependency:** M00-010b through M00-010d
- **Evidence output:** `plans/evidence/M00-010e.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

## Compatibility invariants
- Every random decision must be captured with its seed and source.
- Repeated replay with captured entropy must produce byte-identical output.
- 100 scenarios must be verified.

## DoD
- Entropy capture mechanism designed.
- Dice, deck, and random decision entropy captured.
- 100 scenarios verified to replay identically.
