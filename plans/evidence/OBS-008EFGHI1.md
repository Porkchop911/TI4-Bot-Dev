# Evidence — OBS-008e/f/g/h/i (pass 1), content decision-surface subtype

See `plans/OBS-008EFGHI1_CONTENT_SUBTYPE_SURFACE.md` for design; this records checks.

## Changed paths

- `crates/ti4-policy/src/features.rs` — new `content_decision_features`, wired into
  `explicit_option_features_with`'s dispatch chain; `EXPLICIT_FIXED_FAMILIES` gains `"content"`
  (39 -> 40); the literal-array test's comment and contents updated to match.
- `crates/ti4-policy/src/projection.rs` — `FAMILY_ROLES` gains
  `("content", FamilyRole::Transferable)`, inserted alphabetically between `"combat"` and
  `"critic-state"` (46 -> 47); pinned-length test bumped to match.
- `crates/ti4-policy/src/vocabulary.rs` — `OOV_REGISTRY_VERSION` 9 -> 10; new `OOV_FAMILIES_V10`
  (47 entries, appending `"content"`) and its fingerprint; `oov_families()` and
  `validate_versioned`'s one-version-back arm updated to the new current/prior pair (10/9); reserved-
  order test's fingerprint assertions, append block, and built-vocabulary loop extended; the
  "loads for inference"/"remains refused" test pair shifted up one version
  (`version_eight_loads_for_inference_*` renamed to `version_nine_loads_for_inference_*`, a new
  `version_eight_remains_refused_for_inference` added).
- `plans/OBS-008EFGHI1_CONTENT_SUBTYPE_SURFACE.md`, `plans/evidence/OBS-008EFGHI1.md`,
  `plans/EXECUTION_STATE.md`.

## Focused evidence

- `features.rs::obs008efghi_content_subtypes_reach_the_policy`: `propose_transaction` (fixed) and
  `play_reaction_after_combat` (structural, `starts_with("play_reaction_")`) both produce
  `content:subtype:*`/`content:option-count`; `some_other_subtype_entirely` (unrecognised) produces
  no `content:*` fact; the fixed subtype's fact survives `mlp_option_features`/`projection::admits`.
- `vocabulary.rs` (34 tests, all passing): `the_reserved_order_is_pinned_and_each_version_preserves_
  every_earlier_index` confirms v10 is v9 plus exactly `"content"` appended, with a real (not
  placeholder) fingerprint; `version_nine_loads_for_inference_without_renumbering_its_columns`
  confirms a v9 vocabulary still loads for inference under the v10 registry without its columns
  moving, and that a v10-only fact (`content:new-current-only-fact`) does not alias an old trained
  row; `version_eight_remains_refused_for_inference` confirms the window did not silently widen.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-policy --lib obs008efghi` | 1 passed |
| `cargo test -p ti4-policy --lib vocabulary::` | 34 passed |
| `cargo test -p ti4-policy --lib projection::` | 23 passed |
| `cargo test -p ti4-engine` | 1,198 lib + 4 integration + 5 doc passed (unchanged — no engine
  changes in this package) |
| `cargo test -p ti4-policy --lib` | 226 passed (224 + 2 new, incl. the 102-game campaign, no
  regression) |
| `cargo test -p ti4-training` | 133 passed |
| `cargo clippy -p ti4-engine -p ti4-policy --all-targets -- -D warnings` | clean |
| `rustfmt --edition 2024 --check` (scoped files) + `git diff --check` | clean |

## Independent Tier-C review

OUTSTANDING (deferred per standing repo convention; reviewed elsewhere per user confirmation this
session). Flags for that review: (1) the five structural `ends_with`/`starts_with`/`contains`
subtype-matching rules in `content_decision_features` are a narrow, deliberate exception to this
codebase's closed-literal-list convention for `EXPLICIT_FIXED_FAMILIES` members — each is traced to
one specific, reviewed source shape, not an open catch-all, but this judgment call was made without
a review pass and should be checked; (2) no preview attached to any of these ~39 subtypes in this
pass — left for a later, opportunistic package; (3) `OBS-008d1`'s four still-unpreviewed strategy
subtypes (`place_structure`, `ready_planet`, `politics_choose_speaker`, `politics_place_agenda`)
remain open, unrelated to this package.
