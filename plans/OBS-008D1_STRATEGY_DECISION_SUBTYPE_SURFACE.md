# OBS-008d1 — strategy/technology/scoring subtype surface

## Package

First slice of `OBS-008d`. Deliberately broad and shallow: read the typed context `OBS-003e`
already populated (subtype, option count, optional-ness) across a first set of common
strategy/technology/scoring subtypes, with **zero engine changes** — no new preview is attached
in this slice, matching the "surface state, don't over-slice" direction the owner gave for this
row.

- Dependencies: `OBS-003e` (the typed context these subtypes already carry), `OBS-004`.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008d` row).
- Writable paths: `crates/ti4-policy/src/features.rs`, `crates/ti4-policy/src/projection.rs`,
  `crates/ti4-policy/src/vocabulary.rs`, this spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

New `strategy_decision_features(choice, features)`, guarded on eight subtypes covering research,
token gain, structure placement, politics, and objective scoring:
`research_technology`, `gain_command_token`, `place_structure`, `ready_planet`,
`politics_choose_speaker`, `politics_place_agenda`, `score_objective`, `score_secret_objective`.
Emits `strategy:subtype:{subtype}`, `strategy:option-count`, and `strategy:optional` when the
context marks the decision declinable. Board/unit context these options already carry as payload
(`cost`, `cost_tokens`) reaches the policy through the existing generic `payload-number:*`
pipeline with no further wiring — confirmed by test, not assumed.

New family `strategy`: `EXPLICIT_FIXED_FAMILIES` (38 → 39), `projection::FAMILY_ROLES` (45 → 46,
`Transferable`), `OOV_REGISTRY_VERSION` 8 → 9 (`OOV_FAMILIES_V9` appends `strategy`), new pinned
fingerprint, one-version-back inference window moves to v8.

## Non-goals (explicit residual)

No preview is attached to any of these eight subtypes' options in this slice, and the row's
remaining subtypes (turn/pass mechanics, the other ~13 strategy-card subtypes enumerated during
scoping, and objective-consequence facts beyond "which is offered") are not covered. Recorded as
open follow-up work for later `OBS-008d` slices, not silently claimed complete.

## Tests and commands

One test: `features.rs::obs008d1_strategy_decisions_carry_their_subtype_and_option_count` —
`research_technology` (with its `cost` payload reaching `payload-number:cost` unmodified),
`gain_command_token`, and `score_objective` each carry `strategy:subtype:*`/`strategy:option-count`;
an uncovered subtype gets none of it (the guard is closed); the facts survive `mlp_option_features`
and `projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `cargo fmt`;
`git diff --check`.

## Definition of done

Eight strategy/technology/scoring subtypes expose their typed subtype and option count; the new
`strategy` family is registered end-to-end (grammar, projection, vocabulary); no engine file
changed; full checks pass; only scoped files committed.
