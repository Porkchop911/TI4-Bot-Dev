# M12 — Differential, security, and performance qualification

## Goal

Prove the assembled Rust system is behaviorally trustworthy, secure enough for local system integrity,
and materially faster on the intended Windows workload.

## Work packages

| ID | Package | Depends | Deliverable and acceptance test |
|---|---|---|---|
| M12-001 | Scope-ledger closure | M02–M11 | Every M00 file/interface/test/artifact row links to passing evidence or approved exception. |
| M12-002 | Unit/golden audit | 001 | No orphaned test families; canonical fixture versions and regeneration documented. |
| M12-003 | Full differential harness | M02–M11 | Python and Rust compare state/choices/events/errors per decision with reproducible counterexamples. |
| M12-004 | Tactical differential campaign | 003 | At least 10,000 generated scenarios; zero unexplained mismatches. |
| M12-005 | Full-game differential panel | 003 | Representative seeds/rotations/policies/horizons; all mismatches classified and resolved. |
| M12-006 | Native determinism campaign | M02–M11 | Repeated same-version runs produce identical choice/event/final hashes across worker counts where promised. |
| M12-007 | Complete-game soak | M10 | At least 1,000 games without panic, hang, invariant failure, or hidden dropped failure. |
| M12-008 | Coverage gate | all | At least 85% workspace and 95% critical module coverage, with justified generated-code exclusions. |
| M12-009 | Mutation gate: core | M03,M05,M06 | At least 90% killed in legality, timing, payment, replay, and bridge parser targets. |
| M12-010 | Mutation gate: remaining | all | At least 80% killed elsewhere; surviving meaningful mutants become tests. |
| M12-011 | Parser fuzz campaign | M02,M09–M11 | Content, artifacts, JSON.GZ, Parquet metadata, hex, HTTP, and command parsers have no crash. |
| M12-012 | Transition fuzz campaign | M03–M07 | Generated legal sequences preserve invariants and terminate; minimized corpus retained. |
| M12-013 | Dependency/security audit | all | No unresolved high/critical advisory; licenses/provenance acceptable; secrets/path review clean. |
| M12-014 | Unsafe and concurrency audit | all | No unreviewed unsafe, data race, deadlock, unbounded channel, or uncontrolled thread/process growth. |
| M12-015 | Artifact robustness | M09,M10 | Corrupt/truncated/wrong-version artifacts fail before mutation; checkpoint recovery works. |
| M12-016 | Controlled benchmark rerun | all | Same M00 protocol, raw outputs and environment recorded, semantic gates attached. |
| M12-017 | Hotspot profiling | 016 | Profiles identify actual cost centers; optimization work is split into new bounded packages. |
| M12-018 | Optimization pass | 017 | Minimum 3x game/decision/fork and 2x training targets achieved without parity regression. |
| M12-019 | Memory/resource qualification | 007,016 | No memory regression; file descriptors/handles/threads bounded over soak. |
| M12-020 | Documentation/runbook audit | all | Build, test, convert, simulate, train, bridge, diagnose, and recover procedures executed fresh. |
| M12-021 | Frontier semantic review | 001–007 | Independent parity and known-difference approval. |
| M12-022 | Frontier security review | 011–015,019 | Independent threat-model and integrity approval. |
| M12-023 | Frontier performance review | 016–019 | Independent methodology, statistics, and claimed-speedup approval. |

## Exit gate

All mandatory numeric gates pass, no unexplained semantic difference remains, and independent
frontier reviews approve semantics, security, and performance.

