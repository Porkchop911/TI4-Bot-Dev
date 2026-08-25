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

## F-M09-021-1 round 2 — design and writable-path declarations (implementer, 2026-08-24)

The recheck is correct: `Choice` has public fields, a public constructor and serde derives, so
deriving the acting seat from it authenticates nothing. A public method on `Observed` that takes
any caller-controlled identity data (a seat argument or a choice) can always be pointed at an
opponent. The only design that survives is one where **the binding happens inside engine code and
the bound value has no public constructor**.

### Design

1. New public type in `choice.rs`: `SeatObservation<'a>` — "private observation". Private fields,
   **no public constructor**; the sole production path is `pub(crate) fn bind(observed, seat)`,
   called from inside `Table::ask_seeing` where the engine already authenticates the acting seat
   (the decider lookup is keyed by `choice.player`). Methods: `observed()`, `seat()`, and
   `held_secret_progress()` — no arguments; it answers for the bound seat only. `Deref`s to
   `Observed` so every public-fact call site keeps working unchanged.
2. `Observed::held_secret_progress_for_choice` is removed. No method on `Observed` returns named
   secret data with any argument — criterion 2 of the finding holds by type surface.
3. `Decider::choose_seeing` now takes `&SeatObservation<'_>`. Live play flows: engine ask path →
   bound capability → decider → `LearnedBot::consider(seen.observed(), choice, &seen.held_secret_
   progress())`. A decision's feature path can structurally see only its own seat's secrets.
4. Offline/training/diagnostic contexts (which hold the complete state by design and where hidden
   information does not exist) use an explicit-records API: `explicit_choice_features` /
   `explicit_option_features` take `held_secrets: &[CardProgress]` as a parameter, computed via a
   documented free function `ti4_engine::choice::held_secret_progress(state, content, sources,
   galaxy, viewer)`. Every direct call site names the records it uses — visible cost rather than
   hidden convention (the `redacted_for` philosophy).
5. New public free function `ask_private(choice, seen, decider)` for tests/offline drivers: binds
   internally exactly as the table does and validates the answer; the capability never escapes to
   caller code. Live play additionally authenticates the decider against its seat via the table's
   per-seat map, which `ask_private` documents as the difference.

### Negative boundary (what becomes inexpressible)

- No public constructor for `SeatObservation`: policy-side code cannot bind a view to any seat it
  chooses; the only values that exist are produced by engine ask paths.
- `held_secret_progress()` takes no seat argument: even holding a bound view, there is no call
  that names another seat's cards.
- Engine test: one public `Observed`, two engine-bound views (A and B) — each returns exactly its
  own cards; plus an explicit assertion that the removed arbitrary-seat API cannot be expressed
  (documented by the type surface, asserted at runtime for both bindings).

### Finding-specific writable-path declarations (before editing)

Within the original package list: `crates/ti4-engine/src/choice.rs`, `crates/ti4-policy/src/
features.rs`. Declared extensions (all signature lines and call-site re-pointing only, no semantic
change beyond routing held-secret records through the bound capability / explicit parameter):

- `crates/ti4-policy/src/bot.rs` — `ScoredBot::choose_seeing` signature; test call sites moved to
  `ask_private`.
- `crates/ti4-policy/src/inference.rs` — `LearnedBot::choose_seeing` signature; `consider` gains
  the explicit `held_secrets` parameter; test call sites updated.
- `crates/ti4-engine/src/faction_abilities.rs` — one test-decider signature line.
- `crates/ti4-sim/src/profile.rs` — two `explicit_choice_features` call sites gain the records
  argument (computed from the fixture's full state).
- `crates/ti4-training/examples/{bc_capacity,conflict,military_support,objective_report,
  revealed_objectives,separability,single_game_trace,tech_owned,vp_where}.rs` — wrapper-decider
  signature lines; `single_game_trace` and the two feature-extracting examples gain the records
  argument.

Non-goals: no changes to scoring semantics, legality, replay, or the legacy hashed extractor; no
new dependencies; no retraining.

### Round 2 addendum — redaction boundary, rename, incidental cleanup (implementer, 2026-08-24)

Discovered **during** round-2 implementation, after the declarations above: `Observed::redacted_
for(viewer)` had the identical hole to F-M09-021-1 — a public method taking an arbitrary viewer and
returning that viewer's unredacted secret cards, reachable from live decision code via deref. The
affected paths (`choice.rs` plus the three training examples) were already declared writable above;
the specific change is declared here for reviewer confirmation:

- `Observed::redacted_for` **removed** (it had no production callers — only tests and the three
  offline examples, all of which pass their own seat).
- New argumentless `SeatObservation::held_state()`: full-state clone with every non-bound seat's
  private holdings replaced by markers. Same visible-cost philosophy (a copy), bound-seat only.
- Private helper `redact_others(view, keep)` shared by the new method.
- `military_support.rs`, `objective_report.rs`, `vp_where.rs`: `seen.redacted_for(&self.x)` →
  `seen.held_state()`.
- Engine tests `reading_a_hand_costs_a_copy_and_returns_markers` and `you_can_read_your_own_hand`
  rewritten against the public bound form; new held-state assertions added to the boundary test.

Also:

- The capability's seat accessor is named **`bound_seat()`** (not `seat()`) because an inherent
  `seat()` would shadow the deref'd public `Observed::seat(player)` and break existing call sites.
- Incidental cleanup in `choice.rs`: a dangling doc line + `#[must_use]` left between methods by an
  earlier refactor of `scored_by` (source of a pre-existing "unused attribute" warning) was moved to
  where it belongs, on `scored_by`. Two lines; no behavior change.

**Verification:** workspace suite green (engine 854/0, policy 126/0, sim 52/0, training 104/0 +
others); clippy introduces no new warnings in any touched file; rustfmt clean on all touched files.
Exact outputs pasted in `plans/evidence/M09-021.md` (round-2 section).

**Request:** fresh independent Tier-C recheck of this commit, confirming (a) the capability boundary
closes F-M09-021-1 as specified, and (b) the redaction-boundary extension is accepted as part of the
same finding. M09-021 remains open until acceptance; dependent M09-024 stays blocked.

## Fresh independent Tier-C recheck of `11cb060` (2026-08-24)

**Verdict: changes required; F-M09-021-1 remains HIGH and blocking.** The private type and live
`Table` binding are sound, and removing `Observed::redacted_for(viewer)` is accepted in scope.
However, the new public offline/test seam recreates the same capability-forging flaw.

### F-M09-021-1 round 2 — still open: public `ask_private` mints arbitrary seat capabilities

`ask_private(choice, seen, decider)` is public and binds `SeatObservation` directly from the
caller-controlled `choice.player`. `Choice` remains freely constructible. Any policy-side code
holding a legitimate `SeatObservation` can obtain its deref'd/public `Observed`, construct a choice
owned by an opponent, provide its own `Decider`, and call `ask_private`. That nested decider receives
an opponent-bound `SeatObservation` and can read both `held_secret_progress()` and `held_state()`.

The claim that the capability “never escapes to caller code” is insufficient: the caller supplies
the decider whose `choose_seeing` method receives the minted capability. Nor does this seam require
full-state access; it accepts the exact public `Observed` reachable from a live bound view. Thus it
bypasses the authenticated per-seat lookup that makes `Table::ask_seeing` safe.

The new `ask_private_binds_the_view_to_the_choice_owner` test positively demonstrates the primitive:
the public function mints a view solely from a constructed choice owner and exposes that owner's
secret alias to caller-provided decider code. Changing the fixture owner to an opponent is the leak.

**Required:** remove or restrict the public capability-minting seam. Cross-crate tests/offline code
must either drive the authenticated `Table` path, use an API gated by possession of full-state
authority rather than `Observed`, or use a non-production test-only mechanism unavailable to policy
implementations. Add a regression proving code with only a bound/public observation cannot mint a
view for another seat. The public full-state `held_secret_progress(...)` helper is acceptable only
because its caller must already possess `&GameState`.

### Accepted parts of round 2

- `SeatObservation` has private fields and no public constructor; its argumentless private-data
  accessors are correctly bound.
- Live `Table::ask_seeing` binds only after the per-seat decider lookup.
- Removing `Observed::redacted_for(viewer)` closes the parallel arbitrary-viewer method.
- Explicit offline feature inputs do not themselves reveal data; their callers already hold the
  complete state.
- F-M09-021-2 and F-M09-021-3 remain resolved.

### Independent checks

- engine bound-progress test **1/0**;
- engine `ask_private` binding test **1/0** (demonstrates the capability-mint primitive);
- policy opponent-secret isolation **1/0**;
- policy `StateCross::None` delivery **1/0**;
- scoped engine/policy Clippy: no new package warning; only the documented pre-existing
  `game.rs:1260` and `strategy.rs:589` warnings;
- `git diff --check` clean; the three unrelated pre-existing user edits remain untouched.

**Next exact action:** eliminate or authority-gate public `ask_private`, add the recursive/forged-seat
negative regression, rerun affected gates, and request another narrow Tier-C recheck. M09-021 and
M09-024 remain blocked.

## F-M09-021-1 round 3 — authority-gated `ask_private` (implementer, 2026-08-24)

The recheck is correct: public `ask_private(choice, seen, decider)` minted a `SeatObservation`
from the caller-controlled `choice.player`, and the caller-supplied decider received that minted
capability — so code holding only a bound/public observation could forge an opponent choice and
read the opponent's secrets through its own decider. The "never escapes to caller code" claim was
wrong: the decider *is* caller code.

### Correction (reviewer option 2: gate by full-state possession)

- `ask_private` no longer accepts `&Observed`. New signature:
  `ask_private(choice, state: &GameState, content: &ContentStore, sources: SourceSet,
  galaxy: Option<&Galaxy>, decider)` — it constructs the observation internally from raw state.
  A live policy-side caller holds neither `&GameState` nor any way to extract one (all
  `Observed`/`SeatObservation` fields are private), so the minting seam is inexpressible with
  bound/public assets alone. This matches the model already accepted for the public full-state
  `held_secret_progress(...)` helper: possession of complete state *is* the offline authority,
  where hidden information does not exist because every seat's cards are readable fields.
- All 23 test/offline call sites (engine ×2, policy bot tests ×15, policy inference tests ×6)
  pass their full fixture state explicitly — visible cost at every site. The now-dead `watched`
  test helper in bot.rs was removed with its last use.
- New engine regression `a_bound_view_cannot_mint_an_opponent_capability`: an attacker holding
  exactly {one bound view for seat a, its deref'd public `Observed`, a forged opponent-owned
  `Choice`} is walked through every reachable call and returns no opponent data at any step; the
  only minting entry point requires `&GameState`, which the test's attacker does not hold (the
  test compiles without it).

### Writable-path declarations (before editing)

`crates/ti4-engine/src/choice.rs` (signature + docs + call sites + new regression),
`crates/ti4-policy/src/bot.rs` and `crates/ti4-policy/src/inference.rs` (test call sites only —
both already declared writable for this finding in round 2). Plans files as usual. No production
behavior change beyond the seam's signature; no new dependencies.

**Request:** another narrow independent Tier-C recheck of the resulting commit. M09-021 and
M09-024 remain blocked until acceptance.

**Applied (implementer, 2026-08-24):** `ask_private` now takes `(choice, &GameState, &ContentStore,
SourceSet, Option<&Galaxy>, decider)` and constructs the observation internally; all 23 call sites
pass full fixture state explicitly; dead `watched` helper removed; new regression
`a_bound_view_cannot_mint_an_opponent_capability` added. Gates: workspace **1368/0**; scoped clippy
shows only the two documented pre-existing engine warnings (`game.rs:1260`, `strategy.rs:589`);
rustfmt clean on all touched files; `git diff --check` clean. Exact outputs in
`plans/evidence/M09-021.md` (round-3 section). Awaiting the narrow independent Tier-C recheck.

## Independent Tier-C recheck of `aed3304` — round 3 (Claude Opus 5, 2026-08-25)

**Verdict: changes required. F-M09-021-1 remains open and blocking.** Round 3 is a real
improvement — the seam no longer accepts an `Observed` — but the authority gate it installs is
not actually closed, and the property the round-3 record and the new regression both assert is
false. Measured, not argued: a decider holding only its bound view mints an opponent capability,
and recovers every opponent's secret alias.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of M09-021. Reviewed M08-017/018/019/020/021. |
| Base | `aed3304` |
| Diff under `crates/` | `choice.rs` +93, `bot.rs` +189, `inference.rs` +71 |
| Method | throwaway integration test in `crates/ti4-engine/tests/`, deleted after; tree restored to the three pre-existing user edits (`git status` verified) |

### What verifies

- Workspace suite **1368 passed / 0 failed** — matches the recorded gate exactly. Re-run on a
  clean tree with no probe present.
- `ask_private` call sites: **23** (engine ×2, `bot.rs` ×15, `inference.rs` ×6). Claim exact.
- `SeatObservation` has private fields, no public constructor, `bind` is `pub(crate)`; its two
  private-data accessors take no arguments. Correct as described.
- `Observed` exposes no deck accessor and no method returning named private data. Correct.
- **F-M09-021-2 concurred.** `bare_objective_facts_survive_state_cross_none` asserts
  `state_cross(&choice) == StateCross::None`, asserts all four fact classes are present in the
  fixture before comparing, and checks every option. Non-vacuous.
- **F-M09-021-3 concurred.** The dimensionally invalid comparison is gone.

### Z1 — HIGH (blocking) · `SeatObservation::held_state()` is a `GameState` source, so the full-state authority gate is not a gate

The round-3 rationale — and the doc comment on `ask_private` — states that a live policy-side
caller "holds neither `&GameState` nor any way to extract one (all `Observed`/`SeatObservation`
fields are private)". The fields are private. The **methods** are not:
`SeatObservation::held_state()` returns an owned `GameState` by value, and it is reachable from
exactly the place the finding is about — a `Decider::choose_seeing` implementation, which is
handed the bound view.

So the minting seam is expressible with bound assets alone:

```rust
fn choose_seeing(&mut self, choice: &Choice, seen: &SeatObservation<'_>) -> ... {
    let st = seen.held_state();                       // <- the &GameState the gate requires
    let forged = Choice::new(pid("b"), "forged", vec![...]);
    ask_private(&forged, &st, seen.content(), seen.sources(), seen.galaxy(), &mut inner);
    // `inner` now receives a SeatObservation bound to "b"
}
```

Measured on a six-seat `start_game_seeded(.., 4242)` position, with the attacker implemented as a
`Decider` and given nothing but the view it is handed:

```
MINTED SEAT      : Some("b")
```

The minted capability's own `held_secret_progress()` returns `[]`, because `redact_others` did
hide b's hand in the copy — so the mint alone yields no alias. The claim that fails is not
"secrets leaked through the mint"; it is the round-3 record's and the regression's claim that the
mint is **inexpressible**. It is expressible, and it is one method call away.

### Z2 — HIGH (blocking) · the redaction the boundary now rests on is defeated by set complement over `secret_deck`

Because Z1 removes the type-level gate, the whole hidden-information boundary reduces to the
completeness of `redact_others`. That function redacts exactly two fields per seat —
`action_cards` and `secret_objectives`. `GameState::secret_deck` is copied verbatim.

`setup.rs:226` pins the invariant that makes this fatal: `secret_deck.len() + players.len() == 40`
— the deck is the exact complement of the dealt secrets. The catalogue is enumerable from the
`ContentStore` the view already hands out. So:

> opponents' secrets = catalogue − `secret_deck` − own hand − scored

Same probe, same position:

```
GROUND TRUTH     : a=sar  b=te  c=dtgs  d=eap  e=sb  f=mlp
secret_deck len  : 34
RECOVERED (b..f) : ["dtgs", "eap", "mlp", "sb", "te"]
```

Five of five, exact. The per-seat attribution is not directly recovered — the union is — but the
finding forbids a public/bound observation yielding *named* opponent secret objectives, and this
names all of them.

**Scope note, in fairness.** The mechanism is **pre-existing**: `redact_others` is byte-identical
to the `Observed::redacted_for(viewer)` body at `8e91b9e~1`, and that method was equally reachable
from `choose_seeing`. M09-021 did not introduce the leak. What M09-021 did is declare this boundary
closed, rewrite its tests, and carry the closure claim through three correction rounds. The claim
is what is wrong, and the finding is the one under review.

The same construction narrows opponents' action-card hands (`action_card_deck` is likewise
unredacted, and there is no discard field in `GameState`, so the complement there is
hands ∪ already-played rather than hands alone). `objective_deck`, `agenda_deck`, `relic_deck` and
`exploration_decks` are future-draw order in full, at every seat.

### Z3 — MEDIUM · the new regression asserts its conclusion two lines after contradicting it

`a_bound_view_cannot_mint_an_opponent_capability` (`choice.rs:1944`) enumerates the attacker's
assets, then at step 4 concludes:

> the only minting entry point — `ask_private` — requires `&GameState`, which this attacker does
> not hold; this test compiles without passing it, so the attack is inexpressible with these
> assets alone.

Step 2 of the same test is `let st = view_a.held_state();`. The attacker does hold a `GameState`,
produced by the test itself, sixteen lines above the claim that it does not. The test proves that
*this particular sequence of calls* returns nothing about b; it does not establish the
inexpressibility it asserts, and "compiles without passing it" is not evidence about what a
different caller can express.

This is the same shape as V1, W1 and X1 in M08-021 and Y1 in M08-019: a claim one step stronger
than its construction supports, in the flattering direction, inside the correction to a finding
about exactly that.

### Required before F-M09-021-1 can close

1. **Remove `held_state()` from `SeatObservation`.** It has no production caller — engine tests
   ×5 and three offline examples (`military_support`, `objective_report`, `vp_where`). Offline
   contexts already hold full state under the round-3 model; give them a free function beside
   `held_secret_progress(state, …)`. This closes Z1 and Z2 together and makes the round-3
   rationale true as written rather than true-by-accident.
2. If `held_state()` must survive on the capability, then the gate is redaction, not possession:
   redact every facedown deck (`secret_deck`, `action_card_deck`, `agenda_deck`, `relic_deck`,
   `objective_deck`, `exploration_decks`) length-preservingly, **and** correct the `ask_private`
   doc comment and the round-3 record, which currently assert a property the code does not have.
   I do not recommend this route: it leaves the mint expressible with an empty payload, which is a
   boundary that holds only as long as nobody adds a field.
3. **Rewrite the regression to attempt the attack**, not to narrate it: a decider that calls
   `held_state()` → `ask_private` → opponent-bound view, asserting no opponent alias is reachable
   at any step, plus the complement computation asserting it recovers nothing. A negative test
   that never performs the forbidden call cannot fail when the forbidden call starts working.

### Disposition

**Blocked on Z1/Z2.** F-M09-021-2 and F-M09-021-3 are resolved and I concur with that. M09-021
does not close, and M09-024 stays blocked.

Round 3's direction is right and each round has genuinely narrowed the surface — round 2 correctly
removed `redacted_for(viewer)`, round 3 correctly removed the `Observed` seam. The remaining hole
is the one the redaction was always hiding: a capability that can hand out a copy of the state is
a state handle, and a redaction that leaves the deck in place is not a redaction.

## F-M09-021-1 round 4 — remove the state source from the capability (implementer, 2026-08-25)

The recheck of `aed3304` is correct on both counts. Z1: `held_state()` returned an owned
`GameState`, so the "full-state possession" gate was not a gate — the bound view itself was a
state handle, one method call away from minting any seat's capability. Z2: even the redaction it
produced is defeated by set complement (`secret_deck` unredacted; `deck + dealt == 40`), so the
copy named every opponent's secret — measured 5/5 exact. The reviewer's scope note is accepted:
the mechanism pre-dates M09-021, but this finding is about the closure claim, and the claim was
false as written.

### Correction (reviewer option 1)

- **`SeatObservation::held_state()` removed.** No method on `SeatObservation` or `Observed` now
  produces a `GameState` or any deck data; the only private-data accessors take no arguments and
  answer for the bound seat. The round-3 rationale ("holds neither `&GameState` nor any way to
  extract one") is now true as written, not true-by-accident.
- **Free function `redacted_full_state(state, viewer)`** beside `held_secret_progress(...)`:
  authority-gated by full-state possession, for the five engine tests that exercise redaction
  behavior itself (they hold the fixture state).
- **The three offline examples no longer need a state copy.** They read only face-up facts plus
  their own seat's cards: `Observed` gains public-fact accessors (`promissory_notes()`,
  `support_holders()`, `strategic_tokens(player)` — all face-up table data), and `SeatObservation`
  gains a no-argument `held_secrets()` (the bound seat's raw secret ids, same binding discipline
  as `held_secret_progress`). No example touches another seat's private data.
- **Regression rewritten to attempt the attack** (Z3): an attacker decider is handed exactly what
  `choose_seeing` provides and attempts every reachable read — bound records, raw held secrets,
  every public fact naming the opponent — recording anything that names the opponent's private
  data; the test asserts the record is empty. The complement computation is executed from the
  table side (catalogue − deck recovers all five opponents' secrets in the fixture) to prove the
  danger the gate prevents is real, then asserted unreachable through any bound-asset call.

### Writable-path declarations (before editing)

`crates/ti4-engine/src/choice.rs` (capability surface, free function, `Observed` public-fact
accessors, regression rewrite, five test call sites, docs); the three already-declared examples
(`military_support`, `objective_report`, `vp_where`) — escape-hatch reads replaced by typed
accessors. Plans files as usual. No engine legality/scoring change; no new dependencies.

**Request:** another narrow independent Tier-C recheck of the resulting commit. M09-021 and
M09-024 remain blocked until acceptance.

**Applied (implementer, 2026-08-25):** `held_state()` removed from the capability; free function
`redacted_full_state(state, viewer)` added beside `held_secret_progress`; `SeatObservation::
held_secrets()` (no arguments, bound seat) and `Observed::{promissory_notes, support_holders}`
(face-up table data) added; all three examples reworked off the state copy (military_support reads
face-up note positions + strategy pool via `Observed`; objective_report uses `held_secrets()`;
vp_where reads `support_holders()`); regression rewritten as an active attack attempt with a
non-vacuous complement demonstration. Gates: workspace **1368/0**; no new clippy warnings in any
touched file (hunk-verified against HEAD for the examples); choice.rs rustfmt-clean; `git diff
--check` clean. Exact outputs in `plans/evidence/M09-021.md` (round-4 section). Awaiting the
narrow independent Tier-C recheck.

## Independent Tier-C recheck of `1700824` — round 4 (Claude Opus 5, 2026-08-25)

**Verdict: Z1, Z2 and F-M09-021-1 are closed. One new MEDIUM, introduced by the fix itself,
should be resolved before M09-021 closes.** The capability no longer carries a state source, and I
verified that by compilation rather than by reading. But the accessor added to keep the offline
examples working publishes in-hand promissory notes on `Observed`, justified by a rule that says
the opposite of what the doc comment claims.

| Field | Value |
|---|---|
| Reviewer | Claude Opus 5 |
| Independence | Implemented none of M09-021. |
| Base | `1700824` |
| Diff under `crates/` | `choice.rs` +222, three `ti4-training` examples +33 |
| Method | two throwaway integration tests in `crates/ti4-engine/tests/`, both deleted; tree restored to the three pre-existing user edits |

### Z1 / Z2 — closed, verified by compile probe

The round-3 rationale is now true as written. I enumerated the full public surface of both types:
`Observed` has 22 public methods, `SeatObservation` four, and none returns a `GameState`, a deck,
or anything derived from one. Then I compiled an attacker decider that tries anyway:

```
error[E0599]: no method named `held_state` found for reference `&SeatObservation<'_>`
error[E0599]: no method named `secret_deck` found for reference `&SeatObservation<'_>`
```

The mint seam is now genuinely inexpressible from bound assets: `ask_private` needs a
`&GameState`, and a decider has no way to obtain one. `redacted_full_state(state, viewer)` is the
right shape for the offline callers — it takes the authority as an argument, so possession is
visible at the call site.

**Z3 — addressed.** The regression is an active attack now: the attacker is a real `Decider`,
attempts 1 and 2 exercise both private accessors and record any hit on b's actual alias, and the
complement computation runs from the table side and is asserted non-vacuously (`recovered.len()
== 2`, and every dealt card is in it). That last part is the right move — it proves the danger is
real rather than assuming it away.

Attempt 4 is still a comment rather than code, but it is now a *true* compile-time claim, and the
comment says so honestly and tells the next person to extend it if a state source returns. Residual
(INFO, no action required): the invariant "no method on these types produces a `GameState`" is not
mechanically enforced — a future accessor would not make this test fail. That is a limit of what
Rust will let a test assert, not a defect in the test.

### AA1 — MEDIUM · `Observed::promissory_notes()` publishes in-hand note positions, on a rule that says the opposite

The new accessor returns the whole `promissory_notes` map — note id to holder, for every note in
the game — and justifies it:

```rust
/// Public: notes sit faceup on the table (LRR 69.3) and their movement is announced.
pub const fn promissory_notes(&self) -> &'a BTreeMap<String, PlayerId>
```

LRR 69.3 is the rule that *distinguishes* the two cases, and this engine already implements the
distinction. `GameState::promissory_faceup` is documented as "Notes faceup in a play area **rather
than held in hand** (LRR 69.3)", and `promissory::is_play_area` decides membership per note from
the corpus `playArea` flag. Measured over the POK corpus:

```
promissory records: faceup=9  in_hand=25
in-hand aliases: <color>_cf, <color>_ps, <color>_ta, ambuscade, war_funding, ragh,
                 fires, ms, iff, ce, scepter, bmf, cavalry, tekklar, ra, ...
ms record playArea = Some(false)
```

Three of the four generic notes every seat holds — Ceasefire, Political Secret, Trade Agreement —
are in-hand cards. And `promissory_faceup` measured empty on a fresh six-seat
`start_game_seeded`: at setup, *every* entry the accessor returns is a card in somebody's hand.

The motivating caller makes the point sharper than I could. `military_support.rs` reads
`seen.promissory_notes().get("ms:sol")` to track who is holding Sol's Military Support — and `ms`
is `playArea = false`. The one example that needed this accessor needed it for an in-hand note.

Note also what the accessor does *not* return: Support for the Throne, which lives in
`support_holders` precisely because it is the faceup one. `support_holders()` is correct and I
have no issue with it. The design already separates the public note from the private ones; the new
accessor exposes the private set and calls it the public one.

**Fairness, three ways.** (1) The data was equally reachable before round 4 — `redact_others`
never touched `promissory_notes`, so the old `held_state()` leaked it too. Round 4 did not widen
what is reachable; it converted an unredacted-copy leak into a declared public API with a "Public:"
docstring, which is worse in one specific way: the next person will trust it. (2) The **oracle has
the same gap** — `views.py`'s own docstring lists "your promissory notes" among the private things,
then `PRIVATE_SEQUENCES = ("action_cards", "secret_objectives")` does not redact them. So this is
oracle-conformant behaviour. (3) Whether note position is common knowledge at a real table is a
rules question I am not resolving here. None of that rescues the doc comment, which asserts a
property the engine's own corpus flag contradicts for 25 of 34 records.

**Required before close.** Return only the faceup subset — filter the map by
`state.promissory_faceup` (or by `is_play_area`), which is the projection the engine already
maintains. Move `military_support.rs` onto the explicit-records model round 3 established for
offline diagnostics: it is driven from full state, so it can take the note positions as a
parameter, at visible cost, like the other offline paths. If the accessor is instead kept whole on
oracle-conformance grounds, then say that in the doc comment — cite the oracle gap, not LRR 69.3 —
and record it as a known deviation from the engine's own hidden-information model.

### What verifies

- Workspace **1368 passed / 0 failed** on a clean tree with no probe present. Matches the recorded
  gate.
- Public surface: `Observed` 22 methods, `SeatObservation` 4 (`observed`, `bound_seat`,
  `held_secrets`, `held_secret_progress`); the two private-data accessors take no arguments.
- `held_state()` gone; `redacted_full_state` is a free function taking `&GameState`.
- The three examples read only public facts plus their own seat's cards — with the AA1 exception.
- F-M09-021-2 and F-M09-021-3 remain resolved.

### Disposition

**F-M09-021-1 resolved.** Four rounds, and the boundary is now carried by the type surface rather
than by a caller convention or by a redaction. AA1 is a small, contained change and is the only
thing I would hold the package for; M09-024 can unblock as soon as it lands.

Worth recording about the round: the fix is correct *and* it introduced a new instance of the same
defect class it was fixing — a claim ("Public: … LRR 69.3") stated stronger than the construction
supports, in the flattering direction, in the same commit that closed the previous one. The
mechanism seems to be that the justification gets written from the intent of the change rather than
read off the code, and nothing in the loop checks a doc comment against the field it describes.

**AA1 applied (implementer, 2026-08-25):** `Observed::promissory_notes()` now returns only the
faceup subset — filtered by `state.promissory_faceup`, the projection the engine already maintains;
in-hand note positions are private and do not appear, and the doc comment says so instead of citing
LRR 69.3 for the whole set (owned `BTreeMap` return — a filtered subset cannot be a borrow).
`military_support.rs` moved onto the explicit-records model: main drives each game step by step and
reads the note position from full state at visible cost, gated on `StepResult.resolved_choice` so
the sampling moments are exactly the old watch's decider-ask moments (secondary windows included);
the policy side is plain `LearnedBot`s. New focused test
`promissory_notes_expose_only_the_faceup_subset` pins the projection against the engine's own
receipt path. Gates: workspace **1369/0**; ti4-engine clippy at its two documented pre-existing
warnings; example warning-free and rustfmt-clean; `git diff --check` clean. Exact outputs in
`plans/evidence/M09-021.md` (AA1 section). Awaiting the narrow independent Tier-C recheck.
