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
