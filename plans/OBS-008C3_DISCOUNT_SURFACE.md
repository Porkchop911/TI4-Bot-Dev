# OBS-008c3 — production discount surface

## Package

- Milestone: Stage 2 complete decision contract; final slice of `OBS-008c`.
- Dependencies: `OBS-003a–c`, `OBS-004`, `OBS-007b`, `OBS-008c1`, `OBS-008c2a`, `OBS-008c2b`, and
  `plans/BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md` (the discount this package exposes did not
  exist to expose before that fix).
- Objective: let an actor see the resource-cost discount a build carries (Sarween Tools, AI
  Development Algorithm, Harrugh Gefhara) and, on AI Development Algorithm's own exhaust-or-not ask,
  what accepting it would grant.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008c`, which names "discount"
  explicitly), and the card text and engine behavior fixed and evidenced in
  `plans/evidence/BUG_2026-09-04_PRODUCTION_DISCOUNT.md`.
- Acceptance references: focused `obs008c3` tests in `features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-policy/src/features.rs`, this specification, package evidence, and
  `plans/EXECUTION_STATE.md`. (`crates/ti4-engine/src/technology.rs`'s typed context for the
  exhaust-or-not ask, and the `discount`/`discount_offered` payload facts themselves, were added as
  part of the discount fix and are read here, not re-added.)
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

1. Every build option's already-existing `discount` payload fact (the amount subtracted from
   `printed_cost` to reach `cost`) reaches the policy under the transferable `production` family,
   alongside the `cost`/`printed_cost` facts `OBS-008c1`/`c2a` already expose.
2. AI Development Algorithm's exhaust-or-not ask carries a typed `DecisionContext { source:
   Content("aida"), subtype: "exhaust_for_production_discount" }`, and its `discount_offered`
   payload fact (what accepting would grant) reaches the policy under the same family.
3. Two otherwise-identical build options differing only in discount produce different features and
   the difference survives MLP projection.

## Boundaries

- No change to production/payment legality, option IDs, labels, the legal set, or how the discount
  is computed or spent — that is `plans/BUG_2026-09-04_PRODUCTION_DISCOUNT_UNWIRED.md`.
- Harrugh Gefhara's own ask (whether to purge the leader) is not exposed here: it has no invocation
  path in real play at all (`plans/BUG_2026-09-04_LEADER_USE_UNREACHABLE.md`), so there is no live
  choice to attach a feature to. `free_this_use`'s effect on `cost`/`discount` is already covered by
  point 1 once that separate defect is fixed — no further change would be needed here.
- No vocabulary generation, prompt-free migration, or bundle republish.

## Tests and commands

- Paired build-option fixtures differing only in `discount` produce different
  `production:discount` features and survive MLP projection.
- A fixture for the AI Development Algorithm ask proves its typed subtype and
  `production:discount_offered` feature, and that they are absent from an unrelated production
  choice.
- Focused engine (none — already covered by the discount-fix package) and policy tests, affected
  crate suites, training suite, strict all-target Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

The model can see, for every offered build, the discount it carries alongside its cost, and for AI
Development Algorithm's own ask, what accepting it would grant — under the already-approved
`production` family, agreeing with the engine facts fixed and evidenced separately; all checks and
independent Tier-C review pass; only package files are committed.
