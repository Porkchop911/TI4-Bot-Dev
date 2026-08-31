# Bug: transaction proposer never offers non-Support promissory notes

**Status:** FIXED 2026-08-31

Two changes, matching the report's own split between enumeration and selection.

Enumeration: a note the partner cannot afford is now offered as a *gift* (`pn{note}:0`) rather than
withheld entirely. A gift is a legal transaction (94.3), so requiring the fixed sale price for the
note to appear at all was a gap in the offer set rather than a policy preference (criterion 5).

Selection: `ScoredBot::score_offer` no longer drops promissory notes into the flat `unknown_trade`
zero. The engine already prices both sides of every deal into the option payload (`net` to us,
`their_net` to them), so the policy reads those instead of guessing -- and each note gets its own
feature name (`note:ra`, `note:cf`, ..., `note:other`) so the learner can price a Research Agreement
differently from a Ceasefire (criteria 2 and 10). `their_net` is clamped at zero and included,
because a proposal only pays if it is accepted.

The test that asserted a note is *absent* when the partner cannot pay has been retargeted at the
price rather than the presence -- its premise was the behaviour this report calls wrong.

**Severity:** HIGH

**Reported:** 2026-08-29

## Summary

Automated players offer Support for the Throne in transactions, but do not offer other promissory notes they legally hold. This removes most promissory-note trading from simulated games and makes transaction behavior highly repetitive.

This is an end-to-end proposal defect, not simply an absence of engine data. `crates/ti4-engine/src/transactions.rs` currently enumerates some non-Support note-sale options, but `crates/ti4-policy/src/bot.rs` assigns every such offer the fallback `unknown_trade` score of zero. Support exchanges receive a dedicated, strongly positive score. In practice, the transaction proposer therefore selects Support exchanges and other positively scored trade shapes instead of ordinary promissory notes.

The available offer shapes are also unnecessarily narrow: a non-Support note is listed only as a fixed-price sale for trade goods, and only when the recipient already holds that many trade goods. Legal note gifts, swaps, and deals involving other consideration are not represented by that path.

## Observed behavior

- Transaction summaries repeatedly contain offers to exchange Support for the Throne.
- Other legally transferable promissory notes are never proposed.
- Faction notes and generic notes remain in players' hands even when trading them would be useful to both parties.
- A recipient lacking the hard-coded trade-good price prevents the note from appearing as an offer at all, even though other legal terms could be proposed.

## Expected behavior

Every promissory note legally held and transferable by the proposer must be eligible for transaction proposal. The proposer should evaluate the particular note and the complete terms of the deal, then sometimes select a non-Support note when that deal is preferable.

Support for the Throne may remain unusually valuable, but its dedicated score must not make it the only promissory-note transaction observed in play. Note identity, timing value, ownership, current holder, play-area status, recipient, and requested consideration must all be respected.

## Requirements and invariants

- Enumerate every promissory note the proposer legally holds and may transfer.
- Do not offer a note that is faceup in a play area, has already been transferred out of the proposer's hand, or is otherwise prohibited from transfer.
- Evaluate non-Support notes by their actual note identity rather than assigning all of them a zero-value fallback.
- Evaluate both sides of the complete transaction; a valuable note given away must have a cost to the proposer and value to the recipient.
- Permit legal transaction shapes beyond a fixed-price trade-good sale where the transaction model supports them, including gifts, exchanges, and mixed consideration.
- Do not require the recipient to hold a hard-coded number of trade goods merely for the note to be considered in some other legal deal.
- Preserve fog-of-war boundaries: a player may reason from its own hand and public information, but must not inspect another player's hidden promissory-note hand.
- Support for the Throne must remain subject to its distinct ownership, play-area, and victory-point rules.
- Offered terms must pass the same legality validation used when the transaction is executed.
- Learned and heuristic policies must receive stable features that distinguish the note and its terms without collapsing all non-Support notes into one unknown bucket.

## Acceptance criteria

1. With a transferable non-Support note in hand and a legal partner, at least one offer option containing that note is available.
2. A policy-level test demonstrates that a beneficial non-Support note deal can be selected over declining or an inferior generic trade.
3. Research Agreement, Trade Agreement, Ceasefire, political notes, and faction notes are covered by representative tests where present in the enabled content set.
4. A note held by another player or faceup in a play area is never offered by the original owner.
5. A legal note gift or non-cash exchange is not suppressed solely because the recipient lacks the fixed trade-good sale price.
6. The same note cannot be promised or transferred twice from one hand.
7. The selected offer round-trips through option parsing, legality checking, acceptance, and transfer with the correct note id and terms.
8. Support exchanges continue to work, but a seeded simulation containing eligible ordinary notes produces at least one non-Support promissory-note offer.
9. Reviewer action summaries name the specific promissory note and transaction partner for both offered and completed deals.
10. Regression coverage distinguishes option enumeration from policy selection so an option that is technically present but behaviorally unreachable still fails testing.

## Impact

The defect removes a major negotiation mechanism, suppresses faction-specific interactions, distorts the value of promissory-note abilities, and biases simulator and training data toward Support for the Throne exchanges. It affects engine behavior, policy quality, and reviewer fidelity.
