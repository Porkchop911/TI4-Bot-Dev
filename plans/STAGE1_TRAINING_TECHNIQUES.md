# Stage-1 training techniques

What actually works for getting a Stage-1 MLP champion to high clearance with low waste, as of
2026-09-05, after the OBS-002–012 observation-contract rework (vocabulary generation `fa3d6f9...`,
`oov_registry_version: 10`) retired every previously-trained checkpoint (all schema 6; the current
loader hard-requires schema 7, no migration path — see "Why every old champion is gone" below).

Target this session was asked to hit: **≥95% clearance, ≤5% waste**, both at the standard
measurement convention (below).

## Measurement convention

- **Clearance**: greedy (`--temperature 0.001`), `clearance_eval`, Validation pool
  (`out/pools/full_np8_12_holdout.json`), 600 seeds x 6 rotations = 21,600 seat-games. This is the
  only number that means "stage-1 clearance." Report ± the tool's own standard error.
- **Waste**: a *wasted activation* is a tactical action that accomplished nothing. Measured two
  ways, kept separate on purpose (a policy can look good on one and bad on the other):
  - `any-waste`: fraction of seat-games with **at least one** wasted activation.
  - `waste/tactical`: fraction of **individual tactical actions** that were wasted.
  Measured greedy via `build_positive_corpus --temperatures 0.001` (reports both from the `TABLE`
  line), Train pool by that tool's own design (a corpus is training data and must not be built from
  the seeds a policy is scored on) — clearance from this tool is a cross-check against
  `clearance_eval`, not a replacement for it, since it runs on a different pool.
- **Never trust the in-training report table as either number.** `ppo_update`'s periodic
  `report after update N` table is aggregated straight from the self-play rollout batches used for
  that window's PPO update, at whatever `--temperature` was passed for acting (e.g. 0.25) — not
  greedy. It is a trend diagnostic only. The gap to the real greedy number can be large in either
  direction (one checkpoint this session read 50.33% clearance at sampling temperatures against an
  89.43% greedy score for the exact same weights). Always confirm a checkpoint you intend to keep
  with a real `clearance_eval` + waste measurement before believing anything about it.

## Why every old champion is gone

`ti4_mlp::bundle::read` (`crates/ti4-mlp/src/bundle.rs:50`) hard-requires `schema == 7`, with no
migration path by design (`OBS-003i`, this session). Every checkpoint under `out/champions/` —
including the historical champion lineage that reached 94.97%/1.11% waste before this rework — is
schema 6 and is refused outright. The stored training corpora (`out/corpus/positive`,
`out/corpus/rescued`) are equally orphaned: they store trajectory *specifications* (seed, rotation,
faction, temperature, option id), and replaying them requires loading the exact frozen generating
policy to reproduce the other five seats' moves — which is the same unloadable schema-6 bundle.
There is no partial reuse available; a fresh champion has to be trained from a blank, schema-7,
current-vocabulary bundle (`blank_bundle`, `--like` omitted so it takes the vocabulary in force
rather than pinning to whatever generation a reference bundle happened to use).

## What was tried, in order, and what it means

### 1. BC-cloning on a filtered trajectory corpus, with a waste ceiling (the old recipe)

The historical, pre-rework recipe (`scripts/clone_round.ps1`, `plans/CHAMPIONS.md`): generate a
corpus of clearing, non-wasteful trajectories at several sampling temperatures
(`build_positive_corpus`), search for lines that rescue the policy's own current failures
(`rescue_search`), then behaviour-clone on both with `corpus_train --waste-ceiling`, which only
accepts an epoch's checkpoint if it is under the ceiling — otherwise it correctly reports "NO
IMPROVEMENT" rather than pick a checkpoint that violates the constraint.

Re-run this session from a fresh baseline (`stage1-fa3d6f9/checkpoint-64340`, 2050 plain-PPO
updates from blank, no waste term, 89.43% greedy clearance):

| round | corpus source | result (24 epochs each) |
|---|---|---|
| r1 | checkpoint-64340 | waste 52.31%→33.22% any-waste; clearance 89.43%→90.04% (both real, both held-out-confirmed) |
| r2 | r1's epoch-24 | waste 33.22%→28.36%; clearance 90.04%→89.23% (regressed) |

**Diminishing, then negative, returns.** Round 2's waste improvement was 4x smaller than round 1's,
and clearance started falling instead of holding. The historical version of this recipe worked
because it started BC from an already-mature ~93.58% PPO champion; starting from a much younger
2050-update baseline gives BC weaker demonstrations to imitate from; even the trajectories it is
allowed to keep (cleared, zero waste) reflect a less mature policy. **Conclusion: BC alone, from a
young baseline, plateaus far short of both targets. It is a refinement tool, not a from-scratch
one.**

### 2. Waste-penalty PPO on top of an already-trained policy (also the old recipe)

Historical finding (`plans/CHAMPIONS.md`'s `waste_sweep.ps1`, on the *mature* 93.88% champion
`mixed-epoch14`): a `--waste-penalty` PPO reward term is extremely effective at *removing waste* —
39-fold, 0.277→0.011 waste/tactical — but does it mostly by teaching the policy to take fewer
tactical actions (3.510→1.873/seat) rather than to activate better, so clearance falls with it
(93.88%→93.23% at that penalty; other arms worse).

Repeated this session on round 1's epoch-24 (90.04% clearance, a *less* mature policy than the
historical experiment used), penalty 8, 500 updates:

| checkpoint | clearance (greedy) | any-waste (greedy) |
|---|---|---|
| start | 90.04% | (33.22% before this pass) |
| update 50 | 73.05% | 9.16% |
| update 500 | 72.25% | **3.15%** |

Waste target hit cleanly; clearance collapsed, and **catastrophically for one faction** (letnev
15.53%, down from ~88%). Critically, the damage was already almost fully present by update 50 and
never recovered over the remaining 450 — this is not a "stop earlier" problem, it is a "this
penalty, at this baseline maturity, is simply too strong" problem. The same lever applied to a
younger, less-robust policy breaks specific factions' strategies outright rather than trimming
waste broadly the way it did on the mature champion. **Conclusion: retrofitting a strong waste
penalty onto an already-trained policy is a real, fast, effective lever for the waste number alone,
and a bad idea as the main strategy unless the policy is already strong enough to have slack to
give up.**

### 3. PPO with the waste term from update zero (the one that actually worked)

The insight: train clearance and waste-avoidance as **one objective from the start**, rather than
teaching clearance first and then punishing an already-formed habit. From a fresh
`blank_bundle` (schema 7, current vocabulary, width 256, capacity 20480, shared critic), the same
base recipe that produced the plain 89.43% baseline (`--temperature 0.25 --movement-entropy 0.05
--seed-base 650000000`, matching `checkpoint-64340`'s own recipe exactly for a clean comparison),
plus `--waste-penalty 5` present for every update from the first:

| update | clearance (greedy) | any-waste (greedy) |
|---|---|---|
| 1000 | 89.84% ±0.40 | 4.89% |
| 1600 (training paused here) | **93.40% ±0.33** | **2.31%** |

Both numbers improved **together**, not traded off — the opposite of experiment 2's pattern. At
update 1600, every faction individually clears above 90% (letnev 92.72%, the faction that broke
worst under retrofitted penalty here holds fine), and waste is comfortably under the 5% ceiling with
margin. This is the best result of the session by a wide margin and the clearest lesson: **when
waste-avoidance is wanted, bake it into the reward from blank rather than adding it after the fact.
A young policy has no established wasteful habits to break; it just learns not to form them.**

Training was stopped at update 1600 (of a planned 2050) at the user's request, at a checkpoint
boundary (`--report-every 100`, so the stop lands exactly on a saved, reload-verified checkpoint —
`ppo_update` publishes at every report window specifically so a stop or crash costs at most one
window). 450 updates remain against the original budget; the trend at the stop point was still
positive on both axes (see `plans/HANDOVER_2026-09-05_STAGE1_RETRAIN.md` for the exact resume
command and current artifact paths).

## Recipe, distilled

For a from-scratch Stage-1 champion under a fresh observation contract:

1. `blank_bundle` (no `--like`) — takes the accepted vocabulary generation automatically.
2. `ppo_update --stage 1 --rounds 1 --temperature 0.25 --movement-entropy 0.05 --waste-penalty
   <N> --updates <budget> --report-every <k> --device cuda`, waste-penalty present from update 0.
   `N=5` worked well starting from blank; do not assume it transfers to a fine-tuning pass on an
   already-trained policy (see experiment 2) — that needs a much gentler value and a
   clearance-floor gate, not this recipe.
3. Confirm every candidate checkpoint with real `clearance_eval` (greedy, Validation, 600 seeds)
   and `build_positive_corpus --temperatures 0.001` (greedy waste, Train pool) — never keep a
   number read off the in-training table.
4. `behaviour_report --bundle <checkpoint>` for strategy-card pick and secondary-follow-rate
   sanity checks — CPU-only, safe to run beside a GPU training job, no separate profile/tool needed
   for MLP bundles (the equivalent linear-`Profile` tools in `ti4-training/examples/` do not accept
   MLP bundles at all).
5. Only reach for BC-cloning (`build_positive_corpus` + `rescue_search` + `corpus_train
   --waste-ceiling`) as a *refinement* pass on an already-strong PPO baseline, not as the primary
   route from blank.
6. Only reach for a standalone waste-penalty PPO fine-tuning pass as a *gated finishing move* on a
   policy that already clears comfortably above the target floor, accepting only checkpoints that
   do not regress below that floor (mirrors `plans/STAGE2_RUN1.md`'s "clearance becomes an
   acceptance gate, not a coefficient" finding for a different problem) — not as the main lever, and
   not from a young baseline.

## Tools (`crates/ti4-mlp/examples/`)

| tool | what it does | device |
|---|---|---|
| `blank_bundle` | untrained schema-7 init at the accepted vocabulary | CPU (init only) |
| `ppo_update` | PPO update loop; `--waste-penalty` adds the reward term; `--stage 1\|2 --rounds N` | rollout CPU, optimiser follows `--device` |
| `build_positive_corpus` | plays games, keeps clearing+non-wasteful trajectories; also the standard greedy waste-measurement tool at `--temperatures 0.001` | CPU only (§7.1: no CUDA inference backend) |
| `rescue_search` | branches from the policy's own current failures looking for a clearing line | CPU only |
| `corpus_train` | BC on a corpus (+ optional rescued share), `--waste-ceiling` gates checkpoint acceptance | rollout/replay CPU, gradient step follows `--device` |
| `clearance_eval` | the rigorous greedy clearance number, Validation pool | CPU only |
| `crossplay_eval` | Stage-2 margin against a frozen opponent (not used for Stage-1 clearance) | CPU only |
| `behaviour_report` | strategy-card picks and secondary follow rate, by faction | CPU only |

Inference (`ti4_tensor::inference_device()`) is unconditionally CPU by design (§7.1/§7.2's
determinism argument), so every tool above except the gradient step inside `ppo_update`/
`corpus_train` can run concurrently with a GPU training job without contention.
