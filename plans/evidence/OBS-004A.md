# Evidence — OBS-004a, actor-owned faceup inventory

## Scope and provenance

- Branch: `wp/tier-c-review-remediation-obs008c2b-003e1`, after `c73649b` (OBS-004 handover).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` §1;
  `plans/OBS-004A_ACTOR_PRIVATE_INVENTORY.md`; `plans/HANDOVER_2026-09-04_OBS004.md`; LRR 73.4, 35.9,
  Thunder's Edge, and the Leader Sheet rule (leader lifecycle is not hidden information).
- Historical Python: not inspected and not used as an acceptance oracle.
- Permission: P1 only. No network, external write, destructive action, or committed generated
  artifact.

## Changed paths

- `crates/ti4-engine/src/choice.rs`
- `crates/ti4-policy/src/features.rs`
- `crates/ti4-policy/src/vocabulary.rs`
- `crates/ti4-policy/src/projection.rs`
- `plans/OBS-004A_ACTOR_PRIVATE_INVENTORY.md`
- `plans/evidence/OBS-004A.md`
- `plans/EXECUTION_STATE.md`

## Result

The handover's first-pass instruction was to add relics/exhaustion, exploration cards, relic
fragments, breakthrough, and leaders as **`SeatObservation`** (bound-seat-only) accessors. Reading
the actual `Player` field comments against LRR text overturns that for all five: each is faceup
(73.4 relics; 35.9 fragments; exploration cards and leader sheets are printed/comment-documented as
faceup; a breakthrough is a passive-ability card derived from the already-public
`expedition_slices`). They are added to **`PublicSeat`** instead — the same standing
`technologies`/`strategy_cards` already have — reachable through `Observed::seat` for any seat. No
new `SeatObservation` accessor exists; the existing four (`held_secrets`, `held_action_cards`,
`held_promissory_notes`, `held_secret_progress`) are unchanged. `plots`/`plot_objectives` (Firmament)
are excluded: `seating.rs::the_firmament_is_not_in_scope` confirms the faction is out of scope, and
no producer anywhere populates either field.

At the feature layer, `crates/ti4-policy/src/features.rs` adds `actor_inventory_facts`, reading only
the acting seat's own `PublicSeat` row and emitting ten closed readiness/count facts under one new
bounded family, `actor-inventory`. Opponent crossing is deliberately deferred, matching the existing
restraint `opponent_facts` documents for secrets.

## Focused evidence

- `crates/ti4-engine/src/choice.rs::obs004a_public_seat_carries_faceup_inventory`: gives seat "b" one
  of each new field and reads it back through `Observed::seat(&pid("b"))` from seat "a"'s side of the
  table, proving the accessor is genuinely public; also confirms `SeatObservation` bound to "a"
  still exposes none of "b"'s private secret.
- `crates/ti4-policy/src/features.rs::obs004a_actor_inventory_facts_move_only_the_acting_seats_vector`:
  paired mutation test. A fresh seat emits no `actor-inventory:*` facts. Giving the acting seat
  every field (a relic, an exhausted relic, an exploration card, fragments, a breakthrough, and an
  unlocked leader) makes all six corresponding facts appear. Resetting the actor and giving the
  identical holdings to the *other* seat instead returns the actor's emitted vector to exactly the
  original baseline — proving no opponent crossing exists yet.
- `EXPLICIT_FIXED_FAMILIES`/`live_grammar_families` coverage, `FAMILY_ROLES` classification
  coverage, and the legacy schema-4 baseline-pin test all updated and pass with the new family
  registered and excluded from the frozen legacy pin respectively.

## Vocabulary migration (v4 → v5)

`OOV_REGISTRY_VERSION` 4 → 5. `OOV_FAMILIES_V5` appends `"actor-inventory"` to `OOV_FAMILIES_V4`
(append-only — no earlier index moves, same as every prior version). New
`OOV_FAMILIES_V5_FINGERPRINT` (computed by `registry_fingerprint`, not hand-derived). The
one-version-back inference window shifts: v5 loads for training and inference, v4 loads for
inference only, v3 (previously the inference-compatible version) is now refused exactly as v2 is.
Renamed/renumbered the affected tests:
`version_four_loads_for_inference_without_renumbering_its_columns` (was the v3 test) and
`version_three_remains_refused_for_inference` (new, pins the shifted refusal boundary). Also filled
a pre-existing gap in `the_reserved_order_is_pinned_and_each_version_preserves_every_earlier_index`:
a v3→v4 prefix-preservation block had never been added when v4 shipped; it is added alongside the
new v4→v5 block so the test's own claim ("each version preserves every earlier index") is checked
for every version, not just the two most recent.

## Tests and checks

| command | result |
|---|---|
| `cargo test -p ti4-engine --lib obs004a` | 1 passed |
| `cargo test -p ti4-engine --quiet` | 1,177 lib + 4 integration + 5 docs passed |
| `cargo test -p ti4-policy --lib obs004a` | 1 passed |
| `cargo test -p ti4-policy --lib vocabulary::` | 29 passed |
| `cargo test -p ti4-policy --lib` | 205 passed (includes the 102-game deterministic campaign, 302.93 s — no regression against the 260-305 s range measured across this workstream) |
| `cargo test -p ti4-mlp` | 96/97 passed — see "Accepted fallout" below |
| `cargo test -p ti4-training` | 133 passed |
| `cargo clippy -p ti4-engine -p ti4-policy --all-targets -- -D warnings` | passed |
| `cargo fmt --check` (choice.rs, features.rs, vocabulary.rs, projection.rs) | passed |
| `git diff --check` | passed |

## Accepted fallout: one `ti4-mlp` smoke test

`cargo test -p ti4-mlp --test smoke_refusals::a_pool_without_an_allowed_manifest_role_is_refused`
fails: `REFUSED: out/vocabulary/current.json names no valid generation`.

Root cause, traced exactly: this test omits `--slots`, so `mlp_smoke` resolves the vocabulary from
the local (gitignored, not part of the repository) `out/vocabulary/current.json` pointer.
`ti4_training::vocabulary_corpus::validate_generation_files` loads that generation's `slots.json`
through `Vocabulary::from_json` — the strict, training-load path — which now correctly refuses it,
because that generation was published under `OOV_REGISTRY_VERSION` 4 and this package bumps to 5.
This is the identical, previously-accepted consequence every prior registry-version bump produced —
`mlp_smoke.rs`'s own `ACCEPTED_SLOTS_SHA256` doc comment narrates the same thing happening at
M09-027b, M10-035, and M10-036, each time requiring a new published generation and a moved pin.
`a_vocabulary_that_is_not_the_accepted_generation_is_refused` (the sibling test, which supplies an
explicit `--slots` scratch file) is unaffected and passes.

Republishing a generation is a full discovery/training-pool run, explicitly out of this package's
Non-goals ("No vocabulary/bundle republish or training run"). This is deferred, exactly as OBS-003i
deferred the schema-7 bundle republish it caused.

## Compatibility and non-goals

- No option ID, label, legal set, state transition, prompt, or replay contract changed.
- `seat_facts`'s frozen 8-tuple arity and the schema-4 extractor are untouched.
- No opponent-facing crossing of relic/leader/exploration-card identity added.
- No per-relic or per-leader identity feature; only closed readiness/count buckets.
- `plots`/`plot_objectives` are not represented (unimplemented Firmament content).
- No vocabulary/bundle republish or training run performed.

## Independent Tier-C review

**OUTSTANDING.** This package is a hidden-information boundary change (moving five fields from a
"treat as private" default to a documented public accessor) and an OOV registry schema migration —
both named Tier-C classes in `plans/PI_WORK_PACKAGE_STANDARD.md`. Not committed pending review.
