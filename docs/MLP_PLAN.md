# MLP policy branch — plan

Branch: `codex/mlp-policy`, cut from `605f1c0` on `codex/stage1-parity-fixes`
(carries the planet-trait fix and the secondary-eligibility gates; 1271 tests green).

Status: **draft for review.** Nothing here is implemented. This document is the
argument, not the code, and is expected to be revised several times before any
of it is written.

---

## 1. What this branch is for

Replace the linear softmax policy with a nonlinear one, and — in the same
change — give the policy features that describe what the objectives on the table
actually demand.

These are bundled deliberately, against the usual advice to change one thing at
a time. The reasoning: the measured evidence from runs r1–r6 says the binding
constraints are **information** and **gradient variance**, not capacity. Adding
capacity alone would very likely produce no visible movement and cost a week to
learn that. Adding information alone can be done on the linear model but is
capped by what a dot product can express about "I need four planets of one trait
and I have two." The two changes are complementary, and the plan accepts the
attribution cost.

**Mitigation for the attribution cost:** the objective/ability feature families
go behind an explicit flag from day one, so `--no-objective-features` reproduces
the pure-architecture ablation without a code change. Any headline result should
be reported alongside that ablation.

### Success criterion

**Mean 6 VP per seat at the round-4 horizon on the holdout pool.**

This is the stated exit condition for widening the horizon, and it is an
absolute bar rather than a comparison, so it does not flatter the architecture
for being slow. r6's linear champions sit at **2.89 mean VP / 0.890 clearance**;
that is the number to beat and the run to compare against.

The horizon stays at 4 rounds until that bar is met. It is not a second moving
variable.

---

## 2. Where we are starting from

### The current model, precisely

There is no neural network anywhere in the repo today. The whole model is:

```
score(option) = Σ_slots weight[slot] × value[slot]        learned.rs:384
p             = softmax(score / temperature)
```

- One weight vector per **head**. Schema 4 (what the r6 champions use) has 14
  heads (`learned.rs:82`); schema 5 has 19 (`learned.rs:59`). Routing is
  `decision_head()` at `learned.rs:187`.
- One `Profile` per faction (`learned.rs:261`), ~29k named weights each.
- Features are sparse named strings interned to slots, from a closed list of 13
  prefixes (`features.rs`, ~line 1397). Option words come from the option's
  **id and label only** — never from the prompt.
- No hidden layer, no critic. PPO's baseline is the batch mean (`ppo.rs:98`,
  `Moments`).

### Measured cost, from the r6 log

`learning 5000..5500: 862.3s, 60752955 decisions` →

| quantity | value |
|---|---|
| decisions per update | ~121,500 |
| wall-clock per update | 1.72 s |
| per decision per thread | **~450 µs** |
| games per update | 96 (16 seeds × 6 rotations) |

The linear forward pass is well under 1% of that. **The engine and feature
extraction dominate.** This number is inferred from the aggregate, not profiled
— see Phase 0.

### Hardware

RTX 3090, 24 GB, compute capability 8.6; 32 CPU threads. Installed torch is a
**CPU-only** build (2.9.1+cpu), so a GPU path requires a new install regardless
of language.

---

## 3. Decisions taken

Settled in interview. Each line is a commitment, not a suggestion; reopening one
should be explicit.

| # | Decision | Choice |
|---|---|---|
| D1 | Scope | MLP **and** objective features together, with an ablation flag |
| D2 | Framework | **tch-rs / libtorch**, all in Rust — same C++/CUDA PyTorch uses, no Python on the hot path |
| D3 | Option scoring | **Per-option MLP**: `trunk(state ++ option_i) → scalar`, run per legal option |
| D4 | Warm start | **Distill** the six linear champions into the shared MLP, then hand over to PPO |
| D5 | Objective features | **Requirement + progress decomposition**, derived from the engine's own scoring predicates |
| D6 | Secret visibility | Own secrets in full; opponents' as **counts only** |
| D7 | Critic | **Value head off the shared trunk** |
| D8 | Parameter sharing | **Shared trunk**, faction conditioning at the input, **thin per-faction readout** at the output |
| D9 | Promotion gate | **Table-wide merit + per-faction regression guard** |
| D10 | Trunk shape | **2 × 256** to start; width sweep {64, 128, 256, 512} as a follow-up |
| D11 | Roster | The current **six** (sol, letnev, xxcha, hacan, jolnar, l1z1x) |
| D12 | GPU | **From the start** — CUDA path stood up before the training code |
| D13 | Horizon | Hold at **4 rounds**; extend when mean VP reaches 6 |
| D14 | Missing engine coverage | **In scope for this branch.** The 13 unimplemented secrets get implemented here, not deferred |

### Why shared, when the gate was per-faction

`factions.json` carries **34** factions. Six independent models each see 16
games of gradient per update; thirty would see ~3. Gradient variance is already
the constraint the promotion gate has rejected on for four consecutive runs —
independent models make the one existing problem five times worse, and the
boundary-panel cost (100+100 seeds × 6 rotations, per faction) scales linearly
on top.

The consequence is that promotion can no longer be per-faction: you cannot move
Hacan's trunk without moving Jol-Nar's. Hence D9.

### Why abilities, not a faction one-hot

An identity embedding for faction 35 is untrained on arrival. Decomposing
`abilities.json` into typed features — "resolves the primary when taking this
secondary" (Jol-Nar *Brilliant*), "free secondary on Trade" (Hacan *Master of
Trade*) — means a faction assembled from known mechanics is playable
immediately. This is the same argument accepted for objectives in D5.

A **small identity embedding (dim 8–16) is kept alongside** to absorb whatever
the decomposition misses, zero-initialised for unseen factions.

### The redundancy to watch

Faction information now enters at the input (abilities + embedding) *and* at the
output (per-faction readout). Input-side conditioning is strictly more
expressive — the trunk can compute faction-specific intermediates — while the
readout only re-weights a faction-agnostic representation. Both are kept, but
**the embedding and the readout are exactly where the model will park anything
it fails to learn structurally.** Keep them small, weight-decay them, and treat
a large embedding norm as a diagnostic that the ability decomposition is
incomplete.

The readout buys back a concrete capability: when one faction stalls, copy a
healthy faction's readout onto it and leave the trunk alone — the manual Sol
intervention from r6 update 7500, as a supported operation.

---

## 4. Target architecture

```
per legal option i, for a decision by faction f on head h:

  x_i = state_feats  ++  option_feats_i  ++  ability_feats(f)  ++  emb[f]
  z   = relu(W1 · x_i + b1)          # sparse input, EmbeddingBag-style gather
  z   = relu(W2 · z   + b2)          # trunk: shared across all factions, 2 x 256
  s_i = w_readout[f, h] · z + b[f, h]

  p   = softmax(s / temperature)
  V   = w_value · z_state            # value head, state-only trunk pass
```

**Parameter budget** (2 × 256, ~29k feature slots):

| block | params | notes |
|---|---:|---|
| input layer | 29k × 256 ≈ 7.4M | shared; sparse gather, ~30 active slots per option |
| hidden layer | 256 × 256 ≈ 66k | shared |
| per-faction readout | 256 × 19 heads ≈ 4.9k each | × 34 factions ≈ 165k |
| identity embedding | 16 × 34 ≈ 0.5k | |
| value head | 256 | |
| **total** | **~7.6M** | ~30 MB fp32 — trivial for 24 GB |

The input layer dominates the parameter count but not the compute: with ~30
active slots per option it is a gather of 30 columns, not a 29k-wide matmul.

**Cost concern (open):** D3 runs the trunk once per legal option. Decisions with
large option sets (movement, production) will dominate. If profiling shows this
is the bottleneck, the fallback is a two-tower split — state trunk once per
decision, thin option tower per option, bilinear score — at the price of losing
free state×option interactions. **Flagged for codex: is per-option worth it, or
should the two-tower be the default from the start?**

---

## 5. Feature work

### 5.1 Objectives — requirement and progress

`objectives.rs:564` `requirement_for(alias) -> Option<Requirement>` already
exists, and every registered public decomposes into one of ~12 counting
families:

```
non_home(n)          on_the_rim(n)        same_trait(n)
tech_specialties(n)  unit_upgrades(n)     colours(per, k)
structure_count(n)   structures_away(n)   fleet_in_one_system(n)
planetless_systems(n) attached_planets(n) in_notable_systems(n)
```

plus six bespoke predicates (`conquer_the_weak`, `intimidate_council`,
`push_boundaries`, `rule_distant_lands`, `engineer_a_marvel`,
`achieve_supremacy`) and 10 bought objectives with `Cost` (`objectives.rs:827`).

**Each family already computes a count and compares it to a threshold, then
discards the count.** The refactor is mechanical:

```rust
// today
fn same_trait(count: usize) -> impl Fn(&Position) -> bool { ... >= count }

// after
fn same_trait_progress(p: &Position) -> usize { ... }
// Requirement { family: SameTrait, threshold: n, progress: same_trait_progress }
// satisfied() stays `progress(p) >= threshold` — one source of truth
```

`secrets.rs` has the same shape with six families
(`ground_forces_on_one_planet`, `mechs_on_distinct_planets`,
`planets_of_trait`, `same_colour_technologies`, `ships_in_systems`, `units`).

Emitted features, per revealed public and per held secret:

```
objective-need:<family>:<threshold>      1.0
objective-have:<family>                  count      (normalised)
objective-gap:<family>                   max(0, threshold - count)
objective-met:<alias>                    0/1
objective-stage:<1|2>                    count of each revealed
```

Keying on **family** rather than alias is the point: Outer Rim (`on_the_rim(3)`)
and Control the Borderlands (`on_the_rim(5)`) share machinery, so learning about
one transfers to the other, and to any future objective built from the same
family.

**This is not a heuristic.** No weight, score, or preference is authored. The
features report what the card demands and what the seat currently has; the
policy learns entirely on its own whether to care. The single-source-of-truth
refactor is what keeps it that way — a hand-written requirement table would be
my judgement about what each objective "wants" and was rejected for that reason.

### 5.2 Secrets — redaction

Features are currently built from a raw `Observed`. Per D6 the feature path must
take the acting player's id and build from `Observed::redacted_for(player)`,
emitting for opponents only `opponent-secrets-held:<n>` (which is public
information in TI4).

This is real plumbing through `features.rs` and every caller. It is also
correctness-critical: full information in self-play produces an agent that
reacts to cards it could not legally see and misplays against anything else.

### 5.3 Faction abilities

New family derived from `abilities.json`, plus starting units, home planets and
faction technologies. Must be complete enough that two factions are never
identical under decomposition — **flagged for codex: verify this holds across
all 34 before relying on it.**

### 5.4 Missing engine coverage (D14)

13 of 40 secrets have **no scoring predicate anywhere** in `ti4-engine` or
`ti4-policy`:

```
dtgs  Destroy Their Greatest Ship     bam   Become a Martyr
mew   Make an Example of Their World  baf   Betray a Friend
sar   Spark a Rebellion               btv   Brave the Void
ttfd  Turn Their Fleets to Dust       dts   Darken the Skies
uf    Unveil Flagship                 dyp   Demonstrate Your Power
fwp   Fight with Precision            pe    Prove Endurance
dtd   Drive the Debate (2 incidental hits — confirm)
```

Earlier analysis recorded "24 secrets score zero" and attributed it to policy.
At least half of that is unimplemented scoring. **A mean-6-VP target cannot be
reached while roughly a third of the secret deck is unscoreable**, and no amount
of model capacity will change it. These are combat-outcome and event secrets,
which is also where the round-4 horizon bites hardest.

Publics are fine: all 40 are registered (30 predicates + 10 bought).

Several of these are **event-conditioned rather than state-conditioned** —
"destroy another player's flagship", "win a combat against a player with more
ships" — which the current `Position → bool` shape cannot express. They need an
event ledger on `GameState` that combat and agenda resolution write to. That is
the substantive part of Phase 2 and is where its risk sits.

---

## 6. Training

### 6.1 Distillation (D4)

Six schema-4 linear champions → one shared MLP.

```
phase 0:  minimise  Σ_f  KL( champion_f(·|s) ‖ mlp(·|s, f) )
          over decisions sampled from champion self-play rollouts
          no reward signal, supervised only
phase 1:  PPO from those weights
```

Multi-teacher distillation onto a shared trunk with per-faction readouts is a
well-trodden setup and is arguably cleaner than six separate distillations: the
trunk is forced to find the representation common to all six.

**Open — flagged for codex:** the champions are schema 4 (14 heads); the target
may be schema 5 (19 heads). Schema 4 routes the five later splits into `other`
(`learned.rs:597`). Distilling 14 teachers into 19 student heads needs an
explicit mapping, and the five split-out heads (`scoring`, `agenda`,
`exploration`, `ability`, `transit`) would start from the `other` teacher.
Given that `scoring` is exactly the head the objective features are meant to
inform, this matters. Alternative: target schema 4 and defer the split.

Exit criterion for distillation: mean VP within 0.1 of the r6 champions on the
holdout panel. If it cannot reach that, the architecture cannot represent the
current policy and something is wrong before PPO ever runs.

### 6.2 PPO changes

Existing: clipped surrogate, K=4 epochs, clip 0.2, lr 0.03, entropy 0.01,
`--draft-entropy 0.10` on the `strategy` head. `ppo.rs:398 apply`, `:486 update`.

Changes:
- Optimiser moves to Adam under tch (lr will need retuning — 0.03 is a
  plain-SGD-scale learning rate and is almost certainly wrong for Adam).
- **Value head (D7):** advantage becomes `return − V(s)` instead of
  `return − batch_mean`. Adds a value-loss coefficient to tune; if weighted too
  heavily it destabilises the shared trunk. Start at 0.5 and treat explained
  variance of `V` as the health metric.
- Entropy handling is unchanged in intent but the per-head entropy bonus now
  applies to a shared trunk — worth checking it does not fight the readouts.

### 6.3 Promotion gate (D9)

`promotion.rs` currently promotes per faction (`:212 is_better`, `:231 promote`,
`:381 apply_promotion`). Replacement:

- **Merit:** aggregate mean VP and clearance across all seats, same validation +
  confirmation panel structure, same 2σ significance requirement.
- **Regression guard:** reject the candidate if any single faction's VP drops
  more than a threshold (start at 0.30) or clearance more than 0.05, even when
  the table-wide number improves. This preserves the property that made the old
  gate useful — catching one faction collapsing behind a strong average.
- Panel cost falls: one champion instead of six.

---

## 7. Phases

| # | Phase | Exit criterion |
|---|---|---|
| **0** | **Profile.** `cargo flamegraph` on one rollout batch. Confirm or refute the ~450 µs/decision split between engine and feature extraction. | The split is a measured number, not an inference. If feature-string `format!` allocation is a large share, fix that first — it makes every later phase cheaper. |
| **1** | **libtorch on this machine.** tch-rs + CUDA 12.x, RTX 3090, Windows. Smoke test: a 2×256 MLP trains on synthetic data on the GPU. | A green test, and the DLL/PATH setup written down in this document. |
| **2** | **Engine: close the coverage gap and expose progress.** Implement the 13 missing secrets, including the event ledger the event-conditioned ones need. Refactor `requirement_for` and the secrets families to return counts; `satisfied` derives from them. | 1271 existing tests still green, plus new tests per newly-scoreable secret. `objective_report.rs` shows a non-zero draw-to-score rate for each. No feature work yet. |
| **3** | **Features.** Objective requirement/progress, ability decomposition, secret redaction, all behind flags. | Feature inventory (`examples/feature_inventory.rs`) shows the new families; `--no-objective-features` reproduces today's vector exactly. |
| **4** | **Model.** Shared trunk, per-faction readout, value head, per-option scoring. Inference path only. | An untrained MLP plays legal games end to end at a measured cost per update. |
| **5** | **Distillation.** Six champions → one MLP. | Mean VP within 0.1 of r6 on the holdout panel. |
| **6** | **PPO.** New gate, value head, Adam. | A run that promotes at least twice without a per-faction regression trip. |
| **7** | **Evaluate.** Full run to the mean-6-VP bar, with the `--no-objective-features` ablation. | Mean 6 VP at round 4 on holdout, or a documented account of what stopped it. |

Phases 0–2 touch no machine learning at all and are worth doing whatever
happens to the rest of the plan. **Phase 2 is being done first** — see D14.

---

## 8. Risks

| Risk | Assessment |
|---|---|
| **Attribution** — two changes at once (D1) | Accepted deliberately. Ablation flag is the mitigation; use it. |
| **Per-option cost** (D3) | Decisions with large option sets could dominate. Phase 4 measures it; two-tower is the fallback. |
| **Windows + libtorch** (D2, D12) | Real friction: ~2 GB download, `LIBTORCH` env, DLLs on PATH. Phase 1 exists to hit this before anything depends on it. |
| **Adam lr** | The existing 0.03 will not transfer. Budget a sweep; a bad lr will look like "the MLP doesn't work". |
| **Value head destabilising the trunk** | Watch explained variance. Falling back to the batch-mean baseline is a one-line change and a legitimate result. |
| **Shared trunk averages away faction identity** | The per-faction readout and embedding exist for this. If a faction regresses, check whether its readout is doing anything. |
| **Event ledger touches combat** (§5.4) | The riskiest engine change in the plan: combat resolution is load-bearing for 1271 tests. Additive-only writes, no behaviour change to resolution itself. |
| **Newly-scoreable secrets shift the reward landscape** | Every prior run's VP numbers become non-comparable the moment Phase 2 lands. Re-baseline r6's champions on the fixed engine before drawing any MLP comparison. |
| **`out/` is gitignored** (`.gitignore:24`) | Checkpoints, pools and logs are **not** restorable from git. Any r6-comparison depends on files that exist only on this machine. Worth fixing independently. |
| **Overfitting the 96-game batch** | 7.6M params against 96 games/update is a large ratio. Weight decay, and the holdout pool (`full_np8_12_holdout.json`, zero overlap with train) is the honest measure. |

---

## 9. Open questions for codex

1. **Per-option MLP vs two-tower** (§4). Is the free state×option interaction
   worth running the trunk per option, given movement and production decisions
   have large option sets?
2. **Schema 4 or 5** (§6.1). Distil into 14 heads or 19? The `scoring` head is
   exactly where the objective features should land, which argues for 19 — but
   it has no dedicated teacher.
3. **Ability decomposition completeness** (§5.3). Does it separate all 34
   factions? If not, the identity embedding is doing load-bearing work and the
   generalisation claim weakens.
4. **Regression-guard thresholds** (§6.3). 0.30 VP / 0.05 clearance are guesses.
   What does the r1–r6 boundary history suggest?
5. **Normalisation of count features** (§5.1). Raw counts, `count/threshold`,
   or one-hot buckets? Raw counts in a network with no input normalisation is a
   known way to get a badly conditioned first layer.
6. **Event ledger shape** (§5.4). Per-round or per-game? Cleared when? Several
   secrets say "during a single combat", which needs finer granularity than a
   game-long tally.

---

## 10. What is explicitly not in this branch

- Widening the roster past six (D11) — the architecture supports it; the run
  does not attempt it.
- Extending the horizon past 4 rounds (D13).
- Width sweep beyond 2×256 (D10).
- Replacing the batch-mean baseline in the *linear* pipeline.
- Any hand-authored evaluator, teacher, or preference. The standing constraint is
  straight learning, and it holds throughout: every feature added here reports a
  fact the engine already computes, and every weight is learned.
