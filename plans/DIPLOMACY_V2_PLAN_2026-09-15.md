# Structured diplomacy v2: user feedback and plan, 2026-09-15

Diplomacy v1 is on `codex/diplomacy-v1` (see `DIPLOMACY_HANDOVER_2026-09-15.md`). After watching games in the
reviewer, the user gave this feedback:

- request, threat, assurance and warning are just words, with no clue what they are supposed to mean;
- many components are tradeable (for example the promise to use an agent in a beneficial way), and promissory
  notes are mostly ignored;
- transactions should be subsumed under diplomacy;
- the reviewer's left panel should use black text, and the meaning of green and red in diplomacy was unclear.

## Decisions (user interview, 2026-09-15)

| Question | Decision |
|---|---|
| Signals | **Concrete statements.** Each signal says something checkable, and the engine records whether it was honoured. |
| Additional tradeables | **All four offered:** promissory notes; agent and leader use; action cards and relic fragments; strategy-card secondary and agenda favours. |
| Transactions | **One window when diplomacy is enabled.** The separate transaction window disappears; a contact with a transaction partner offers trades, deals and signals together, while non-partners get only promises and signals. With diplomacy off, today's transactions are unchanged. |
| Reviewer left panel | **Light panel, black text.** The board and the decision panel stay dark. |
| Relationship colours | Replaced by stance words (friendly / neutral / wary / hostile) with a written legend. |

## Packages

Each package keeps diplomacy-off games bit-identical: legacy transactions and existing checkpoints must behave
exactly as before, and the ti4-sim bounds must not move.

### R: reviewer readability (done in this session)

- The left panel has a light fill and black text. Seat colour appears only as a "●" swatch or a pastel chip
  background.
- Relationship cells read "friendly · T+25 C+15 Th0 H0", with the legend spelled out, in both the native window
  and the HTML export.

### T: transactions inside the contact window

- With diplomacy on, `transactions::available_actions` offers nothing, so `open_transaction` disappears.
- A diplomacy contact with a transaction partner (`transactions::partners`, which respects neighbours, Guild
  Ships and Trade Convoys) adds `Trade` bundles. Each is built from the existing `offer_options` shapes through
  `offer_from`, then mapped to `ImmediateTransfer` terms on both sides: trade goods, commodities, fragments,
  promissory notes (the Support swap and note sales and gifts), action cards and Black Market secrets.
- Counters keep the existing amount and deadline edits.
- On acceptance of a bundle with immediate transfers:
  - record the transaction (`record_transaction`);
  - emit `TRANSACTION_OPENED` and `TRANSACTION_RESOLVED` exactly where the legacy window does, so Lie in Wait,
    Black Market Dealings and every other transaction-watching effect keep working;
  - apply the fair-transaction relationship change instead of the deal-accepted one when the trade is fair.
- Bounds: trade bundles are capped separately (proposed 12) so trade shapes do not crowd out promises. The
  initial-candidate cap rises to fit, and every other bound stays deterministic.

### S: concrete signals

Replace the bare kind and subject with a typed statement. The kind is derived from the statement, never chosen
as a word.

| Kind | Statement | Judged |
|---|---|---|
| assurance | "I will not attack you through round N" | broken if the speaker attacks the target before the deadline, honoured at the deadline |
| request | "Stay out of system X through round N" (X near the speaker: home, or a system with the speaker's units adjacent to the target) | ignored if the target activates X, honoured at the deadline |
| request | "Do not attack me through round N" | ignored if the target attacks the speaker, honoured at the deadline |
| threat | "If you attack me before round N ends, I will attack you" | carried out if the speaker attacks the target after being attacked, a bluff if the round ends without retaliation, void if never triggered |
| warning | "If you activate system X, I will attack you" | the same, triggered by the activation |
| assurance, agenda phase | "I will vote OUTCOME on AGENDA" | honoured or broken when votes are recorded |

- **Model:** a `SignalStatement` enum plus a signal status (open / honoured / ignored / broken / carried out /
  bluff / void). Validation bounds stay as today, with expiry at most one round ahead.
- **Relationships:** small fixed changes per outcome, kept in the single delta table in `relations.rs`.
- **Offering:** a bounded set per contact (at most 6), and only statements that are relevant, such as systems
  that actually border both seats.

### L: new tradeables and favours

Each new promise is judged at the engine moment where it can be observed.

- **Promissory notes:**
  - "note for non-aggression" and "note for payment" templates;
  - a promise to return a lent note by a deadline (`ReturnNote`), judged at the deadline from
    `promissory_notes`.
- **Agent and leader use:**
  - `UseLeaderFor { leader, beneficiary, deadline }`;
  - fulfilled when the leader resolves with that beneficiary as its target;
  - needs a typed hook where `use_leader` picks a target (Carth `hacanagent` and the L1Z1X agent already choose
    one; each targeted agent gets the same hook).
- **Action cards and relic fragments:** delivered through the trade bundles in T, under the same legality as today
  (Arbiters for action cards, Black Market for fragments and secrets).
- **Strategy-card secondaries:**
  - `FollowSecondary { card, deadline }` and `SkipSecondary { card, deadline }`;
  - judged in `step_secondary` from `STRATEGY_SECONDARY_FOLLOWED` / `DECLINED` together with the follower and
    card.
- **Agenda favours:**
  - After an agenda is revealed and before the first vote, each seat in voting order gets one contact
    opportunity: the same window with a pair allowance per agenda.
  - The pay-for-vote template is generated from `agenda_choices`, and `Vote { agenda, outcome }` is judged when
    votes are recorded (the existing hook).

#### L as built (2026-09-15)

- **Terms:** `UseLeaderFor`, `SkipSecondary` and `ReturnNote` were added to `DealTerm`. `FollowSecondary` was
  dropped: following a secondary almost never helps the card's holder, so no template would offer it.
- **Judging:**
  - `DiplomacyEventContext::LeaderUsedFor` is raised by `hacanagent` when it replenishes another seat, and by
    `l1z1xagent` when it swaps a mech for the active seat.
  - `SecondaryResolved` is raised in `step_secondary`: followed breaks `SkipSecondary`, and declined keeps it.
  - At the deadline, `SkipSecondary` is kept (the secondary was never offered). `ReturnNote` is kept if the note
    sits with its owner, whether handed back or played, and broken otherwise.
- **Templates:** `PayForAgentFavour` and `SellAgentFavour` (readied Hacan or L1Z1X agent), `PayToSkipSecondary`
  (a card the proposer has not played yet), `NoteForNonAggression` and `NoteLoan` (the proposer's own notes still
  in hand), and `PayForVote` (only in the talks before a vote).
- **Cap:** the 36-bundle cap takes templates in turns. Plain truncation of the id-sorted list had been cutting
  whole templates alphabetically, and trades went first.
- **Agenda talks:**
  - With diplomacy on and a map, each revealed agenda first opens `AgendaTalks`. Transactions and pair allowances
    are reset for that agenda.
  - Each seat, in voting order (speaker last), is asked `diplomacy_agenda_talks`: open a contact with any seat it
    has not contacted for this agenda, or `diplomacy|end_talks`.
  - The vote opens when no seat is still negotiating. The decision site is registered in the delivery inventory.
- **Rule:** during the agenda phase any two players may transact (94), so `transactions::why_illegal` skips the
  neighbour check in that phase. Legacy transactions are only offered during turns, so diplomacy-off play does
  not reach this path. ti4-sim must confirm that before merge.
- **Built in the continuation:** the agenda-phase assurance signal ("I will vote OUTCOME") is generated from
  the concrete revealed agenda choices, judged from the recorded ballot, rendered in the reviewer, and exposed
  as a bounded structured policy feature.
- **Still not built:** hooks for targeted leaders other than the two agents.

### Continuation corrections (2026-09-15)

- Contact option IDs now carry the seating-order index rather than a faction name, so duplicate factions are
  independently addressable. The policy and reviewer resolve the same index.
- The cost bound is derived from 36 bundles + 6 signals + no-op = 43; the measured 2-round probe observed 8.
- Breaking v2 output is explicitly versioned as diplomacy log v2, offline corpus v3, and canonical observation
  v3. The older declared pairs remain reader inputs rather than being silently relabelled.
- The 60-seed x 6-round release soak passed and exercised 69 favours, including all three implemented favour
  families. Goods-free favour swaps are therefore optional tuning, not a correctness prerequisite.

### Follow-through for every package

- **Policy features:** option features for the new term, statement and template kinds, under the diplomacy
  family. The OOV family is already registered.
- **Reviewer:** plain-language text for every new term, statement and outcome, in the window, action summaries
  and HTML.
- **Soak:** every new offer, statement and promise kind must occur and settle without a refused step. Replay stays
  identical.
- **Journal and log export:** new events carry their own terms, as offers already do.
- **Handover:** update this plan and the handover document as packages land.
