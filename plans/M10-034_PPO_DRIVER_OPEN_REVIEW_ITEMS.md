# M10-034 PPO driver open review items

## Concurrent independent review of the first end-to-end driver (2026-08-26)

Reviewer: Codex. Scope is the uncommitted `MlpBot` PPO recording path and
`crates/ti4-mlp/examples/ppo_update.rs`; this is deliberately written while Claude fixes the first
reported return defect so the remaining findings are not lost or silently folded into a run.

| ID | Severity | Finding | Required correction |
|---|---|---|---|
| F-M10-034-D1 | **BLOCKER** | Every decision in one seat-game receives one value derived from a synthetic one-step terminal episode. This is not §6.1's per-decision shaped Monte-Carlo return and erases within-game credit assignment. | Record `progress::measure` at every non-forced MLP decision using the acting seat's bound observation and setup baseline, exactly aligned with the PPO step. Build the real `Episode.steps`, call `reward::returns`, and require return count equals step count before assignment. Add a fixture with distinct progress snapshots and assert distinct returns. |
| F-M10-034-D2 | **HIGH** | The recording path is fail-open: critic evaluation uses `unwrap_or(0.0)`, head lookup uses `unwrap_or(0)`, and the sampled probability uses `unwrap_or(0.0)` plus a floor. These conversions can publish a valid-looking batch after a model/schema failure. | Refuse the game on the first failed critic/head/probability lookup or invalid distribution. A sampled option must have a present, finite, strictly positive probability; do not repair malformed behavior data with a floor. Add falsification probes for each refusal. |
| F-M10-034-D3 | **BLOCKER** | `ppo_update` constructs `ppo::Adam::new` inside the update loop. Every update after the first therefore discards both moments and the step counter, turning Adam into repeated first steps and making resume/state evidence false. | Construct Adam once for the run, retain it across updates, and move its moments with the optimizer device. Persist/restore its exact state and RNG/update cursor at checkpoint boundaries; test uninterrupted two-update execution against save/reload continuation bit-for-bit. |
| F-M10-034-D4 | **HIGH** | Seat/step alignment can silently shrink: a missing handle executes `continue`, terminal-return extraction falls back to `0.0`, and no exact per-seat/global count is asserted before `Batch::freeze`. | Refuse a missing/duplicate handle, empty return vector, or any progress/step/return count mismatch. Preserve failing seed/rotation/seat in the error. Assert summed per-seat counts equal the frozen batch length. |
| F-M10-034-D5 | **HIGH** | Critic mode is not part of `Batch::freeze`. It always constructs `return - behavior_value`, although §6.3 requires the fixed batch-mean baseline when `critic_mode=batch_mean`, and the driver still evaluates a nominal critic in that mode. | Make the baseline choice typed by `CriticMode`: shared/separate use recorded behavior `V(s)`; batch-mean computes the pre-optimization fixed batch mean and does not evaluate/store an unused critic value. Add a test with deliberately nonconstant fake behavior values proving batch-mean advantages ignore them. |
| F-M10-034-D6 | **MEDIUM** | The driver creates no retained updated bundle or optimizer checkpoint and checks no actual parameter delta after the end-to-end update. Nonzero loss telemetry is insufficient; the previous vacuous tests failed exactly this way. | Fingerprint trainable parameters and Adam before/after, require nonzero movement and advanced moments/step count, then atomically publish a reloadable checkpoint with lineage and verify it. |

**Status: changes required.** Do not use an update from the current driver as M10-034 evidence. The
right order remains: correct per-decision returns and fail-closed recording, run one honest
single-threaded update with retained state, then optimise rollout parallelism.
