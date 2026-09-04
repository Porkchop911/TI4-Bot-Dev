# OBS-003e slice 2 — turn, token, and scoring context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`, `OBS-003e` slice 1 (strategy-card producers, `plans/OBS-003E1_STRATEGY_CARD_CONTEXT.md`).
- Objective: close the remainder of `OBS-003e`'s row — the turn (`technology.rs`'s own reactive
  asks), token (`tokens.rs`), and scoring (`objectives.rs`, `draft.rs`, `vote.rs`) producers not
  covered by slice 1 or by the production-discount fix's single `technology::production_used`
  addition.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003e`), LRR 61.6 (scoring),
  8.10/8.11/8.16 (agenda voting), 52.4 (command tokens), 26 (strategy phase draft), and each
  technology's own printed text read from the embedded content store.
- Acceptance references: focused `obs003e` tests across `technology.rs`, `game.rs`, `objectives.rs`,
  `draft.rs`, and `vote.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/technology.rs`, `crates/ti4-engine/src/game.rs`,
  `crates/ti4-engine/src/objectives.rs`, `crates/ti4-engine/src/draft.rs`,
  `crates/ti4-engine/src/vote.rs`, `crates/ti4-engine/src/choice.rs` (a shared test-only
  `Capturing` decider), `crates/ti4-engine/tests/decision_delivery_inventory.rs` (one baseline count
  the new decider's own `choose` implementation moves), this specification, package evidence, and
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

- `technology.rs`'s five remaining reactive asks (Psychoarchaeology, Transit Diodes, Chaos Mapping,
  Predictive Intelligence, Bio-Stims) attach typed context (`Content(alias)`, one subtype each).
- `tokens.rs::TokenGain::pending_choice` has no board position to read `phase`/`round` from, so its
  context is attached at `game.rs::legal_options`'s dispatch site — the one real caller — naming
  Rule 52.4 and subtype `gain_command_token`, the same subtype `strategy_cards::gain_tokens` (slice
  1) already uses for the same rule reached through Leadership instead.
- `objectives.rs::ScoringWindow::pending_choice` attaches Rule 61.6 with subtype `score_objective`
  for the status-phase ask (which mixes public and secret candidates in one list, so "public" would
  misdescribe it) and `score_secret_objective` for the event-scoped, secret-only path.
- `draft.rs::strategy_options` attaches Rule 26, subtype `draft_strategy_card`.
- `vote.rs::pending_choice`'s three branches attach distinct subtypes: `cast_vote`,
  `vote_exhaust_planet`, `vote_tiebreak`.
- A new shared `choice::Capturing` decider records every `Choice` (context included) it is asked
  while delegating the answer to an inner decider, replacing the ad hoc capturing structs written
  locally in earlier `OBS-003` packages for this package's own new tests.

## Boundaries

- No option ID, label, legal set, or application-side behavior change anywhere. Every existing test
  in the five touched files passed unmodified.
- No features.rs change, matching every other `OBS-003` slice: this is the typed-context
  foundation, read into policy features by `OBS-008d` later.
- Earlier packages' own local capturing deciders (`invasion.rs`, `strategy_cards.rs`) are not
  retrofitted to use the new shared one — that would be unrelated cleanup riding on this package.

## Tests and commands

- One `obs003e_technology_reactive_asks_carry_typed_context` test covering Chaos Mapping and
  Bio-Stims directly; Psychoarchaeology and Transit Diodes share the same construction shape and
  are covered by the file's existing suite passing unmodified.
- One `obs003e_status_phase_token_gain_carries_its_typed_context` test constructing the internal
  `Game` state directly (the window itself has no position to build a real driven fixture from).
- One `obs003e_status_scoring_carries_its_typed_context` test.
- One `obs003e_strategy_draft_carries_its_typed_context` test.
- One `obs003e_vote_and_tiebreak_are_typed_distinctly` test covering two of the three vote
  subtypes; the third (`vote_exhaust_planet`) shares the branch's construction shape.
- Full engine suite, policy suite plus the 102-game deterministic campaign, training suite, strict
  Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

`OBS-003e`'s row is complete: every production/payment (slice 0, from `OBS-008c`), strategy-card
(slice 1), turn/token/scoring (this slice) producer states a stable, machine-readable subtype and
source; legal sets, option IDs, and replay/serde behavior are unchanged; all checks and independent
Tier-C review pass; only package files are committed.
