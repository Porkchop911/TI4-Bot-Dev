# Plan: learning-approach evaluation ("the arena") and algorithm-agnostic compute work

Date 2026-08-17. Branch `codex/stage1-parity-fixes`.
Companion to `plans/ANALYSIS_2026-08-17_LEARNING_AND_COMPUTE.md`, which establishes the problem
profile this plan is built on. Numbers here are budgets and estimates, not measurements, except
where a source is named.

---

# Part A — Evaluating learning approaches

## A0. The measurement problem, stated first

Everything in Part A is shaped by one fact from the analysis: **at the current operating point the
learning signal per unit compute is smaller than the panel's resolution.** Measured at u16100,
mean paired SE ≈ 0.072 VP on a 32-seed panel against a per-1000-update effect of +0.03–0.05.

Three consequences that drive every design choice below:

1. **A short screen run from the u15100 champion cannot separate any two algorithms.** 50,000
   games would move VP by ~0.02 under current dynamics. Undetectable. So the screen must not be
   run from the converged champion.
2. **Screen from blank profiles instead.** Early learning is fast and the between-arm differences
   are large; an algorithm that is 3× more sample-efficient shows it in the first 50k games.
   *Risk, stated up front:* early-learning speed does not perfectly predict asymptote. This is why
   the schedule is staged — screen on slope, confirm on asymptote.
3. **The relevant error bar is across training runs, not within a panel.** The 0.072 currently
   reported is within-panel sampling error for one policy. "Is algorithm A better than B" needs
   the variance *between independent training runs of the same algorithm*, which in RL is routinely
   larger. Every arm therefore runs ≥3 training seeds and is reported as mean ± across-seed SE.
   Single-run comparisons are not admissible evidence.

## A1. The arena — one harness, swappable arms

The seam already exists and part of it was built in this very branch.

- **`Decider`** (`ti4-engine/src/choice.rs:685`) is the policy interface. Any arm that produces a
  playable policy implements it.
- **`rollout::play_with_deciders`** (added in this branch) already accepts caller-supplied
  deciders per seat. This is exactly the arena entry point; it was written for the T5 trace but
  generalises.

What needs building:

**A1.1 `trait Arm`** — the training side, so arms are swappable at the top level:

```rust
trait Arm {
    /// Train until the budget is spent. Budget is in GAMES SIMULATED, not updates.
    fn train(&mut self, budget: Budget, ctx: &ArenaCtx) -> ArmCheckpoint;
    /// A playable policy from the current parameters.
    fn policy(&self) -> Box<dyn Fn(&PlayerId) -> Box<dyn Decider>>;
    fn name(&self) -> &str;
    /// Everything needed to reproduce this run.
    fn config(&self) -> BTreeMap<String, String>;
}
```

**Budget must be games simulated, not updates.** PPO takes 8 epochs over one batch; REINFORCE
takes one. Their "updates" are not comparable units, and budgeting by update would hand PPO an 8×
compute advantage or an 8× disadvantage depending on which way you squint. Games simulated is
algorithm-neutral. Record core-seconds alongside it as the secondary axis.

**A1.2 Separate *policy class* from *optimiser*.** These are independent axes and conflating them
is how you learn nothing from a 2×2. Concretely:

| Axis | Options |
|---|---|
| Policy class | linear (current) · bilinear low-rank · small MLP · heuristic-parameterised (evolution's evaluator) |
| Optimiser | REINFORCE+scalar baseline (current) · PPO+GAE · CMA-ES/sep-CMA · ExIt (supervised) |
| Opponent scheme | co-adaptive self-play (current) · frozen champion · league/historical pool |

The arms in A4 are *points* in this space chosen to discriminate hypotheses, not an exhaustive
grid.

**A1.3 Reproducibility envelope.** Every run writes a manifest in the shape the evolution archive
already uses (`manifest.json` with git commit, dirty flag, full argument map, horizon, board,
seeds). The existing checkpoint `arguments` map is most of this already.

## A2. Measurement protocol

**A2.1 The common-opponent problem.** The current gate always compares a candidate to *its own*
champion. That makes cross-algorithm numbers incomparable — arm A's "+0.05 vs its champion" and
arm B's "+0.05 vs its champion" say nothing about A vs B. The arena needs a fixed yardstick.

Define **Reference Table R** = the frozen u15100 champion heads, all six factions, pinned by hash.
Then every arm's policy P is measured three ways:

| Measurement | Setup | Answers |
|---|---|---|
| **vs-R** (primary) | P in one seat, R in the other five, all 6 rotations, fixed eval seed block | Comparable across arms |
| **self-play** | P in all six seats | Absolute quality; the 13.3 → 17.4 table-total axis |
| **cross-play** | round-robin of arm finals against each other | Non-transitivity check |

vs-R is primary because it is the only one that is directly comparable between arms. Self-play is
kept because a policy that beats R but plays badly against itself has overfitted to R.
Cross-play is cheap at the end and catches rock-paper-scissors, which self-play VP hides.

**A2.2 Common random numbers.** All arms are evaluated on the *same* eval seed block, paired
per-seed, exactly as `GainEvidence::pair_tables` already does. This removes shared map/deal
difficulty and is the single biggest variance reduction available for free.

**A2.3 Panel sizes.** Sized to the effect we need to resolve:

| Stage | Eval seeds | Approx paired SE | Resolves |
|---|---|---|---|
| Screen | 64 | ~0.05 | ≥0.15 VP differences |
| Confirm | 128 | ~0.036 | ≥0.10 VP |
| Final | 256, fresh block | ~0.025 | ≥0.07 VP |

Derived by scaling the measured 0.072 @ 32 seeds by 1/√n. Verify empirically on the first panel
rather than trusting the extrapolation.

**A2.4 Held-out discipline.** Three disjoint seed ranges, fixed for the whole programme:
training (93M+), arena selection (96M/97M), and a **final block never used for any selection
decision** (98M+). The final block is opened once, at the end, for the winner and the runner-up
only. This is the guard against selecting on noise across 8 arms × several stages.

## A3. Metrics

Primary: **mean VP vs-R**, per faction and pooled.

Secondary, all cheap once the games are played:

- **Clearance** — an arm that buys VP by abandoning openings should be visible, not hidden.
- **Self-play table-total VP** — the absolute-quality axis (PG 13.30, evolution 17.39).
- **Faction spread** (max − min across the six). Current PG: **0.24**. Evolution: **2.49**. This is
  the fingerprint of the representation hypothesis; an arm that starts differentiating factions is
  evidence for H2 independent of its VP.
- **Sample-efficiency curve**: VP vs games simulated, logged continuously — the primary screen
  signal, since the screen is about slope not asymptote.
- **Compute-efficiency curve**: VP vs core-hours. Diverges from the above whenever an arm does
  more work per game (search).
- **Entropy / movement / return-sd** — the existing telemetry, kept for pathology detection.

## A4. The arms, as hypothesis-discriminating experiments

Each arm is chosen to be the cheapest thing that could falsify a hypothesis. The hypotheses:

- **H0 — nothing is wrong; the plateau is the game's ceiling.** (Already weakened: evolution
  reaches 17.4 table-total vs PG's 13.3.)
- **H1 — the gradient estimator is the binding constraint** (variance, credit assignment).
- **H2 — the policy class is the binding constraint** (linear over lexical token crosses).
- **H3 — co-adaptive self-play is the binding constraint** (non-stationarity, train/eval mismatch).
- **H4 — search at decision time is required.**

| # | Arm | Tests | Effort | Falsifiable prediction |
|---|---|---|---|---|
| 0 | **Baseline**: current REINFORCE, reproduced in the arena | — | 1 d | Reproduces u15100 dynamics; if not, the harness is wrong |
| 1 | **Hygiene**: skip degenerate decisions, `clearance-weight 0`, baseline bucketed by (head, round) | H1 (cheap part) | 2 d | return-sd 2.9 → ~2.3; slope up, asymptote unchanged |
| 2 | **PPO + GAE + value head** | H1 | 1–2 wk | Same asymptote, reached in ~1/5 the games. *If the asymptote rises, H2 is wrong.* |
| 3 | **Frozen-opponent training** (one faction vs 5 frozen champion heads, rotating) | H3 | 2–3 d | Lower gradient variance, removes train/eval mismatch. Cheapest arm with a real mechanism. |
| 4 | **Bilinear low-rank scorer** `φ_sᵀWφ_a` | H2 | 1 wk | Faction spread rises above 0.5 even if VP moves little |
| 5 | **Small MLP over option features** (DeepSets/pointer shape) | H2 | 2 wk | Asymptote above 2.5; spread above 1.0 |
| 6 | **ExIt with selective search** (search only on ≥10-option decisions and high-stakes heads) | H4 | 3–4 wk | Highest ceiling; worst compute-efficiency curve |
| 7 | **sep-CMA-ES on the heuristic parameter vector** (port the evolution scheme) | H0/H2 reference | 1–2 wk | Reproduces ~2.90 mean / 17.4 table-total. **The skill ceiling reference.** |

**Arm −1, run first, before any of the above: the flat Monte-Carlo probe.** No training at all.
At each decision, roll out each option k times under the current champion policy and take the
best. Measure vs-R.

This is the highest-information-per-hour experiment in the whole plan:

- If flat-MC substantially beats 2.2 with the *current* features and *current* weights, then the
  policy class can already represent good play and the problem is the optimiser/credit assignment
  (**H1**), not representation.
- If flat-MC also plateaus near 2.2, the evaluation itself cannot distinguish good from bad play
  at this feature resolution — strong evidence for **H2** and a reason to jump straight to arms 4/5.

One day of work, no training run, and it splits the hypothesis space in half. Do it first.

## A5. Schedule — successive halving on a compute budget

Sizing from the measured ~0.62 core-s/game and 32 cores at ~63% utilisation (~13 min per 10k games
wall-clock; better once Part B lands).

| Stage | Arms | Training seeds | Budget/run | Runs | Est. wall |
|---|---|---|---|---|---|
| **0. Probe** | flat-MC (arm −1) | — | eval only | 1 | ~1 day |
| **1. Screen** | 0,1,2,3,4,5,7 (from **blank**) | 3 | 50k games | 21 | ~10 h |
| **2. Confirm** | top 4 (from blank **and** from u15100) | 3 | 200k games | 24 | ~2 days |
| **3. Final** | top 2 | 5 | 1M games | 10 | ~4 days |

Total ≈ 8–9 days of machine time, spread over the ~8 weeks of implementation, so arms enter the
arena as they are finished rather than all at once. Arm 6 (ExIt) joins at stage 2 if the flat-MC
probe supports H4.

**Stage 2 runs from blank *and* from the champion.** This is where the "does early slope predict
asymptote" risk from §A0 gets tested rather than assumed. If an arm's ranking flips between the
two starts, that is itself the finding.

## A6. Decision rules, pre-registered

Written before the runs, so a marginal result cannot be argued into a promotion afterwards.

- **Advance** stage 1 → 2: top 4 by mean vs-R slope over the last 20k games, requiring the arm to
  beat arm 0 by ≥1 across-seed SE. Arms that fail to beat baseline are dropped regardless of rank.
- **Advance** stage 2 → 3: top 2 by final vs-R, requiring ≥2 across-seed SE over arm 0.
- **Winner**: highest vs-R on the **untouched 98M block**, with self-play table-total not worse
  than baseline, and clearance within 0.05 of baseline.
- **Kill criteria** (any arm, any stage): NaN/divergent weights; entropy collapse (mean policy
  entropy < 0.05 nats sustained); clearance below 0.5; or wall-clock >3× the budget estimate.
- **Null result is a result.** If nothing beats arm 0 by 2 SE at stage 3, the finding is that the
  plateau is not addressable by any of these, and the next move is arm 7's parameterisation
  (i.e. accept a stronger inductive bias) or a re-examination of H0.

---

# Part B — Compute work that does not depend on the algorithm

## B0. The conflict this framing exposes — read this first

My earlier analysis recommended **removing trajectory storage** using the identity
`Σₐ Gₐ∇ₐ = Σₜ rₜ·Sₜ`. That is exact, and it is **the wrong thing to build now.**

PPO, ExIt, and every multi-epoch or off-policy method **must** retain per-decision data to
re-evaluate the policy on it across epochs. Streaming the statistics away would foreclose arms 2
and 6 — two of the strongest candidates — to optimise the one arm we already know plateaus.

**Revised recommendation: keep the trajectory, make it compact.** Interned `u32` feature ids with
`f32` values gives roughly an order of magnitude on both memory and allocation churn while
preserving every arm's ability to replay a batch. It serves REINFORCE, PPO, ExIt and CMA-ES
equally. Streaming removal stays on the shelf, to be revisited only if a single-pass method wins
outright.

This is the general test for everything below: **would this work be wasted if any arm won?**

## B1. Tier A — unconditional, do immediately

None of these can be invalidated by an algorithm choice. All are measurable with the existing
M00-012 protocol (`ti4-sim/src/benchmark.rs`) and the `out/ab_measure.sh` harness.

| # | Change | Effort | Est. gain | Notes |
|---|---|---|---|---|
| A-1 | **mimalloc/snmalloc global allocator** | 1 h | 1.15–1.4× | No allocator is set; Windows sysalloc under 32-thread small-alloc churn. Do this first — it also tells you how much of the feature cost is allocator vs algorithm. |
| A-2 | **Hoist prompt tokenisation** out of the per-option loop | 2 h | 1.05–1.1× | `tokens(&choice.prompt)` runs once per *option*; a transaction decision has up to 37. Identical feature set, zero parity risk. |
| A-3 | **Skip degenerate decisions** (`probabilities.len() < 2`) | 1 h | small compute, **real learning fix** | 6% of decisions (74/1198 in trace). Zero gradient, but they inflate `actions` and pollute the baseline mean and normalising scale, systematically — they cluster in payment loops. |
| A-4 | `RAYON_NUM_THREADS=16` A/B | 1 h | 0–1.15× | SMT often hurts allocator-bound work; some of the "52–63% utilisation" may be SMT contention counted as idle. |
| A-5 | `lto = "fat"` | build time only | 0–1.05× | `codegen-units = 1` already set; may inline the engine↔policy boundary. |

## B2. Tier B — agnostic, substantial

| # | Change | Effort | Est. gain | Why agnostic |
|---|---|---|---|---|
| B-1 | **Intern feature names → `u32`; weights → dense `Vec<f32>`** | 3–5 d | ~1.35× end-to-end | Every learned arm builds and scores features. Only arm 7 (heuristic params) doesn't, and it doesn't regress. |
| B-2 | **Compositional hashing** for `prompt-option:{p}:{o}` — never materialise the string | 2 d | included above | The cross product is quadratic in token count and probably the majority of all features. |
| B-3 | **Compact trajectory encoding** (`Vec<(u32,f32)>` not `BTreeMap<String,f64>`) | 2 d | ~10× memory | See B0. Unblocks `rollout_depth > 4` and makes `--pipeline` viable. Required by PPO/ExIt. |
| B-4 | **Batched scoring interface** `score_batch(&[Features]) -> Vec<f64>` | 1 d | enabling | Costs nothing now, keeps SIMD *and* GPU doors open without committing to either. See B5. |
| B-5 | **Don't clone option ids twice** in `consider()` | 2 h | small | Return `Vec<(usize, Features)>`; the caller has the `Choice`. |

## B3. Tier C — infrastructure the arena needs (also agnostic)

| # | Item | Effort | Notes |
|---|---|---|---|
| C-1 | **`trait Arm` + budget accounting in games and core-seconds** | 3 d | §A1.1. Nothing else in the plan works without it. |
| C-2 | **Fast state clone + rollout API** | 3 d | Needed by the flat-MC probe (arm −1), ExIt (arm 6), and any what-if analysis. Worth building even though only some arms use it, because arm −1 runs first and is the highest-value experiment. |
| C-3 | **vs-R / self-play / cross-play evaluation harness** | 3 d | §A2.1. Generalises the existing isolated-panel code. |
| C-4 | **Criterion microbenches**: feature construction, scoring, full game, batch reduce | 2 d | Nothing in B1/B2 should be merged on an estimate. `criterion` is already a dependency of `ti4-sim` only. |
| C-5 | **Per-run manifest** in the evolution-archive shape | 1 d | Reproducibility across 50+ arena runs. |

## B4. The engine — 68% of cost, and the least explored

`STAGE2-SCHEDULING-WAVES.md` deferred this as "requires a dedicated profiling package with parity
risk review." It remains the largest single block of compute and is entirely algorithm-agnostic.

- **C-4 first**: no engine change without a profile. The hot-path attribution that produced the
  32%/68% split was temporary instrumentation; make it permanent and cheap.
- **Decision-count reduction.** hacan generates **24.6M** decisions per 1,000 updates vs l1z1x's
  **15.5M** (+59%), and `trade` is 21% of all decisions (247/1198 in the trace). `offer_options`
  emits the full `for give in 0..=3 { for want in 0..=3 }` cross product plus commodity, note and
  card shapes. Many are strictly dominated from the proposer's side. **This is the only lever that
  reduces work rather than cost-per-work, and it scales every arm equally.**
  *Carries oracle-parity risk* — it changes the offered option set. Needs a parity decision before
  implementation, and should be gated behind a flag with a differential trace comparison.
- **Utilisation.** 63% at `--rollout-depth 4`. B-3 removes the memory pressure that made depth 8
  regress and `--pipeline` lose; re-measure both afterwards. Target 85%.

## B5. GPU — where it would and would not pay

Taking the "fine but not necessary" framing at face value, here is the honest read:

**Would not pay:**
- **The simulator.** Branchy, pointer-chasing, sequential dependencies, tiny per-step work. This is
  68% of current cost and it is not GPU-shaped. No plausible port.
- **The learner.** `apply()` cost is already measured at ~0.
- **A small (~100k param) MLP policy.** At 7,071 option-evaluations per game with ~60 non-zero
  sparse features each, CPU with interned features and SIMD is likely competitive, and PCIe
  round-trips per decision would dominate.

**Would pay, conditionally:**
- If arm 5 wins **and** the network grows past roughly 1M parameters, or if dense embeddings
  replace sparse features. Then the architecture that makes it work is centralised batched
  inference (SEED-RL style): actors send observations to an inference server that batches across
  *many concurrent games*, rather than each worker running its own forward pass. That is a
  significant architecture change and should not be pre-built.

**The agnostic hedge is B-4**: a batched scoring interface. It costs a day, is useful immediately
for CPU SIMD, and is the prerequisite for a GPU backend if one is ever justified. Build the
interface; do not build the backend.

**Recommendation: no GPU work in this plan.** Revisit only if stage 3 selects arm 5 or 6 with a
large network, and treat it as a separate package with its own measurement.

## B6. Measurement discipline

- Extend the existing **M00-012 protocol** (30 interleaved samples, monotonic ns, pre-declared
  variance thresholds) rather than inventing a second one. It is already the right shape.
- Every Tier A/B item merges with a before/after A/B on the same machine, same champion, same
  pool — the `ab_seq`/`ab_pipe` pattern already in `out/`.
- **Estimates in this document are estimates.** The 32%/68% split and 0.62 core-s/game are
  measured; every "est. gain" column is not. Treat a Tier B item that fails to beat its estimate
  by a wide margin as a signal that the profile has moved, and re-profile.

---

# Part C — Sequencing

The ordering principle: **Tier A and C-2 first**, because they are cheap and because the flat-MC
probe (arm −1) is the highest-information experiment and depends only on C-2.

| Week | Compute track | Learning track |
|---|---|---|
| 1 | A-1…A-5, C-4 (benches) | C-2 (rollout API) → **arm −1 flat-MC probe** |
| 2 | B-1, B-2 (interning) | Arm 1 (hygiene), arm 3 (frozen opponents) — both small |
| 3 | B-3 (compact trajectory), re-measure depth/pipeline | C-1, C-3, C-5 (arena harness) |
| 4 | Engine profile (B-4 first half) | **Stage 1 screen** (7 arms × 3 seeds, from blank) |
| 5–6 | Decision-count reduction (parity review first) | Arms 2, 4 built; **stage 2 confirm** |
| 7–8 | Utilisation to 85%; opportunistic | Arm 5 (MLP) or 6 (ExIt) per probe result; **stage 3 final** |

**Two decision points where the plan should be allowed to change:**

1. **End of week 1 — the flat-MC probe.** If it beats 2.2 comfortably, weight the programme toward
   arms 1/2/3 (optimiser) and de-prioritise arms 4/5. If it plateaus, skip arm 2's long form and go
   straight at representation (arms 4/5). This is the single most schedule-relevant result.
2. **End of stage 1.** If arm 7 (CMA-ES on heuristic parameters) reproduces ~2.90 mean while every
   learned arm sits at ~2.2, that is decisive evidence for the representation hypothesis, and the
   right response is distillation from arm 7 into a learned policy rather than more optimiser work.

**What would tell us this plan is wrong:** if arm 0 fails to reproduce the current dynamics in the
arena (harness bug); if across-training-seed SE turns out larger than the between-arm differences
at stage 2 (then the whole comparison needs more seeds, not more arms); or if the flat-MC probe
shows the *evaluation* is insensitive to play quality (then the objective, not the algorithm, is
the problem, and the reward design needs revisiting before anything else).
