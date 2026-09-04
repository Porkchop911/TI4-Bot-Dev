# OBS-008h3 — secret-objective-count content consequence surface

## Package

Fourth preview slice of the `content` family, closing the gap `OBS-008h2` flagged and deferred:
three subtypes across the secret hand-limit rule and Thunder's Edge whose consequence is exactly
the seat's own held-secret count falling by one. First use of `Quantity::SecretObjectivesHeld`,
declared alongside `ActionCardsHeld` in `OBS-007a` but never wired to a producer until now.

- Dependencies: `OBS-008h2` (`7894b85`, `ActionCardsHeld`'s own first wiring, the sibling
  quantity this package completes the pair for).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008g`/`OBS-008h`/`OBS-008i`
  rows), LRR 45.4 (secret hand limit), the Thunder's Edge expedition rules.
- Writable paths: `crates/ti4-engine/src/preview.rs`, `crates/ti4-engine/src/secrets.rs`,
  `crates/ti4-engine/src/thunders_edge.rs`, `crates/ti4-policy/src/features.rs`, this spec,
  evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **New `Quantity::SecretObjectivesHeld`** (`preview.rs`): secret objectives a seat holds, scored
   or not (LRR 45) -- the same total `secrets::held_count` already computes for the hand-limit
   check itself.
2. **`return_over_secret_hand_limit`** (`secrets::enforce_hand_limit`, LRR 45.4): every offered
   return option previews the seat's own held-secret total (via `held_count`, so a scored secret
   still counts) falling by exactly one.
3. **`expedition_discard_action_card`** (`thunders_edge::pay`, "action_cards" slice): every discard
   option previews the seat's own action-card count falling by exactly one, read fresh each of the
   two iterations -- reuses `ActionCardsHeld`, no new quantity needed.
4. **`expedition_discard_secret`** (`thunders_edge::pay`, "secret" slice): every discard option
   previews the seat's own held-secret total (via `secrets::held_count`, for the same
   scored-counts-too reason) falling by exactly one.
5. **Policy.** `content_decision_features` gains a fifth quantity mapping: `SecretObjectivesHeld`
   -> `content:secret-objectives-*`. No new family, no vocabulary change.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes.
- The `held_count`/`ActionCardsHeld`-reuse choice keeps both producers' previews consistent with
  what their own hand-limit rule actually counts against, rather than reading a different total
  than the one gating the ask.

## Tests and commands

- `secrets.rs::obs008h3_return_over_secret_hand_limit_previews_the_exact_loss`: a hand one over the
  limit previews the return option `HAND_LIMIT+1 -> HAND_LIMIT`.
- `thunders_edge.rs::obs008h3_expedition_action_card_discard_previews_the_exact_loss`: a two-card
  hand previews the first discard option `2 -> 1`.
- `thunders_edge.rs::obs008h3_expedition_secret_discard_previews_the_exact_loss`: a two-secret hand
  previews the discard option `2 -> 1`.
- `features.rs::obs008h3_secret_objective_count_preview_reaches_the_policy`: maps to
  `content:secret-objectives-after` and `-change` survives `projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

Three more `content` subtypes preview their exact consequence via a first-used
`SecretObjectivesHeld` quantity; the policy reads all three under the existing `content` family;
no new family, no vocabulary change; option identity and V1/V2 replay hashes unchanged; full checks
pass; only scoped files committed.
