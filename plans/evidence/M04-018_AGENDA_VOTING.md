# M04-018 — Agenda voting (LRR 8.2ii to 8.21)

## Package

| Field | Value |
|---|---|
| IDs | M04-018 — outcomes, votes, planet exhaustion, speaker tie-break, and law enactment |
| Depends | M04-017 (scoring), M04-011 (structural agenda reveal), M03-001…005 (choice model) |
| Objective | Close the last structural boundary in the round loop. |
| Permission class | P1, plus read-only oracle inspection. |

## Oracle

Commit `37061c511a4780d4c0719e0342533a498cd4b457`, verified clean before and after by
`tools/oracle_integrity_guard.py` (`oracle integrity verified: 238 files`).

Ported from `engine/agenda.py`: `outcomes`, `votable_planets`, `cast_votes`, `tally`,
`winning_outcome`, and the resolution tail of `resolve_one`.

## What this closes

`AgendaChoicesUnimplemented` is gone. **The round loop no longer contains a structural
boundary**: strategy, action, status and agenda all resolve through generated choices.

## A corpus field that does not exist

The first implementation read `electType` off the agenda record, following the oracle's
`Agenda.elects`. **That field is `null` on every card in the corpus.** Reading it would have
made every agenda a silent For/Against, and no election would ever have been offered — a whole
category of agenda quietly disabled, with nothing failing.

The oracle derives it from the printed `target` instead, up to any parenthetical:

```
"Elect Non-Home Planet Other Than Mecatol Rex"  → an election
"Elect Law (When this agenda is revealed, …)"   → an election, parenthetical ignored
"For/Against"                                    → not an election
```

`an_election_is_read_off_the_printed_target_not_a_missing_field` pins it, checking that an
Elect Player agenda elects between the seated players and an Elect Planet agenda offers only
planets somebody controls (8.11).

This was caught by inspecting the corpus rather than by a test, which is worth noting: nothing
in the port would have failed.

## Voting is a state machine, not a loop

Voting is the most choice-dense window in the game — an outcome per player, then a planet per
vote, then possibly the speaker. The oracle writes it as nested loops calling `ask`. This driver
resolves exactly one decision per `step()`, so `VoteWindow` is a resumable machine over
`Outcome → Planets → Tiebreak → Done`, and `settle()` advances past anyone with nothing to
decide.

The rules it encodes, each with a test:

| Rule | Behaviour | Test |
|---|---|---|
| 8.2ii | Voting starts left of the speaker; the speaker votes **last**, knowing every other vote | `the_speaker_votes_last` |
| 8.6a | Exhausting a planet casts its **full** influence, never part of it | `exhausting_a_planet_casts_its_full_influence` |
| 8.14 | An abstention casts nothing and is not recorded as a vote | `an_abstention_casts_nothing_and_is_not_recorded` |
| 8.19 | Most votes wins; a tie **or a silent table** goes to the speaker | `a_silent_table_hands_the_decision_to_the_speaker` |
| 8.19a | The speaker's decision is not a vote | asserted in the same test — the ballot stays empty |
| 8.20/8.21 | A passed law stays in play; everything else is discarded | `an_agenda_is_voted_on_and_a_passed_law_stays_in_play` |

Two behaviours are worth calling out because they are easy to get wrong and neither is obvious:

* **Picking a side and then exhausting nothing is not a vote for that side.** Votes are banked
  only when influence was actually spent, so an outcome nobody paid for never enters the tally
  (`choosing_an_outcome_then_casting_no_votes_records_nothing`).
* **A player with no readied influential planet is never asked to exhaust one.** The window
  falls through to the next voter rather than offering a choice with no options.

## Effects are not applied, and it says so

This engine has no agenda effect registry. The oracle's `resolve_one` handles exactly this case:
when no handler is registered it emits `AGENDA_EFFECT_UNRESOLVED` and carries on. So does this —
`close_vote` records the outcome, announces the unresolved effect, and enacts the law if it
passed.

Proceeding silently would have been the failure mode; proceeding *and saying so* is what the
oracle does. `an_agenda_is_voted_on_and_a_passed_law_stays_in_play` asserts the announcement is
present, not merely that the vote finished.

## Differences from the oracle

| Difference | Reason |
|---|---|
| No effect registry (`EFFECTS`). | Agenda effects are their own body of work. Every resolution announces itself as unresolved. |
| No riders, Political Secret/Favor, Committee Formation, Quash, or "discard and reveal another". | Promissory notes, action cards and faction abilities are unimplemented. `must_be_replaced` therefore has no counterpart, so a card whose premise fails is still voted on. |
| No Representative Government flat votes, Predictive Intelligence, or leader vote bonuses. | Laws, that technology and leaders are unimplemented. Votes come only from exhausted planet influence. |
| No `session.barred` (8.16). | Nothing can bar a player yet. |
| Influence comes from the planet record. | Attachments and laws that change influence are unimplemented; `planet_value_now` has no counterpart. |
| "Elect Scored Secret Objective" and "Elect Strategy Card" yield no candidates. | Secrets are not modelled and strategy-card election is not implemented, so those agendas are discarded rather than voted on — matching the oracle's "no eligible outcome" path. |

## Commands and results

```
$ python tools/oracle_integrity_guard.py
oracle integrity verified: 238 files

$ cargo test --workspace
121 passed  (ti4-content)
174 passed  (ti4-engine)
 68 passed  (ti4-model)
  1 passed  (doc-test)
364 total, 0 failed        (347 before this package)

$ cargo clippy -p ti4-engine --all-targets
0 findings in vote.rs, game.rs, objectives.rs or tokens.rs

$ cargo fmt --all      # clean
```

17 new tests: 16 in `vote.rs`, plus two driver-level agenda tests in `game.rs` (one replacing
the old boundary assertion).

## Open findings

1. **No agenda effects are applied.** Every resolution emits `AGENDA_EFFECT_UNRESOLVED`. Laws
   are recorded in `state.laws` but nothing reads them, so a law in play changes no rule.
2. **No "discard and reveal another".** Cards whose printed premise fails (Judicial Abolishment
   with no law in play, Classified Document Leaks with no scored secret) are still put to a
   vote instead of being replaced.
3. **Votes come only from planet influence** — no flat votes, technology, leaders, or bars.
4. **Secret objectives remain unmodelled**, which also removes one election type.
5. **No independent review.** Waived by the project owner.
