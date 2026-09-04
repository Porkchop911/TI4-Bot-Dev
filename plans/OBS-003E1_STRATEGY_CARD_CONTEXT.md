# OBS-003e slice 1 — strategy-card context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`.
- Objective: populate typed `DecisionContext` for the strategy-card producers named in
  `OBS-003e`'s row (`Populate typed context for production, payment, turn, strategy, technology,
  token, and scoring producers`). This slice is the `strategy_cards.rs` cluster — every card's
  primary/secondary ask; the production/payment producers `OBS-003e` also names were already typed
  by `OBS-008c1/c2a/c2b/c3`, and turn/token/scoring producers remain for a following slice.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003e`), LRR 52
  (command tokens), 34.2 (readying a planet), and each card's own printed text read from the
  embedded content store.
- Acceptance references: focused `obs003e` tests in `strategy_cards.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/strategy_cards.rs`, this specification, package evidence,
  and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

Fourteen producers, eighteen individual `Choice` construction sites, now attach typed context:
`gain_tokens` (Rule 52.4), `influence_purchase_choice` (Rule 52.3), `ready_planets` (Rule 34.2),
`offer_research`/`paid_research` (Technology primary/secondary, same subtype `research_technology`
distinguished by the `secondary` flag), `specialist_compounds` (two asks, Jol-Nar faction ability),
`doctor_sucaban` (two asks, Jol-Nar agent), `diplomacy_primary`, `politics_primary` (two asks:
speaker, agenda placement), `place_structure` (shared across every card offering the ability),
`trade_primary`, `warfare_primary` plus `redistribute_tokens`, `imperial_primary`, and `primary`'s
own two Thunder's Edge sites (`te6warfare`, `te4construction`).

## Boundaries

- No option ID, label, legal set, or application-side behavior change anywhere. Every existing test
  in the file passed unmodified.
- No features.rs change, matching `OBS-003d`'s own boundary: this is the typed-context foundation,
  read into policy features by `OBS-008d` later.
- Turn producers (the remainder of `technology.rs`'s own `start_turn`/`end_turn` reactive asks
  beyond the one `production_used` addition from the discount fix), token producers (`tokens.rs`),
  and scoring producers (`objectives.rs`, `draft.rs`, `vote.rs`) are explicitly deferred to a
  following slice, named here so the row is not silently claimed complete.

## Tests and commands

- One `obs003e_strategy_card_choices_carry_typed_context` test covering the generic token/planet
  mechanics and the Technology primary/secondary distinction (same subtype, different `secondary`
  flag).
- One `obs003e_politics_primary_types_its_two_choices_distinctly` test proving Politics' two asks
  (speaker, agenda placement) carry different subtypes though both arise from one primary.
- Full engine suite, policy suite plus the 102-game deterministic campaign, training suite, strict
  Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

Every strategy-card producer named above states a stable, machine-readable subtype and source;
legal sets, option IDs, and replay/serde behavior are unchanged; all checks and independent Tier-C
review pass; only package files are committed.
