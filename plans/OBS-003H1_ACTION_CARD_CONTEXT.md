# OBS-003h slice 1 — action-card context producers

## Package

- Milestone: Stage 2 complete decision contract; typed-context foundation.
- Dependencies: `OBS-003b–c`.
- Objective: populate typed `DecisionContext` for `action_cards.rs`'s registered producers. The
  plan's own row (`OBS-003h`) names reactions, cards, abilities, exploration, relics, leaders, and
  breakthroughs together and explicitly allows splitting by crate/file if it exceeds the atomic
  size limit — with 12 producers across one already-large file, `action_cards.rs` is split out as
  its own slice; the remaining files (`faction_abilities.rs`, `reactions.rs`, `relics.rs`,
  `exploration.rs`, `thunders_edge.rs`, `secrets.rs`, `laws.rs`) follow in a second slice.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-003h`), LRR 2 (action
  cards), 2.4 (hand limit), and each card's own printed text read from the embedded content store.
- Acceptance references: focused `obs003h` test in `action_cards.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/action_cards.rs`, this specification, package evidence,
  and `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Behavior

All 12 registered `action_cards.rs` producers (13 `Choice` sites; `reparations` has two) attach
typed context:

| producer | subtype(s) | source |
|---|---|---|
| `enforce_hand_limit` | `discard_over_hand_limit` | Rule 2.4 |
| `confusing` | `confusing_legal_text_elect` | ActionCard("confusing") |
| `public_disgrace` | `public_disgrace_choose_card` | ActionCard("public_disgrace") |
| `reparations` | `reparations_exhaust`, `reparations_ready` | ActionCard("reparations") |
| `choose_crashlanding_ground` | `crashlanding_choose_ground` | ActionCard("crashlanding") |
| `choose_crashlanding_planet` | `crashlanding_choose_planet` | ActionCard("crashlanding") |
| `in_the_silence_of_space` | `silence_choose_system` | ActionCard("in_the_silence_of_space") |
| `skilled_retreat` | `skilled_retreat_choose_system` | ActionCard("skilled_retreat") |
| `predicted_outcome` | `predict_agenda_outcome` | Rule 8 (shared by every rider) |
| `ghost_squad` | `ghost_squad_move` | ActionCard("ghost_squad") |
| `exchange_program` | `exchange_program_answer` | ActionCard("exchange_program") |
| `pick` | `pick_{kind}` | Rule 2 (generic infrastructure, ~25 call sites) |

## Boundaries

- No option ID, label, legal set, or application-side behavior change. The existing suite in
  `action_cards.rs` passed unmodified.
- **`pick` and `predicted_outcome` are genuinely card-agnostic helpers**, not one card's own
  producer. `pick` alone has roughly 25 real call sites across the file; threading a card alias
  into every one is out of this package's scope and is recorded as a residual rather than silently
  attempted. Their context uses the caller-supplied `kind`/generic Rule citation instead of a card
  identity.
- No features.rs change, matching every other `OBS-003` package: `OBS-008g` reads this into policy
  features later.

## Tests and commands

- One `obs003h_action_card_effects_carry_typed_context` test covering four representative subtypes
  (Confusing Legal Text, both Reparations asks, Exchange Program's answer) through real card
  resolution via a new `resolve_card_capturing` test helper (extending the file's existing
  `resolve_card`/`resolve_card_on` pattern with the shared `choice::Capturing` decider from
  `OBS-003e` slice 2).
- Full engine suite, policy suite plus the 102-game deterministic campaign, training suite, strict
  Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

All 12 `action_cards.rs` producers state a stable, machine-readable subtype and source; legal sets,
option IDs, and replay/serde behavior are unchanged; all checks and independent Tier-C review pass;
only package files are committed.
