# M10-031 through M10-034 open review items

## Independent Tier-C frontier review through `7abcfdb` (2026-08-26) — changes required

Reviewer: Codex, independent of the Claude Opus 5 implementation.

Scope: teacher-corpus capture/reading, factual distillation driver, critic warm-up, PPO core and
their evidence. This review deliberately re-examined every claimed gate for a falsifying input or
an assertion that the measured operation actually occurred. It does not accept the packages'
unmet dependency on M09-030; see `M09-029_030_OPEN_REVIEW_ITEMS.md`.

Focused gates reproduced during review:

```
cargo test -p ti4-training --lib teacher_corpus   8 passed, 0 failed
cargo test -p ti4-mlp                            51 lib + 3 + 3 + 2 + 2 doc passed
cargo clippy -p ti4-mlp --all-targets             exit 0; two new ppo.rs cast warnings
git diff --check                                  clean before review documentation
```

Passing focused tests do not close the findings below: the missing cases sit at artifact,
fail-closed, and actual-update boundaries those tests do not exercise.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M10-031-R1 | **HIGH** | `teacher_corpus::capture` is public and writes directly into its destination with `File::create`, truncating shard files before writing the manifest last. The example refuses an existing manifest, but the library—the artifact boundary—does not. A caller can overwrite an accepted corpus in place and leave the old manifest beside partial/new shard bytes if capture fails. “Manifest last” detects incompleteness only when the destination was initially empty. | Enforce immutability inside the library. Generate into a new sibling staging directory, fully close and verify every shard and the manifest there, then publish once through an atomic directory/pointer transition. Refuse an existing generation before opening any payload. Add interruption and existing-destination regressions proving accepted bytes remain unchanged. |
| F-M10-031-R2 | **HIGH** | `verified_text` verifies the requested shard checksum but does not validate the corpus contract: supported schema, exact 768 games/four rounds, seed clusters, temperature, expected teacher/pool/vocabulary identities, decision counts, or a closed shard set. The distillation driver separately checks only the vocabulary digest. A checksum-valid corpus captured from the wrong teacher, pool, schedule, or partial manifest can therefore train as M10-031. | Add one typed, fail-closed manifest validator covering the complete §6.1 contract and expected accepted input identities. Make every consumer validate the whole manifest and closed file set before reading records. Add wrong-teacher, wrong-pool, wrong-schedule/count, unsupported-schema, extra/missing-shard, and manifest-mutation regressions. |
| F-M10-032-R1 | **HIGH** | `distill` drops every record whose faction/head cannot compile, prints only a warning, and continues whenever both splits remain nonempty. The evidence calls drops “reported and bounded,” but no bound is enforced. A head migration or corrupted corpus can silently discard an arbitrary fraction—including one whole head—and still produce a model and attractive aggregate KL. | Require exactly zero dropped records for this fixed corpus (or a separately reviewed exact bound, identity, and reason). Validate before training and refuse publication. Report per-head/per-faction input and compiled counts, and falsify with an unknown head/faction record. |
| F-M10-032-R2 | **BLOCKER** | M10-032 has not met its package exit. Its evidence still has an empty Results section. No retained base-pass verdict, per-head gates, paired gameplay gate, selected checkpoint, or bounded DAgger disposition is recorded. Implementation of a distillation loop is not completion of §6.1, whose gates must pass before PPO begins. | Complete and retain the predeclared base run and all imitation/gameplay gates against a validated corpus. If a gate fails, execute at most the two declared DAgger rounds and record lineage; otherwise explicitly record that no DAgger round was needed. Do not start/accept PPO from an unselected distillation checkpoint. |
| F-M10-033-R1 | **BLOCKER** | The shared critic warm-up is implemented and tested but has never run on a real selected bundle, has no evidence file or retained checkpoint, and does not implement §6.2's complete fallback ladder. On a missed threshold the example merely prints that the separate-trunk retry and batch-mean fallback are the “next step” and exits. | Run the shared warm-up only after M10-032 selects a valid checkpoint. Implement and retain exactly one bounded separate 2x128 retry and then the batch-mean fallback, selecting one of the three declared critic modes for all ablations. Record the real validation EV, selected epoch/mode, policy fingerprint, checkpoint identity, and falsification. |
| F-M10-033-R2 | **HIGH** | The warm-up silently skips any training sample for which `value_of` fails, ignores `RowAdam::step`'s false result, and represents failed validation predictions as NaN. Its fingerprint routine also skips failed probe logits. Although current fixtures are valid, these are fail-open/vacuous boundaries: a partially unscorable corpus changes denominators, while zero successfully fingerprinted logits still compares equal before/after. | Return `Result` and refuse the first failed train/validation/probe evaluation or optimizer step. Require exact processed counts and a nonempty fingerprint with a pinned expected scalar count. Add invalid-row/column/head probes and prove they abort rather than being skipped. |
| F-M10-034-R1 | **BLOCKER** | M10-034 does not implement or exercise Adam. `ppo::update` delegates the actual step to a caller callback, while its learning-rate/betas/epsilon/weight-decay/gradient-clip settings are unused. Every update test passes `|_| {}`. Consequently `the_same_update_twice_produces_the_same_numbers` compares deterministic loss telemetry from two unchanged actors—not gradients, Adam moments, or updated weights—and its “non-vacuity” assertion only checks that actor loss is nonzero. | Implement the package-owned Adam step (including global clipping and the full named parameter set) or take a typed optimizer whose state is part of the contract. In the repeatability test, assert nonzero parameter movement, advance moments/step count, and compare loss trace, gradients or reduced gradients, moments, and every updated tensor bit-for-bit from identical starts. Falsify with a no-op/broken optimizer. |
| F-M10-034-R2 | **HIGH** | PPO silently `continue`s when policy scoring fails and silently omits critic loss when value evaluation fails. `seen.max(1)` then permits an all-invalid batch to return plausible zero statistics successfully. Loss is divided by the nominal minibatch length even when records were skipped. | Make batch construction/update validate nonempty batches, settings, heads, choices, finite values, option vectors, and critic inputs, returning an error on any failed score/value. Require processed count exactly equals batch length and add all-invalid plus one-invalid-among-valid regressions. |
| F-M10-034-R3 | **HIGH** | `Step::critic` is the generic public `SparseOption`, and PPO uses the crate-private escape hatch `CriticInput::from_sparse`. The public training boundary can therefore feed option/legal-set columns into the value head, reopening the input-type defect closed in M09-027 for inference. | Store a validated critic-only type in `Step` and construct it only through the option-free critic-vector path. Add an API/compile-fail regression proving a policy `SparseOption` cannot become PPO critic input, plus integrated legal-set invariance/non-vacuity coverage. |
| F-M10-034-R4 | **LOW** | The package-owned `distinguishable_step` test adds two `cast_precision_loss` warnings at `ppo.rs:534`, so the touched package is not warning-clean despite Clippy exiting zero. | Use checked integer-to-float fixture values or narrowly justified expectations, then rerun the stated all-targets Clippy gate with zero warnings attributable to this package. |

### What did survive the vacuity audit

- The critic invariance corrections now genuinely vary legal sets and include input non-vacuity.
- Distillation refuses zero total parameter movement, and its numerical grouping comparison is not
  merely `f(x) == f(x)`.
- PPO's finite-difference test uses distinguishable options and requires at least one nonzero
  analytic coordinate; that narrow gradient-formula check is meaningful.
- M09-028's immutable bundle publication and closed-file validation looked structurally sound in
  this targeted pass, but M09-030 still has to review the full row before acceptance.

**Verdict: changes required.** M10-031 through M10-034 are not accepted. M10-031/032 have artifact
and completion blockers; M10-033 is an incomplete real-run/fallback package; M10-034's advertised
optimizer/update proof is presently vacuous. All also inherit the unresolved M09-029 STOP and
missing M09-030 exit review.
