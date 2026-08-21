# M13 — Workload qualification and cutover

## Goal

Move the incumbent Python workload only after an isolated Rust release candidate satisfies the
accepted Rust rules/contracts and can be rolled back safely. Behavioral parity is not required;
Python remains the operational rollback for this first cutover.

## Work packages

| ID | Package | Depends | Deliverable and acceptance test |
|---|---|---|---|
| M13-001 | Release manifest | M12 | Pin source/content/schema/RNG/toolchain/dependencies and hashes for every shipped binary/artifact. |
| M13-002 | Legacy artifact conversion rehearsal | 001 | Convert copied profiles, checkpoints, pools, baselines, corpora, and replay inputs; originals remain unchanged. |
| M13-003 | Representative workload definition | M12 | Freeze commands, inputs, duration, expected outputs, resource limits, and success thresholds. |
| M13-004 | Offline incumbent control run | 003 | Execute the frozen workload on Python and preserve operational error/throughput/resource metrics; its decisions are not a behavioral oracle. |
| M13-005 | Offline Rust candidate run | 002–004 | Execute the same workload inputs; accepted Rust semantic, integrity, resource and performance thresholds pass without requiring Python output equality. |
| M13-006 | Rust extended soak | 005 | Run agreed production-duration multiple with no crash, corruption, leak, or performance decay. |
| M13-007 | Operational packaging | 001 | Windows binaries/configs, checksums, install/uninstall, log locations, and least-privilege defaults. |
| M13-008 | Backup procedure | 002,007 | Copy and verify every mutable workload artifact before replacement. |
| M13-009 | Rollback procedure | 004,007,008 | Restore the incumbent Python executable/config/artifacts and successfully resume a copied workload. |
| M13-010 | Failure drills | 007–009 | Interrupted checkpoint, corrupt input, bridge refusal, worker crash, disk-full simulation, and version mismatch. |
| M13-011 | Known-difference register | 005,006 | Every accepted difference includes impact, rationale, detection, and rollback relevance. |
| M13-012 | Release candidate freeze | 001–011 | Tag candidate; rerun all required CI and qualification checks without code changes. |
| M13-013 | Dual frontier go/no-go | 012 | Two independent reviews confirm evidence, rollback, and absence of unresolved critical findings. |
| M13-014 | Workload switch | 013 | Apply documented switch, verify health/result hashes/throughput, retain Python operational rollback. |
| M13-015 | Post-cutover observation | 014 | Run bounded observation window; compare errors, throughput, memory, artifacts; rollback on threshold breach. |
| M13-016 | Migration closure | 015 | Archive evidence, document final version and measured speedup, keep the historical-reference retention policy. |

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
