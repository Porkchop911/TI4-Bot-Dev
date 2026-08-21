# M06-025 — Play-area note scoring (baf and sb)

## Status

**Accepted 2026-08-21** (independent Tier-C review, Claude Opus 5 — `plans/M06-025_OPEN_REVIEW_
ITEMS.md`). Findings: L1 recorded in evidence with a standing re-check condition for any future
roster widening; L2 recorded in M06-023 evidence; L3 resolved by comment. The independent
frontier adjudication of M06-024 (Claude Opus 5, recorded in `plans/M06-024_OPEN_REVIEW_ITEMS.md`)
confirmed F2 on every factual claim and judged the escalation to this package correct; that
satisfies this package's dependency. M06-024 itself is not acceptable until this package lands
(plus J1's instrumentation run, now recorded in the M06-024 ledger), so its verified F1 fix
remains uncommitted and carries over to this package's branch; both packages commit together at
closure, M06-024 first.

**Scope extension declared during implementation:** `crates/ti4-engine/src/transactions.rs`
signature threading (mechanical consequence of requirement 1) — see the evidence file.

| Field | Value |
|---|---|
| Milestone | M06 — General rules (reopened) |
| Depends | accepted M06-024 (F2 escalation), accepted M06-021a (baf window), accepted M06-023 (sb progress) |
| Permission class | P1 |
| Review tier | C — scoring legality and hidden information |
| Compatibility | accepted Rust predicates; printed card text in the content corpus; Python parity not applicable |

## Objective

Make Betray a Friend (`baf`) and Strengthen Bonds (`sb`) count only promissory notes that are in
the acting player's **play area** (face-up), exactly as both cards print, by deriving face-up
assignment from the accepted content corpus instead of a hard-coded alias list. Support for the
Throne is already play-area by construction via `support_holders` and stays unchanged.

## Normative sources

- Printed text in `crates/ti4-content/content/secret_objectives.json`: `baf` ("Win a combat
  against a player whose promissory note you had in your play area at the start of your tactical
  action") and `sb` ("Have another player's promissory note in your play area"), including both
  cards' notes field distinguishing play area from hand.
- The `playArea` field per record in `crates/ti4-content/content/promissory_notes.json` (eleven
  records true: `<color>_sftt`, `<color>_an`, and nine faction notes — convoys, blood_pact, pop,
  gift, antivirus, terraform, dark_pact, shareknowledge, sever).
- LRR 69 as implemented by the accepted `promissory.rs` module (deal at setup; receipt moves a
  note to the holder; face-up placement on receipt for play-area notes).

## Required behavior

1. **Content-driven face-up model.** A received note goes face-up in the recipient's play area iff
   its corpus record has `playArea: true`. The hard-coded `promissory::FACEUP` list is replaced by
   a lookup over the accepted content (generic `<color>` records resolve against the note key's
   owner faction). Unknown aliases are not face-up. The model must be deterministic and read-only
   with respect to game state.
2. **baf filter.** `combat.rs::note_holdings` counts, for each holder seat, only notes that are
   held **and** face-up (play area), plus all `support_holders` entries unchanged. The tactical-
   action-start snapshot timing from M06-021a is preserved exactly; no other feat changes.
3. **sb filter.** `secrets.rs::rival_note_issuers_count` counts only face-up held rival notes,
   keeping the accepted issuer-deduplication semantics (M06-023 H1 fix) and the non-zero-threshold
   contract. Its progress/legality derivation from the same count is unchanged.
4. **No other scoring changes.** No other secret, objective, ability, or transaction reads note
   play-area status differently than today; `promissory_faceup` consumers (convoys/an abilities)
   keep their exact behavior.

## Scoped access

```text
Writable paths:
  crates/ti4-engine/src/promissory.rs
  crates/ti4-engine/src/combat.rs
  crates/ti4-engine/src/secrets.rs
  plans/M06-025_PLAY_AREA_NOTE_SCORING.md
  plans/evidence/M06-025.md
  plans/EXECUTION_STATE.md
Read-only supporting paths:
  crates/ti4-content/content/promissory_notes.json
  crates/ti4-content/content/secret_objectives.json
Network/process needs: bounded Cargo format/test/lint/property commands only
Generated artifacts: Cargo target output only
External-state effects/destructive actions: none
```

## Tests to add

- Table-driven face-up model over all eleven `playArea: true` records and a sample of `false`
  ones, including generic `<color>` resolution for both seated and unseated owner factions.
- Receipt transitions: each play-area faction note goes face-up on receipt; hand notes do not;
  returning a note removes it from the holder's play area (existing give-back paths).
- baf decision boundary: winner holds loser's note in hand only → no feat; same note face-up at
  tactical start → feat; note received after the snapshot → no feat (timing preserved).
- sb decision boundary: rival note in hand only → progress 0 / unmet; face-up → counts once per
  distinct issuer with the accepted deduplication.
- No regression for `convoys`/`an` ability consumers and for Support-for-the-Throne scoring.
- Existing payment, objective-scoring, affected-crate, and workspace suites pass.

## Commands and evidence

Scoped `rustfmt`, focused tests, `cargo test -p ti4-engine`, `cargo test --workspace`, engine
Clippy with every warning classified, and `git diff --check`. Evidence records the exact face-up
table, decision-boundary cases, the behavior change versus M06-023/M06-021a (which makes later
VP/clearance numbers non-comparable until re-baselined), and the independent Tier-C review.

## Definition of done

Face-up assignment is content-driven over the accepted corpus; baf and sb count play-area notes
only with all M06-021a timing and M06-023 deduplication semantics preserved; decision-boundary,
regression, crate, and workspace suites pass; the independent Tier-C review is resolved; evidence
is complete; and only scoped paths are committed. Until accepted, the M06 exit gate remains
blocked by F2.
