# Training report for astra — 2026-09-12

Written by the Claude Code session that ran the greedy evaluation below. Everything here is
verifiable from the repository; where something is unverified or is my inference, it says so.

## 1. Headline

**The current run is making the policy worse, and it is still running by the owner's decision.**

A paired greedy evaluation over 21,600 seat-games says the policy has regressed on both axes in
the last ~1,100 updates:

| | start of run (`checkpoint-236556`) | now (`checkpoint-184960`) | delta, paired 95% |
|---|---|---|---|
| stage-1 clearance | 85.61% | 79.58% | **−6.03 pp** [−6.50, −5.56] |
| mean VP | 3.360 | 3.199 | **−0.162** [−0.184, −0.139] |

Per faction (same pairing, on `seed`+`rotation`+`faction`, 0 unpaired):

| faction | clearance | mean VP |
|---|---|---|
| hacan | 89.17 → 88.28 (−0.89) worse | 3.559 → 3.116 (**−0.443**) worse |
| **jolnar** | **67.81 → 32.44 (−35.36)** worse | 3.451 → 3.591 (**+0.140**) better |
| l1z1x | 91.56 → 91.22 (−0.33) worse | 3.292 → 3.174 (−0.118) worse |
| letnev | 86.28 → 85.78 (−0.50) no change | 3.384 → 3.006 (−0.378) worse |
| sol | 91.17 → 91.06 (−0.11) no change | 3.520 → 3.323 (−0.197) worse |
| xxcha | 87.69 → 88.69 (+1.00) better | 2.955 → 2.981 (+0.03) no change |

Two readings matter more than the totals:

1. **Jol-Nar is trading clearance for points.** It is the only seat whose VP improved, and its
   clearance halved. The reward as configured pays for that trade. Four of six seats also lost
   VP, so this is not a clean exchange — the run is worse at both.
2. **The in-training table hid it completely.** At T=2.5 the table read a flat 72–73% across the
   whole period while greedy clearance fell 6 points. This is the second recorded instance;
   judge runs by paired greedy evals, never by the training tables.

Protocol: frozen `clearance_eval.exe` (built from `0bd2e75`), `--temperature 0.01 --rounds 4
--seeds 600 --seed-base 910001000`, holdout pool (Validation), per-seat CSV. Both arms ran
alongside the live trainer, so the wall times in those logs are not idle-machine numbers; the
results are unaffected.

## 2. Reference levels, same protocol (T=0.01, holdout)

| bundle | clearance | VP | what it is |
|---|---|---|---|
| `checkpoint-7472` | 91.88% | 3.852 | older VP-only line, different start and architecture |
| `checkpoint-33616` | 82.81% | 2.940 | this line at update 250 |
| `checkpoint-236556` | 85.61% | 3.360 | **best measured policy on this line** |
| `checkpoint-184960` | 79.58% | 3.199 | current run, ~update 1100 |

`eval-187084-t0001` in the same folder used `--temperature 0.001`, not 0.01 — a different
protocol, so do not put its 84.33% / 3.329 in the same column as the rows above.

## 3. What is running right now

- `out/perf-20260911/ppo_update-pre-lazy-planets.exe`, pid 21188, started 2026-09-12 10:19:46.
- Output `out/blank-shaped-4layers/continue-20260912b/`; at 18:13 it was on **update 1218**,
  ~22–25 s per update, latest checkpoint `checkpoint-202516` (18:04:21).
- Configured for 10,000 updates. Nothing will stop it on its own.
- To stop: kill `ppo_update-pre-lazy-planets.exe`; progress since the last 100-update checkpoint
  is lost. **I did not stop it** — the owner chose to keep it running after seeing the numbers
  above.

In-training table (T=2.5, 57,600 seat-games per report) for this run, for what it is worth:
100 → 71.60%/3.048, 800 → 76.72%/3.127 (peak), 1100 → 72.28%/3.097, 1200 → 73.44%/3.086.

## 4. Lineage and the exact recipe

A sequence, each step resuming from the previous one's final bundle:

1. blank bundle — schema 8, width 256, capacity 20480, shared critic, **2 residual blocks**
2. 250 updates → `out/blank-shaped-4layers/checkpoints/checkpoint-33616`
3. 300 updates → `out/blank-shaped-4layers/continue/checkpoint-45496`
4. 1500 updates → `out/blank-shaped-4layers/continue-20260912/checkpoint-236556`
   (stopped itself at 09:55:19 via a `stop-at-1500.json` watchdog, not by failure)
5. current → `out/blank-shaped-4layers/continue-20260912b/`, seeds from 2400232800

Flags, unchanged across steps 3–5:

```
--stage 2 --rounds 4 --temperature 2.5 --movement-entropy 0.05 --entropy-final 1
--learning-rate 3e-4 --updates 10000 --report-every 100 --device cuda
--waste-penalty 1 --fleet-weight 0.03 --tech-weight 0.1 --strategy-diversity-weight 1
--r1-bonus 2 --fleet-hoard-penalty 1 --zero-fleet-penalty 10
--trade-goods-hoard-weight 0.1 --styx-bonus 1
```

Resuming always starts Adam cold — bundles carry no optimiser state — so checkpoint numbering
restarts from zero and each resume needs **its own output folder**. Creating one requires the
owner's approval; do not create folders unasked.

## 5. Uncommitted work in the shared checkout

HEAD is `22266e1`, six commits ahead of `origin/main`, nothing pushed. The working tree holds
**three separate streams**, and none of them are in the running trainer (it is a frozen binary
built at `22266e1` before any of this):

1. **Someone else's reward work** (`choice.rs`, `progress.rs`, `reward.rs`, modified 09:35 today,
   not mine). Fleet value now counts infantry and mechs in space and on planets; new
   `Progress::in_fracture` and `Observed::has_units_in_fracture`; new reward term
   `fracture_entry_bonus`. Tests pass: ti4-policy 244, ti4-training 142.
   **Hazard: `fracture_entry_bonus` defaults to 0.1, not 0, and `ppo_update` has no
   `--fracture-entry-bonus` flag.** Any rebuilt trainer therefore changes the objective silently
   and cannot be switched off from the command line. Add the flag before rebuilding.
   `choice.rs` is an engine change, so the ti4-sim behaviour suite must run before it merges.
2. **My A3 overlap patch** (`ppo.rs`, `lib.rs`) — a producer thread assembling the next
   minibatch while the GPU works on the current one. It compiles (one `unused_mut` warning) but
   **has never passed tests or the CPU equivalence gate**. Treat it as unverified. It is
   reproducible from `out/perf-20260911/script-patch_a3.py`, so reverting loses nothing.
3. **My diagnostics patch** (`ppo_update.rs`, part of `ppo.rs`) — adds `--no-device-batch` and
   per-update parameter/Adam digests under `--hash-games`. Used to prove the device and host
   assembly paths agree; harmless, off unless asked for.

## 6. Operational hazards, all learned the hard way

- **CUDA training is not bit-repeatable** (embedding-bag backward accumulates with atomics;
  30 replays of one batch gave 30 distinct parameter digests). Prove any optimiser change on
  **CPU**, where a full update reproduces a checkpoint bit for bit. Never gate on a CUDA output.
- **A detached launch must set the working directory to the repo root.** The trainer resolves
  `out/pools/full_np8_12_train.json` relative to the CWD; `Win32_Process.Create` starts in a
  system directory, and the run refuses instantly. Pass the root as `Create`'s second argument,
  or `cd /d` inside the batch file.
- **`GIT_COMMIT` must be set** or writing a checkpoint is refused: `git_commit must be a recorded
  7-64 digit hexadecimal commit`.
- **Do not run `cargo test -p ti4-training -p ti4-mlp` together** — both crates have an example
  named `vp_sources` writing the same output path, which fails with LNK1104. One crate at a time.
- **ti4-bridge's six `hexsummary_golden` tests fail on main**: `crates/ti4-bridge/tests/golden/`
  was never committed (it exists only in `37b236b` on a work branch). Pre-existing since
  2026-09-10, unrelated to training.
- This is a **shared checkout** with other sessions working in it. Do not switch its branch, do
  not reset or stash, append rather than overwrite under `out/`.

## 7. What I would do next

In rough order of value:

1. **Treat the Jol-Nar clearance-for-VP trade as the real problem.** It is not the known
   early-lag pattern — that is a seat climbing from blank with VP at the table average. This is
   a trained policy falling 35 points while its VP leads the table. The distinguishing test is
   the VP direction.
2. **Add `--fracture-entry-bonus` to `ppo_update`** before any rebuild, so the new default of
   0.1 is a choice rather than an accident.
3. **Decide A3**: either finish it (lib tests, then the CPU equivalence gate against
   `checkpoint-140-baseline`, pattern in `out/perf-20260911/script-a2_verify.sh`) or revert it.
   It should not sit half-applied in a shared tree.
4. **Re-evaluate a later checkpoint against `checkpoint-236556`** to see whether the regression
   is deepening or turning. Roughly 25 minutes for both arms alongside the trainer.

## 8. Where things are

- Greedy evals, CSVs, paired analysis, protocol: `out/blank-shaped-4layers/greedy/`
  (`paired-236556-vs-checkpoint-184960.txt`, `script-paired_eval.py`, `provenance.txt`)
- Current run: `out/blank-shaped-4layers/continue-20260912b/` (`train.out.log`, `run.cmd`,
  `provenance.txt`)
- Trainer performance work, reports and frozen binaries: `out/perf-20260911/`
  (one update went 20.32 s → 17.10 s; `report-performance.md` is the consolidated account)
- Earlier handover, still accurate on performance: `plans/HANDOVER_2026-09-12_TRAINING_AND_PERFORMANCE.md`
- The governing plan for the performance work: `plans/TRAINING_PERFORMANCE_HANDOFF_2026-09-11.md`
