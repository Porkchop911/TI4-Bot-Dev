# Handover: blank-start shared training, best-game records, faction fine-tuning — 2026-10-18

Branch `claude/negotiation-penalty-projection-rank-4e48b3` (worktree
`.claude/worktrees/negotiation-penalty-projection-rank-4e48b3`), tip `14943dc5`. Contains the Feelx
merge (rules 2–11, rule bot v5 staging), the Crimson setup fix and link-aware adjacency (`22b87bbc`,
`804bc74c`, merged in by another session). All runs below are in the main checkout's `out/`.

## State right now

| | |
|---|---|
| Running | **nothing**. GPU and CPU free. |
| Main line | two identical shared arms (A, B; seeds differ), stopped at **round 1,400** of a 5,000-round continuation |
| Best model | **arm A @1400**: `out/ppo-blankwide-shared-rec-A-sftt03-cont5000-20261018/checkpoints/checkpoint-128900` (3.38 VP greedy, wide holdout; −0.12 VP vs champion 16448) |
| Arm B @1400 | `out/ppo-blankwide-shared-rec-B-sftt03-cont5000-20261018/checkpoints/checkpoint-121196` (3.23 VP; −0.22 vs 16448) |
| Disk | D: 172 GB free; the two continuation folders hold ~6 GB each (16k records each) |
| Pending decision | restart A and B from round 1,400 **on the new engine** (Crimson fix) — rebuild first, see "How to run" |

The arms were stopped before the Crimson fix landed; the binary in `target/release` has since been
rebuilt with it, but rebuild again before launching (later merges may have touched the engine).

## Lineage of the main line

| Stage | Folder (`out/…`) | Rounds | Settings |
|---|---|---|---|
| Blank bundle | `blank-like16448-diplomacy-arena-20261017` | 0 | untrained twin of 16448's architecture (schema 12, diplomacy head, battle predictor) |
| Six factions, shared | `ppo-blank6-shared-long1000-20261017` → `checkpoint-33240` | 450 | arm-2 rewards (below), `--roster six` |
| Wide, recording | `ppo-blankwide-shared-rec-{A,B}-20261017` | 100 (stopped at 108) | `--roster wide --faction-focus 1`, recording on |
| + SftT at 0.3 | `ppo-blankwide-shared-rec-{A,B}-sftt03-20261017` | 450 | `--sftt-weight 0.3` from here on |
| Continuation | `ppo-blankwide-shared-rec-{A,B}-sftt03-cont5000-20261018` | 1,400 of 5,000 | unchanged |

Rewards throughout ("arm-2" of the negotiation-penalty line): vp 1, objective 0.3, secret 0.2,
zero-terminal-objectives, home-loss 0.5, tech 0.1, projection 0.02 (kept at horizon), projection
rank 0.3 (mean of projection rank and non-home-planet rank), negotiation step penalty 0.0005,
**r1 clearance terms off**, round-one expansion 0.5 per resource+influence of non-home planets,
temperature 2.5, lr 1e-4, entropy 0.25 constant, GAE λ 0.95, 30 seeds × 1 rotation per round,
`--rollout-backend infer`, `--phase-lock out/` (two arms alternate CPU rollout / GPU optimise).

Configs: `scripts/long_blank6_shared.psd1`, `scripts/long_blankwide_shared_rec_{A,B}*.psd1`
(the `_cont5000` pair is the current one).

## Results (greedy T 0.001, wide roster, holdout pool, diplomacy on, paired by map seed)

| Model | Mean VP | vs previous | vs champion 16448 (60 seeds) |
|---|---|---|---|
| shared@450 (six-faction trained) on wide | 2.675 | | |
| A / B after wide 100 + SftT 450 (60 seeds) | 3.059 / 3.058 | +0.38 each | −0.40 / −0.40 |
| **A @1400** (300 seeds) | **3.375** | +0.32 [+0.29, +0.35] | **−0.12** [−0.21, −0.03] |
| B @1400 (300 seeds) | 3.229 | +0.14 [+0.11, +0.18] | −0.22 [−0.31, −0.14] |

Per-faction tables are in each folder's `eval*.per-seat.tsv` / `.log`. Clearance is not rewarded;
arm A drifted to low clearance at no VP cost and later recovered (27% → 49% at training temperature).

### Other experiments this session (all done, all stopped)

| Experiment | Result |
|---|---|
| Negotiation penalty 0.0005 + rank 0.3 from 16448 (`ppo-negpen-rank03-*`) | arm with r1 clearance: −0.08 VP vs parent at 300 total updates; arm with round-one expansion instead: −0.03 (n.s.). Haggling grew in the clearance arm (91k → 115k decisions/update), not in the expansion arm |
| Blank six-faction: shared vs separate per-faction models (`ppo-blank6-{shared,separate}-*`) | at 450: shared 3.17 VP, separate 2.78 (−0.39). Cause: each separate model gets 1/6 of the data and ~1/6 of the optimiser steps |
| Sardakk-for-Hacan, separate models + cross-faction imitation (`ppo-blank6sar-*`) | stopped at round ~15 by the operator; no result |
| Rule bot v4 on the MLP protocol (`out/eval-rulebot-v4-wide-*`) | diplomacy off 3.08 VP, on 2.57; 16448 off 2.11, on 3.46. The MLP depends on diplomacy; the rule bot answers diplomacy at random |
| Rule bot golden bench on the merged branch | 200/200 hashes differ from `baseline.json` (baseline predates 71 Feelx engine commits); pace identical (+0.004 VP/seat/round [−0.016, +0.023]) |

## Best-game records

Every seat that ends at ≥ 6 VP (Hacan ≥ 7) writes its exact PPO training records to
`<run>/best/<faction>/u<update>-s<seed>-r<rotation>-<seat>-vp<points>.ti4ppo.zst`:
`TI4PPO01` capture in one zstd frame, read by `ti4_mlp::perf::read_capture` (plain or compressed);
`ppo_replay` trains on one directly.

- ~250–300 KB per record; 8–12 records per round per arm (more as play improves) → ~2.5–3 GB per
  1,000 rounds per arm. Uncompressed would be ~55× larger.
- Records were played at **training temperature 2.5** (exploration) and come from a moving policy.

## Faction fine-tuning (Sol): a dead end in its imitation form

Tool: `faction_finetune` (advantage-weighted cloning on one faction's records, one unit of weight
per record, a tenth held out, Adam). Bench: `clearance_eval --faction-bundle sol=<model>` (only Sol's
seats use it), 300 seeds = 360 Sol seat-games per arm, paired against the same base.

| Fine-tune | Sol VP change (95% CI) |
|---|---|
| Round-450 finals, each arm's own records (60-seed bench) | A +0.23, B +0.03 — noise, see next row |
| Round-450 finals, all 403 records, 300 seeds | pooled **−0.15** [−0.26, −0.03] |
| Round-1,400, each arm's ~780 fresh records | pooled −0.03 [−0.16, +0.10]; A alone −0.15 |
| Round-1,400 A, **102 greedy records from that one checkpoint** (`--collect-only`) | **+0.03** [−0.15, +0.22] |

Reading: held-out imitation loss starts at 3.5–4.2 on training-temperature records (the model gives
low probability to many recorded exploratory choices) and at 0.36 on greedy records (it already
plays them). Exploration data adds noise and costs points; greedy data adds nothing. A gain needs a
signal that separates better moves from worse — RL fine-tuning (PPO with only Sol learning, other
seats frozen on the shared model) or search — not copying. Other factions' seats were unaffected in
every bench.

## Code built this session (all on the branch)

| Where | What |
|---|---|
| `ppo_update` | `--negotiation-step-penalty`, `--r1-expansion-weight`, `--sftt-weight`, `--factions a,b,c,d,e,f`, `--separate-models` (+ `--self-imitation` cross-faction relabelling, per-faction resume), `--record-best <dir> --record-vp 6,hacan=7`, `--collect-only`, `--require-faction` |
| `ppo_train.ps1` | allow-lists the above; `SeparateModels = $true` setting |
| `ti4-training::reward` | `negotiation_step_penalty`, `r1_expansion_weight`, `sftt_weight` (default 1), projection rank = mean of projection and non-home-planet ranks |
| `ti4-policy::progress` | `non_home_planet_value`, `support_for_the_throne_points` |
| `ti4-mlp` | `perf::write_capture_zstd`, `read_capture` opens both; `bundle::separate_model_bundles` / `trained_updates`; batched advantage critic in `advantage_weighted_clone_loss` |
| `clearance_eval` | `--rule-bot`, `--factions`, separate-model layouts, `--faction-bundle` |
| `faction_finetune` | new example (above) |
| `blank_bundle` | `--diplomacy`, `--predictor` |
| reviewer / replayer | power overlay fixes (stale map, both Nexus faces, galaxy brought to the frame); loads separate-model runs |

## How to run

```bash
cd D:/Projects/ti4-engine-rs/.claude/worktrees/negotiation-penalty-projection-rank-4e48b3
export LIBTORCH='D:\Projects\ti4-engine-rs\out\libtorch-2.9.1-cu128-20260908' LIBTORCH_BYPASS_VERSION_CHECK=1
cargo build --release -j 30 -p ti4-mlp --example ppo_update --example clearance_eval --example faction_finetune
```

Restart the arms from round 1,400: copy the `_cont5000` configs, set `Bundle` to the checkpoints
above, `Run` to a new folder, `updates` 3600, `seed-base` +1400×30 (A 1270058500, B 1271058500),
then `scripts/ppo_train.ps1 -Config <psd1>` with `RAYON_NUM_THREADS=30` (both together with the
phase lock: ~20 s per round each, ~20 h). The optimiser state starts fresh on every resume.

Greedy eval (≈2 min per 60 seeds, ≈10 min per 300): `clearance_eval --bundle <ckpt> --map-pool
out/pools/full_np8_12_holdout.json --roster wide --seeds 300 --seed-base 900000000 --rounds 4
--temperature 0.001 --diplomacy --per-seat <file>`; pair by `(seed, rotation, faction)` and
bootstrap over seeds. `scripts/paired_vp_bootstrap_20261011.py` fails on six-faction tables (no
"new" factions) — pair inline.

Pause/resume trainers without killing them: hold both `out/ppo-{rollout,optimise}.lock` files
(a PowerShell `FileStream.Lock` loop); they stop at the next phase and continue when released.

## Pitfalls met

- Python heredocs writing Windows paths: `\t`, `\b` become control characters. Build paths with
  `chr(92)` or forward slashes.
- In bash, `"$O\\$r"` does not expand `$r` (`\$`); use forward slashes.
- A rebuild fails to link while a trainer runs from the same `ppo_update.exe`.
- Faction focus (`--faction-focus 1`) ignores `--roster` and draws wide; use 0 with `six` or
  `--require-faction`.
- Per-faction eval samples are small (Sol: 96 seat-games per 60-seed eval, ±0.4 VP); use ≥ 300 seeds
  for one-faction questions.

## Open choices for the operator

1. Restart A and B from round 1,400 on the Crimson-fixed engine (3,600 rounds left).
2. RL fine-tune for one faction: `--opponent` = the shared model frozen, learner = that faction only.
3. Stop recording best games (≈3 GB per 1,000 rounds per arm) now that imitation fine-tuning is shelved.
