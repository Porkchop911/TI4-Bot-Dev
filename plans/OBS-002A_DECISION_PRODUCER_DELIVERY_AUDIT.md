# OBS-002a — decision producer and delivery audit

## Package

- Milestone: Stage 2 complete decision contract, after `STAGE2-OBS-001`.
- Objective: inventory the engine's decision-producing modules and every direct delivery path,
  distinguish seat-bound asks from consequential viewless asks, and add a deterministic empirical
  census of non-forced decisions by delivery path, head, and option-kind set.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-002a;
  `ti4_engine::choice::{Decider,Table,SeatObservation}`; `ti4_policy::learned::decision_head`.
- Acceptance references: the source-inventory integration test and the bounded census example added
  by this package.

## Scope and permissions

- Permission class: P1.
- Writable paths:
  - `crates/ti4-engine/tests/decision_delivery_inventory.rs`
  - `crates/ti4-training/examples/decision_surface.rs`
  - this specification, package evidence, and `plans/EXECUTION_STATE.md`
- Read-only external paths: none.
- Network, ports, external state, and destructive actions: none.
- Processes: bounded Cargo test/lint and a small deterministic local census.
- Generated artifacts: Cargo target output only; census text is captured in evidence, not retained as
  a large artifact.

## Invariants and non-goals

- This is an audit package. It does not migrate viewless asks, alter legal options, add decision
  context, change replay, or change model features.
- Production-source scanning ends at each module's top-level `#[cfg(test)]`, so unit-test asks are not
  misclassified as live engine delivery.
- The source inventory pins all modules that construct choices or directly ask a decider. Any new or
  moved site fails visibly and requires classification.
- A direct `.ask(` in production code is classified as viewless even if the current authored bot can
  answer it. A returned `pending_choice` is not called seat-bound until its consuming driver is
  verified separately.
- The empirical census records only non-forced choices and labels whether the engine invoked
  `choose` or `choose_seeing`. It supplements static coverage; absence from the sample is not proof
  that a producer is unreachable.
- Current `Choice` has no typed source/subtype. Prompt samples are diagnostic only and are not treated
  as a stable subtype contract. This limitation is the input to OBS-003a.

## Tests and commands

- `cargo test -p ti4-engine --test decision_delivery_inventory -- --nocapture`
- `cargo run -p ti4-training --example decision_surface --release -- --games 4 --rounds 4`
- `cargo test -p ti4-engine`
- `cargo test -p ti4-training`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine -p ti4-training --all-targets`
- targeted `rustfmt --check` and `git diff --check`

## Definition of done

The checked source inventory and empirical census agree on the two delivery APIs, all 15 current
production viewless asks are named and classified as migration work (not exceptions), no production
module calls a decider directly outside `Table`, checks pass, evidence records the census and
limitations, an independent Tier-C reviewer approves the boundary, and the package is committed
without unrelated files.
