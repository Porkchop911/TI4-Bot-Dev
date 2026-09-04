# OBS-008e/f/g/h/i (pass 1) — content decision-surface subtype

## Package

A single broad, shallow read across the plan's remaining `OBS-008` rows — trade (e), agenda (f),
reactions/action-cards/faction-abilities (g), exploration/relics (h), and leaders/breakthroughs/
remaining audit rows (i) — mirroring the pattern `OBS-008d1` established for strategy/technology/
scoring: subtype and option count only, one new family, zero engine changes, zero vocabulary risk
beyond the one family this package adds.

- Dependencies: `OBS-003` (the `DecisionContext` typed producers this reads), `OBS-008d1`
  (`451eded`, the same broad/shallow pattern applied to a different row).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008e/f/g/h/i` rows).
- Writable paths: `crates/ti4-policy/src/features.rs`, `crates/ti4-policy/src/projection.rs`,
  `crates/ti4-policy/src/vocabulary.rs`, this spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **New `content` family.** `content_decision_features(choice, features)` reads
   `choice.context.subtype` and, when it matches one of ~34 fixed strings (each an existing,
   already-shipped `DecisionContext::new` call site — trade in `transactions.rs`, agenda votes in
   `vote.rs`, agenda effects in `agenda_effects.rs`, reactions/action-cards/faction abilities across
   their respective modules, exploration/relics in `exploration.rs`/`relics.rs`, leaders and the
   remaining audit rows) or one of 5 structural patterns for the small number of genuinely
   runtime-constructed subtype strings (`ends_with("_choose_technology")` for relic technology
   picks, `ends_with("_choose_reward")` for exploration card rewards, `starts_with("play_reaction_")`
   for a reaction's timing relation folded into its subtype, `starts_with("pick_")` and
   `contains("_pick_")` for action-card "pick" mechanics), emits:
   - `content:subtype:<subtype>` = 1.0
   - `content:option-count` = the option count
   - `content:optional` = 1.0 when `context.optional`
   An unmatched subtype gets no `content:*` fact — the family stays a closed grammar, not an open
   catch-all; the structural rules are traced to real, specific source patterns rather than "any
   string".
2. **No preview.** As with `OBS-008d1`, this pass is subtype/option-count only. Not every one of
   these ~39 subtypes has an equally clean single-quantity consequence; a preview slice (mirroring
   `OBS-008d2`) is left for a later, opportunistic pass.
3. **No engine changes.** Every subtype string this reads already exists in a shipped
   `DecisionContext::new` call from `OBS-003`; this package only adds a reader.
4. **Vocabulary.** `EXPLICIT_FIXED_FAMILIES` gains `"content"` (40 -> unchanged this pass, already
   done by `OBS-008d1`'s successor work — see evidence for the exact bump). `FAMILY_ROLES` in
   `projection.rs` gains `("content", FamilyRole::Transferable)`, inserted alphabetically between
   `"combat"` and `"critic-state"`. `vocabulary.rs` migrates `OOV_REGISTRY_VERSION` 9 -> 10,
   appending `"content"` to a new `OOV_FAMILIES_V10`, matching the mechanical pattern used for
   every prior family bump this session.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes.
- The five structural (`ends_with`/`starts_with`/`contains`) rules are a deliberate, narrow
  exception to this codebase's "closed literal list" convention for `EXPLICIT_FIXED_FAMILIES`
  member functions — justified because these five patterns are each traced to one specific,
  reviewed source shape (a relic's or exploration card's own name folded into its subtype string,
  or a reaction/action-card's fixed naming convention), not an open-ended catch-all. Flagged here
  for independent review.
- Preview attachment for any of these subtypes is out of scope; a later package may add it where a
  clean single-quantity consequence exists.

## Tests and commands

- `features.rs::obs008efghi_content_subtypes_reach_the_policy`: a fixed subtype
  (`propose_transaction`) and a structurally-matched dynamic subtype
  (`play_reaction_after_combat`) both produce `content:subtype:*`/`content:option-count`; an
  unrecognised subtype produces no `content:*` fact; the fixed subtype's fact survives
  `mlp_option_features`/`projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

The content decision surface (trade/agenda/reactions/action-cards/faction-abilities/exploration/
relics/leaders) has a typed subtype and option count reaching the policy under one new `content`
family; vocabulary migrated to v10 with a real fingerprint; option identity and V1/V2 replay hashes
unchanged; full checks pass; only scoped files committed.
