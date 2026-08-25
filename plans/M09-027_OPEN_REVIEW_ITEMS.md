# M09-027 open review items

## Independent Tier-C review of `ab9c896` (2026-08-26) — changes required

Reviewer: Codex frontier model, independent of the Claude Opus 5 implementation.

Reviewed `docs/MLP_PLAN.md` revision 5 §§4.1–4.2, the package diff, extractor and
value APIs, invariance tests, evidence, and the existing typed hidden-information
boundary in `SeatObservation`.

Independent gates:

```
cargo test -p ti4-policy --lib critic             5 passed, 0 failed
cargo test -p ti4-mlp --test critic_invariance    2 passed, 0 failed
cargo clippy -p ti4-policy --lib                  exit 0; one pre-existing engine warning
cargo clippy -p ti4-mlp --test critic_invariance exit 0; no warning in touched files
rustfmt --edition 2024 --check <touched files>    clean
git diff aed3304..ab9c896 --check                 clean
```

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-027-1 | **HIGH** | The critic's private input is enforced by caller convention, not the typed boundary. `critic_facts`/`critic_vector` accept a public `Observed`, an arbitrary `PlayerId`, and arbitrary caller-supplied `held_secrets`. This recreates the exact boundary that `SeatObservation` was introduced to remove: a caller can request any seat's public position and attach any seat's unredacted secret progress. The documentation's claim that the records are engine-bound is not expressed by the API. | Take `&SeatObservation` and derive both the acting player and held-secret progress from that capability. Do not accept either as caller arguments. Add an API regression proving an `Observed` value cannot produce a full critic and a runtime regression proving two bound seats receive only their own secret progress. |
| F-M09-027-2 | **HIGH** | `Actor::value` accepts the public, generic `SparseOption`. A caller can pass the sparse vector for a policy option—including option/legal-set-derived columns—directly into the value head. Thus the statement that the value function “has no way to see the legal set” is false at the actual inference API. The invariance test does not exercise a production boundary: it constructs each `Choice`, discards it with `let _ = choice`, and repeats the identical extractor call. | Introduce a critic-specific input type that cannot be constructed from option vectors through the public API, and make `Actor::value` accept only that type. Exercise the real extraction-to-value path while changing legal-set order and contents; add an API/compile-fail regression preventing `SparseOption` from being supplied as critic input. |
| F-M09-027-3 | **HIGH** | The submitted accepted vocabulary maps every `critic-state:*` fact to the single global OOV column. The evidence correctly measures that all semantic identities collapse, but then calls M09-027 implemented and moves the missing acceptance behavior to 027b. Under the package split rule, the original acceptance criterion must be preserved across the children; the parent cannot be complete while its value input is only a rank-1 sum over one shared row. | Keep M09-027 open through M09-027b. Implement the registry/projection/corpus migration, regenerate the accepted generation, and rerun the value/invariance tests against that accepted vocabulary with a non-vacuity assertion that distinct critic facts occupy distinct intended/reserved columns. |

The separate namespace, deterministic ordering, gated objective/ability groups, shared trunk/value
readout, and focused numerical checks are otherwise sound. No source correction was made by the
reviewer.

**Verdict: changes required.** M09-027 and M09-027b remain open; M09-028 remains blocked.

---

## Correction round 1 (implementer, 2026-08-26) — all three findings accepted

Base `ab9c896`; corrections in `f52f00c` (F-1, F-2) and this commit (F-3).
**No finding was disputed.** All three were right, and the third one's framing was right too.

### F-M09-027-1 — the capability, not the convention

Accepted in full. `critic_facts`/`critic_vector` took a public `Observed`, an arbitrary `PlayerId`,
and a caller-supplied `held_secrets` slice — which is precisely the hole `SeatObservation` was built
to close, reopened one package later by the person who had just watched it close. The documentation
said the records were engine-bound. Only the signature says anything.

Both entry points now take `&SeatObservation` and read the seat and its secret progress off the
capability. The in-crate tests obtain one the way production does, through `ask_private` — they
cannot mint it, because `bind` is `pub(crate)` to `ti4-engine`, and that is the point.

`two_bound_seats_receive_only_their_own_secret_progress` is the runtime regression. Seat `a` holds
`ans` (scored off its own technologies), seat `b` holds `dp` (scored off the laws in play). `dp` is
the sharper case: laws are *global* state that `a` can read, but the card is `b`'s, so a `laws`
progress family appearing in `a`'s critic means `b`'s hand leaked. Ground truth comes from the
offline `held_secret_progress`, which a test may call because it holds the whole state anyway.

**Falsified** by making `SeatObservation::held_secret_progress` answer for a different seat: the
test fails. `choice.rs` restored byte-identically (`c5ddd05ff6713031…` both sides).

### F-M09-027-2 — the value head's input type, and a test that could not fail

Accepted in full, including the part about the test, which is the more serious half.

`Actor::value` took the public `SparseOption`. So the claim that the value function "has no way to
see the legal set" was true of the *extractor* and I wrote it about the *model* — one step further
than the construction supported, in the flattering direction, which is the same error this milestone
keeps producing.

`CriticInput` is now the only accepted type: a private field, and one constructor taking a
`CriticVector`, which only the option-free extractor produces. The chain is
`SeatObservation -> CriticVector -> CriticInput -> Actor::value`, typed end to end.

The API regression is a doc-test pinned to **`compile_fail,E0308`** rather than a bare
`compile_fail`, which would pass on any compile error at all — a typo included. A normal doc-test
sits beside it running the identical setup lines, so the refusal is demonstrably about the argument
type and not about the fixture.

And the invariance tests: the reviewer is right that they never touched a production boundary. They
built a `Choice`, discarded it with `let _ = choice`, and called the extractor three times **with
identical arguments**, asserting the results agreed. That is `f(x) == f(x)` — a determinism check
wearing an invariance test's name, which no implementation of the property could ever fail. All
three now run through `ask_private` -> `choose_seeing` with the legal set genuinely varied, and
non-vacuity is asserted on the inputs (`assert_ne!` on the per-option column sets) rather than on
the outputs.

### F-M09-027-3 — the parent stays open, and the acceptance criterion is preserved

Accepted. Splitting a row does not let the parent bank the easy half: "value inference" whose value
is a rank-1 sum over one shared row is not the criterion the row states. M09-027 stayed open and is
submitted together with M09-027b.

**Registry v3.** `critic-state` appended to `OOV_FAMILIES_V2` — appended, not sorted in, per
M09-024a's discipline — with `OOV_FAMILIES_V3_FINGERPRINT` pinned separately and the three digests
asserted distinct. `OOV_REGISTRY_VERSION` is 3.

V2's doc says that once an artifact is published, growing the reserved block is a full reviewed
tensor/layout migration. An artifact *is* published, so that sentence applies and the reason this is
still only a regeneration is written into the code rather than assumed: no trained tensor is
addressed by these columns (the bundle is M09-028 and does not exist; the r6 checkpoint is keyed by
feature **name**), and the one thing the shift invalidates is the generation itself, which is
republished. After M09-028 writes a bundle, neither holds and the route closes.

**Admission.** `critic-state` classified `Transferable` in `FAMILY_ROLES`. The closed default and
`the_classification_covers_exactly_the_registry` forced the decision rather than letting it happen
as a side effect — that gate worked exactly as designed.

**Discovery.** The corpus `Collector` already held the bound capability, so it now collects
`critic_vector` names on the same decisions, with every group enabled: a column that exists and goes
unused costs one slot, a name with no column is the defect.

**Regenerated.** 768/768 games, registry v3, `oov_count` 41.

| | before | after |
|---|---|---|
| generation | `14c19387…8479` | **`8805cfdd…9295`** |
| slots | 10,997 | **11,118** (+121 critic names) |
| `V_cap` | 16,384 | 16,384 |
| `critic-state:round` | col 0, `assigned=false` | **col 571, `assigned=true`** |

**Three gates, each falsified.**

1. *Publication.* `vocabulary_discovery` refuses before publishing if fewer than 60 critic names are
   discovered, if any is unassigned, or if distinct facts share columns. Falsified by disabling
   critic collection and running the real campaign: `REFUSED: discovery found only 0 critic-state
   names`, exit 2, no artifact written, accepted pointer untouched. Source restored byte-identically.
2. *Accepted artifact.* The smoke now runs the extractor over the position the game reached, through
   `ask_private`, against the generation it loaded: `critic: 38 facts over 38 distinct columns`.
   Falsified by pointing the probe at a stranger vocabulary: `REFUSED: the critic collapsed onto 1
   column(s): 38 facts, so V is a rank-1 projection` — the exact state the previous generation was
   in. Source restored byte-identically.
3. *Hermetic.* `the_critic_input_reports_how_many_columns_it_actually_occupies` asserts both
   directions in-tree, so a fresh checkout has the property without the artifact.

The smoke's `V = 0.000000` is labelled `(zero actor, plumbing only)` in the output. That actor is
zero-initialised like the seats', so the number carries no information and the finiteness check
proves only that the gather, trunk and readout complete over real columns. The column count is the
measurement.

### Gates

```
cargo test --workspace                          1482 passed, 0 failed
cargo test -p ti4-policy --lib                   180 passed, 0 failed
cargo test -p ti4-mlp (lib + 3 integration)       23 + 3 + 2 + 2 passed, 0 failed
cargo test -p ti4-mlp --doc                        2 passed (1 compile_fail E0308, 1 control)
clippy, touched files                              0 warnings
rustfmt --edition 2024 --check                     clean on touched files
vocabulary_discovery                               768/768 games, 11,118 slots, published 8805cfdd…
release smoke                                      exit 0, 409 decisions, 0 fallbacks, 38/38 critic columns
```

`cargo fmt --all --check` still reports `crates/ti4-content/src/galaxy.rs`; pre-existing, verified by
stashing, untouched here.

**Requesting recheck of M09-027 and M09-027b together.**
