# OBS-007c — stochastic preview foundation

## Package

- Milestone: Stage 2 complete decision contract.
- Dependencies: `OBS-007a` (preview contract).
- Objective: provide the shared, reusable math for representing a *known* public dice mechanic as
  an exact `Preview::chanced` distribution — the stochastic counterpart to OBS-007b's deterministic
  `preview::deterministic::spend` — and establish, by test and by non-goal, that a *hidden* draw
  (deck order, hand contents) stays `Preview::unknown` rather than being approximated.
- Normative sources: `plans/STAGE2_COMPLETE_DECISION_CONTRACT.md` (OBS-007c row; §3's stochastic
  paragraph: "describe the lawful information about the distribution rather than a sampled future
  ... Hidden deck order, hidden card identity, and the eventual random result remain unavailable");
  LRR 78.13 (roll and compare against a hit value); `crates/ti4-engine/src/dice.rs` (the engine's own
  `hits_on`/`hits()` threshold convention, matched exactly).
- Acceptance references: focused `obs007c` tests in `crates/ti4-engine/src/preview.rs`.

## Scope and permissions

- Permission class: P1.
- Writable paths: `crates/ti4-engine/src/preview.rs`, this specification, package evidence,
  `plans/EXECUTION_STATE.md`.
- Read-only external paths, network, external state, ports, destructive actions: none.
- Generated artifacts: normal bounded Cargo output only.

## Design

`preview::stochastic::hit_count_preview(dice, hit_on, deltas_for)` computes the **exact** hit-count
distribution for `dice` ten-sided dice each hitting on `hit_on` or higher — the same threshold
convention `dice.rs`'s `Roll::hits_on`/`hits()` already use, so a caller reads this against the same
mental model the engine's own roll resolution uses. `deltas_for(hits)` is caller-supplied because
what a given hit count *means* (ships destroyed, planets invaded, a die rerolled) is producer-specific
(OBS-008b's job), while the probability math is not.

- `hit_on <= 1`: every die hits. Certain, not a one-case distribution — there is no chance involved.
- `hit_on > 10`: no die can hit (a d10 has no eleventh face). Certain at zero hits, for the same
  reason.
- `dice == 0`: certain at zero hits regardless of `hit_on`.
- Otherwise, the binomial weight of exactly `k` hits out of `dice` is
  `C(dice, k) * successes^k * misses^(dice - k)`, out of `10^dice`, computed in `u128` and cast down
  once bounded. Every case's weight and the total are counts, never a rounded float — the same
  discipline `Preview::chanced`'s doc comment already states for its `Chance::weight`.
- `MAX_DICE = 9`: `Chance::weight` is `u32` (an OBS-007a decision, not reopened here), and `10^9`
  is the largest power of ten that fits. A pool of ten or more dice returns `Preview::unknown`
  rather than a distribution silently rescaled to fit — approximating a rules-exact quantity would
  contradict the fail-closed principle `preview.rs`'s module doc already states. Ten-plus-die pools
  are rare (a full six-carrier fleet plus escorts) and OBS-008b may special-case them later; this
  foundation does not guess at what such a case should report.

## Non-goal: no new "hidden draw" helper

The contract also asks this package to represent "irreducible unknown draws without reading hidden
deck order." No new API is added for this: `Preview::unknown(reason)` — already shipped in OBS-007a
— is the correct and sufficient answer, and inventing a parallel helper would only create two ways to
say the same thing. What matters is discipline at the call site, not new surface here: a producer
must not infer a card-type distribution from public discard-pile counts, because a deck's remaining
composition also depends on which cards sit in hidden hands, which discard-pile accounting cannot
recover. No current producer attaches any preview to a draw (that is OBS-008h's job); this package
records the constraint so that work starts from the right default rather than reaching for a
plausible-looking shortcut.

## Tests and commands

- `hit_count_distribution_matches_hand_computed_binomial_odds`: dice=2, hit_on=6 (5/10 per die)
  against the hand-computed 1/4, 1/2, 1/4 split, exactly (weights 25/100, 50/100, 25/100).
- `hit_on_at_or_below_one_is_certain_not_a_one_case_distribution`: `Outcome::Certain`, not `Chanced`.
- `hit_on_above_ten_is_certain_at_zero_hits`.
- `zero_dice_is_certain_at_zero_hits_for_any_threshold`.
- `nine_dice_is_the_largest_exact_pool_and_ten_is_refused`: `MAX_DICE` boundary, `Preview::unknown`
  beyond it — not a truncated or rescaled distribution.
- `the_expected_hit_count_is_the_textbook_binomial_mean`: `Preview::expected` against `dice *
  (11 - hit_on) / 10` as an exact rational, for a case that does not reduce evenly, proving the
  rational is exact rather than a rounded float.
- Full engine suite, strict Clippy, `cargo fmt --check`, `git diff --check`.

## Definition of done

`preview::stochastic::hit_count_preview` computes the exact binomial hit-count distribution for
bounded dice pools and fails closed (never approximates) beyond `MAX_DICE`; every value is an
integer count, never a float; the hidden-draw non-goal is documented and untouched; every existing
suite passes unmodified in behavior; independent Tier-C review OUTSTANDING per current instruction
to continue without waiting on it.
