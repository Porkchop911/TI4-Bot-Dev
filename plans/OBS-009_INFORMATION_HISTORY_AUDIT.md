# OBS-009 — information-history audit

## Package

An audit, per the plan's own charter: "Identify strategically relevant lawful facts not
reconstructible from current state; persist bounded sufficient summaries. Only then decide whether
recurrence/belief features merit an ablation." This records what was checked, what was found, and
the resulting decision -- not a mechanical feature sweep like `OBS-008`'s packages.

- Dependencies: `OBS-004`-`OBS-008i` (the completed actor/table/board/decision-context/preview
  surface this audit checks for gaps against).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` section 4 ("Continuation state")
  and the `OBS-009` row.
- Writable paths: `crates/ti4-engine/src/vote.rs`, `crates/ti4-policy/src/features.rs`, this spec,
  evidence, `plans/EXECUTION_STATE.md`.

## Method

Checked each of section 4's five continuation clusters, plus the general "public history" question
at the end of that section, against what this session's own OBS-003/007/008 work already surfaced,
reading the actual producer code (not just the plan's description) for each:

1. **Tactical action** (activated system, movement origins, units moved/loaded, capacity
   remaining, invasion commitments, remaining pipeline steps): `DecisionContext.outstanding`
   (`ConstraintKind::FleetSupply`/`TransportCapacity`) plus `OBS-008a`'s previews
   (`FleetSupplyHeadroom`/`CapacityFree`) read the *current* state fresh at every ask, which is
   sufficient -- a tactical action's progress is fully recorded in `GameState` (`active_system`,
   `pending`, board unit positions) rather than in an ephemeral struct, so there is nothing an
   observation could fail to reconstruct.
2. **Combat** (sides, round/step, rolled effects, hits to assign, retreats announced, sustain
   flags, pending destruction): `DecisionContext.round`/`phase` cover round/step; `OBS-005`'s
   opponent slots cover sides; `Quantity::Hits` (`OBS-008b2`) and the `hits` payload
   (`combat.rs:471`) cover rolled effects; `ConstraintKind::UnitsToRemove` and the
   `ShipsInSystem`/casualty previews (`OBS-008b1/b3/b4`) cover the rest. All of it is either in
   `GameState` directly (board composition) or read fresh from the `CombatWindow`'s own stage at
   ask time.
3. **Production/payment** (source, limit, resources owed/paid, discounts, selected units, capacity
   result, cancellation legality): fully covered by `OBS-008c`'s `production_decision_features` --
   `ConstraintKind::ProductionCapacity`/`FleetSupply`/`TransportCapacity` plus the
   `cost`/`count`/`discount`/`capacity_used`/`fleet_headroom_after` payload facts, all read fresh
   from `GameState` each ask.
4. **Strategy cards/component actions** (source, selections already made, costs paid, remaining
   selections): `DecisionSource::StrategyCard{card, secondary}` names the source;
   `OBS-008d2`'s previews (`gain_tokens`, `research_option`) read the *current* pool/technology
   count fresh on every iteration of a multi-pick ask, so "selections already made" is implicit in
   the before-value rather than needing a separate field.
5. **Agenda/trade** (revealed agenda, public vote ledger, outcome/target, offer/counter, promises,
   transaction allowance): revealed agenda and outcome/target are `DecisionContext`/state fields
   already; offer/counter terms reach the policy through the generic payload pipeline; transaction
   allowance is `GameState.transactions_this_round` (durable, queried by
   `transactions::neighbours_who_transacted`/`transacted_with`). **The public vote ledger was a
   real gap** -- see Finding below.

## Finding: the public vote ledger was not reconstructible from `Observed`

LRR 8.2ii seats the speaker last specifically so they vote *knowing every other vote already
cast* -- the ordering exists for exactly this reason, not merely to break ties fairly. `Ballot`
(the running per-outcome tally) lives on `VoteWindow`, a struct the caller holds separately from
`GameState`; nothing `Observed`/`SeatObservation` builds from state alone could recover it,
because `GameState` itself never stores it. A later voter's `cast_vote` options carried no fact
distinguishing "everyone voted FOR so far" from "the table is split" -- exactly the kind of
non-reconstructible, strategically relevant fact this package exists to find.

**Fix**: `VoteWindow::pending_choice`'s `Stage::Outcome` branch now attaches a `current_votes`
payload to each outcome option, read from `self.ballot.counts` fresh at every ask. This reaches
the policy through the existing generic `payload-number:*` pipeline (`features.rs:1167-1184`)
with **zero policy-side code changes** -- the same mechanism every other numeric payload fact
(`cost`, `count`, `hits`, ...) already uses.

## Conclusion

One real gap found and closed (the vote ledger). Every other continuation-state cluster and the
general "public history" question is already either durable in `GameState` (confirmed by explicit
design comments such as `state.rs:1010`, "It is history the rest of the state cannot recover") or
read fresh from an in-progress window's own struct at ask time, which is what "current state"
means for a decision still under way in that window. **Recurrence/belief features are not
warranted**: nothing checked needs a bounded public-history summary beyond the one payload fact
added here, and adding recurrence to recover information the engine can already expose directly
would violate the plan's own representation rule 6 ("missing context is explicit, never silently
encoded"). One item is flagged for a future package rather than fixed here (see Invariants).

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field or hash-participation
  changes -- `current_votes` is ordinary option payload, exactly like `cost` or `hits`.
- `neighbours_who_transacted`/`transacted_with` (trade partner history) is durable state,
  confirmed reconstructible, but not yet surfaced as an explicit feature anywhere -- flagged as a
  scoped follow-up for a future trade/relationship package, not a history gap this package needs
  to close.
- Agenda prediction history (`state.agenda_predictions`) and once-per-turn/round ability usage
  (`timing.rs`'s `is_used`/`mark_used`) were checked and found to be legality-gated rather than
  needing a separate recurrence feature: the option's absence *is* the fact, matching the plan's
  own instruction not to duplicate what the engine already expresses through the legal set.

## Tests and commands

- `vote.rs::obs009_a_later_voter_sees_the_running_tally_of_every_outcome`: the first voter's
  options both preview `current_votes: 0`; after they vote FOR with an influential planet, the
  second voter's FOR option carries the exact influence just cast, AGAINST still carries 0.
- `features.rs::obs009_vote_ledger_payload_reaches_the_policy`: `current_votes` reaches
  `payload-number:current_votes` through the existing generic pipeline (a `0` value is sparse --
  absent, not a stored zero, per this codebase's feature-vector convention) and survives
  `mlp_option_features`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

The five continuation-state clusters and the general public-history question are audited against
the completed OBS-003/007/008 surface; one real gap (the vote ledger) is found and closed with no
new family, quantity, or vocabulary change; the audit's conclusion (no further recurrence/belief
features warranted) is recorded; full checks pass; only scoped files committed.
