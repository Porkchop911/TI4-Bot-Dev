# OBS-008h2 — action-card-count content consequence surface

## Package

Third preview slice of the `content` family: two subtypes across the hand-limit rule and
exploration/relics whose consequence is exactly the seat's own action-card count moving by one, in
opposite directions -- `discard_over_hand_limit` (LRR 2.4) loses one, `codex_take_action_card`
(The Codex relic) gains one. First use of `Quantity::ActionCardsHeld`, declared since `OBS-007a`
but never wired to a producer until now.

- Dependencies: `OBS-008g2` (`7ba99a3`, the `content` family's second preview batch).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008g`/`OBS-008h` rows),
  LRR 2.4 (hand limit), The Codex relic text.
- Writable paths: `crates/ti4-engine/src/action_cards.rs`, `crates/ti4-engine/src/relics.rs`,
  `crates/ti4-policy/src/features.rs`, this spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **`discard_over_hand_limit`** (`enforce_hand_limit`): every discard option previews the seat's
   own hand count falling by exactly one, read fresh each iteration of the loop (a discard already
   made in the same over-limit sequence changes what the next one previews from).
2. **`codex_take_action_card`** (`codex`): every take option previews the seat's own action-card
   count rising by exactly one, read fresh each of the card's up-to-three iterations. Declining
   stays unpreviewed, matching every other decline this session left unpreviewed.
3. **Policy.** `content_decision_features` gains a fourth quantity mapping: `ActionCardsHeld` ->
   `content:action-cards-*`. No new family, no vocabulary change.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes.
- Checked and left unpreviewed this pass: `expedition_discard_action_card`/
  `expedition_discard_secret` (`thunders_edge.rs`) and `return_over_secret_hand_limit`
  (`secrets.rs`) are the same shape for secret objectives, but no `SecretObjectivesHeld` quantity
  exists yet; adding one is left for a future package rather than growing this one's scope.
  `titan_prototype_choose_builder` (identity choice among players) and `stellar_converter_choose_
  target`/`crown_of_emphidia_choose_planet`/`dominus_orb_purge_to_move`/
  `neuraloop_choose_relic_to_purge` (identity or permission choices without a uniform per-option
  quantity) stay unpreviewed for the same reason `OBS-008g2` left similar producers alone.

## Tests and commands

- `action_cards.rs::obs008h2_discard_over_hand_limit_previews_the_exact_hand_loss`: with a
  hand one over the limit, the discard option previews `HAND_LIMIT+1 -> HAND_LIMIT`.
- `relics.rs::obs008h2_codex_previews_the_exact_action_card_gain`: with an empty hand and one
  discarded card, the take option previews `0 -> 1`.
- `features.rs::obs008h2_action_card_count_previews_reach_the_policy`: both map to
  `content:action-cards-after` (7.0 / 1.0 respectively) and `-change` survives
  `projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

Two more `content` subtypes preview their exact consequence via a first-used `ActionCardsHeld`
quantity; the policy reads both under the existing `content` family; no new family, no vocabulary
change; option identity and V1/V2 replay hashes unchanged; full checks pass; only scoped files
committed.
