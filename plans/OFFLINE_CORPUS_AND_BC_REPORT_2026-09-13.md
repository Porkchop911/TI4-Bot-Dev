# TI4 Offline Self-Play Corpus and Behavior-Cloning Training Report

**Date:** 2026-09-13  
**Repository:** `D:\Projects\ti4-engine-rs`  
**Generation corpus:** `E:\ti4-corpus\pilot-retained-32k-20260913`  
**Base checkpoint:** `out\blank-shaped-4layers\shaped-r1bonus3-20260912\checkpoint-318956`  
**Trained checkpoint:** `out\offline-bc-32k-20260913-full-v1`

## 1. Executive summary

This work established an offline self-play data path for six TI4 factions, generated 32,768 games
with a deliberately heterogeneous policy population, retained 5,374 outcome-selected games, and
trained the existing MLP actor from checkpoint 318956 on a curated set of 1,186,627 training
decisions. A game-separated validation set contained 142,721 decisions. Five behavior-cloning
epochs produced 1,450 optimizer steps and reduced validation negative log-likelihood (NLL) from
1.33182 after epoch 1 to 0.98588 after epoch 5.

That loss improvement proves that the resulting network became better at imitating the selected
actions. It does **not** yet prove that the network became a stronger TI4 policy. The paired
gameplay evaluation requested after training has not been run. Until base checkpoint 318956 and
the trained checkpoint are compared on identical maps, seeds, faction rotations, and opponents,
the gameplay result remains unknown.

The project also exposed significant engineering failures. The first corpus packer and the first
training loader were serial. This left almost all CPU cores and the GPU idle for long periods. A
CUDA availability check was performed too late, causing an entire sample-selection pass to be
discarded when the binary was found to be linked against CPU-only libtorch. Several attempts were
therefore spent on setup rather than optimization. The final successful path fixed the central
bottleneck by exploiting the 5,374 independent zstd frames in the original corpus and decoding
them concurrently with 24 workers before beginning CUDA optimization.

For future corpora, the operator has made an explicit decision: generated datasets will be
uncompressed. The repository briefly implemented plain `decisions.jsonl` and `games.jsonl` output
in commit `412f706`, and the offline BC reader gained plain-JSONL support in `1d9c14f`. However,
the later commit `8bb530c` changed the generator back to bucketed zstd output. The current generator
therefore conflicts with the operator's stated requirement and must be corrected before another
generation run.

## 2. Objective and scope

The objective was to create a training-ready offline corpus for policy pretraining, with enough
variation to support:

- behavior cloning from successful trajectories;
- later pairwise ranking between stronger and weaker trajectories;
- matched-state action-preference analysis;
- subsequent PPO fine-tuning;
- stratification by faction, seat, map, policy type, and outcome.

Only six factions were in scope:

1. Jol-Nar;
2. Letnev;
3. Sol;
4. Xxcha;
5. Hacan;
6. L1Z1X.

The corpus was deliberately not generated from one checkpoint at one temperature. Such a corpus
would contain many correlated trajectories and would mostly teach the student to reproduce one
policy's existing blind spots. Instead, games used a rotating mixture of learned, historical,
heuristic, evolutionary, high-temperature, and strategically biased policies.

## 3. Generated policy population

Eleven policy variants were cycled deterministically across seats. Across the unfiltered 196,608
seat trajectories (32,768 games times six seats), this made assignment approximately uniform
before outcome retention. Retention can and does change the final proportions because policies do
not have identical performance or game length.

| Policy ID | Family | Source/configuration |
|---|---|---|
| `current_mlp_t025` | MLP | checkpoint 318956, temperature 0.25 |
| `current_mlp_t100` | MLP | checkpoint 318956, temperature 1.0 |
| `current_mlp_t250` | MLP | checkpoint 318956, temperature 2.5 |
| `older_mlp_t025` | MLP | checkpoint 236556, temperature 0.25 |
| `older_mlp_t250` | MLP | checkpoint 236556, temperature 2.5 |
| `authored_heuristic` | heuristic | scored authored bot, temperature 1.5 |
| `evolutionary_linear` | evolutionary linear | faction profile from `final10000.zst`, temperature 1.0 |
| `heuristic_aggressive` | strategically biased heuristic | combat and activation preference |
| `heuristic_defensive` | strategically biased heuristic | production, sustain, retreat, and repair preference |
| `heuristic_economy_first` | strategically biased heuristic | trade, production, refresh, and spending preference |
| `heuristic_objective_first` | strategically biased heuristic | scoring, research, and objective preference |

The biased policies used the authored heuristic as fallback. When at least one legal action matched
the bias, the bot narrowed to matching actions with probability 0.8. This mechanism creates
behavioral diversity without inventing illegal actions or bypassing the normal decision interface.

Behavior probabilities were not recorded. That is acceptable for one-hot behavior cloning and
offline ranking. It means the corpus cannot be treated as an importance-sampled off-policy PPO
rollout without reconstructing or replacing the behavior-policy likelihoods.

## 4. Game randomization and reproducibility

The generation job used:

- 32,768 scheduled games;
- 32 workers;
- four requested rounds per game;
- seed base `1026091400`;
- training map pool `out/pools/full_np8_12_train.json`;
- map-pool SHA-256
  `106153d4384435b19bd27d7210140b4b46da84c72d7e5ce704ffc52083f2c6df`;
- randomized faction order, maps, policy offsets, and policy RNG streams derived from game index.

Each retained game records game and policy seeds, faction placement, map placement, checkpoint
identity, temperature or bias, terminal progress, and per-seat trajectories. Each decision records
the actor's faction and policy, head, chosen action, ordered legal set, canonical sparse actor
features, critic features, progress, and a seat-authorized observation. Hidden information is
limited to what the acting seat was allowed to observe.

The vocabulary digest is
`fa3d6f945988cc9f210fffafff115422c9bf883c077ae8aac8aaf483d1ec41fc`.
Packing checked every recorded feature name against its recorded column and the checkpoint
vocabulary before dropping names from the compact representation.

### Reproducibility caveat

The corpus manifest says `engine_git_commit` is `3f1868d` and that the worktree was clean. The
process began before that commit existed and obtained the repository identity when publishing the
manifest. The recorded commit therefore describes publication-time repository state, not
necessarily the exact executable that generated every game. In particular, the running binary was
built when the standout comparator in the inspected source was `> 6`, whereas the agreed canonical
rule was `>= 6`. Some exactly-six-VP games survived through strong-table, weak-table, or random
control retention, but exactly-six alone was not necessarily sufficient in the running executable.

Future manifests must record a build-time binary identity, not query mutable repository state at
the end of a long run. Recommended fields are executable SHA-256, compile-time Git commit,
compile-time dirty flag, retention constants, and complete CLI arguments.

## 5. Retention design and resulting corpus

The generation process captured complete games in memory and made retention decisions at game end.
Discarded games never wrote full trajectories. Retained games were selected for one of four
reasons:

| Retention reason | Games | Share of retained games |
|---|---:|---:|
| Standout | 1,699 | 31.6% |
| Strong table | 285 | 5.3% |
| Weak table | 1,863 | 34.7% |
| Random control | 1,527 | 28.4% |
| **Total** | **5,374** | **100%** |

The corpus retained 5,374 of 32,768 generated games, approximately 16.4%. It contains 8,177,628
decision records, including forced decisions. The later compact representation excluded forced
choices and contained 7,743,299 non-forced decisions:

| Derived decision bucket | Non-forced decisions |
|---|---:|
| Standout seat | 896,956 |
| Strong-table productive seat | 300,940 |
| Ordinary control | 4,364,971 |
| Weak game | 2,180,432 |
| **Total** | **7,743,299** |

The `strong_productive` label was derived after generation: in a table with at least 24 total VP,
the top three seats were ranked by VP, then scoreable public plus secret objectives, then progress
features. A standout seat was one with at least six VP. This is trajectory-level outcome
selection, not a proof that every chosen action was locally good.

### Why retain weak and control data?

Weak and control trajectories are not needed in full volume for positive-only behavior cloning.
They were retained because the requested dataset was intended to support later ranking, failure
analysis, and critic work:

- weak trajectories supply a lower outcome tail for pairwise comparisons;
- controls reveal ordinary state/action coverage and selection bias;
- all seats in the same game provide context for why one seat succeeded;
- lower-return outcomes are important for value-function calibration;
- failure trajectories reveal recurring action patterns that positive data cannot expose.

If ranking and critic training are not imminent, retaining 2.18 million detailed weak decisions is
not cost-effective—especially once compression is forbidden. A better future policy is to retain
all positive games, retain a bounded stratified weak pool, retain a small random control pool, and
write only summary metadata for the rest.

## 6. Compact representation and data selection

The first post-processing path converted JSONL into a custom compact ragged representation. Each
record retained:

- outcome bucket;
- faction row;
- decision-head index;
- policy hash;
- game hash and deterministic record hash;
- chosen legal-option index;
- option offsets;
- sparse feature columns and f32 values.

The compact train split contained 6,960,434 decisions and occupied 2,314,608,631 compressed bytes.
The validation split contained 782,865 decisions and occupied 261,327,449 compressed bytes. The
split was made by game hash, not by individual decision, preventing decisions from one game from
appearing in both training and validation.

The first successful 100,000-decision run was only a smoke test. It scanned the full packed corpus,
selected 60,000 standout, 30,000 strong-productive, and 10,000 control decisions, and performed
five epochs. Calling that run "training complete" was misleading because it used only about 1.4%
of the packed training split.

The final run did not use that smoke checkpoint. It restarted from base checkpoint 318956 and used
the original retained corpus through a new parallel frame loader.

## 7. Final training set

The final run used every eligible positive decision in the training split plus a deterministic
control sample:

- 799,650 standout-seat training decisions;
- 268,544 strong-table productive training decisions;
- approximately 118,433 deterministic control decisions;
- **1,186,627 total training decisions**.

The held-out validation set contained:

- 97,306 standout-seat decisions;
- 32,396 strong-table productive decisions;
- approximately 13,019 deterministic controls;
- **142,721 total validation decisions**.

Across train and validation, the final curated set contained 1,329,348 decisions. Weak-game
decisions were not behavior-cloned. Most ordinary controls were also excluded.

This selection matches the intent of positive pretraining better than cloning all 7.74 million
non-forced decisions. However, it introduces a strong outcome-conditioned distribution shift. The
model sees actions disproportionately from unusually productive states and seats. Controls partly
mitigate this, but gameplay evaluation is essential.

## 8. Training objective and model treatment

Each selected action was converted into a one-hot target over that decision's ordered legal set.
Training minimized cross-entropy/NLL. The student temperature was fixed at 1.0.

The optimizer settings were:

- learning rate: `3e-5`;
- batch size: 4,096 decisions;
- micro-batch size: 512 decisions;
- epochs: 5;
- total optimizer steps: 1,450;
- CUDA device: RTX 3090 through pinned libtorch 2.9.1 cu128.

Training began from the full actor in checkpoint 318956. A new `preserve_untrained_rows` setting
prevented feature rows absent from this selected corpus from being reset to zero. The pre-existing
distillation implementation otherwise assumes zero initialization and pins absent rows to zero,
which would have silently destroyed useful pretrained weights during fine-tuning.

The objective balances faction contributions within each optimizer batch. It does not explicitly
balance decision heads or policy families. The generator's policy cycle and faction rotation help,
but retained outcome filtering can skew both. Future runs should report and, if necessary,
hierarchically sample by faction, head, policy family, game, and decision.

### Important omissions

The proposed strategy included a KL anchor to the base checkpoint. The implemented run did not
include that anchor. It relied on the low learning rate, limited epoch count, validation NLL, and
preservation of unseen rows. This leaves more risk of catastrophic policy drift than the intended
design.

The bundle reports `critic_mode: shared`. The value readout tensors were not directly optimized,
but the actor trunk was. Because the critic shares that trunk, the critic is **not functionally
unchanged** even when its own readout weights remain fixed. It must be recalibrated or at least
evaluated before PPO.

## 9. Training result

| Epoch | Training NLL | Validation NLL | Cumulative steps |
|---:|---:|---:|---:|
| 1 | 1.39160 | 1.33182 | 290 |
| 2 | 1.17041 | 1.15987 | 580 |
| 3 | 1.05247 | 1.06375 | 870 |
| 4 | 0.98815 | 1.01491 | 1,160 |
| 5 | 0.95373 | 0.98588 | 1,450 |

Epoch 5 was selected. Both training and validation NLL improved monotonically over the five
epochs. The validation gap remained modest, and there was no early evidence of classic supervised
overfitting within this schedule.

The output bundle is schema 8, width 256, with two residual blocks, 14 decision heads, 20,480 slot
capacity, and 14,877 assigned slots. Its manifest records training provenance commit `ff76e61` and
update 1,450. The bundle was written to:

`D:\Projects\ti4-engine-rs\out\offline-bc-32k-20260913-full-v1`

## 10. Why the final run completed quickly

Once data reached the GPU, training was expected to be fast. The actor is a relatively small MLP,
and 1,186,627 decisions at batch size 4,096 require about 290 optimizer steps per epoch. Five epochs
therefore require only 1,450 steps. Sparse legal-option inputs increase preprocessing complexity
but do not turn the model into a large transformer-scale workload.

The surprising wall-clock behavior was caused by data preparation, not model compute. The initial
design spent far more time reading and selecting data than optimizing it.

## 11. Engineering challenges and failures

### 11.1 Serial first packer

The first packer read one combined zstd stream, parsed JSON, validated features, and recompressed
output on one thread. It used roughly one CPU core while the GPU remained idle. This was an
inappropriate architecture for 8.18 million detailed decision records.

The replacement kept one streaming decompressor but parsed 4,096-record batches with 24 Rayon
workers. It reached approximately 8.1 million decisions in minutes rather than leaving the machine
mostly idle. Even that design retained a serial decompression and output stage.

### 11.2 Serial packed-corpus loader

The first trainer loader performed two serial operations before CUDA:

1. a complete SHA-256 pass over approximately 2.58 GB of packed files;
2. a complete decode and deterministic reservoir-selection pass.

The `--workers 24` argument configured Rayon but did not affect this loader. Presenting that flag as
if it made training input parallel was incorrect.

### 11.3 CUDA checked too late

The workspace `.cargo/config.toml` deliberately points to pinned CPU libtorch by default. The first
training launch selected all samples and only then requested CUDA, failing with:

`CUDA was requested but the linked libtorch has no CUDA device`

The machine had a working RTX 3090 and a pinned CUDA libtorch distribution at
`out/libtorch-2.9.1-cu128`. A separate `target-cuda` binary had to be built with the CUDA libtorch
directory prepended to `PATH`. A device probe then confirmed one CUDA device and successful GPU
tensor arithmetic.

CUDA capability should have been a preflight check before any corpus scan. The correct launch
order is: validate device, validate output path, load only minimal manifest metadata, then begin
data preparation.

### 11.4 Repeated discarded work

Several preprocessing passes were stopped or invalidated:

- the serial packer was stopped after its poor utilization became clear;
- the first full sample selection was discarded at the CPU-only libtorch failure;
- another serial full-loader pass was stopped after the operator prohibited one-worker paths;
- the first parallel frame scan correctly refused because raw magic-byte detection found two
  false zstd signatures inside compressed data.

The frame scanner was corrected to validate candidate offsets with zstd's frame-size API in
parallel. The final run found and decoded the 5,374 genuine frames across 24 workers.

### 11.5 Inadequate progress instrumentation

Early pack and load paths emitted no progress until completion. Zero-byte staging files were a
buffering artifact, but without counters they looked stalled. The parallel packer added periodic
decision counts, and the final raw-frame loader reported every 256 completed frames.

Long-running paths should always expose phase, units completed, total units, active worker count,
throughput, memory, and whether CUDA optimization has actually started.

### 11.6 Terminology failure

The 100,000-decision smoke result was initially described too broadly. It had scanned the entire
corpus but trained on only a reservoir sample. Scanning data is not training on data. Reports must
distinguish:

- records generated;
- records retained;
- records parsed or validated;
- records eligible;
- unique records selected for training;
- total example presentations across epochs;
- optimizer steps.

## 12. Statistical and learning risks

### Outcome attribution

Every action from a successful seat is treated as a positive demonstration. Some of those actions
were mistakes rescued by later events, opponent errors, faction strength, or luck. Trajectory
outcome is a noisy label for local action quality.

### Policy-mixture confounding

The selected positive set may overrepresent policy variants that score well under this simulator,
map pool, four-round horizon, or opponent mixture. Without per-policy selection reports, BC may
partly imitate the most retained policy rather than synthesize the best behaviors from all policy
families.

### Faction and context confounding

Faction-balanced loss prevents raw decision volume from dominating by faction, but it does not
fully remove map, seat, speaker-order, policy, or decision-head imbalance.

### Truncated horizon

Games requested four rounds. Six VP in four rounds is a useful standout signal, but strategies
that invest for later rounds may be mislabeled weak. The learned policy may become more
short-horizon and objective-greedy.

### Validation is imitation validation

The held-out split is game-separated, which is good, but its target actions come from the same
generation and selection process. Lower NLL shows generalization within that distribution, not
greater win rate or VP against peers.

## 13. Required gameplay evaluation

The next decision gate is a paired gameplay comparison of checkpoint 318956 against
`offline-bc-32k-20260913-full-v1` using:

- identical game seeds;
- identical tile/map seeds and the same map pool;
- all six faction rotations through physical seats;
- identical opponent checkpoint mixtures;
- enough games for faction-level uncertainty estimates;
- mean VP, win rate, scoreable objectives, progress, and failure rate;
- paired deltas rather than two unrelated aggregate runs.

The minimum useful first gate is 200 seeds times six rotations, or 1,200 paired setups. Confidence
intervals or bootstrap intervals should accompany overall and per-faction deltas. The evaluation
should also compare action entropy and illegal/fallback rates to catch collapse that average VP can
hide.

No claim that the BC model is stronger should be made before this gate.

## 14. Future storage and generation design

The operator's requirement is that future datasets are not compressed. The desired published
format should therefore be plain JSONL with multiple independently readable shards, not one giant
file:

```text
corpus/
  manifest.json
  good/
    games-00000.jsonl
    decisions-00000.jsonl
    ...
  weak/
    games-00000.jsonl
    decisions-00000.jsonl
    ...
  control/
    games-00000.jsonl
    decisions-00000.jsonl
    ...
```

Shard boundaries should be game boundaries. A practical target is 128–512 retained games or a
bounded byte size per shard. The manifest should list each shard's byte length, SHA-256, game
range, decision count, faction counts, policy counts, and outcome-bucket counts.

Plain JSONL allows workers to seek to independent shard files or byte ranges without zstd frame
indexing. It does not eliminate JSON parsing cost. For repeated training, a derived tensor cache
can still be created, but it should be an explicitly derived artifact rather than the canonical
generated dataset.

### Current repository conflict

Commit `412f706` implemented plain JSONL generation, and `1d9c14f` made the offline BC pipeline
accept plain or legacy zstd input. Commit `8bb530c`, made later, changed the generator back to
bucketed zstd and currently declares `storage_encoding: "zstd"`. That current state violates the
operator decision. The bucket-directory organization is useful and should be retained, but its
contents must be changed back to uncompressed `.jsonl` before future generation.

## 15. Recommended next steps

1. **Run paired gameplay evaluation now.** Compare base 318956 and the full BC checkpoint on 1,200
   paired setups with six rotations.
2. **Restore uncompressed future output.** Preserve the new `good/`, `bad/`, and `random/` bucket
   layout from `8bb530c`, but replace zstd writers and `.jsonl.zst` shards with plain JSONL.
3. **Record build-time provenance.** Store executable SHA-256 and compile-time Git identity.
4. **Reduce weak-data retention.** Keep a stratified quota by faction, policy, map context, and
   failure mode; store summaries for the remainder.
5. **Publish training-ready shard indexes.** Avoid full rescans and make every loader parallel by
   construction.
6. **Add policy/head composition reports.** Verify that outcome filtering did not collapse the
   intended diversity.
7. **Add a KL anchor for future BC.** Constrain drift from checkpoint 318956 unless paired gameplay
   evidence supports unconstrained imitation.
8. **Recalibrate the critic before PPO.** The shared trunk changed even though the value readout was
   not directly optimized.
9. **Use weak data deliberately.** Train a separate ranking or preference objective; do not simply
   one-hot clone losing behavior.
10. **Make preflight mandatory.** CUDA probe, paths, manifests, worker utilization, RAM budget, and
    output immutability must pass before scanning data.

## 16. Bottom line

The process produced a real, training-ready self-play corpus and a materially different MLP that
fits selected successful behavior substantially better than it did at the start of supervised
training. The final data loader and training run used parallel CPU frame processing and CUDA as
intended. The supervised metrics are internally encouraging.

The process was nevertheless operationally inefficient because serial data paths and CUDA runtime
configuration were discovered after expensive work had started. More importantly, the central
scientific question remains unanswered: whether the BC checkpoint plays TI4 better than checkpoint
318956. That requires paired gameplay evaluation, not further interpretation of NLL.

The canonical raw dataset should remain rich enough for future ranking and critic work, but future
generation should not retain unlimited weak detail and must follow the explicit uncompressed-data
requirement.
