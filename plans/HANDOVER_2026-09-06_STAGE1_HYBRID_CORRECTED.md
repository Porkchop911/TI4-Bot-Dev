# Handover — corrected Stage-1 PPO + clean-demonstration experiment

Date: 2026-09-06 (Europe/Vienna)

## Mission

Complete the owner-requested screening experiment from
`out/checkpoints/blank-waste-mine-p5/checkpoint-59540`:

- three paired training seeds;
- arm A: PPO control;
- arm B: the same PPO rollouts, seeds, optimiser and reward plus a small advantage-weighted clean
  demonstration actor loss;
- serialise all CUDA training jobs;
- greedy evaluation at updates 0, 25, 50, 75 and 100;
- overall and per-faction clearance, any-waste, tactical actions/seat, waste/tactical, and
  `clear AND zero-waste`;
- reference KL and greedy-action flip rate against checkpoint-59540;
- enforce collapse gates;
- compare paired results and update the situation report and this handover with the conclusion.

The final acceptance target remains at least 95.00% overall greedy clearance and at most 0.50%
overall greedy any-waste. Report every faction separately. The screening question is narrower:
does the auxiliary improve clearance and reduce waste relative to paired PPO consistently across
three seeds?

## Live process — do not stop

The corrected control completed normally. Its matched hybrid is now active and healthy:

- PID: `99736`
- executable: `target/release/examples/ppo_update.exe`
- arm: corrected four-component PPO + advantage-weighted imitation, seed 1
- output: `out/checkpoints/hybrid-guided-aux-s1`
- stdout: `out/hybrid-guided-aux-s1.log`
- stderr: `out/hybrid-guided-aux-s1.err.log`
- seed base: `1700000000`
- progress at final handoff check: startup/first update; process still active

Completed seed-1 corrected control:

- output: `out/checkpoints/hybrid-guided-control-s1`
- update-25 checkpoint: `out/checkpoints/hybrid-guided-control-s1/checkpoint-992`
- update-50 checkpoint: `out/checkpoints/hybrid-guided-control-s1/checkpoint-1936`
- update-75 checkpoint: `out/checkpoints/hybrid-guided-control-s1/checkpoint-2860`
- update-100 checkpoint: `out/checkpoints/hybrid-guided-control-s1/checkpoint-3792`

Check the process and tail without interfering:

```powershell
Get-Process -Id 99736 -ErrorAction SilentlyContinue
Get-Content out/hybrid-guided-aux-s1.log -Tail 30
Get-Content out/hybrid-guided-aux-s1.err.log -Tail 30
```

The live log header must say:

```text
potential   planets+systems 2 | capacity+infantry 1 | conjunctive 0 | clear bonus 22
```

That proves this run uses the corrected dense signal.

## Critical correction history

The prior implementation did **not** fully follow the supplied guidance. Two material defects were
found and both affected training evidence.

### 1. Reversed map offsets

The intended conventions are:

- fresh PPO rollout maps: `TILE_SEED_OFFSET = 20_000_000`;
- replay of `build_positive_corpus` trajectories: `CORPUS_TILE_SEED_OFFSET = 0`.

They had accidentally been wired in reverse. The invalid process was stopped. Source now has
corpus replay using 0 at the `replay_advantage` call and normal PPO `play_one` using 20,000,000.

### 2. Stage-1 dense signal still used the obsolete unit proxy

`reward::potential` explicitly said `Progress` did not carry capacity ships or infantry and paid
only for the first generic unit gained. The guidance specifically required auditing the potential
against all four authoritative bar components and fixing it if incomplete.

This is now corrected:

- `Progress` carries absolute `capacity_ships` and `infantry`;
- `Observed::opening_fleet` delegates to the opening gate's exact catalogue-aware measurement;
- the potential is capped independently at 3 planets, 3 systems, 2 capacity ships and 3 ground
  forces;
- PDS/space docks do not count as infantry; faction capacity ships are classified through content;
- historical serialized `Progress` fixtures remain readable through serde defaults.

Because the common reward changed, the already-finished old seed-1 control is not a valid paired
control. Both arms are being rerun under `hybrid-guided-*` names.

## Invalid artifacts — never use as experiment evidence

Keep these for diagnosis, but exclude them from all tables and conclusions:

- `out/checkpoints/hybrid-screen-aux-s1` — first hybrid; wrong map convention and wrong gradient
  reference norm;
- `out/checkpoints/hybrid-screen-correct-aux-s1` — still had the map offsets reversed;
- `out/checkpoints/hybrid-screen-guided-aux-s1` — offsets fixed, but started before the missing
  four-component reward audit was implemented;
- `out/checkpoints/hybrid-screen-control-s1` — old one-unit shaping proxy;
- `out/checkpoints/blank-waste-mine-p5-continued/checkpoint-17636` — regressed PPO continuation;
- any standalone BC result branched from checkpoint-17636.

The diagnostic update-25 result from the reversed-offset run (92.99% Validation clearance and
2.55% Train any-waste) is invalid and must not be reused.

## Correct implementation now in the dirty worktree

Do not reset or clean the worktree. Changes belonging to this experiment include:

- `crates/ti4-mlp/src/ppo.rs`
  - `update_with_auxiliary`;
  - measures PPO policy gradient separately from critic gradient;
  - caps the demonstration contribution at 10% of PPO actor-gradient norm;
  - recomputes and applies one combined loss in one Adam step.
- `crates/ti4-mlp/src/positive_corpus.rs`
  - `AdvantageDemo`;
  - detached positive `[G - V(s)]_+` weighting;
  - batch standard-deviation normalization and clip to 2;
  - no auxiliary loss when all advantages are non-positive.
- `crates/ti4-mlp/examples/ppo_update.rs`
  - deterministic clean-trajectory replay under the frozen checkpoint-59540 actor;
  - 120 trajectories/update, exactly balanced over six factions and four generating temperatures
    (5 per faction-temperature bucket);
  - correct split between corpus and PPO map offsets;
  - reference KL and greedy flips at hybrid report boundaries;
  - final replay `Progress` includes capacity ships and infantry.
- `crates/ti4-policy/src/progress.rs`
  - authoritative capacity-ship and infantry progress fields.
- `crates/ti4-engine/src/choice.rs`, `crates/ti4-engine/src/opening.rs`
  - public-observation bridge to the exact opening fleet measurement.
- `crates/ti4-training/src/reward.rs`
  - four-component dense potential.
- `crates/ti4-mlp/examples/build_positive_corpus.rs`
  - overall and per-faction greedy clear/waste/tactical/joint metrics;
  - raw faction counters persisted in the output manifest.
- `crates/ti4-mlp/examples/demo_benchmark.rs`
  - separate scoring, replay and reference bundles;
  - per-head and overall `KL(reference || policy)` and greedy flips.

Other dirty files predate this experiment and must be preserved. In particular, do not discard
the actor-exploration critic fix in `crates/ti4-policy/src/critic.rs` or unrelated engine changes.

## Tests already passed

After the four-component change:

- `cargo test -p ti4-policy progress --lib`: 10 passed;
- `cargo test -p ti4-training reward --lib`: 19 passed;
- `cargo test -p ti4-mlp ppo --lib`: 18 passed;
- `cargo test -p ti4-mlp positive_corpus --lib`: 18 passed;
- `cargo check -p ti4-mlp --example demo_benchmark`: passed;
- release examples `ppo_update`, `build_positive_corpus`, `clearance_eval`, and
  `demo_benchmark` were built against CUDA libtorch.

Drift smoke test on checkpoint-59540 against itself scored 1,587 decisions with 0 unreplayable,
overall reference KL `0.00000` and flips `0.00%`.

## Authoritative baseline

Bundle: `out/checkpoints/blank-waste-mine-p5/checkpoint-59540`.

Validation greedy clearance, 600 seeds × 6 rotations:

| faction | clearance |
|---|---:|
| Hacan | 92.56% |
| Jol-Nar | 90.58% |
| L1Z1X | 96.89% |
| Letnev | 92.72% |
| Sol | 92.36% |
| Xxcha | 95.28% |
| overall | 93.40% ±0.33 |

Train greedy waste evaluation:

- clearance 93.26%;
- tactical/seat 3.000;
- waste/seat 0.023;
- any-waste 2.31%;
- waste/tactical 0.008.

Original logs still exist at:

- `$env:TEMP/clearance_blank_p5_u1600.log`
- `$env:TEMP/waste_blank_p5_u1600.log`

The enhanced baseline evaluation is now complete at
`out/eval-guided-baseline-u0-waste.log` (raw counters and trajectories under
`out/corpus/eval-guided-baseline-u0-waste`). Reuse it as update 0 for every pair:

| faction | Train clear | any-waste | tactical/seat | waste/tactical | clear+zero |
|---|---:|---:|---:|---:|---:|
| Hacan | 92.22% | 1.78% | 3.000 | 0.006 | 91.39% |
| Jol-Nar | 89.47% | 0.56% | 3.000 | 0.002 | 89.31% |
| L1Z1X | 96.86% | 1.44% | 3.000 | 0.005 | 95.53% |
| Letnev | 93.39% | 0.64% | 3.000 | 0.002 | 92.75% |
| Sol | 92.81% | 7.17% | 3.000 | 0.024 | 86.14% |
| Xxcha | 94.83% | 2.28% | 3.000 | 0.008 | 93.03% |
| overall | 93.26% | 2.31% | 3.000 | 0.008 | 91.36% |

## Demonstration corpus

Use only:

`out/corpus/positive-hybrid-p5-multitemp`

It contains 47,336 full-Round-1 trajectories generated by checkpoint-59540 at temperatures
0.001, 0.25, 0.5 and 0.75. Admission was `opening clear AND zero wasted activation`. Its generating
bundle, map pool, seed range and temperatures are frozen in `manifest.txt`.

Do not substitute `positive-bc-latest-greedy`; that corpus came from the regressed continuation
and is greedy-only.

## Exact training commands

Set the CUDA environment in every shell:

```powershell
$env:LIBTORCH = "$PWD/out/libtorch-2.9.1-cu128"
$env:LIBTORCH_BYPASS_VERSION_CHECK = "1"
$env:PATH = "$env:LIBTORCH/lib;$env:PATH"
$env:GIT_COMMIT = (git rev-parse HEAD)
```

Common control command (replace seed base and suffix):

```powershell
target/release/examples/ppo_update.exe `
  --bundle out/checkpoints/blank-waste-mine-p5/checkpoint-59540 `
  --stage 1 --rounds 1 `
  --temperature 0.25 --movement-entropy 0.05 `
  --waste-penalty 5 --updates 100 --report-every 25 `
  --device cuda --seed-base <SEED_BASE> `
  --out out/checkpoints/hybrid-guided-control-<SUFFIX>
```

Hybrid is identical except:

```text
--demo-corpus out/corpus/positive-hybrid-p5-multitemp
--demo-per-update 120
--out out/checkpoints/hybrid-guided-aux-<SUFFIX>
```

Paired seed bases:

| pair | seed base | suffix |
|---|---:|---|
| 1 | 1700000000 | s1 |
| 2 | 1800000000 | s2 |
| 3 | 1900000000 | s3 |

Only one `ppo_update.exe` may run at a time. Current order is control s1 (live), hybrid s1, control
s2, hybrid s2, control s3, hybrid s3.

## Checkpoint discovery

Checkpoint directory numbers are Adam-step counts, not update numbers. Read each manifest's
`source` field. Expected examples are roughly `checkpoint-992`, `checkpoint-198x`, etc., but never
infer update from the directory number.

```powershell
Get-ChildItem <RUN_DIR> -Directory | ForEach-Object {
  $m = Join-Path $_.FullName 'manifest.json'
  if (Test-Path $m) {
    $j = Get-Content $m -Raw | ConvertFrom-Json
    [pscustomobject]@{ Path = $_.FullName; Source = $j.source }
  }
}
```

## Greedy evaluations

Clearance uses the Validation pool and the established seed convention:

```powershell
target/release/examples/clearance_eval.exe `
  --bundle <CHECKPOINT> --temperature 0.001 --seeds 600
```

Waste/tactical/joint uses the Train pool and writes a scratch corpus:

```powershell
target/release/examples/build_positive_corpus.exe `
  --bundle <CHECKPOINT> --seeds 600 --temperatures "0.001" `
  --out out/corpus/eval-<ARM>-<SUFFIX>-u<UPDATE>-waste
```

The enhanced output prints, overall and per faction:

- clearance;
- any-waste;
- tactical/seat;
- waste/tactical;
- clear+zero.

Use unique output directories. Do not overwrite a corpus directory. Redirect stdout to a named log
and keep it as evidence.

Drift against checkpoint-59540 on a deterministic held-out demonstration subset:

```powershell
target/release/examples/demo_benchmark.exe `
  --bundle <CHECKPOINT> `
  --replay-bundle out/checkpoints/blank-waste-mine-p5/checkpoint-59540 `
  --reference-bundle out/checkpoints/blank-waste-mine-p5/checkpoint-59540 `
  --corpus out/corpus/positive-hybrid-p5-multitemp `
  --rescued out/corpus/does-not-exist `
  --per-corpus 400
```

Use the same `--per-corpus` and corpus for every checkpoint. Capture the `ALL` row's `ref-KL` and
`flips`. Update 0 is exactly KL 0 / flips 0. Hybrid logs also print drift at report boundaries, but
use the standalone evaluator for a directly comparable control/hybrid table.

## Collapse gates

Evaluate hybrid at updates 25 and 50 while it is running. Stop that hybrid run only if two
consecutive evaluations show either:

1. overall Validation greedy clearance below 92.5%; or
2. any faction more than 2.0 percentage points below its own checkpoint-59540 starting clearance.

One bad checkpoint is one strike, not a stop. Log which condition/faction caused each strike. If
the second consecutive checkpoint confirms it, stop only that hybrid process and continue the next
paired seed. The control is the matched comparison and should complete 100 updates unless it fails
operationally.

## Remaining execution sequence

1. Keep PID 99736 (hybrid s1) active.
2. Run greedy clearance, waste/joint and drift for the completed control s1 updates 25/50/75/100.
3. Evaluate hybrid s1 at update 25, then update 50; enforce the two-strike gate. If it survives,
   evaluate 75/100.
4. Repeat control/hybrid for seed bases 1800000000 and 1900000000, serially on CUDA.
5. Reuse one freshly enhanced baseline evaluation as update 0 for all pairs.
6. Build paired tables for each checkpoint and faction. Include `clear AND zero-waste`, since that
   is the actual joint goal.
7. Across the three seeds, decide whether hybrid-control has consistently positive clearance delta
   and negative any-waste delta. Do not select on a single lucky seed.
8. Update `plans/SITUATION_REPORT_2026-09-05_STAGE1_BC_FAILURE.md` with the corrected protocol,
   results and recommendation. Update this handover with final process state and evidence paths.

## Final decision standard

Continue developing the hybrid only if the qualitative paired effect agrees across seeds:

```text
delta clearance (hybrid - control) > 0
delta any-waste (hybrid - control) < 0
```

It need not reach 95% / 0.5% in this 100-update screen. If the direction is consistently good,
the next experiment is a 200–300 update hybrid with one mid-run elite recollection, retaining 50%
checkpoint-59540 demonstrations and 50% current-policy clean demonstrations. If not, do not spend
another night tuning the demonstration coefficient.

## Operational cautions

- Training-window metrics are sampled at T=0.25 and are **not** acceptance evidence.
- Acceptance is greedy (`T=0.001`).
- Label the pool in every table: clearance is Validation; authoritative waste is Train.
- Do not run two CUDA training processes concurrently.
- CPU evaluations can run while training, but they slow rollouts. Prefer evaluating published
  checkpoints sequentially when time permits.
- Preserve all dirty worktree changes; no reset, checkout, clean, or broad deletion.
- The manifest `git_commit` records HEAD and cannot express uncommitted changes. The unique
  `hybrid-guided-*` directory names and the four-component log header distinguish corrected runs.
