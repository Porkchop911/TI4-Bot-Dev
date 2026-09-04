# OBS-008d2 — strategy/technology/scoring consequence surface

## Package

Second slice of `OBS-008d`, closing the "no preview" non-goal `OBS-008d1` recorded. Attaches an
exact preview to three of the eight subtypes `OBS-008d1` covered, each reusing an already-defined
`preview::Quantity` with zero new engine risk beyond attaching the preview itself.

- Dependencies: `OBS-008d1` (`451eded`, the `strategy` family and subtype guard).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008d` row), LRR 52.4
  (command tokens), 98 (scoring victory points).
- Writable paths: `crates/ti4-engine/src/strategy_cards.rs`, `crates/ti4-engine/src/objectives.rs`,
  `crates/ti4-policy/src/features.rs`, this spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **`gain_command_token`** (`gain_tokens`, LRR 52.4): each of the three pool options
   (tactic/fleet/strategic) previews its own pool rising by exactly one, read fresh every
   iteration of the surrounding loop (an earlier pick in the same multi-token ask already changed
   the count).
2. **`research_technology`** (`research_option`, shared by `offer_research` and `paid_research`):
   every research option previews the seat's technology count rising by exactly one, regardless of
   price — the one consequence every research shares. The resource/token bill itself stays an
   exact payload fact (`cost`/`cost_tokens`, already reaching the policy via the generic
   `payload-number:*` pipeline); a full payment preview is not this package's job.
3. **`score_objective`/`score_secret_objective`** (`ScoringWindow::pending_choice`, LRR 98): every
   scoring option previews the seat's own victory-point count rising by exactly one, capped the
   same way `OBS-008b6`'s custodians removal already caps it; declining previews no change.
4. **Policy.** `strategy_decision_features` (OBS-008d1) now also reads `option.preview`, mapping
   `TacticTokens`/`FleetTokens`/`StrategicTokens`/`TechnologiesOwned`/`VictoryPoints` to
   `strategy:{quantity}-{before,after,change}` under the existing `strategy` family — the same
   shape `tactical`/`combat` already use. No new family, no vocabulary change.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes.
- `place_structure`, `ready_planet`, `politics_choose_speaker`, `politics_place_agenda` (four of
  `OBS-008d1`'s eight subtypes) are left without a preview in this slice; not every subtype has an
  equally clean single-quantity consequence, and this package covers the three that do.

## Tests and commands

- `strategy_cards.rs::obs008d2_token_gain_and_research_preview_their_exact_consequence`: all three
  pool options preview their own count rising by one; a research option previews the seat's
  existing one technology reaching two.
- `objectives.rs::obs008d2_scoring_previews_the_exact_victory_point_gain`: scoring previews the
  seat's two victory points reaching three; declining previews no change.
- `features.rs::obs008d2_strategy_previews_reach_the_policy`: exact
  `strategy:tactic-tokens-{before,after,change}` and `strategy:technologies-after` reach the
  policy and survive `mlp_option_features`/`projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `cargo fmt` (a pre-existing,
unrelated formatting drift in `strategy_cards.rs::redistribute_tokens` was left untouched, matching
prior packages' treatment of the same hunk); `git diff --check`.

## Definition of done

Three strategy/technology/scoring subtypes preview their exact consequence; the policy reads it
under the existing `strategy` family; no vocabulary change; option identity and V1/V2 replay
hashes unchanged; full checks pass; only scoped files committed.
