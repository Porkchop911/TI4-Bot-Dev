# M09-029 / M09-030 open review items

## Targeted frontier review through `544732e` (2026-08-26) — STOP remains in force

Reviewer: Codex, independent of the Claude Opus 5 implementation.

This is a targeted review of the M09-029 throughput decision and the missing M09-030 exit gate. It
is **not** either of M09-030's two complete Tier-D passes over M09-019 through M09-029. No source
code was changed by the reviewer.

Reviewed the normative `docs/MLP_PLAN.md` revision 5 §7.1 protocol, committed `cpu_gate`, M09-029
evidence and history, the M09 dependency table, and the still-open M09-018 exit-review ledger.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-029-R1 | **HIGH** | The committed gate cannot reproduce the protocol claimed by the evidence. `cpu_gate.rs` at `cd88ef1` uses two seeds (`900_000_000..900_000_002`), one warm-up, and two timed samples by default. The plan and evidence require 16 seeds x 6 rotations x 4 rounds, five warm-ups, and at least 20 timed batches. `git blame` confirms these reduced constants were present in the submitted M09-029 commit; they are not later drift. Passing `--samples 20` repairs only one of the three mismatches. | Restore the exact predeclared workload and retain a fresh result from the committed harness on an idle machine. The evidence must report the actual constants read from the run rather than restating the plan. Add a test or run-time assertion pinning all protocol constants so another reduced probe cannot publish as the gate. |
| F-M09-029-R2 | **HIGH** | The shadow path discards both `Actor::probabilities` and `Actor::value` errors with `let _ = ...`, then increments its scored-decision counter anyway. A model that refuses every inference can therefore produce plausible timing and decision counts. | Propagate every inference error and refuse the batch/run. Assert successful policy and value evaluations equal the expected decision count; falsify the gate with an intentionally invalid actor/input and require non-zero exit without evidence publication. |
| F-M09-029-R3 | **BLOCKER** | The recorded specified metric is width-256 shadow/linear = **2.888x** and width-128 = **2.716x**. Under §7.1, width 256 above 2x requires the width-128 rerun, and width 128 above 2x is an explicit **STOP**. The 1.681x “MLP-deciding per-decision” number is an implementer-created replacement metric. It compares different policy trajectories and therefore different option-set/head/state distributions; dividing each by its own decision count does not make those workloads paired or equivalent. It cannot override a predeclared gate after seeing the result. | Keep STOP in force. Either optimize until the corrected specified harness passes, or obtain an explicit architecture/plan revision with a predeclared replacement gate before generating new evidence. A defensible replacement would replay one fixed captured decision stream through both scorers or otherwise share extraction while keeping inputs identical. Do not retroactively accept width 256 from the current direct-policy number. |
| F-M09-030-R1 | **BLOCKER** | M09-030 was never run, although M10-031 directly depends on it. In addition, the accepted M09-018 review leaves F-M09-018-1/2/3/7 mandatory for resolution or explicit scope decisions before M09-030. Operator authorization to continue implementation past the dependency permits provisional work; it does not constitute either independent Tier-D acceptance pass. | Resolve or explicitly disposition F-M09-018-1/2/3/7, resolve M09-029's STOP, then perform both complete independent M09-030 Tier-D passes over M09-019..029. Until then M09 has not exited and every M10-031+ result is provisional. Use the recorded artifact digests to recapture/retrain anything invalidated by the resolution. |

### Non-vacuity assessment

The later note that the measurements predate the `DENSE_WEIGHT_LIMIT` change is useful provenance,
but it does not cure the protocol mismatch or fail-open inference. No new timing run was attempted:
the committed harness is not yet capable of producing acceptable evidence, and the existing note
requires an idle machine for the eventual rerun.

**Verdict: changes required; STOP.** M09-029 is rejected under the current plan. M09-030 is not
started, M09 has not exited, and M10-031 onward remains provisional and mechanically invalidatable.

---

## Independent recheck of corrected paired harness `770568b` / evidence `8d1eb36` (2026-08-26)

Reviewer: Codex, independent of the Claude Opus 5 implementation and measurement.

The proposed fixed-stream form is permitted by F-M09-029-R3, but the submitted 1.329x accounting
does not yet implement that form correctly.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M09-029-R4 | **BLOCKER** | `Paired::choose_seeing` executes four relevant paths per decision: an explicit extraction, `inner.consider` (another explicit extraction plus linear score), `mlp_choice_features` plus MLP score, and finally `inner.choose_seeing` (a third explicit extraction plus linear score and sampling). `Split::engine` subtracts only the first three timed buckets from total rollout time. It therefore assigns the final deciding linear extraction/score to “engine”, then includes that cost in both reconstructed arms. This is not shared engine time and biases the ratio toward 1. | Measure an uncontaminated ordinary linear rollout total separately. On the identical deterministic decision stream, measure complete linear and complete MLP scorer costs, then substitute them: `(linear_rollout_total - complete_linear_cost + complete_mlp_cost) / linear_rollout_total`. Keep probe overhead out of the rollout total and prove the probed and ordinary runs have identical outcomes/decision identities. |
| F-M09-029-R5 | **HIGH** | The harness subtracts the duration of `explicit_choice_features` from `mlp_choice_features` even though these are different extraction/projection functions and the latter intentionally suppresses and remaps families. Their costs are not interchangeable. Per-decision `saturating_sub` hides rather than validates this mismatch. | Time the complete scorer paths used by each model, including each model's actual extraction/projection. Do not subtract one extractor from another. The substitution formula above needs complete costs and avoids this assumption. |
| F-M09-029-R6 | **HIGH** | `Actor::probabilities` is discarded with `let _`, recreating the fail-open boundary fixed in F-M09-029-R2. A refusing actor can still contribute elapsed time and an accepting ratio. The MLP path also maps an impossible column conversion to column zero. | Refuse the first projection/conversion/inference failure and require the successful MLP evaluation count to equal the exact non-forced decision count. Retain a falsification proving a refusing input exits non-zero. |

**Verdict: changes required; STOP remains.** The 1.329x evidence is rejected, not because a corrected
fixed-stream gate is disallowed, but because the submitted reconstruction contains the deciding
linear scorer inside its purported shared-engine term. M09-030 remains blocked until a corrected
committed harness produces fresh evidence and this recheck accepts it.

---

## R4–R6 correction and independent acceptance (2026-08-26)

Resolved in `62e6472`, `323d6d2`, `a65272d`, and the pre-measurement revision-7 specification
`ad87ac5`:

- the denominator is now a separately executed, unwrapped ordinary linear rollout;
- both complete, genuinely different scorer paths retain their own extraction/projection cost;
- substitution is `(linear total - complete linear + complete MLP) / linear total`;
- an untimed audit pins the exact non-forced decision fingerprint and outcomes, and every warm-up
  and timed run must reproduce them;
- conversion/inference failures propagate, and successful MLP evaluations must equal the exact
  decision count.

The clean committed width-256 run completed 5 warm-up and 20 timed pairs over the full workload:
median **1.857x**, range **1.849–1.866x**, sample SD **0.004617**, 132,722 non-forced decisions per
batch, exit 0. Raw samples and provenance are retained in `plans/evidence/M09-029.md`.

**M09-029 verdict: accepted at width 256.** R4–R6 are closed and the earlier STOP is superseded by
the preregistered revision-7 result. M09-030 is now dependency-ready, but still requires its two
complete independent Tier-D passes.
