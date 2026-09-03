# OBS-003a — typed choice-context schema

## Package

- Milestone: Stage 2 complete decision contract, after `OBS-002b`.
- Objective: define a versioned `DecisionContext` and `OutstandingConstraint`, their field
  visibility, and a canonical serialization, without migrating producers.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` OBS-003a;
  `plans/evidence/OBS-002B_RULE_DEPENDENCY_MATRIX.md`;
  `plans/evidence/OBS-002B_ALIASING_CENSUS.md`.
- Acceptance reference: the unit tests in `crates/ti4-engine/src/decision_context.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/decision_context.rs`, the module registration in
  `crates/ti4-engine/src/lib.rs`, this specification, and package evidence.
- Network, external state, destructive actions: none.

## Invariants and non-goals

- **No producer is migrated.** No `Choice` gains a context, no legal option set changes, no option id
  changes, and replay is untouched. Those are OBS-003b (hash), OBS-003c (delivery) and OBS-003d–h
  (producers).
- The type is additive and unused on landing. That is deliberate: a schema argued over while
  eighty producers are being edited is a schema nobody can review.
- `subtype` is a machine identifier chosen by the producer. It is never derived from prompt text,
  because rewording a prompt must not reclassify a decision.
- Visibility is data, not prose: `DecisionContext::visibility()` states it and a test asserts it.
- Canonical serialization is order-independent, because it feeds the OBS-003b fingerprint and a
  fingerprint that depends on a producer's push order is not one.

## What the matrix required of it

- A typed source and subtype, because `other` conflates scoring, agenda riders, exploration,
  transit, faction abilities and card effects, and the prompt reaches the option-invariant
  observation in only 1,753 of 3,678 decisions.
- Continuation state, because several decisions are one transaction over several prompts. A seat
  that exhausted a four-influence planet toward a three-influence token holds one influence of
  credit that exists nowhere in a between-prompts board snapshot; asking again without it charged
  seven influence for two tokens.
- An explicit `optional`, because declining an obligation and choosing between obligations are not
  the same decision and cannot be told apart from an option list.

## Tests and commands

- `cargo test -p ti4-engine --lib decision_context`
- `cargo test -p ti4-engine`
- `RUSTFLAGS=-D warnings cargo clippy -p ti4-engine --all-targets`
- `cargo fmt -p ti4-engine -- --check`

## Definition of done

The type exists and is versioned; overpayment is credit and never a negative debt; a non-actor seat
cannot read the outstanding quantities; the canonical form is independent of constraint push order
and leads with the version; two decisions the `other` catch-all currently conflates render
differently; the schema round-trips through serde; the visibility table is asserted rather than
described; checks pass; and no producer, option set or replay artifact changed.
