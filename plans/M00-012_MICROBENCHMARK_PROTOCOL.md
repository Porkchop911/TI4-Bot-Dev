# M00-012 — Microbenchmark protocol

## Package details
- **ID:** M00-012
- **Title:** Microbenchmark protocol
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-012
- **Dependencies:** M00-001 (Environment record) ✅

## Objective
Fix warmup, repetitions, affinity policy, worker counts, output schema, and variance thresholds before measuring performance gates.

## Work packages

### M00-012a — Warmup protocol
- **Objective:** Define warmup procedure (iterations, threshold).
- **Dependency:** M00-001
- **Evidence output:** `plans/evidence/M00-012a.md`
- **Permissions:** P1 (write evidence)

### M00-012b — Repetition protocol
- **Objective:** Define repetition count, interleaving, and variance thresholds.
- **Dependency:** M00-001
- **Evidence output:** `plans/evidence/M00-012b.md`
- **Permissions:** P1 (write evidence)

### M00-012c — Affinity and worker policy
- **Objective:** Define CPU affinity, worker counts, and threading policy.
- **Dependency:** M00-001
- **Evidence output:** `plans/evidence/M00-012c.md`
- **Permissions:** P1 (write evidence)

### M00-012d — Output schema
- **Objective:** Define benchmark output schema (JSON format).
- **Dependency:** M00-001
- **Evidence output:** `plans/evidence/M00-012d.md`
- **Permissions:** P1 (write evidence)

### M00-012e — Variance thresholds
- **Objective:** Define acceptable variance thresholds and rejection criteria.
- **Dependency:** M00-012b
- **Evidence output:** `plans/evidence/M00-012e.md`
- **Permissions:** P1 (write evidence)

## Compatibility invariants
- All benchmarks run on the same Windows host as the oracle.
- Warmup must precede measurement.
- Variance thresholds must be fixed before measuring.

## DoD
- Warmup, repetitions, affinity, output schema, and variance thresholds all fixed.
