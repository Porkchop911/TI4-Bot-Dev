# M00-007 — Canonical projection spec

## Package details
- **ID:** M00-007
- **Title:** Canonical projection spec
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-007
- **Dependencies:** M00-004 (Interface inventory) ✅

## Objective
Specify normalized state, view, choice, event, outcome, and error projections with explicit ordering and redaction rules. This defines the canonical format that the Oracle exporter (M00-009) will emit.

## Work packages

### M00-007a — State projection spec
- **Objective:** Define canonical state projection (board, fleet, resources, tech, cards, phase, turn).
- **Dependency:** M00-004
- **Evidence output:** `plans/evidence/M00-007a.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-007b — View projection spec
- **Objective:** Define per-player view projection (hidden information, redaction rules).
- **Dependency:** M00-004
- **Evidence output:** `plans/evidence/M00-007b.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-007c — Choice projection spec
- **Objective:** Define canonical choice projection (legal actions, option IDs, stable IDs).
- **Dependency:** M00-004
- **Evidence output:** `plans/evidence/M00-007c.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-007d — Event projection spec
- **Objective:** Define canonical event projection (game events, ordering, causality).
- **Dependency:** M00-004
- **Evidence output:** `plans/evidence/M00-007d.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-007e — Outcome projection spec
- **Objective:** Define canonical outcome projection (scoring, victory conditions, game end).
- **Dependency:** M00-004
- **Evidence output:** `plans/evidence/M00-007e.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-007f — Error projection spec
- **Objective:** Define canonical error projection (error types, codes, handling).
- **Dependency:** M00-004
- **Evidence output:** `plans/evidence/M00-007f.md`
- **Permissions:** P0 (read oracle), P1 (write evidence)

### M00-007g — Ordering and redaction rules
- **Objective:** Define explicit ordering rules and redaction rules for all projections.
- **Dependency:** M00-007a through M00-007f
- **Evidence output:** `plans/evidence/M00-007g.md`
- **Permissions:** P1 (write evidence)

### M00-007h — Projection schema (NDJSON format)
- **Objective:** Define the NDJSON schema for the Oracle exporter.
- **Dependency:** M00-007a through M00-007g
- **Evidence output:** `plans/evidence/M00-007h.md`
- **Permissions:** P1 (write evidence)

## Compatibility invariants
- Every projection must be deterministic given the same state.
- Hidden information must be explicitly redacted in per-player views.
- Option IDs must be stable across projections.
- Event ordering must be causal and reproducible.

## DoD
- All six projection types specified with schema.
- Ordering and redaction rules documented.
- NDJSON schema defined for Oracle exporter.
