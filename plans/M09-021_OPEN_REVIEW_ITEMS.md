# M09-021 open review items

| ID | Severity | Item | Status |
|----|----------|------|--------|
| O-M09-021-1 | LOW (documentation) | `plans/evidence/M09-019.md` W2 numbers predate this package; the post-change extraction-cost measurement in `plans/evidence/M09-021.md` is the current reference. No M08-021 behavioral re-baseline triggered (game-level distributions unchanged; authored bot uses the untouched legacy hashed path). | Accepted by Tier-C review: no behavioral bound changes. The separate performance interpretation is corrected by F-M09-021-3 below. |
| O-M09-021-2 | INFO | Two small pre-existing rustfmt drifts inside files this package edits (`choice.rs` `strategy_card_goods`, `features.rs` `owed` chain) became fmt-conformant via the whole-file format pass; pure line-wrapping, no semantic change. Out-of-scope engine files with pre-existing drift (`action_cards.rs`, `exploration.rs`, `strategy.rs`) were restored to HEAD after formatting. | Accepted by Tier-C review: formatting-only changes are harmless and scoped. |
| O-M09-021-3 | INFO | Crossed emission is an architectural reconciliation with accepted StateCross (MLP §5.1 bare names preserved as fact-name portion). Recorded in spec + evidence; not a deviation, but flagged for the frontier reviewer to confirm the reconciliation stands. | Rejected by Tier-C review; superseded by F-M09-021-2. The linear-only argument does not satisfy the nonlinear MLP input contract. |

## Independent Tier-C frontier review of `51ca544` (2026-08-24)

**Verdict: changes required; M09-021 is not accepted.** The scoring-source calculations,
aggregation, deterministic ordering, legacy-subvector pin, and focused tests are sound, but the
hidden-information boundary and the future MLP feature contract are not enforced as claimed. The
performance interpretation also contains a unit mismatch.

### F-M09-021-1 — HIGH: public observation exposes opponent secret aliases

`Observed` is explicitly documented as exposing only public facts and keeping hand contents behind
a deliberate redacted/private boundary. The new public method
`held_secret_progress(&self, player: &PlayerId)` instead reads the unredacted state and returns the
named secret objectives and progress for any supplied seat. Its engine test explicitly proves that
one `Observed` instance can request another seat's secret alias. Live inference currently passes
`choice.player`, but that makes confidentiality a caller convention rather than the typed/API
boundary required by the repository accuracy rules.

**Required:** bind held-secret progress to an acting-seat/private-view capability such that an
opponent seat cannot be requested through a public `Observed` value. Replace the cross-seat-success
test with a negative boundary test, and retain an end-to-end assertion that live inference sees
only the acting seat's secrets.

### F-M09-021-2 — HIGH: objective facts disappear from major nonlinear-MLP decisions

Objective facts are emitted only inside `StateCross::ByKind` or `ByOption`; `StateCross::None`
drops them completely. The justification that an option-invariant feature cancels in linear
softmax is true for schemas 3–5, but MLP plan §4.1/§5.1 defines these facts as input to a nonlinear
per-option trunk. There, an option-invariant state fact can interact with option facts and must be
present. Uniform-kind board-identity choices commonly resolve to `None`, so production and similar
decisions receive no objective requirement/progress input at all. Renaming every objective fact
under a kind/id cross also fails to preserve the plan's bare transferable feature namespace.

**Required:** preserve the bare objective facts for every option in the future MLP input contract,
without weakening the legacy explicit-schema compatibility guarantee. Add a focused
`StateCross::None` choice proving objective need/progress/met/stage facts survive and remain
option-order deterministic. If linear schemas retain crossed copies, keep the nonlinear/bare
namespace explicit and disjoint rather than treating the linear cross as the MLP delivery path.

### F-M09-021-3 — HIGH: extraction-cost impact compares game time with decision time

The evidence compares W2+W3's approximately 0.16 ms **per decision** with W1's approximately
47 ms **per complete game**, then calls extraction roughly 0.3% of per-decision cost. M09-019b's
W1 normalizer is about 42 microseconds per decision, not 47 milliseconds. The comparison is
dimensionally invalid by roughly three orders of magnitude; on this feature-heavy fixture W2 alone
is several times the recorded W1 engine cost per decision.

**Required:** remove the negligible-impact/0.3% claim and make only dimensionally valid statements.
Do not extrapolate the production-choice fixture to whole-game overhead without a measured live
choice distribution. Preserve the raw 145–152 microsecond W2 measurement and its honest variance
dispositions.

### Independent checks

- Commit frontier `432f20a..51ca544`: diff-check clean; package files committed; only the three
  unrelated pre-existing user edits remain in the worktree.
- Engine focused: accessor **1/0**, positive-threshold registry **1/0**, family-token disjointness
  **1/0**.
- Policy focused: objective source/determinism **8/0** under the `objective_` filter; opponent
  secret test **1/0**; max-before-vector aggregation **1/0**.
- These green tests do not close F1/F2: F1's test endorses the forbidden cross-seat access, and
  F2 lacks a `StateCross::None` delivery assertion.

**Next exact action:** resolve F1–F3, rerun focused/affected/workspace gates, correct spec/evidence,
then request a fresh independent Tier-C recheck. Do not close M09-021 or begin a dependent package
on the basis of `51ca544`.

## F1–F3 correction round (implementer, 2026-08-24)

All three findings resolved in-package on `wp/m09-021-objective-policy-features`, within the
package's originally declared writable paths (`choice.rs`, `features.rs`, plans files). No new
writable path was needed.

### F-M09-021-1 — resolved (typed boundary by signature)

`Observed::held_secret_progress(player)` is **removed**. Its replacement,
`Observed::held_secret_progress_for_choice(choice)`, derives the acting seat from `choice.player`;
there is no parameter through which an opponent could be requested, so a public `Observed` value
cannot name another seat's cards. The feature path (`features.rs`) now calls the choice-bound form;
it was the only production caller.

- Engine test rewritten: owner-binding assertions for both seats **through one public `Observed`
  value**, plus an explicit negative assertion that answering through a's choice never names b's
  card, with a comment recording that the arbitrary-seat signature no longer exists.
- Policy-level end-to-end redaction test retained and re-pointed at the new accessor: for two seats
  in one position (met channel provably active via affordable `trade_routes`), no seat's features
  contain an alias held by the other.

### F-M09-021-2 — resolved (bare namespace on every option)

Objective facts are now emitted in **two disjoint namespaces**:

- **Bare** — MLP §5.1 names verbatim, on every option under every crossing mode including
  `StateCross::None` (the nonlinear per-option trunk input contract).
- **Crossed** — unchanged from the initial implementation (`state-kind:`/`state-option:` under
  `ByKind`/`ByOption`, absent under `None`), remaining the linear-schema delivery path.

The five bare families (`objective-progress`, `objective-met`, `objective-need`,
`objective-count`, `objective-stage`) were added to `EXPLICIT_FIXED_FAMILIES` (22 → 27) — a
reviewed extension of the closed grammar; every legacy name is unchanged, and M09-019b's pinned
inventory test was updated with that rationale. New focused test
`bare_objective_facts_survive_state_cross_none`: a uniform-kind choice with composite ids (asserted
to resolve to `StateCross::None`) proves all four fact classes survive on every option under their
bare names, that no crossed copy exists under `None`, and that the bare set is option-order
deterministic.

### F-M09-021-3 — resolved (records-only)

The "negligible at game scale / ~0.3%" claim is removed from `plans/evidence/M09-021.md`. The
replacement makes only dimensionally valid statements: W2 = 145–152 µs **per extraction** on this
feature-heavy fixture versus M09-019b's W1 normalizer ≈42 µs **per decision**; on this fixture the
objective-fact construction alone costs several times the engine per-decision cost, and no
whole-game extrapolation is made without a measured live choice distribution. The raw measurements
and variance dispositions are preserved unchanged.

### Gates after correction (exact results in `plans/evidence/M09-021.md` addendum)

Pending: fresh independent Tier-C recheck of the corrected commit. M09-021 remains open until then;
no dependent package may start on the basis of this branch's pre-correction commits.

## Fresh independent Tier-C recheck of `870a8f5` (2026-08-24)

**Verdict: changes required; M09-021 remains open.** F-M09-021-2 and F-M09-021-3 are resolved,
but F-M09-021-1 is not.

### F-M09-021-1 — still open (HIGH): `Choice` is forgeable, not a private-view capability

`held_secret_progress_for_choice(choice)` still exposes named secret progress through the public
`Observed` type. `Choice` has public fields, a public constructor, and `Serialize`/`Deserialize`;
therefore deriving the requested seat from `choice.player` does not authenticate the acting seat or
make another seat unrequestable. Any caller holding a public `Observed` can construct a `Choice`
whose `player` is an opponent and retrieve that opponent's secret aliases and progress.

The rewritten engine test demonstrates the leak rather than a negative boundary: one `seen_a`
value is passed a freely constructed `choice_b`, and the test asserts that B's secret alias `mlp`
is returned. The later assertion only proves that `choice_a` returns A's cards; it does not prevent
the preceding cross-seat request. The production extractor's use of an engine-generated choice is
still caller convention, not the typed hidden-information boundary required by `AGENTS.md`.

**Required:** move held-secret progress behind an unforgeable acting-seat/private-view capability,
or an equivalent API whose construction validates and binds the viewer. A public `Observed` value
must be unable to retrieve another seat's aliases even when supplied caller-constructed data. Add a
negative test that attempts the cross-seat request and proves it cannot be expressed or is rejected.

### Resolved findings

- **F-M09-021-2 resolved:** bare objective facts survive on every option under
  `StateCross::None`; the focused test is non-vacuous and the legacy subvector/inventory pins pass.
- **F-M09-021-3 resolved:** the invalid per-game/per-decision comparison is removed. The updated
  evidence reports only dimensionally valid component costs and preserves variance rejection.

### Independent checks

- `cargo test -p ti4-engine objective_progress_accessors_are_seat_scoped_and_source_complete` —
  **1 passed, 0 failed** (but the test positively demonstrates the forgeable cross-seat request).
- `cargo test -p ti4-policy bare_objective_facts_survive_state_cross_none` — **1/0**.
- `cargo test -p ti4-policy opponent_secrets_never_enter_any_seat_features` — **1/0**.
- `cargo test -p ti4-policy the_legacy_subvector_is_pinned_against_the_recorded_baseline` — **1/0**.
- `cargo test -p ti4-policy m09_019b_feature_inventory_is_pinned` — **1/0**.
- `git diff --check` — clean; only the three unrelated pre-existing user edits were present before
  this review record.

**Next exact action:** replace the forgeable `Choice` binding with a genuine typed/private-view
boundary, add the cross-seat negative test, rerun the affected gates, and request another narrow
independent Tier-C recheck. M09-021 and dependent M09-024 remain blocked until acceptance.
