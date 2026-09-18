# Training recovery — 2026-09-12

## Frozen facts and operational boundary

- Repository: `D:\Projects\ti4-engine-rs`, HEAD `22266e1e1029750fdbd37d794d5148417c5687de`.
- The shared tree is dirty. Reward work owned by the coordinating session is in
  `crates/ti4-engine/src/choice.rs`, `crates/ti4-policy/src/progress.rs`, and
  `crates/ti4-training/src/reward.rs`; unverified A3/diagnostic work is in
  `crates/ti4-mlp/src/lib.rs`, `crates/ti4-mlp/src/ppo.rs`, and
  `crates/ti4-mlp/examples/ppo_update.rs`. Preserve all of it.
- Live CUDA training remains running and must not be restarted or competed with. The user
  authorized stopping it at its next completed checkpoint only after the save/reload and every
  manifest checksum are verified; the identity-checked watcher owns that stop:
  PID 21188, `out/perf-20260911/ppo_update-pre-lazy-planets.exe`, writing
  `out/blank-shaped-4layers/continue-20260912b/`. It resumed
  `continue-20260912/checkpoint-236556`; `checkpoint-202516` is the latest complete checkpoint
  observed at 2026-09-12 18:04:21 (local update 1200, 3250 updates from blank).
- The frozen evaluator is `out/blank-shaped-4layers/greedy/clearance_eval.exe`, SHA-256
  `1245b392923c519ccfb294edc7a3f806f1c65c1e9835a23f32199ba4261ec588`, built from `0bd2e75`.
  Protocol: validation pool, T=0.01, four rounds, 600 seeds beginning 910001000, six rotations,
  per-seat CSV.
- Existing cached baseline is `greedy/eval-236556-t001.csv`. It is reusable only after protocol
  and evaluator hash match. Outputs use new names in the existing `greedy/` directory; no directory
  creation or overwrite.

## Evidence so far

Lineage is blank -> checkpoint-33616 (250) -> checkpoint-45496 (+300) -> checkpoint-236556
(+1500, 2050 total) -> current continuation. `checkpoint-184960` is local 1100 / 3150 total;
`checkpoint-202516` is local 1200 / 3250 total. Checkpoint suffixes are Adam minibatch steps,
not rollout updates.

The prior paired 236556 -> 184960 comparison measured mean VP 3.360 -> 3.199
(-0.161759; approximate seed-group normal 95% CI [-0.182201, -0.141317]) and clearance
85.611% -> 79.579% (-6.032407 pp; [-6.461396, -5.603419] pp). Hacan and Letnev account for
about 85% of the VP decline. Jol-Nar had higher VP but a 35.36 pp clearance decline, so a larger
clearance penalty is not a justified repair without component-level diagnosis.

## Ordered execution and bounds

1. Freeze `checkpoint-202516`; run the frozen 21,600-seat CPU evaluation once, then verify CSV
   row count, duplicate keys, set equality with the cached baseline, and seed-group paired intervals.
   This measures recovery; it is not an idle-machine benchmark. If VP remains materially below
   236556, recommend that parent for any approved experiment but leave the live job untouched.
2. Diagnose with `vp_sources` only after an explicit temperature flag is validated and logged for
   T=0.01. Start one or two seeds against the same fixed opponent, prioritizing Hacan and Letnev;
   reconcile VP sources and inspect bounded objective/award/resource/ground-force/activation evidence.
   Preserve choice/event/final-state hashes with instrumentation toggled. Split the Jol-Nar opening
   clearance conjunction into its four components before proposing reward changes.
3. Make the authorized ground-force fleet valuation controllable: infantry=0.5 and mech=2 at the
   existing fleet weight 0.03; ships retain existing pricing. Keep fracture default zero and expose
   `--fracture-entry-bonus`, with intended recipe 0.1, validation, and effective-settings printing.
   Coordinate driver hunks because `ppo_update.rs` is owned by another session. Do not claim
   observed presence is an event-entry signal without event-level evidence.
4. Keep A3 opt-in/default-off unless its library tests and same-captured-batch CPU parameter/Adam
   equivalence pass. Never use CUDA output hashes as that gate. Prove inference game hashes unchanged
   across reward-only changes. Run affected crates one at a time and the ti4-sim behaviour suite
   before merging engine work; do not rebaseline fixtures.
5. A two-arm 250-update CUDA experiment needs a coordinated handoff and explicit approval for two
   new output folders first. It uses a selected same parent, same seeds and full stated recipe,
   sequential single-GPU execution, cold Adam in both arms, and a roughly 1 GiB artifact cap.
   Endpoint and acceptance are predeclared: VP primary; at least +0.05 VP and seed-group 95% CI
   excluding zero, then disjoint-block/fixed-opponent confirmation. No automatic fallback run.

## Current next action

Evaluate frozen `continue-20260912b/checkpoint-202516` with the frozen evaluator to uniquely named
files in the existing greedy directory, then validate exact pairing against `eval-236556-t001.csv`.

Cleanup: an accidental unlogged evaluator started by an unsupported `--help` probe was identified
as PID 35848 at the frozen evaluator path and stopped after exact path and creation-time validation.
It was not the trainer or the intended four-worker evaluator.
