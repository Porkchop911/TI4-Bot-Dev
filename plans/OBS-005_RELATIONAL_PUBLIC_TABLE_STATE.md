# OBS-005 — relational public table state

## Package

- Milestone: Stage 2 complete decision contract.
- Dependencies: `OBS-002b` (rule-dependency and aliasing matrix).
- Objective: replace aggregate-only opponent blindness for *public* facts with deterministic
  actor-relative opponent slots and relationship facts, per
  `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` representation rule 3. `opponent_facts`'s existing
  anonymous-distribution design for *hidden* information (secrets held) is correct as is and is
  untouched — OBS-005 is about information that is already public and currently only reachable in
  aggregate.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` §"What a bot needs" (each
  opponent's public score, economy, tokens, technologies, controlled territory, visible forces,
  passed status, and relationship to the actor) and representation rule 3 (actor-relative slots,
  not player IDs; relationship first, then initiative rank, then seating offset; permutation
  equivariance).
- Acceptance references: focused `obs005` tests in `crates/ti4-engine/src/choice.rs` and
  `crates/ti4-policy/src/features.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/choice.rs`, `crates/ti4-policy/src/features.rs`,
  `crates/ti4-policy/src/vocabulary.rs`, `crates/ti4-policy/src/projection.rs`, this specification,
  package evidence, `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only. No vocabulary/bundle republish.

## Design

The engine (`Observed`) is always seated with exactly six players (`seating.rs`'s `IN_SCOPE_
FACTIONS`, "six, by owner decision"), so every game has exactly five opponents. A closed, bounded
five-slot family is therefore safe without any player-count generalisation problem.

1. `OpponentRelationship` (new, `choice.rs`): a total, ordered classification of one opponent's
   relationship to the acting seat, ranked by strategic salience (most urgent first) since the plan
   does not fix the tie-break order among the three named relationship kinds:
   - `CombatCounterpart` — some system holds space units of both seats right now. A live/imminent
     conflict is the most salient relationship, so it ranks first.
   - `Support` — a Support for the Throne note is held between the two seats in either direction
     (`state.support_holders`), a standing, binding trust relationship.
   - `Neighbor` — with a galaxy known, some system holding either seat's units is adjacent to a
     system holding the other's. Without a galaxy, this is `None` rather than guessed (missing
     context is explicit, never silently zero — read here as "not computable", not "false"; the
     bounded fact this produces is simply absent).
   - `None` — none of the above.
2. `Observed::opponent_slots(player) -> Vec<&PlayerId>`: every other seat, sorted by
   `(relationship, initiative_rank, seating_offset)` ascending — relationship first as the contract
   requires, initiative rank (position in `initiative_order()`) next, then clockwise seating offset
   from the acting seat last. The **slot index** (0..4) is this sort position, never the player ID:
   relabeling every player ID in a game with the same relative structure produces the same slot
   assignment (equivariance).
3. `crates/ti4-policy/src/features.rs::opponent_slot_facts(seen, player) -> Vec<(String, f64)>`:
   for each slot `i`, emit up to seven bounded facts under one family, `opponent-slot`:
   `opponent-slot:{i}:victory-points`, `:trade-goods`, `:technologies` (count), `:passed` (0/1),
   `:relationship-combat`, `:relationship-support`, `:relationship-neighbor` (0/1 each). Every value
   comes from `PublicSeat`/`OpponentRelationship` — no card identity, no player-ID-derived name.
   Zero values are naturally dropped at emission, matching this module's existing convention.

## Boundaries

- No player ID, planet ID, or system ID ever becomes part of a feature *name*. The slot index is a
  sort position, not an identity; two different games with different seatings that happen to share
  the same relative structure produce the same names.
- Hidden information is untouched: nothing here reads action cards, secret objectives, promissory
  notes still in hand, or hidden deck order. `opponent_facts` (secrets-held, anonymous distribution)
  is unmodified.
- No opponent's exact board topology (systems, planets, routes) is represented here — that is
  `OBS-006` (candidate-centred board state), which depends on this package for its opponent-relative
  half.

## Tests and commands

- `crates/ti4-engine/src/choice.rs::obs005_opponent_slots_are_relationship_then_initiative_then_seating`:
  constructs a six-seat position where one opponent shares a system with the actor (combat), one
  holds a mutual Support note, and the remainder differ only by initiative/seating, and asserts the
  exact slot order.
- `crates/ti4-engine/src/choice.rs::obs005_opponent_slots_are_invariant_under_player_id_relabeling`:
  the permutation-equivariance test the contract requires — the same relative structure built from
  a different set of `PlayerId`s produces the same *shape* of slot assignment (same relationship
  classification sequence), not a coincidentally identical list of IDs.
- `crates/ti4-policy/src/features.rs::obs005_opponent_slot_facts_are_relationship_relative_not_identity_relative`:
  swapping which concrete seat holds a given relationship (while keeping the relationship itself
  fixed) leaves the acting seat's emitted `opponent-slot:*` facts unchanged; changing which
  relationship applies changes them.
- Full engine suite, ordinary policy suite (deterministic campaign included), training suite, strict
  Clippy on engine + policy, `cargo fmt --check`, `git diff --check`.

## Definition of done

`Observed::opponent_slots` assigns exactly five deterministic, relationship-then-initiative-then-
seating-ordered slots; `opponent_slot_facts` emits bounded per-slot facts naming no player, planet,
or system identity; permutation equivariance is proved by test; every existing suite passes
unmodified in behavior; independent Tier-C review OUTSTANDING per current instruction to continue
without waiting on it.
