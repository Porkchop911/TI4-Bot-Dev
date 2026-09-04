# OBS-010 — critic alignment

## Package

Rebuilds the option-free critic inventory (`critic.rs`, M09-027) from the completed actor
information state. The module predates `OBS-004a`'s actor-owned faceup inventory and `OBS-005`'s
deterministic opponent slots, so it never gained either: the critic could not see a seat's own
relics, exploration cards, breakthrough, or leader readiness at all, and its opponent view stayed
anonymized-aggregate-only even after the policy path gained relational, slot-indexed opponent
facts.

- Dependencies: `OBS-004a` (`fbd5548`, actor-owned faceup inventory), `OBS-005` (`4b69835`,
  relational public table state) -- the two policy-path facts this package mirrors into the critic.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-010` row); MLP plan §4.1/
  §4.2 (the critic's own design constraints, already documented at the top of `critic.rs`).
- Writable paths: `crates/ti4-policy/src/critic.rs`, this spec, evidence,
  `plans/EXECUTION_STATE.md`.

## Behavior

1. **`actor_inventory_facts`** (new, critic-local): the bound seat's own relics (held/exhausted,
   per-relic readiness), exploration cards held, relic fragments, breakthrough, and leaders by
   status and by identity -- the same fields `OBS-004a`'s policy-path `actor_inventory_facts`
   already reads, renamed to the critic's own flat `snake_case` convention and pushed under
   `CRITIC_FAMILY` rather than `actor-inventory:`. Unconditional (not gated by `CriticFeatures`),
   matching how the policy path never gated it either.
2. **`opponent_slot_facts`** (new, critic-local): deterministic actor-relative opponent slots
   (`Observed::opponent_slots`'s relationship-then-initiative-then-seating order) carrying each
   slot's victory points, trade goods, technology count, passed status, and relationship
   (combat counterpart / Support / neighbor) -- the same fields `OBS-005`'s policy-path
   `opponent_slot_facts` already reads. Added *alongside* the existing anonymized `vp_spread`/
   `secret_spread` aggregate in `table_aggregate`, not replacing it: the two convey different
   information (distribution vs. relational structure) and existing trained critic columns for the
   aggregate stay meaningful.
3. Neither new group needed a policy-side (`features.rs`) change, a new vocabulary family, or a
   `FAMILY_ROLES` entry: `critic-state` is already the one reserved family every critic fact lives
   under, and these are new fact *names* inside it, the same situation this session's `content`
   family previews were in.

## Invariants and boundaries

- No `Choice` is read anywhere in either new function -- both take exactly the same
  `(seen: &Observed<'_>, player: &PlayerId)` signature every other critic-local function uses, so
  permutation and legal-set invariance stay structural rather than coincidental (per the module's
  own stated design).
- No raw seat identity leaks: opponent facts are keyed by deterministic slot index, exactly as
  `opponents_contribute_counts_and_never_identities` already checks over the *entire* inventory
  (not just the pre-existing aggregate) -- this test passed unmodified against the new facts,
  which is real evidence rather than an assumption.
- `OBS-006`'s candidate-centred board facts were checked and correctly excluded: they are
  inherently about a specific target the critic must not see, by the module's own founding
  constraint ("No fact derived by iterating the legal set at all").

## Tests and commands

- `critic.rs::obs010_the_critic_carries_actor_inventory_and_opponent_relationship_facts`: a seat
  holding two relics (one exhausted), a breakthrough, and a leader, plus a combat-counterpart
  opponent (shared-system units), produces every expected `critic-state:relic*`/
  `critic-state:breakthrough*`/`critic-state:leader*` fact and at least one
  `critic-state:opponent_slot:*:relationship_combat` fact.
- The five pre-existing critic tests (`opponents_contribute_counts_and_never_identities`,
  `the_inventory_excludes_everything_section_four_one_forbids`,
  `every_critic_name_is_in_its_own_namespace`, `the_gated_groups_are_absent_unless_enabled`,
  `the_vector_is_ordered_and_deduplicated`) pass unmodified against the enlarged inventory --
  confirming the new facts respect every invariant those tests police.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

The critic's inventory carries the actor-owned faceup facts (`OBS-004a`) and the deterministic
opponent-slot relationship facts (`OBS-005`) the policy path already has; every pre-existing
critic invariant test passes unmodified against the enlarged inventory; no vocabulary change;
option identity and V1/V2 replay hashes unchanged (policy-only, no engine touched); full checks
pass; only scoped files committed.
