# OBS-008f2 — agenda vote consequence surface

## Package

First preview slice of the `content` family `OBS-008efghi1` opened subtype-only. Attaches an exact
preview to `vote_exhaust_planet` (LRR 8.11), the one `content` subtype with the cleanest
single-quantity consequence of the batch: exhausting a planet adds its full influence to the
running vote total for the outcome the seat is backing.

- Dependencies: `OBS-008efghi1` (`b59335c`, the `content` family and subtype guard), `OBS-007b`
  (deterministic preview foundation).
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (`OBS-008f` row), LRR 8.10-8.11.
- Writable paths: `crates/ti4-engine/src/preview.rs`, `crates/ti4-engine/src/vote.rs`,
  `crates/ti4-policy/src/features.rs`, this spec, evidence, `plans/EXECUTION_STATE.md`.

## Behavior

1. **New `Quantity::Votes`** (`preview.rs`): the running total a seat's chosen outcome would reach,
   distinct from `ConstraintKind::Votes` (which bounds an ongoing multi-pick ask's remaining
   allowance rather than naming an exact reachable total).
2. **`vote_exhaust_planet`** (`VoteWindow::pending_choice`, `Stage::Planets`): each offered planet
   previews the seat's running vote total (already-cast votes for this outcome, read fresh off
   `Stage::Planets { votes, .. }`) rising by exactly that planet's printed influence (LRR 8.6a: the
   full amount, never partial). A second exhaust in the same multi-pick ask previews from the first
   exhaust's own total, not from zero.
3. **Policy.** `content_decision_features` (subtype/option-count only since `OBS-008efghi1`) now
   also reads `option.preview`, mapping `Quantity::Votes` to `content:votes-{before,after,change}`
   — the same shape `strategy`/`tactical`/`combat` already use for their own quantities. No new
   family, no vocabulary change: a specific quantity name inside an already-registered family is
   not itself a reserved column.

## Invariants and boundaries

- No option ID, label, or legal set changes. No `DecisionContext` field changes.
- `cast_vote` (which outcome to back) and `vote_tiebreak` (which outcome wins a tie) are identity
  choices among outcomes, not quantities, and stay without a preview — consistent with how
  `OBS-008d1`'s `ready_planet`/`politics_choose_speaker`/`politics_place_agenda` were left
  unpreviewed for the same reason.
- The other ~37 `content` subtypes remain subtype/option-count only; this package covers exactly the
  one with the cleanest single-quantity consequence, not the whole row.

## Tests and commands

- `vote.rs::obs008f2_exhausting_planets_previews_the_running_vote_total`: two distinct planets on
  the same seat preview `0 -> influence1` then `influence1 -> influence1+influence2`, confirming the
  running total is read fresh each ask rather than reset per option.
- `features.rs::obs008f2_vote_exhaust_preview_reaches_the_policy`: exact
  `content:votes-{before,after,change}` reach the policy and survive
  `mlp_option_features`/`projection::admits`.

Full engine + policy (incl. campaign) + training suites; strict Clippy; `rustfmt --check` (scoped
files); `git diff --check`.

## Definition of done

`vote_exhaust_planet` previews its exact running-vote-total consequence; the policy reads it under
the existing `content` family via a new `Quantity::Votes`; no new family, no vocabulary change;
option identity and V1/V2 replay hashes unchanged; full checks pass; only scoped files committed.
