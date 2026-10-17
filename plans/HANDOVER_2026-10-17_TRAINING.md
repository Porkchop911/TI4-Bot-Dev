# Handover: reward-terms training line, 2026-10-17

Branch `claude/reward-terms-training-e8c88a` (worktree `.claude/worktrees/reward-terms-training-e8c88a`),
tip `17194e22`, not pushed. Built on `wp/base-factions` (1f3556d1) + `pp/dev` (c955cd15, ti4-infer)
+ the engine timing fix from `wp/rule-bot` 98fe8b1d6. **Read
[HANDOVER_2026-10-17_MEMORY_CORRUPTION.md](HANDOVER_2026-10-17_MEMORY_CORRUPTION.md) first**: the
machine shows RAM-cache corruption; reboot and test memory before the next long run.

## State

| | |
|---|---|
| Running | nothing; GPU and CPU free |
| Champion line | `out/ppo-focusgae-FG-wide-long1000-20261010/checkpoints/checkpoint-16448` (unchanged) |
| Last long run | R, stopped itself at update 608, worse than control (below) |
| Diplomacy limits | confirmed by the operator 2026-10-17, unchanged (below) |

## What was built

Reward (`crates/ti4-training/src/reward.rs`, flags in `crates/ti4-mlp/examples/ppo_update.rs`,
allow-listed in `scripts/ppo_train.ps1`). All default off; old runs score exactly as before.

| Flag | Meaning |
|---|---|
| `--zero-terminal-objectives 1` | scoreable public/secret counts zeroed at the horizon: `objective_weight`/`secret_weight` become pure shaping |
| `--home-loss-weight w` | potential −w per home planet not controlled (`Progress::home_planets_lost`, from `objectives::home_planets_not_controlled`, 61.16) |
| `--keep-terminal-projection 1` | projection stands at the horizon (last measured value) instead of being zeroed |
| `--projection-rank-weight w` | per round, `w·(below−above)/(seats−1)` on projection rank, credited at the seat's last decision of the round. Shortcut: each seat measured at its own last decision of the round, not one shared state |

Trainer and evaluator:

| Change | Why |
|---|---|
| `--rollout-backend infer` (from pp/dev) | f32 ti4-infer rollouts. On checkpoint-16448 and R update 600: value gap ≤4e-6, log-prob gap ≤3e-4, rollouts 2.7× faster (23.8 s → 8.7 s per 4-seed update). Needs a shared critic. |
| `--phase-lock <dir>` | file locks `<dir>/ppo-rollout.lock` / `ppo-optimise.lock`; concurrent arms alternate CPU rollout and GPU optimise. Two arms ran at ~18 s/update each instead of 70–90 s in lockstep |
| `--check-likelihood` now also checks recorded critic values | catches backend value drift |
| No-progress-guard games skipped (≤10% of an update), runaway seats (>6000 asks) dropped | engine stalls killed whole updates; with the timing fix they no longer occur (0 in 360-game evals, 0 in the long run) |
| `clearance_eval` skips guard stalls (refuses above 5%) | same |
| Freeze refusal names the failing field | the update-608 message was not actionable |
| `TI4_DIPLOMACY_CENSUS=1` | one stderr line per seat-game: diplomacy choices by option kind |

## Results

All evals: greedy (T 0.001), holdout `full_np8_12_holdout.json`, wide roster, diplomacy on,
60 seeds × 6 rotations (360 games, 2160 seat-games), paired cluster bootstrap by map seed
(`scripts/paired_vp_bootstrap_20261011.py`; run Python with `-I`, see memory handover).

Pilot (50 updates each from checkpoint-16448, interleaved): C = FG rewards; R = objective 0.3 /
secret 0.2 zeroed, home-loss 0.5, clearance 0, projection 0.02 kept, rank 1.5 per round.

| | mean VP | clearance | R − C |
|---|---|---|---|
| C pilot `out/ppo-rewardterms-C-pilot50e-20261016` checkpoint-4600 | 3.453 | 67.8% | |
| R pilot `out/ppo-rewardterms-R-pilot50e-20261016` checkpoint-4576 | 3.411 | 62.8% | −0.04 [−0.12, +0.03] |

R long run `out/ppo-rewardterms-R-long1000-20261017` (from R pilot, fixed engine, infer), vs the C
pilot checkpoint:

| update | mean VP | clearance | R − C |
|---|---|---|---|
| 200 | 3.141 | 62.6% | −0.31 [−0.40, −0.22] |
| 400 | 3.171 | 62.1% | −0.28 [−0.37, −0.21] |
| 600 | 3.105 | 55.1% | −0.35 [−0.43, −0.27] |

Then `REFUSED: freezing: PPO step 71224 is malformed or non-finite` at update 608. Not reproduced
(replay of 600–609 clean on infer and cpu); memory corruption is the only concrete fault found.

## What went wrong in R: diplomacy haggling

Decisions per 30-game update rose from ~101k to ~163k, nearly all on the diplomacy head (census,
same 30 seeds): 56,307 at checkpoint-16448 → 130,784 at R update 600. Growth is inside
negotiations, not in their number (~17–20 initiated per player-game, cap 24):

| choice | 16448 | R 600 |
|---|---|---|
| remove a term | 5,725 | 17,097 |
| set an amount | 4,712 | 15,852 |
| propose | 2,476 | 9,682 |
| counter | 1,495 | 6,793 |
| promises (attack / don't activate / don't attack) | 17,447 | 33,756 |
| pay later (tg / commodities) | 4,154 | 14,467 |
| cancel | 2,897 | 1,098 |

GAE λ 0.95 decays **per decision**, so thousands of extra diplomacy steps per game shorten how far
point credit reaches. Plausible mechanism for the VP loss; not measured. The rank bonus at 1.5/round
(up to ±6 per game against ~3.5 VP) was flagged as outweighing points before launch.

## Diplomacy limits (operator-confirmed 2026-10-17, unchanged)

| Limit | Value | Where |
|---|---|---|
| negotiations initiated per action / per round, per player | 2 / 6 | `ti4-model/src/state.rs` `MAX_NEGOTIATIONS_PER_*` |
| contact the same player | once per turn | `DiplomacyState::consume_initiation` |
| counteroffers per negotiation | 2, alternating = one per side; an abandoned counter spends the chance | `ti4-model/src/diplomacy.rs` `MAX_COUNTERS`; test `a_gift_can_be_proposed_and_two_counters_is_the_limit` now asserts authors proposer → recipient → proposer |
| build steps per revision | 8 (an edit costs 4) | `diplomacy/builder.rs` |
| amount per item / pay-later | 10 / 5 | `diplomacy/builder.rs` |
| terms per side / promissory notes per side | 12 / 1 | `ti4-model/src/diplomacy.rs` |
| option menus: opening / trades / counters / signals | 36 / 12 / 8 / 6 | `diplomacy/candidates.rs` |

## How to run

Build (PowerShell; until the memory issue is cleared use the backup libtorch):

```powershell
$env:LIBTORCH='D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908'; $env:LIBTORCH_BYPASS_VERSION_CHECK='1'; $env:PATH="$env:LIBTORCH\lib;$env:PATH"
cargo build --release -j 30 -p ti4-mlp --example ppo_update --example clearance_eval
```

Train: `scripts/ppo_train.ps1 -Config <psd1>`; set `RAYON_NUM_THREADS` first (the trainer
otherwise takes all 32 threads per process). Configs: `scripts/pilot_rewardterms_{C,R,T}.psd1`,
`scripts/long_rewardterms_R.psd1`. Periodic evals: `scripts/long_rewardterms_R_evals.sh`.

Pitfalls met this session:

- Windows PowerShell 5.1 with `ErrorActionPreference=Stop` turns any native stderr line into a
  terminating error: run `clearance_eval` from bash or without `Stop`.
- PowerShell `2>` re-wraps stderr lines; capture census output from bash.
- Writing Windows paths from Python heredocs: `\t` in `\ti4-engine-rs` becomes a tab. Use raw
  strings or the Edit tool.
- Sorting checkpoints by directory name is wrong (`checkpoint-88` vs `checkpoint-4600`); use
  `manifest.json` `source` ("N update(s)").
- Several arms in lockstep on one GPU/CPU are slower than one at a time; use `--phase-lock`.

## Open choices for the operator

1. R again with projection rank 0.3 (or 0), from checkpoint-16448, 50-update pilot vs C.
2. A C (control rewards) long run on the fixed engine with infer rollouts.
3. Whether diplomacy steps should count toward GAE decay (e.g. decay per non-diplomacy decision or
   per round) so haggling cannot dilute credit. Untested idea.
