# M00 — Historical reference and baseline

## Goal

Freeze Python commit `37061c5` as a read-only historical scope, artifact, and performance reference.
Inventory the incumbent surfaces and preserve reproducible baselines without making Python behavior
the acceptance standard for the Rust redesign.

Rows below retain the names under which M00 was executed. Detailed `M00-NNN` package documents and
evidence may contain the former parity policy; that language is historical and does not override the
2026-08-21 compatibility decision in `MASTER_PLAN.md`. A later package compares behavior with Python
only when its current specification explicitly opts into that comparison.

## Work packages

| ID | Package | Depends | Deliverable and acceptance test |
|---|---|---|---|
| M00-001 | Environment record | — | Record Windows, CPU, memory, Python, dependencies, Git commit, and commands; values can be regenerated. |
| M00-002 | Tracked-file scope ledger | 001 | Map every tracked `engine`, `bridge`, `tools`, `tts`, content, config, and relevant doc file to a milestone or explicit exclusion; zero unclassified branch files. |
| M00-003 | Test ledger | 001 | Inventory all 2,097 collected tests by behavior family and map retained contracts to Rust acceptance tests or reviewed exclusions; collection count is machine-verified. |
| M00-004 | Interface inventory | 002 | Catalogue supported Python entry points, CLI tools, bridge endpoints, wire messages, and public construction APIs. |
| M00-005 | Artifact inventory | 002 | Catalogue JSON, JSON.GZ, Parquet, checkpoints, profiles, baselines, map pools, telemetry, decision logs, and TTS captures with schema/version evidence. |
| M00-006 | Compatibility classification | 004,005 | Mark every interface/artifact `exact`, `semantic`, `translated`, `intentional-change`, or `N/A`; frontier review finds no unlabeled surface. |
| M00-007 | Canonical projection spec | 004 | Specify normalized state, view, choice, event, outcome, and error projections for retained replay/tooling contracts; ordering and redaction are explicit. |
| M00-008 | Fixture selection | 003,007 | Select minimal historical fixtures covering setup, phases, tactical steps, payments, cards, factions, TE, policies, training, and bridge. |
| M00-009 | Historical exporter | 007,008 | New-repo tool invokes the old repo read-only and emits versioned NDJSON projections; repeated export is byte-identical. |
| M00-010 | Entropy/replay corpus | 008,009 | Capture explicit dice/deck/random decisions for legacy scenarios; 100 scenarios replay identically. |
| M00-011 | Historical suite baseline | 003 | Record the complete incumbent suite result without modifying the Python repository; the result describes the incumbent and is not a Rust acceptance gate. |
| M00-012 | Microbenchmark protocol | 001 | Warmup, repetitions, affinity policy, worker counts, output schema, and variance thresholds are fixed before measuring. |
| M00-013 | Python performance baseline | 012 | Measure game, decision, snapshot, tactical, rollout, training, memory, and artifact I/O; at least five interleaved repetitions and variance report. |
| M00-014 | Historical-reference integrity guard | 001 | Hash critical tracked Python source/content and fail an explicitly requested historical comparison if the pinned reference changes; ordinary Rust acceptance does not depend on Python worktree cleanliness or runtime availability. |
| M00-015 | Frontier scope review | 002–014 | Independent review confirms scope completeness, trustworthy measurements, and adequate fixtures. |

## Source anchors

`README.md`, `pyproject.toml`, `engine/choice.py`, `engine/state.py`, `engine/timing.py`,
`engine/game.py`, `engine/sim.py`, `engine/learned_policy.py`, `engine/policy_linear.py`,
`engine/ml/`, `bridge/`, `tools/train_stage1_policy_gradient.py`, and all `tests/`.

## Exit gate

All ledgers are closed, the pinned tracked Python revision and worktree status are recorded without
mutation, applicable historical fixtures are reproducible, the incumbent suite status is known,
benchmark variance is acceptable or explained, and the frontier review is resolved. No M00 result
turns Python decisions into the Rust behavioral oracle.
