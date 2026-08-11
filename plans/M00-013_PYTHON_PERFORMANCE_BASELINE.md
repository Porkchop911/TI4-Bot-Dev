# M00-013 — Python performance baseline

## Package details
- **ID:** M00-013
- **Title:** Python performance baseline
- **Milestone:** M00 — Oracle and baseline
- **Package:** M00-013
- **Dependencies:** M00-012 (Microbenchmark protocol) ✅

## Objective
Measure game, decision, snapshot, tactical, rollout, training, memory, and artifact I/O performance. At least five interleaved repetitions and variance report.

## Work packages

### M00-013a — Game performance measurement
- **Objective:** Measure single-core four-round game and fixed-worker throughput.
- **Dependency:** M00-012
- **Evidence output:** `plans/evidence/M00-013a.md`
- **Permissions:** P1 (write evidence)

### M00-013b — Decision performance measurement
- **Objective:** Measure decision latency (scoring cost, tactical action).
- **Dependency:** M00-012
- **Evidence output:** `plans/evidence/M00-013b.md`
- **Permissions:** P1 (write evidence)

### M00-013c — Snapshot/fork performance measurement
- **Objective:** Measure snapshot/fork latency.
- **Dependency:** M00-012
- **Evidence output:** `plans/evidence/M00-013c.md`
- **Permissions:** P1 (write evidence)

### M00-013d — Training throughput measurement
- **Objective:** Measure stage 2 training throughput.
- **Dependency:** M00-012
- **Evidence output:** `plans/evidence/M00-013d.md`
- **Permissions:** P1 (write evidence)

### M00-013e — Memory measurement
- **Objective:** Measure peak memory per worker.
- **Dependency:** M00-012
- **Evidence output:** `plans/evidence/M00-013e.md`
- **Permissions:** P1 (write evidence)

### M00-013f — Artifact I/O measurement
- **Objective:** Measure content loading, map pool loading, checkpoint I/O.
- **Dependency:** M00-012
- **Evidence output:** `plans/evidence/M00-013f.md`
- **Permissions:** P1 (write evidence)

### M00-013g — Baseline consolidation
- **Objective:** Consolidate all measurements into the baseline report.
- **Dependency:** M00-013a through M00-013f
- **Evidence output:** `plans/evidence/M00-013g.md`
- **Permissions:** P1 (write evidence)

## Compatibility invariants
- All measurements on the same Windows host as the oracle.
- At least five interleaved repetitions per metric.
- Variance reported for every measurement.

## DoD
- All performance metrics measured with variance reported.
