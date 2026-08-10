# M13 — Workload qualification and cutover

## Goal

Move workload only after an isolated release candidate proves it can replace Python and be rolled back safely.

## Work packages

| ID | Package | Depends | Deliverable and acceptance test |
|---|---|---|---|
| M13-001 | Release manifest | M12 | Pin source/content/schema/RNG/toolchain/dependencies and hashes for every shipped binary/artifact. |
| M13-002 | Legacy artifact conversion rehearsal | 001 | Convert copied profiles, checkpoints, pools, baselines, corpora, and replay inputs; originals remain unchanged. |
| M13-003 | Representative workload definition | M12 | Freeze commands, inputs, duration, expected outputs, resource limits, and success thresholds. |
| M13-004 | Offline Python control run | 003 | Execute frozen workload on Python oracle and preserve hashed results/metrics. |
| M13-005 | Offline Rust candidate run | 002–004 | Execute identical semantic workload; qualification comparison passes. |
| M13-006 | Rust extended soak | 005 | Run agreed production-duration multiple with no crash, corruption, leak, or performance decay. |
| M13-007 | Operational packaging | 001 | Windows binaries/configs, checksums, install/uninstall, log locations, and least-privilege defaults. |
| M13-008 | Backup procedure | 002,007 | Copy and verify every mutable workload artifact before replacement. |
| M13-009 | Rollback procedure | 004,007,008 | Restore Python executable/config/artifacts and successfully resume a copied workload. |
| M13-010 | Failure drills | 007–009 | Interrupted checkpoint, corrupt input, bridge refusal, worker crash, disk-full simulation, and version mismatch. |
| M13-011 | Known-difference register | 005,006 | Every accepted difference includes impact, rationale, detection, and rollback relevance. |
| M13-012 | Release candidate freeze | 001–011 | Tag candidate; rerun all required CI and qualification checks without code changes. |
| M13-013 | Dual frontier go/no-go | 012 | Two independent reviews confirm evidence, rollback, and absence of unresolved critical findings. |
| M13-014 | Workload switch | 013 | Apply documented switch, verify health/result hashes/throughput, retain Python rollback. |
| M13-015 | Post-cutover observation | 014 | Run bounded observation window; compare errors, throughput, memory, artifacts; rollback on threshold breach. |
| M13-016 | Migration closure | 015 | Archive evidence, document final version and measured speedup, keep oracle/reference retention policy. |

## Rollback triggers

- Any artifact corruption or failure to resume
- Nondeterministic result under a promised deterministic workload
- Unexplained legality/state mismatch
- Critical/high security finding affecting the running boundary
- Sustained throughput below the minimum gate
- Resource growth beyond qualified bounds

## Exit gate

The Rust workload completes the observation window within all semantic, integrity, performance, and
resource thresholds; rollback remains tested and available.

