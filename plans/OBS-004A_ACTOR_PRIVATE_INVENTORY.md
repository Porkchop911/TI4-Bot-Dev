# OBS-004a — actor-owned faceup inventory

## Package

- Milestone: Stage 2 complete decision contract.
- Dependencies: `OBS-002b`, `OBS-003a-i` (typed context foundation, already committed).
- Objective: extend the observation surface with the actor-owned holdings named in
  `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` §1 that are not yet represented anywhere: relics and
  their exhaustion, relic fragments, exploration-card play-area actions, Thunder's Edge
  breakthroughs, and leader lifecycle status. Represent each as bounded readiness/count facts for
  the acting seat.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` §1 and its hidden-information
  counterfactual gate; LRR 73.4 (relics), 35.9 (fragments), Thunder's Edge (breakthroughs), the
  Leader Sheet rule (leader lifecycle is not hidden information); `plans/HANDOVER_2026-09-04_OBS004.md`.
- Acceptance references: focused `obs004a` tests in `crates/ti4-engine/src/choice.rs` and
  `crates/ti4-policy/src/features.rs`, plus the vocabulary registry migration tests in
  `crates/ti4-policy/src/vocabulary.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/choice.rs`, `crates/ti4-policy/src/features.rs`,
  `crates/ti4-policy/src/vocabulary.rs`, `crates/ti4-policy/src/projection.rs`, this specification,
  package evidence, `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only. No vocabulary/bundle republish — the
  migration only defines the new reserved layout; nothing is retrained in this package.

## Field-by-field privacy finding

The handover's first-pass instruction was to add these as `SeatObservation` (bound-seat-only,
private) accessors. Reading the actual `Player` field comments and LRR text overturns that
assumption for five of the six named collections:

| `Player` field | LRR basis | Finding |
|---|---|---|
| `relics` | 73.4: relics are held faceup, cannot be traded | **Public** |
| `exhausted_relics` | exhaustion of a faceup card is itself visible | **Public** |
| `exploration_cards` | comment: "placed faceup in your play area" (Enigmatic Device) | **Public** |
| `relic_fragments` | comment: "kept faceup until purged" (35.9) | **Public** |
| `breakthrough` | granted from claiming a public `expedition_slices` entry; the faction breakthrough is a passive-ability card, not a hand card | **Public** |
| `leaders` | leader sheets (identity and lock/ready/exhaust/purge status) are never hidden information in TI4 | **Public** |
| `plots` / `plot_objectives` | Firmament content; `seating.rs::the_firmament_is_not_in_scope` confirms the faction is out of scope, and no producer anywhere reads or writes these fields | **Excluded — dead, unimplemented content, not a privacy question** |

Because the first five are genuinely public, they belong on `PublicSeat` (reachable for any seat
through `Observed::seat`), not on the bound-only `SeatObservation` capability — the same home
`technologies` and `strategy_cards` already have. No new `SeatObservation` accessor is added by this
package; the "actor-owned" framing is preserved at the *feature* layer instead, which reads only the
acting seat's own row and defers opponent crossing (see Non-goals).

`plots`/`plot_objectives` are left untouched: adding facts for a field nothing in the engine ever
populates would be dead feature surface with no fixture that could ever exercise it honestly.

## Behavior

1. `PublicSeat` (`crates/ti4-engine/src/choice.rs`) gains five fields: `relics: &'a [RelicId]`,
   `exhausted_relics: &'a BTreeSet<RelicId>`, `exploration_cards: &'a [String]`,
   `relic_fragments: &'a BTreeMap<String, i32>`, `breakthrough: Option<&'a BreakthroughId>`,
   `leaders: &'a BTreeMap<LeaderId, LeaderStatus>`. `Observed::seat` populates them from the same
   `Player` record it already reads.
2. `crates/ti4-policy/src/features.rs` adds `actor_inventory_facts(seen, player) -> Vec<(String,
   f64)>`, following the existing `ability_facts`/`opponent_facts` shape: computed once per choice,
   added to `ChoiceContext`, emitted bare and under both `state-kind:`/`state-option:` crosses. One
   new bounded family, `actor-inventory`, with a closed set of suffixes:
   - `actor-inventory:relics-held`, `actor-inventory:relics-exhausted` (counts)
   - `actor-inventory:exploration-cards-held` (count)
   - `actor-inventory:relic-fragments-held` (sum across traits)
   - `actor-inventory:breakthrough-held` (0/1)
   - `actor-inventory:leaders-locked`, `-unlocked`, `-readied`, `-exhausted`, `-purged` (counts by
     `LeaderStatus` bucket)

   Suffixes are a closed, fixed set (not open card/relic-alias vocabulary) — the same "readiness
   bucket, not identity" restraint `opponent_facts` documents, applied here to the actor's own
   holdings so the family stays small and the specific relic/leader identity (which the acting
   player already knows without a feature) is not what is being taught.
3. `EXPLICIT_FIXED_FAMILIES` gains `"actor-inventory"`.
4. `crates/ti4-policy/src/vocabulary.rs`: `OOV_REGISTRY_VERSION` 4 → 5. `OOV_FAMILIES_V5` appends
   `"actor-inventory"` to `OOV_FAMILIES_V4` (append-only, per the v2/v3/v4 precedent — no existing
   index moves). New `OOV_FAMILIES_V5_FINGERPRINT`. `validate_versioned` accepts v5 for
   training+inference and v4 for inference only (the one-version-back rule the v3/v4 tests already
   establish); v3 becomes refused for inference, same as v2 is today.
5. `crates/ti4-policy/src/projection.rs`: `FAMILY_ROLES` gains `("actor-inventory",
   FamilyRole::Transferable)` — every fact is a bounded count/flag about the acting seat, transfers
   between games exactly as `ability`/`faction-commodities` already do.

## Boundaries (hidden-information invariants)

- No accessor takes a caller-supplied opponent identity to read another seat's *private* holding —
  none of these fields were ever private, so this restates rather than changes the existing
  boundary. `SeatObservation`'s no-argument shape (`held_secrets`, `held_action_cards`,
  `held_promissory_notes`, `held_secret_progress`) is untouched.
- `actor_inventory_facts` reads only `seen.seat(player)` for the *acting* player — no opponent
  crossing is added in this package (deferred, matching how `opponent_facts` is the intentionally
  narrow exception today).
- Paired mutation tests (below) prove both directions: the actor's own faceup holding changes the
  actor's feature vector, and an opponent's mutation of the *same* public fields leaves the actor's
  feature vector bit-identical (since `actor_inventory_facts` never reads another seat).

## Non-goals

- No opponent-facing crossing of relics/leaders/exploration-card identity — that is a relational
  (OBS-005) or later OBS-004 slice, not this one.
- No per-relic or per-leader *identity* feature — only closed readiness/count buckets, per the
  restraint above.
- `plots`/`plot_objectives` (Firmament) are not represented; the content is out of scope and no
  producer ever populates them.
- No change to `features.rs`'s frozen schema-4 extractor, `seat_facts`'s 8-tuple arity, or any
  option ID, label, legal set, or replay contract.
- No vocabulary/bundle republish or training run.

## Tests and commands

- `crates/ti4-engine/src/choice.rs`: an `obs004a_public_seat_carries_faceup_inventory` test proving
  `Observed::seat` reports relics/exhaustion/exploration cards/fragments/breakthrough/leaders for
  any seat (not only the bound one), and that `SeatObservation`'s existing accessors are unaffected.
- `crates/ti4-policy/src/features.rs`:
  - an actor-mutation test: giving the acting seat a relic, an exhausted relic, an exploration card,
    fragments, a breakthrough, and each leader status moves the corresponding
    `actor-inventory:*` fact.
  - an opponent-mutation test: the same mutations applied to a second seat leave the acting seat's
    emitted `actor-inventory:*` facts (and every other family) bit-identical.
  - `EXPLICIT_FIXED_FAMILIES`/`live_grammar_families` coverage continues to pass with the new family
    registered.
- `crates/ti4-policy/src/vocabulary.rs`: the reserved-order pinning test extends to v5; a
  `version_four_loads_for_inference_without_renumbering_its_columns` test (renamed/renumbered from
  the current v3 test) and a `version_three_remains_refused_for_inference` test (renumbered from the
  current v2 test) pin the shifted one-version-back window.
- Full engine suite, ordinary policy suite (deterministic campaign included), training suite, strict
  Clippy on every touched crate, `cargo fmt --check`, `git diff --check`.

## Definition of done

`PublicSeat` exposes the five faceup collections for any seat; `actor_inventory_facts` emits closed
readiness/count facts for the acting seat only; the vocabulary registry migration to v5 is complete
with a pinned fingerprint and correct inference-compatibility window; paired actor/opponent-mutation
tests pass; every existing suite passes unmodified in behavior; independent Tier-C review is
obtained before commit (hidden-information and schema-migration package).
