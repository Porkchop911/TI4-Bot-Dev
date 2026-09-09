# Inference and training optimization investigation — 2026-09-08

Status: measurement report and diagnostic prototypes; no production optimization applied, staged, or committed. This is an operator-requested investigation, not a migration acceptance gate. Independent review remains open.

## Measured breakdown first

The main CPU opportunity is feature construction, not dense matrix multiplication. All six deciders were timed, with shared-critic PPO recording enabled. The fixed workload is 96 games (16 seeds × six rotations), four rounds, checkpoint-473312, temperature 1, and the validation map pool used by the earlier report. All configurations produced 162,281 decisions, 1,019,727 legal options, 192,122,831 projected feature entries, and 158,035 recorded steps.

| Phase | 1 worker, summed seconds | 32 workers, summed seconds | Share of 32-worker decider time |
|---|---:|---:|---:|
| Actor feature extraction | 66.302 | 118.030 | 46.63% |
| Vocabulary conversion | 8.630 | 14.543 | 5.75% |
| Sparse gather preparation | 0.462 | 0.753 | 0.30% |
| Tensor construction | 1.030 | 2.178 | 0.86% |
| Embedding-bag reduction | 26.946 | 46.768 | 18.48% |
| Identity addition and first ReLU | 2.216 | 4.508 | 1.78% |
| Hidden matmul, bias and ReLU | 3.926 | 6.287 | 2.48% |
| Readout | 2.948 | 5.313 | 2.10% |
| Softmax and CPU result copy | 0.486 | 0.842 | 0.33% |
| Sampling | 0.011 | 0.020 | 0.008% |
| Critic feature construction | 12.622 | 23.425 | 9.25% |
| Critic forward | 6.433 | 13.262 | 5.24% |
| Progress measurement and record creation | 8.581 | 16.128 | 6.37% |
| Total inside deciders, including residual overhead | 141.185 | 253.117 | 100% |

Summed worker time is not elapsed rollout time. Minor reference-list construction and unassigned timer/drop overhead account for the remainder. Feature counts include repetition across options; they are not distinct vocabulary entries. Sampling itself is negligible.

| Workers | Elapsed rollout (s) | Summed game time (s) | Actor copies (s) | Direct Batch::freeze (s) |
|---|---:|---:|---:|---:|
| 1 | 163.415 | 163.267 | 0.00158 | 1.02473 |
| 8 | 32.400 | 227.887 | 0.03217 | 0.76249 |
| 32 | 12.494 | 291.038 | 0.09456 | 0.75516 |

These are one run each, from frozen v1, not confidence intervals or an end-to-end PPO A/B. The 32-worker throughput is 13.08× the single-worker throughput. CPU stage costs per decision grow by broadly similar factors; this alone cannot distinguish allocator contention from memory bandwidth, SMT, frequency, or scheduling.

The historical 60.1% rollout / 34.5% optimize / 4.4% assembly budget remains useful context. Its ~49% inference share came from an evaluation-style attribution; this profile includes critic and training records. Multiplying a stage fraction by 49% below is a **planning approximation**, not a new measurement of training wall shares. In particular, the three separately timed freeze results corroborate a roughly 0.75–1.0 s assembly cost for this batch size, rather than proving a fixed 4.4% for every run.

### Finer feature split

Frozen v2 added nested timers. In its baseline, features totaled 386.862 summed seconds: seat features 140.505 (36.3% of feature time), explicit option features 159.873 (41.3%), per-option action facts 52.576 (13.6%), and final projection 23.864 (6.2%). Remaining feature time includes held-secret progress and instrumentation. These are nested inside the parent feature timer; do not add them twice.

This later run took 47.805 s elapsed versus v1's 12.494 s, with slowdowns throughout the pipeline. Its new timers, rebuild, and machine conditions prevent a performance comparison across versions. Concurrent unrelated rustc builds were subsequently observed consuming substantial CPU during v3 paired runs (many compiler processes, each accruing CPU time). The identical 96 trajectory triples support behavioral equivalence, not timing equivalence. Use the v2 subphase proportions to localize work, and keep raw datasets separate.

## Ranked opportunities and estimates

Estimates are conditional, overlap, and must not be added. Risk columns distinguish seeded trajectory determinism from the frozen observation/checkpoint surface.

| Rank | Opportunity | Estimated training-wall opportunity | Determinism risk | Checkpoint/observation risk |
|---|---|---|---|---|
| 1 | Reuse decision-local feature facts and buffers; first inspect common seat and explicit features | A 10–25% reduction in actor feature cost suggests ~2.3–5.7% wall. This is a target scenario, not a measured patch. | Medium: preserve insertion, duplicate folding and floating sum order; cache only within an unchanged observation. | Low only with exact feature vectors, ids and option order preserved. Never remove facts because current weights are zero. |
| 2 | Cache immutable FeatureKey → (column, assigned) resolution per bot/vocabulary | Measured lookup reduction in diagnostic A/B; approximately ~2% wall if the reduction transfers. Whole-rollout savings require repeated controlled evidence below. | Low for lookup-only caching; seeded RNG untouched, map iteration unused. | Low with immutable vocabulary identity and exact OOV/counter behavior. Rebuild cache when vocabulary changes. |
| 3 | Shared immutable CPU inference snapshot, especially the input table | Input-table replay suggests a possible ~1–2% wall contribution, plus <0.5% copy setup on the initial workload; unqualified until repeated full-actor A/B. | Medium: prohibit concurrent mutation and preserve snapshot lifetime. | Low for shared storage of identical tensors and identical kernels; not permission to share training weights unsafely. |
| 4 | Skip canonicalisation allocation/sort for strictly increasing sparse columns in Batch::freeze | Up to the 4.4% assembly budget; actual achievable fraction is unmeasured for the full freeze. Most sampled options qualify. | Low if fast path requires strict increase and original fallback is retained. | Low: duplicates must retain the exact total-order sort and summation. |
| 5 | Reuse critic/progress facts within the same decision | A 10–25% reduction across critic-feature plus record/progress stages suggests ~0.8–1.9% wall. Unmeasured scenario. | Medium: setup baseline, temporal reward snapshot and critic data must remain exact. | Medium: shared actor and critic observations must retain their distinct information boundaries. |
| 6 | Optimize PPO host preparation/transfers without changing minibatches or arithmetic | Unquantified subset of the historical 34.5% optimize budget. Profile before estimating. | High: preserve shuffle, batch order, gradients and optimizer state, not merely rollout hashes. | High for kernel or precision changes; data-layout-only work still needs tensor/gradient evidence. |
| 7 | Cross-game forward batching | Replay saves 8–19% of forward latency, not rollout wall; current mixed path fails bitwise logits. | High; scheduling must leave each game's RNG and ordered harvest unchanged. | **Surface-affecting in measured implementation.** Not eligible for adoption under current evidence. |

### Feature construction

`projection::mlp_choice_features` already calculates common seat state once per decision; `prompt_free_choice_features` already shares choice context across options. Do not propose these existing hoists as new fixes. `action_facts` still constructs controlled-planet/held-system facts per option; a decision-local context is a concrete next target, though the measured action-facts slice is only ~14% of actor feature cost. Avoid caching across decisions without a complete invalidation scheme. `features.rs`, `critic.rs`, and `progress.rs` were already dirty and were not edited.

Thread-local admission/intern caches already exist. The sampled corpus contains only 1,127 OOV feature entries out of 2,009,354 (~0.056%); global name lookup/string allocation on OOV cannot plausibly explain most vocabulary time here. The ordinary path resolves assigned membership and column separately in the vocabulary's ordered index. The diagnostic cache replaces those repeated lookups, preserving output order and counter updates.

### Vocabulary cache A/B (contended; not a rollout speedup claim)

Frozen v3, identical 96-game workload, three alternating pairs:

| Pair | Baseline rollout (s) | Cached rollout (s) | Baseline vocabulary, summed (s) | Cached vocabulary, summed (s) |
|---|---:|---:|---:|---:|
| 0, cache first | 35.294 | 21.815 | 34.386 | 11.085 |
| 1, baseline first | 40.361 | 34.891 | 45.735 | 16.573 |
| 2, cache first | 40.538 | 44.192 | 48.848 | 19.740 |

The final pair actually regresses whole-rollout time. Concurrent compiler load makes a headline speedup indefensible. Lookup falls from roughly 5.7–6.0% of measured policy time to 2.2%, consistently enough to justify a clean-machine follow-up. All 288 paired game triples match, and the separate 96-game production-decider reference matches too. A single-game verification additionally compares diagnostic feature vectors against production and every actor probability bit; cached column/assigned results are checked against uncached lookup. This is a diagnostic-only cache, with no change to production bot.rs.

### Process-screened follow-up

After the compiler wave ended, the same frozen v3 executable ran three further alternating pairs. Process snapshots before each invocation screened for rustc/ppo_update; a later snapshot found one rustc process near the final cached run, so these are not certified exclusive-machine measurements.

| Pair | Baseline rollout (s) | Cached rollout (s) | Paired reduction |
|---|---:|---:|---:|
| 0, baseline first | 14.506 | 13.472 | 7.13% |
| 1, cache first | 13.299 | 12.564 | 5.52% |
| 2, baseline first | 13.334 | 11.946 | 10.41% |

Median diagnostic rollout reduction: **7.13%**, range 5.52–10.41%. Vocabulary summed time falls 18.053→5.575, 14.975→5.090, and 15.371→5.154 s. All 288 paired game triples match the full production reference. This is promising controlled flag A/B evidence from a frozen executable, but only three pairs, instrumentation allocations, changing machine conditions and no full PPO run limit the claim. Applying the historical 60.1% rollout share would give a **4.29% training-wall extrapolation**, not a measured gain; it exceeds the simple lookup-only budget estimate, so do not attribute the difference to allocator contention without further evidence. The ranked table uses the more conservative stage-budget estimate. Raw logs are inference-v3-clean{0,1,2}-{base,cache}-20260908.log; summary is inference-screened-summary-20260908.json. The filename “clean” means process-screened, not a clean Git tree or guaranteed idle machine.

### Actor storage

The input table is 20,971,520 bytes (20 MiB): 20,480 columns × 256 floats. Thirty-two deep copies consume 640 MiB for this table alone, plus hidden/readout/critic weights. Six seats already share an Rc inside each worker; there are not 192 independent networks. The diagnostic copy timer measures CPU inference_copy operations, not the trainer's preceding device-to-host snapshot.

Replay used 1,720 captured decisions, 11,120 options, ten passes, with one output-hash pass included in timing. Deep versus shallow table handles: one worker 5.092 vs 5.660 s, eight workers 1.961 vs 1.503 s, 32 workers 0.895 vs 0.756 s. All gather output hashes matched within each worker configuration. The 32-worker sample is 15.6% faster in the gather-only probe, while one worker regressed. This supports testing cache/storage pressure; it is not a bandwidth-counter measurement or a proven training gain. Fixed order, warm-cache effects and machine variance remain confounds.

`tch::Tensor` being Send but not Sync does not require duplicating underlying immutable storage: independent owned shallow handles can cross workers. However, globally replacing Actor::inference_copy with shallow copies would also change snapshot isolation. Use a dedicated immutable inference snapshot with no reachable mutation, and prove full-game hashes before proposing integration.

### Batching feasibility and rejection of the current numeric path

The six seats are dependent: a chosen action mutates the game before later seats see their next observation or legal set. They cannot be pre-evaluated as six independent decisions. Separate games are independent, but the current worker loop runs its approximately three games serially to completion. Cross-game batching therefore needs suspended decision requests/coroutines or an inference service, per-game seeded RNG, and harvest in original job order. Three queued games per worker are not automatically three available requests in the current synchronous API.

Actor::logits already evaluates all legal options of a single decision together; describing every internal matmul as literally batch-1 is misleading. Cross-game batching increases the number of option rows further.

The replay called the existing logits_mixed path for groups of 3, 8, and 32 decisions. Ten-pass times: separate 6.787 s; groups 3: 6.276 s; 8: 5.819 s; 32: 5.483 s. Batched timings include constructing/cloning the combined option list. Respectively 7,797, 7,799, and 7,799 of 11,120 logits differed bitwise; maximum absolute difference 0.000061035156. Mathematical equivalence is insufficient. No batched game-equivalence claim is made, and no batching optimization was applied.

### Freeze and logging

Direct Batch::freeze timing is now printed by the diagnostic example. It freezes actual harvested sparse inputs and behavior values, but returns are placeholder zero: this is an assembly measurement, not a PPO learning run. In the real trainer, total still excludes freeze. `ppo_update.rs` and `ppo.rs` remain owned by other dirty work, so this investigation reports the required logging correction instead of editing them: time freeze explicitly and print complete elapsed update time, including snapshot/assembly/report/checkpoint intervals.

Of 11,120 sampled actor options, 10,473 (94.18%) already have strictly increasing columns. The production freeze always allocates pairs, sorts and rebuilds them. Its gather counterpart already has a canonical-order fast path. A matching freeze shortcut is concrete and avoids both sorting and an allocation; full-freeze savings still require a separate A/B.

### Optimizer and GPU scope

Read-only inspection of dirty ppo.rs found that minibatch forward/loss computation is already vectorized, telemetry already drains once per epoch, host vectors reserve known sizes, and padding masks are constructed on device. Recommending those changes again would be stale. Remaining candidates include repeated sparse host flattening and host-to-device construction each epoch; measure these against forward/backward/Adam before changing them. No optimizer subphase timing or CUDA training A/B was collected in this task, so no new optimizer speedup is claimed.

CPU inference remains required by §7.1. The profile does not establish that CPU inference is near its ceiling: most decider work is ordinary feature/vocabulary/recording code, while the hidden matmul is only ~2.5%. GPU inference is therefore not supported as the first intervention by these numbers. No GPU inference path, smaller network, fused operations, reduced precision, or changed kernels were implemented. Any such proposal requires specification approval plus numerical and trajectory evidence.

## Allocation-contention hypothesis

A targeted replay tests the strictly-increasing canonicalisation shortcut against the original allocate/sort/fold implementation at one and 32 workers. Three alternating pairs over 11,120 real options × 20 passes yielded one-worker baseline 0.05918–0.06206 s versus shortcut 0.01087–0.01177 s; 32-worker baseline 0.00435–0.00493 s versus shortcut 0.00109–0.00131 s. Column vectors and every f32 value bit matched for all sampled options.

This experiment shows a substantial local opportunity, but **does not show the proposed multithread amplification**: paired fractional savings are roughly 81% at one worker and 72–78% at 32. It removes sorting as well as allocation, and after the first pass both variants operate on canonicalized inputs, so it is not a full-freeze benchmark or an allocation-only causal test. Sparse sample replay is also much smaller than a 32-worker rollout heap. Do not multiply these percentages by the entire freeze budget.

An allocation-only follow-up keeps exactly the original sort/fold algorithm and reuses only the temporary pairs buffer. Five alternating pairs, 11,120 options × 100 passes, with buffers allocated outside timing: one-worker original 0.33590–0.34784 s versus reuse 0.31765–0.33183 s; median paired saving **4.91%**. At 32 workers, original 0.02858–0.03160 s versus reuse 0.02933–0.03214 s; median paired saving **−2.34%** (range −6.23% to +7.17%). Every sampled canonical output column and value bit matched. This more direct intervention also **does not support amplification in this workload**. It removes the pairs allocation, not any allocations internal to the unchanged sorting algorithm, and repeatedly uses canonicalized replay inputs. The rollout allocation/heap pattern may behave differently. No rustc processes were present immediately before this follow-up; raw evidence is inference-v4-allocation-20260908.log and inference-allocation-summary-20260908.json.

The supply training result remains plausible evidence for a contention effect, but its one before-sample and bundled engine fixes prevent isolating it. Neither the general thread-scaling matrix nor this fast-path experiment establishes that every allocation-removal estimate should receive a multiplier. Use actual 32-worker controlled A/B results and report variation; keep the hypothesis open.

## Evidence and reproducibility

Scope: P1/P2 local diagnostic sources/report, bounded CPU processes (at most 32 workers), release builds, and ignored out/ artifacts. No network, GPU jobs, external writes, branch changes, staging or commits. No unrelated dirty source was edited. Repeated tasklist checks found no ppo_update.exe. A two-second process CPU-delta check during the v3 pairs found the profiler using 19.33 CPU-seconds and individual unrelated rustc processes using about 1.7–1.9 CPU-seconds each; more than 30 rustc processes were present in the following snapshot. These paired runs are explicitly contended and do not qualify a rollout speedup. No unrelated process was interrupted. The shared tree advanced during the investigation, so frozen executables rather than recompilation define each timing comparison.

Sources: `crates/ti4-mlp/examples/inference_cost.rs` and its `inference_cost_support` modules. They are diagnostic copies, not an alternate supported policy implementation. Timers preserve operation order and use the production tensor primitives. `--reference` invokes the production MlpBot. `--verify` checks probabilities bitwise; the final diagnostic also compares projected vectors to production.

Workload flags: `--seeds 16 --threads N --temperature 1 --record --capture`, N=1,8,32; default seed base 900000100, six rotations, four rounds, bundle out/checkpoints/stage2-mlp-shaped/checkpoint-473312. Replay: `--micro out/inference-samples-t1-20260908.json --repeats 10`. Build with the pinned 2.9.1-cu128 libtorch and deterministic single-thread tensor settings; inference remains on CPU.

Raw evidence in out/: inference-profile-t{1,8,32}-20260908.log, inference-matrix-summary-20260908.json, inference-v2-{base,cache}-20260908.log, inference-micro-20260908.log, inference-samples-t1-20260908.json, and frozen inference-cost-v{1,2,3,4}-20260908.exe. The final allocation-only executable was built against the exact v3 dependency artifacts using the saved inference-v4-rustc-command-20260908.json; its build log is inference-v4-build-20260908.log. These local artifacts are intentionally not staged.

All 96 ordered choice/result, event, and final-state SHA-256 triples match across v1's 1/8/32-worker runs, v2's baseline/cache runs, v3's six paired runs, and the v3 full production reference. Initial single-game production-reference checks passed at temperature 0.001 with PPO recording, and diagnostic probability verification passed at temperature 1. This is exact evidence for the exercised games, not a proof over every decision site or future checkpoint. Choice hashes use ordered Debug choice/result bytes; events and final state use serde JSON bytes. These are diagnostic hashes, not the versioned engine fingerprint format. Per-stage instrumentation itself adds timer calls, bookkeeping and short-lived allocations; no exact overhead correction is claimed. Reference-run parity validates behavior, not zero timing overhead.

The full workspace suite was not used as a workaround for the supplied inventory failures, and no fixtures were regenerated. Other sessions have since committed inventory-related work; this report does not certify their current suite status. Diagnostic release compilation passes with seven unused copied-helper warnings. Isolated Clippy against the frozen dependencies exits 0 with eight warnings (seven unused copied helpers and the copied from_setup naming convention); this is not a strict warning-free lint pass. rustfmt --check passes on the diagnostic module tree. No independent reviewer or subagent was launched, following the operator's preference; production integration remains subject to review and full applicable checks.

Final validation details: out/inference-v3-validation-20260908.json; input/source checksums: out/inference-source-input-manifest-20260908.json. The source manifest is a final-source capture, not a reconstruction of earlier shared-tree builds.

Ruled out as first targets: sampling (<0.01% of decider time); actor tensor construction (<1%); OOV name locking on this low-OOV corpus; report/checkpoint work (historical 0.02% of wall); additional within-decision option batching (already present); blind six-seat batching (state dependencies); shrinking the observation, legal set, vocabulary, network or precision (frozen surface); and treating the 34.5% CUDA optimizer budget as automatically exhausted merely because it scales with batch size. Optimizer subphase profiling remains an unmeasured opportunity, not a ruled-out bottleneck.

Final artifact SHA-256/size manifest: out/inference-artifact-manifest-20260908.json. Scope at completion: only the new inference_cost example/support files and this report are task-owned source changes; other dirty files and plans/EXECUTION_STATE.md remain untouched. Shared branch at final inspection: wp/tier-c-review-remediation-obs008c2b-003e1; HEAD at source capture c73cadf1bc994b45f4e00c9f868b590d7e60560a. No migration milestone or production optimization is declared accepted.
