# Log review, learning-approach analysis, and compute headroom

Date 2026-08-17. Branch `codex/stage1-parity-fixes`.
Sources: `out/run_pure.log`, `out/run_nobound.log`, `out/run_2path.log`, `out/run_pure_u5000.json`,
`plans/evidence/STAGE2-SCHEDULING-WAVES.md`, `plans/EXECUTION_STATE.md`, and the evolution archive
at `E:\ti4-engine\archive\stage2_blank_002` (read-only).

---

## Part 1 — What the logs actually say

### 1.1 The plateau is total, and it is not a measurement artifact

`run_pure.log`, updates 15,100 → 19,100 (four blocks of 1,000, ~3.1 h, ~428M decisions). The
fourth block landed while this document was being written and changed nothing:

| Block | Wall | Decisions | Movement (hacan) | mean-return-sd | Errors |
|---|---|---|---|---|---|
| 15100→16100 | 2616.6 s | 109.4M | 41.28 | 2.919 | 0 |
| 16100→17100 | 2782.4 s | 107.6M | 41.53 | 2.940 | 0 |
| 17100→18100 | 2869.5 s | 106.2M | 42.14 | 2.959 | 0 |
| 18100→19100 | 2980.2 s | 105.2M | 41.72 | 2.940 | 0 |

Every diagnostic is flat to three significant figures. Weights are moving (`movement` ~41–56,
zero zero-movement updates), return variance is healthy (~2.9), and nothing improves. This is not
entropy collapse and not a dead reward — it is convergence within the current policy class.

### 1.2 The gate is underpowered by roughly an order of magnitude

`run_nobound.log`, boundary at u16100 — every faction rejected, and the reason is the same one
six times:

| Faction | own VP gain | SE | Verdict |
|---|---|---|---|
| sol | +0.0677 | 0.0705 | fails 1σ |
| letnev | +0.0104 | 0.1015 | fails margin **and** 1σ |
| xxcha | +0.0469 | 0.0742 | fails margin and 1σ |
| hacan | +0.0000 | 0.0594 | fails margin and 1σ |
| jolnar | −0.0260 | 0.0613 | negative |
| l1z1x | +0.0573 | 0.0659 | fails 1σ |

Mean paired SE ≈ **0.072** on a 32-seed panel, against a true per-1000-update effect that looks
like **+0.03 to +0.05**. The panel cannot see the thing it is being asked to measure — and note
`accept_sigmas` has already been dropped to 1.0 (from the 2.0 default) and *still* nothing passes.

Resolving a +0.04 gain at 2σ needs SE ≈ 0.02, i.e. `(0.072/0.02)² ≈ 13×` the seeds — ~415 seeds
per panel, ~30,000 games per boundary. That is ~⅓ the cost of the 1,000 updates it adjudicates,
so it is affordable. **But it would not help**, because of §1.1: at u15k+ there is no +0.04/1000
trend left to resolve. Underpowered measurement is a real defect, and it is the *second* problem.

### 1.3 Panel-to-panel variance is being read as learning

Same log, consecutive boundaries on **different** seed blocks (`--panel-step 32`):

- u15100, panel base 96000000: VP 2.05 – 2.29
- u16100, panel base 96000032: VP 2.44 – 2.62

A +0.2 to +0.4 jump across the board from 1,000 updates would be extraordinary. It is panel
difficulty, not learning — which is exactly why the per-boundary champion re-measurement was
added, and it is working correctly. Worth stating plainly in the run reports, because the raw
`report()` table invites the wrong read.

### 1.4 The six factions are suspiciously identical

| | sol | letnev | xxcha | hacan | jolnar | l1z1x | spread |
|---|---|---|---|---|---|---|---|
| Rust PG @u15100 | 2.26 | 2.05 | 2.29 | 2.24 | 2.22 | 2.24 | **0.24** |
| Evolution anchor @g122 | 2.49 | 2.15 | 4.57 | 2.97 | 2.08 | 3.13 | **2.49** |

Six independently-trained heads, six factions with genuinely different abilities, landing within
0.24 VP of each other. The learned policy is not differentiating faction-specific play at all.
That is a **representation** signal, not an optimizer signal (§2.3).

---

## Part 2 — Learning approach

### 2.1 The evolution comparison is valid — I checked

`EXECUTION_STATE.md` treats the archive result as proof that ≥3 VP is reachable. The script is
named `evolve_save54_three_player.py`, which made me suspect a 3-player/6-player mismatch. It is
not one. From `generations/g00001/cases.parquet`:

```
seats per game: 6   (864 games, all with nunique(result_faction) == 6)
rotations:      0..5
factions:       hacan jolnar l1z1x letnev sol xxcha
horizon_round:  4   (manifest)
```

Same six factions, six seats, four rounds, `save52` board. The comparison holds.

### 2.2 But the headline number is being quoted at the wrong altitude

From the g00122 **anchor** games (the champion table played 6-way against itself — the honest
self-play evaluation):

| Metric | Rust PG @u15100 | Evolution @g122 |
|---|---|---|
| Table total VP / game | **13.30** | **17.39** |
| Mean VP per faction | 2.217 | 2.898 |
| Best faction | 2.29 (xxcha) | 4.57 (xxcha) |
| Worst faction | 2.05 (letnev) | 2.08 (jolnar) |

Two things fall out:

1. **VP is not conserved** — better play produces +31% more total VP per table. So the objective
   is not zero-sum and the plateau is not a self-play equilibrium artifact. Good news.
2. **The "4.5 VP" target is one faction, not the table.** The evolution run's *mean* advantage is
   +0.68 VP (+31%), not +2.3. Its worst two factions (2.08, 2.15) are exactly where PG sits. The
   real result is "evolution found a much better xxcha and l1z1x, and left jolnar/letnev alone."

That reframing matters for target-setting: "get one faction to 3.0" is a fair goal and is
demonstrated reachable, but chasing 4.5 as a table-wide expectation is chasing something the
reference run never did either.

### 2.3 Why evolution wins here — three structural advantages, in order of size

**(a) Policy class / inductive bias.** The evolution trainer optimizes 3,449 bounded parameters
(`parameter_schema.json: profile_leaf_count = 3449`) that modulate a *hand-written nonlinear
evaluator* — `DENIAL_PRIZE`, `FREE_PLANET_PRIZE`, `HOME_SYSTEM_DEFENCE_BY_ROUND`, `POOL_NEED`,
and ~40 more named concepts, each with a neutral value and bounds. That evaluator already knows
"take undefended planets", "defend home in rounds 2–3", "don't reinforce a garrison that is
already superior."

The PG policy is a **linear** function (dot product then softmax) over sparse binary features that
are overwhelmingly *token co-occurrences*: `option:{token}`, `prompt-kind:{tok}:{kind}`,
`prompt-option:{ptok}:{otok}`. A linear model over lexical crosses of prompt and option text
cannot represent "commit ground forces iff the defending garrison is weaker than what I can
land" — it can only correlate with the words `commit` and `infantry` appearing together. The
hand-written `ScoredBot::score_commit_seen` in `ti4-policy/src/bot.rs` encodes exactly that
relational fact; the learned heads have no way to.

This is my primary diagnosis, and §1.4 is its fingerprint: a faction-agnostic lexical policy
converges to the same play for all six factions.

**(b) Credit assignment.** `reward.rs::returns` is undiscounted REINFORCE:

```
G_a = Σ_{t ≥ a} r_t                       (suffix sum, no discount)
A_a = G_a − mean_over_batch_and_head       (gradient.rs:210, a scalar baseline)
```

The ~1,140 decisions per game are split across six seats, so the credit-assignment episode is
per-seat: **~190 decisions on average, ~256 for hacan** (measured from
`out/rust_trace_seed83000001_rot0.json`: 1,198 decisions in one game — hacan 366, sol 201,
letnev 195, jolnar 164, xxcha 151, l1z1x 121). Every one of those is credited with the entire
remaining game.
There is no value function, no per-timestep baseline, no GAE. The single scalar baseline also
cannot correct the systematic magnitude difference between early decisions (large suffix sums)
and late ones (small) — so the estimator is biased across game-time *and* has variance scaling
with episode length. Evolution sidesteps all of this: it optimizes panel mean VP directly and has
no credit-assignment problem to get wrong.

**(c) Evaluation structure.** Evolution scores mutations against a **fixed anchor** — a stationary
opponent. The PG trainer co-adapts all six heads simultaneously, so each head is chasing a moving
target and the measured gradient partly reflects opponents drifting.

### 2.4 The current reward shaping is injecting more noise than signal

`run_pure.log` config: `--entropy 0.05` (5× reference), `--high-vp-bonus 1.0`,
`--clearance-weight 5.0`.

The clearance term lands in the **final** reward slot, so via the suffix sum every one of the
~190 decisions in that seat's episode carries the full −5.0. With clearance ≈ 0.87:

```
Var contribution = 25 × 0.13 × 0.87 ≈ 2.83
Observed total return variance = 2.9² ≈ 8.4
→ the clearance term alone is ~34% of all return variance
```

And it is **constant within a game** — it discriminates between games, never between decisions
inside one. Against a batch-mean baseline that variance flows straight into every decision's
advantage as pure noise.

The code already knows this. `reward.rs` says of the round-one bonus:

> *"Credited at the last decision taken in round one, so every round-one decision carries it and
> no later one does. A round-three decision cannot change whether round one cleared, and **paying
> it there would only add variance**."*

`clearance_weight` then does precisely that, deliberately, at magnitude 5.0. The stated
justification ("prices the clearance risk everywhere") is a real concern, but the fix for
"clearance regressions slip past the gate" is a gate constraint — which the two-path gate already
implements — not a −5.0 terminal term smeared over every decision in the episode.

### 2.5 What I would change, in order

1. **Cut `--clearance-weight` to 0 and re-measure.** The gate already guards clearance
   per-faction. Free, one flag, testable in one block. Expected: return-sd drops from ~2.9 toward
   ~2.3, gradient SNR improves ~25%.
2. **Add a value baseline.** Even a cheap one — a per-head, per-round-number running mean of
   returns — removes the game-time bias a scalar baseline cannot. `Statistics` would need
   bucketing by `round_number` (already on `Progress`). This is a small change with a large
   variance reduction, and it is the single highest-ROI algorithmic fix.
3. **Discount, or truncate credit.** γ ≈ 0.99 over ~190 steps, or simply cap each decision's
   return at the end of its own round. The existing round-one bonus already demonstrates the
   pattern works.
4. **Enrich the feature space with relational features.** This is the big one and it is the
   diagnosis in §2.3(a). The `structured_features` path already exists; it needs features that
   compare quantities rather than co-occur tokens — garrison ratio at the target, fleet-strength
   delta, distance-to-Mecatol, objective-progress delta. The `ScoredBot` heuristics are a ready
   list of what to featurize. Without this I do not expect any optimizer change to reach 3.0.
5. **Consider the hybrid the state doc already names.** Evolution over the ~3,449 structured
   heuristic parameters is *demonstrated* to reach 3.0+, is trivially parallel, and has no credit
   assignment. Using it to produce a strong teacher and then distilling into the learned heads
   (or simply initialising PG from it) is a much shorter path than making REINFORCE-with-scalar-
   baseline work on ~190-step episodes. The operator has ruled this out in favour of a pure
   gradient policy; that is a legitimate call, but §2.3 is the price.
6. **Re-tune the gate cadence, not its power.** With learning this flat, boundaries every 1,000
   updates burn ~14 panels each to measure nothing. Either train far longer between boundaries
   or (better) stop gating during exploratory runs — `--no-boundaries` already does this.

---

## Part 3 — Compute headroom in the Rust training path

Baseline from `STAGE2-SCHEDULING-WAVES.md` (Ryzen 9 9950X, 32 logical cores): ~0.62 core-s/game,
policy side ≈32% of game time (features 644 core-s vs scoring 117 core-s over 5.8M decisions),
engine ≈68%, learning-phase utilization 52% → 63% with `--rollout-depth 4`.

So **feature construction alone is ~27% of total compute** and is 5.5× the cost of scoring. That
is where to look, and the reason is visible in the types.

### 3.1 `FeatureVector = BTreeMap<String, f64>` in the innermost loop — the root cause

`ti4-policy/src/features.rs:34`. Every feature is a heap-allocated `String` key in a B-tree, built
fresh for every option of every decision, ~107M decisions per 1,000 updates.

```rust
fn add_named(features: &mut FeatureVector, name: &str, value: f64) {
    *features.entry(name.to_owned()).or_insert(0.0) += value;   // allocates even on update
}
```

and the callers feed it `format!("prompt-option:{prompt_token}:{option_token}")` — one `format!`
allocation per feature, then one `to_owned()` per insert, then an O(log n) descent with **string
comparisons** at every node. Scoring then does the same again against
`weights: BTreeMap<String, f64>` holding up to **14,220 entries** for the `other` head (measured
from `run_pure_u5000.json`: 47,267 weights per faction across 14 heads).

**Fix:** intern feature names to `u32` once and carry `Vec<(u32, f32)>` (or
`hashbrown::HashMap<u32, f32>`) instead. Weights become a dense `Vec<f32>` indexed directly — one
load instead of ~14 string comparisons. `hashbrown`, `smallvec` and `indexmap` are already
workspace dependencies and unused on this path.

For composed names, hash compositionally rather than materialising the string:
`h = FAMILY_SEED ^ rot(id(prompt_tok)) ^ rot2(id(option_tok))`. No `format!` at all.

*Estimated: 4–8× on feature construction, ~10× on scoring → ~26% of total compute recovered
(≈1.35× end-to-end).* Estimate, not a measurement — worth a criterion bench before committing.

### 3.2 The prompt is re-tokenized once per option (trivial, do it first)

`explicit_option_features` is called per option from `LearnedBot::consider`, and contains:

```rust
for prompt_token in tokens(&choice.prompt) { ... }
```

`tokens()` does a full `to_lowercase()` (one String allocation) plus one `to_owned()` per token —
and this runs again for every option in the choice. A transaction decision can offer 25+ options,
so the prompt is tokenized 25 times. Hoist it into `consider()` and pass a slice.

Same for `canonical_feature_kind` and the `kind:` feature. Pure win, no semantic change, no
parity risk — the feature *set* is identical.

### 3.3 The prompt × option cross product is quadratic and probably the dominant family

```rust
for prompt_token in tokens(&choice.prompt) {
    for option_token in &option_tokens {
        add_named(&mut features, &format!("prompt-option:{prompt_token}:{option_token}"), 1.0);
    }
}
```

6 prompt tokens × 8 option tokens = 48 `format!` + 48 B-tree inserts, **per option**. This is
almost certainly the majority of all features generated. Beyond §3.1's compositional hashing,
worth asking whether the family earns its keep: with 14k weights in the `other` head, a
prune-by-magnitude pass (drop `|w| < ε` with no recent gradient) would shrink both the map and
the scoring cost, and can be validated by a panel A/B.

### 3.4 Trajectory retention is mathematically unnecessary — remove it entirely

This is the most interesting one. `consider()` returns
`BTreeMap<String, BTreeMap<String, f64>>` per decision, and the recording path stores all of it
for the whole game. That is the "gigabytes of feature vectors" F15 worked around, and the direct
cause of `--pipeline` measuring *worse* ("memory pressure from ~192 live game states").

But `Statistics` only ever accumulates three sums (`gradient.rs:64`):

```
feature_difference_sum         = Σ_a ∇_a
return_feature_difference_sum  = Σ_a G_a ∇_a
entropy_gradient_sum           = Σ_a ∇H_a
```

The first and third are plain sums — streamable. The second looks like it needs the whole
trajectory, but with `G_a = Σ_{t≥a} r_t` it rearranges:

```
Σ_a G_a ∇_a  =  Σ_a (Σ_{t≥a} r_t) ∇_a  =  Σ_t r_t · S_t      where  S_t = Σ_{a≤t} ∇_a
```

So maintaining a running prefix sum `S_t` of feature differences lets the whole thing be computed
in **one forward pass with O(features) state and no trajectory storage at all**. The batch mean
subtracted later (`centered = Σ G∇ − mean·Σ∇`) applies to both accumulated sums independently, so
it is unaffected. `return_sum` / `return_square_sum` need only the scalar per-step rewards, which
are tiny.

This is exact, not an approximation. It removes essentially all training-path allocation churn,
should make `--pipeline` viable, and would let `rollout_depth` go well past 4 (where it currently
regresses on memory, not on staleness).

### 3.5 `consider()` clones every option id twice per decision

```rust
let legal: BTreeMap<String, FeatureVector> = ... (option.id.clone(), ...)
let scores: BTreeMap<String, f64> = legal.iter().map(|(id, v)| (id.clone(), ...))
```

Two `String` clones and two B-tree builds per option, per decision. Return
`Vec<(usize, FeatureVector)>` indexed by option position — the caller already has the `Choice`.

### 3.6 Swap the global allocator — one line, possibly the best ROI here

No `#[global_allocator]` is set, so this runs on the Windows system allocator, which is poor under
exactly this workload's profile (enormous small short-lived allocation churn across 32 threads).
`mimalloc` or `snmalloc` typically returns **20–40%** on allocation-bound multithreaded Rust on
Windows. It is a dependency plus three lines, needs no `unsafe` in your code (the crate provides
it, and `unsafe_code = "forbid"` is per-crate so the trainer binary is unaffected), and it is
independently verifiable with the existing `ab_measure.sh` harness.

Do this **before** §3.1 — it will also tell you how much of the feature cost is allocator vs
algorithm.

### 3.7 Also worth measuring, cheap

- **`RAYON_NUM_THREADS=16`** (physical cores only). SMT frequently hurts allocator- and
  memory-bandwidth-bound workloads; the 52–63% utilization figure may partly be SMT contention
  being counted as idle. One env var, one A/B.
- **`lto = "fat"`** instead of `"thin"`. `codegen-units = 1` is already set; fat LTO across the
  engine↔policy boundary may inline the scoring hot path. Costs build time only.
- **Reduce total decisions.** The engine is 68% of cost and hacan generates **24.6M** decisions
  vs xxcha's **15.7M** (+57%) — that delta is the transaction surface. `offer_options` emits the
  full `for give in 0..=3 { for want in 0..=3 }` cross product (16 options) plus commodity, note
  and card shapes. Many are strictly dominated from the proposer's side. If oracle parity permits
  pruning them, this cuts engine *and* policy cost proportionally — the only lever here that
  reduces work rather than cost-per-work.

### 3.8 Expected combined effect

| Change | Effort | Est. end-to-end |
|---|---|---|
| mimalloc (§3.6) | 10 min | 1.15–1.4× |
| Hoist prompt tokenization (§3.2) | 30 min | 1.05–1.1× |
| Streaming statistics (§3.4) | 1 day | memory, unlocks depth > 4 |
| Interned features + dense weights (§3.1, §3.3) | 2–4 days | 1.3–1.4× |
| `RAYON_NUM_THREADS=16`, fat LTO (§3.7) | 1 h | 1.0–1.15× |
| Utilization 63% → 85% (via §3.4) | — | ~1.3× |

Compounded, roughly **2–2.5×** on the learning phase, which would take the ~4.7× ceiling against
Python@w32 recorded in `STAGE2-SCHEDULING-WAVES.md` comfortably past the 5× requirement.

**Caveat worth stating plainly:** none of this addresses Part 2. Making a plateaued run 2.5×
faster gets you to the same 2.2 VP in less time. If the goal is 3.0 VP, §2.5 items 1, 2 and 4
matter and §3 does not — except insofar as cheaper experiments mean more of them.
