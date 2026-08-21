# MLP policy branch — plan

Branch: `codex/mlp-policy`, cut from `605f1c0` on `codex/stage1-parity-fixes`
(carries the planet-trait fix and the secondary-eligibility gates; 1271 tests green).

Status: **revision 2**, after codex review. Phase 2a is implemented and merged
(`0d751a8`); everything from Phase 2b onward is still argument, not code. Seven
points were raised on revision 1 — all seven accepted, all addressed below. The
two that changed a decision rather than adding detail are flagged inline.

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

**Mitigation for the attribution cost:** three independently testable feature
sets, not one flag.

| set | contents |
|---|---|
| `factual` | today's 13 prefixes, unchanged |
| `factual+objective` | plus §5.1 requirement/progress |
| `factual+objective+ability` | plus §5.3 faction decomposition |

Revision 1 claimed `--no-objective-features` would "reproduce today's vector
exactly". Codex is right that this could not hold with ability features enabled
and the observation path changed — one flag was being asked to gate three
independent things. Split as above, and **redaction is not one of the switches:
it is mandatory in every set**, because an agent trained on cards it cannot
legally see is wrong rather than merely differently-configured.

One thing that makes the split cleaner than codex assumed: **redaction changes
today's vector not at all.** Verified by grep — no feature in `features.rs`
derives from any secret or objective; the only match on "objective" in the whole
file is a comment. So `factual` *is* bit-identical to r6's vector, redaction on
or off, and it stays a valid baseline. That is a fact about today's extractor,
not a guarantee: once §5.2 lands, redaction is doing real work and the property
holds only for the `factual` set. A test pins it.

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
| D14 | Missing engine coverage | **In scope, done.** 14 (not 13) unreachable secrets implemented in `0d751a8` — see §5.4 |
| D15 | Value input | **State-only encoder**, option block zeroed; invariance to option order and to the legal set is a tested property — §4.2 |
| D16 | Distillation target | **Schema 4** (14 heads). The 19-head split is a later controlled migration, not part of this branch — §6.1 |
| D17 | Count normalisation | **Clipped `progress / threshold`**, plus the threshold as its own feature — §5.1 |
| D18 | Guard thresholds | **Derived from the r6 paired-panel variance**, not chosen — §6.3 |
| D19 | GPU | Kept, but **conditional**: the CUDA path ships only if it beats the CPU path on a measured end-to-end gate — §7 Phase 1 |

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

### 4.1 The feature vector, split

The input is split into two blocks that never overlap, because the critic needs
one of them alone:

```
x_state(s, f) = state_feats(s) ++ ability_feats(f) ++ emb[f]     # no option content
x_option(o)   = option_feats(o)                                  # no state content
```

`x_state` is what today's `state-kind:` / `state-option:` prefixes already
produce, plus the two new faction families. `x_option` is what `option:`,
`option-faction:`, `kind:` and the payload families produce. The existing
extractor already emits these as separate name families, so the split is a
partition of names, not a rewrite.

### 4.2 Policy and value (D7, D15)

```
per legal option i, for a decision by faction f on head h:

  z_i = trunk( x_state(s, f) ++ x_option(o_i) )
  s_i = w_readout[f, h] · z_i + b[f, h]
  p   = softmax(s / temperature)

once per decision, options absent:

  z_s = trunk( x_state(s, f) ++ 0 )        # option block zeroed, same trunk
  V   = w_value · z_s + b_value

  trunk(x) = relu(W2 · relu(W1 · x + b1) + b2)      # shared, 2 x 256
```

**The value input contains no option content at all.** Revision 1 wrote
`V = w_value · z_state` without saying where `z_state` came from, which left the
critic undefined — codex's blocker, and correctly raised. The fix is that `V` is
computed from a *separate forward pass* whose option block is zeroed, not from
anything derived from the option set.

This buys the two properties a value must have, and both are cheap to test
rather than argue:

| property | test |
|---|---|
| **Permutation invariance** — `V` unchanged when the legal options are reordered | shuffle `choice.options`, assert `V` bit-identical |
| **Legal-set invariance** — `V` unchanged when an option is added or removed | drop one legal option, assert `V` bit-identical |
| **Policy is not accidentally invariant** — the same shuffle *does* permute `p` correspondingly | shuffle, assert `p` permutes with it and its entropy is unchanged |

The third exists because the first two are satisfiable by a bug: a model that
ignores option features entirely passes both. Testing only invariance would let
that through.

Cost: **one extra trunk pass per decision**, against N passes for N options. At
the measured ~8 options per decision that is about +12% of model compute, which
is itself under 1% of the 450 µs/decision total. Not a consideration.

**Known wrinkle, accepted:** the shared trunk now sees two input distributions —
one with a populated option block, one with a zeroed one. That is a real
asymmetry and it is why D7 offered a separate value trunk as the alternative.
Kept shared because the representation the policy learns is the one the value
should be reading, and a zeroed block is a distribution the trunk can learn to
recognise. **Fallback if the value fails to fit** (explained variance stays near
zero while the policy improves): give the critic its own 2 × 128 trunk. That is
a contained change and does not touch the policy path.

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

### 4.3 Cost of per-option scoring

D3 runs the trunk once per legal option, so decisions with large option sets
(movement, production) dominate. **Codex's ruling: keep per-option initially,
but batch every option of a decision into one forward pass and keep a CPU path.**
Batching within a decision is not optional — it is what turns N small matmuls
into one `[N, d]` matmul, and it is the difference between the per-option model
being free and being the bottleneck.

The fallback stays a two-tower split — state trunk once per decision, thin
option tower per option, bilinear score — at the price of losing free
state×option interactions. Phase 4 measures; it is not decided in advance.

### 4.4 The checkpoint artifact (blocker, revision 1 had none)

Today a checkpoint is a JSON document whose `profiles` field is a map of faction
to `Profile` — schema, mode, and one named-weight map per head
(`learned.rs:261`). Nothing in that shape holds a tensor, and revision 1 said
nothing about what replaces it. Specified here before any model code, because
the distillation gate and every later comparison read these files.

**Format.** A directory, not a single file, so tensors are not base64 inside
JSON:

```
checkpoint-<update>/
  manifest.json        # schema, shapes, dtypes, provenance, checksums
  trunk.safetensors    # W1 b1 W2 b2
  readout.safetensors  # per-faction [heads, width] + bias
  value.safetensors    # w_value, b_value
  embedding.safetensors
  slots.json           # interned feature name -> column index, ordered
```

`safetensors` rather than a bespoke encoding: it is a flat, checksummable,
zero-copy format with no code execution on load, and it is readable from both
Rust and Python without agreeing on anything else. `slots.json` is
load-bearing — a tensor whose columns are feature slots is meaningless without
the name-to-column map that produced it, and the interner assigns ids in
first-seen order, which is not stable across runs.

**`manifest.json` carries, and loading verifies:**

| field | why it is checked |
|---|---|
| `schema: 6` | distinct from the linear schemas 2–5 so a wrong loader fails loudly |
| `trunk: {width, depth, activation}` | a shape mismatch must not be silently broadcast |
| `dtype: "f32"` | training and inference must agree; f16 is a later decision |
| `factions: [...]` | readout rows are positional; a reordered roster silently mislabels every faction |
| `slot_count` and `slots_sha256` | the single most likely silent corruption — same weights, different feature meaning |
| `heads: [...]` (schema-4 order) | see D16 below |
| `source`, `git_commit`, `update` | provenance for any number quoted from it |
| `sha256` per tensor file | integrity |

**Device behaviour.** Weights are always stored on CPU in the file and moved at
load. A checkpoint written from a CUDA run must load and play identically on a
CPU-only machine, bit-for-bit in f32 — that is a test, not an intention, and it
is what makes the GPU path revertible.

**Head mapping (D16).** Codex recommends schema 4 for the first distillation and
that is adopted. The r6 champions are schema-4 (14 heads); distilling them into
19 would leave `scoring`, `agenda`, `exploration`, `ability` and `transit`
initialised from the `other` teacher, which is exactly the head the objective
features are supposed to inform. **So: schema 4 for this branch**, `heads` in the
manifest records which, and the 14→19 split becomes a separate controlled
migration with its own before/after panel. Recorded as a decision rather than an
open question.

**Migration path.** No conversion of linear checkpoints to MLP checkpoints is
written — distillation replaces it, and a converter would be a second, worse
answer to the same problem. Linear checkpoints stay loadable and playable
unchanged, because the MLP-vs-linear panels need them.

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
objective-need:<family>:<threshold>      1.0        # threshold as its own feature
objective-progress:<family>              min(1, have / threshold)
objective-met:<alias>                    0/1
objective-stage:<1|2>                    count of each revealed
```

**Normalisation (D17).** Codex's ruling: clipped `progress / threshold`, with the
threshold emitted separately. Revision 1 proposed raw counts plus a raw gap, which
is the standard way to hand a network a badly conditioned first layer — `have` for
"control 11 planets" and `have` for "own 2 faction technologies" would share a
scale and a weight while differing by an order of magnitude. The ratio puts every
family on `[0, 1]` and makes "80% of the way there" mean the same thing across all
of them; clipping at 1 stops an overshoot from reading as more urgent than
completion. The threshold is kept as a distinct feature because the ratio alone
cannot distinguish "3 of 4" from "9 of 12", and the difficulty of the remaining
step is not the same.

The gap feature is dropped: `1 - progress` is a linear function of what is already
there, and a layer that wants it can form it.

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

### 5.4 Missing engine coverage (D14) — **done, commit `0d751a8`**

The count was 14, not 13. Twelve secrets had no scoring predicate, and two more
(`dp` Dictate Policy, `dtd` Drive the Debate) had or would have had one but were
never asked: `scoreable_on` returns status-timed secrets only, and no code path
anywhere offered an action- or agenda-timed secret. `Timing::Action` and
`Timing::Agenda` were read from the corpus, stored, and never consulted.

The twelve could not have a predicate in the shape `secrets.rs` was built for.
`Requirement` is `fn(&Position) -> bool` and a `Position` is the board; "destroy
another player's war sun" is not a fact about the board, because by the time
anything could ask, the war sun is gone and what remains looks identical to a
board where it never existed. Resolved with a **feat ledger** —
`GameState::record_feat`, scoped to `turn_seq`, in the same shape as the
one-shot markers already on `Player` — plus `ScoringWindow::for_event`, opened
from `Game::advance_turn` (the single point every kind of turn passes through)
and from `close_vote` after 8.20.

Evidence: **`docs/evidence/phase2a-secrets.md`** — command, fixed seed block,
pool and checkpoint sha256, and raw output, committed. Revision 1 quoted the
result with none of that, which codex correctly refused to accept as a completed
gate; the numbers were real but not reproducible from anything in the repo.

Measured on the r6 champions, 150 games, holdout pool: eight of the fourteen now
score — Prove Endurance 40% of draws, Drive the Debate 17%, Dictate Policy 16%,
Betray a Friend 7%, Turn Their Fleets to Dust 6%, Spark a Rebellion 5%, Make an
Example of Their World 3%, Unveil Flagship 2%. The remaining six are rare events
inside a four-round horizon rather than engine gaps. Direct scoring effect is
about **+0.05 VP/seat** (mean 2.918, from 2.89) — though the 2.89 came from the
r6 promotion panel rather than this command, so **the delta is indicative, not a
paired measurement**, and the pre-fix binary is not archived to make it one.
Treat 2.918 as the new baseline and the delta as unverified. The value is not the
delta anyway: it is that fourteen cards are now pursuable at all.

The original finding, kept for the record — 13 of 40 secrets had **no scoring
predicate anywhere** in `ti4-engine` or `ti4-policy`:

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
`:381 apply_promotion`). Codex is right that revision 1's "2σ plus guessed
thresholds" was not a specification. Pre-registered here, and the numbers are
**measured from the r6 panel history rather than chosen** (D18).

**The variance, from 60 per-faction boundary records across the r6 logs**
(100 paired seeds each):

| quantity | mean | median | range |
|---|---|---|---|
| VP paired SE | 0.0526 | 0.0517 | 0.035 – 0.072 |
| clearance paired SE | 0.0161 | 0.0161 | 0.009 – 0.021 |

Two things fall straight out, and the first is a real defect in revision 1:

- **The 0.30 VP guard was 5.7σ.** It could not have fired on anything short of a
  total collapse. It was decorative, and it would have been quietly decorative
  for the whole run.
- **The 0.05 clearance guard was 3.1σ** — a defensible bar, but arrived at by
  guessing, and it happened to land in a reasonable place.

**Pre-registration:**

| element | specification |
|---|---|
| **paired unit** | one `(seed, rotation)` pair, candidate and champion on the identical board and seating. Pairing is what makes the SE 0.05 instead of the ~0.9 of unpaired game-to-game VP |
| **panel** | 100 validation seeds, then 100 disjoint confirmation seeds, × 6 rotations. Fixed blocks, recorded per boundary |
| **aggregate metric** | mean VP per seat across all 6 factions, paired difference |
| **confidence** | SE of the paired differences over seeds, treating a rotation-set as one unit. Promote on `gain > 2 × SE` **and** `gain > 0.05` absolute — both, since at n=100 a 2σ bar alone admits gains too small to matter |
| **confirmation rule** | the confirmation panel must independently clear the same bar. Two panels rather than one 200-seed panel because it also catches a candidate that is merely lucky on one block |
| **regression guard** | per faction: reject if paired VP loss exceeds **0.14** or clearance loss exceeds **0.043** |
| **multiplicity** | those two numbers are `z = 2.64 × mean SE`, the Bonferroni-corrected two-sided 5% level for **six simultaneous guards**. Uncorrected 2σ guards false-alarm 4.6% each, so **any of six trips 24% of the time under the null** — a quarter of genuinely neutral candidates rejected for nothing. Revision 1 did not account for this at all |
| **re-derivation** | the SEs are recomputed from this branch's own panels after the first ten boundaries and the guards reset if they have moved; r6's variance is a prior, not a constant |

Panel cost falls either way: one champion instead of six.

---

## 7. Phases

| # | Phase | Exit criterion |
|---|---|---|
| **0** | **Profile.** `cargo flamegraph` on one rollout batch. Confirm or refute the ~450 µs/decision split between engine and feature extraction. | The split is a measured number, not an inference. If feature-string `format!` allocation is a large share, fix that first — it makes every later phase cheaper. |
| **1** | **libtorch, pinned, with a throughput gate.** See §7.1 — a smoke test is not the exit criterion. | The CPU path meets the throughput gate. The CUDA path ships only if it also beats it. |
| **2a** | ~~**Engine: close the coverage gap.** Implement the missing secrets and the event ledger they need.~~ **Done, `0d751a8`.** | ~~1274 tests green (was 1271). Eight of the fourteen show a non-zero draw-to-score rate; the rest are decidable and rare.~~ |
| **2b** | **Engine: expose progress.** Refactor `requirement_for` and the secrets families to return counts rather than bools; `satisfied` derives from them. | Tests still green. No feature work yet. |
| **3** | **Features.** Objective requirement/progress, ability decomposition, secret redaction. Three independent feature sets per §1; redaction always on. | `feature_inventory.rs` shows the new families in each set; a test asserts the `factual` set is byte-identical to r6's emitted names on a fixed decision corpus. |
| **4** | **Model.** Shared trunk, per-faction readout, value head, per-option scoring, all options of a decision batched into one pass. Inference path only. | An untrained MLP plays legal games end to end; §7.1's gate is re-measured with the real model and the two-tower fallback decided on the number. |
| **5** | **Distillation.** Six champions → one MLP. | Mean VP within 0.1 of r6 on the holdout panel. |
| **6** | **PPO.** New gate, value head, Adam. | A run that promotes at least twice without a per-faction regression trip. |
| **7** | **Evaluate.** Full run to the mean-6-VP bar, with the `--no-objective-features` ablation. | Mean 6 VP at round 4 on holdout, or a documented account of what stopped it. |

Phases 0–2 touch no machine learning at all and are worth doing whatever
happens to the rest of the plan. **Phase 2a is done** — see §5.4 and
`docs/evidence/phase2a-secrets.md`.

### 7.1 The GPU gate (D19)

Revision 1 made Phase 1 a smoke test. Codex is right that this is the wrong exit
criterion, and the underlying worry is well founded: **a sequential game engine
calling CUDA once per decision will lose to CPU**, because a 2 × 256 MLP over
~8 rows is microseconds of arithmetic wrapped in tens of microseconds of launch
and synchronisation, and the engine is already the dominant cost. Nothing about
owning a 3090 changes that arithmetic.

So the GPU is no longer assumed. D12 becomes D19: **the CUDA path ships only if
it wins a measured gate.**

**Pinned before anything is written:** `tch` version, the libtorch build it links
(cu12x), the CUDA runtime, and the driver. Recorded in this document and in each
checkpoint manifest, because "it got slower" is unanswerable without them.

**Batching, in two tiers.** Within-decision batching — every legal option of one
decision as a single `[N, d]` forward pass — is mandatory and is the only tier
Phase 4 requires. It is also *not enough on its own*: N ≈ 8 leaves the GPU
almost entirely idle. The tier that would actually use the hardware is
cross-game batching, a request queue fed by the 96 games in flight, and it is
deliberately **out of scope for this branch** — it means restructuring the
rollout loop around a scheduler, and it should not be attempted before the gate
below says there is anything to win.

**CPU fallback is the default, not the emergency.** `--device cpu|cuda`, CPU
selected unless asked otherwise, and the bit-identical-load test in §4.4 is what
keeps that honest.

**The gate.** One representative end-to-end measurement, not a microbenchmark:
a full update — 16 seeds × 6 rotations, 4 rounds, `full_np8_12_train`, 32
threads — timed for the current linear policy, the MLP on CPU, and the MLP on
CUDA.

| result | consequence |
|---|---|
| MLP-CPU within ~2× of linear | acceptable; the earlier estimate says 1.6–2.8× at these widths |
| MLP-CUDA beats MLP-CPU | CUDA becomes the default |
| MLP-CUDA loses to MLP-CPU | **CPU ships, CUDA is deleted from the branch.** Not kept "for later" — an unused second device path is a source of divergence, and §4.4's load test is the thing that lets it come back cheaply |
| MLP-CPU worse than ~3× linear | stop and reconsider width or the two-tower fallback before Phase 5 |

Recorded with the same evidence discipline as §5.4: command, seeds, pool
checksum, raw output, committed.

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
| ~~**Event ledger touches combat**~~ (§5.4) | Landed additive-only; no change to combat resolution itself. 1274 tests green. Risk retired. |
| **Newly-scoreable secrets shift the reward landscape** | Every prior run's VP numbers become non-comparable the moment Phase 2 lands. Re-baseline r6's champions on the fixed engine before drawing any MLP comparison. |
| **`out/` is gitignored** (`.gitignore:24`) | Was listed as worth fixing independently; codex is right that it is a **prerequisite**, since the success and distillation gates both read those files. Addressed in §11. |
| **Overfitting the 96-game batch** | 7.6M params against 96 games/update is a large ratio. Weight decay, and the holdout pool (`full_np8_12_holdout.json`, zero overlap with train) is the honest measure. |

---

## 9. Open questions

**Closed by codex review (revision 2):**

1. ~~Per-option vs two-tower~~ — per-option, batched across a decision's options,
   CPU path retained. Two-tower stays the measured fallback (§4.3).
2. ~~Schema 4 or 5~~ — schema 4 for distillation; the 14→19 split becomes a
   separate controlled migration (D16, §4.4).
4. ~~Regression-guard thresholds~~ — derived from the r6 paired-panel variance,
   not chosen: 0.14 VP / 0.043 clearance, Bonferroni-corrected for six
   simultaneous guards (D18, §6.3).
5. ~~Count normalisation~~ — clipped `progress / threshold`, with the threshold
   emitted as its own feature so the ratio is not asked to carry both (D17).
6. ~~Event ledger shape~~ — `turn_seq`-scoped, one variant per card, offered once
   per turn from `advance_turn` (§5.4).

**Still open:**

3. **Ability decomposition completeness** (§5.3). Does it separate all 34
   factions? If not, the identity embedding is doing load-bearing work and the
   generalisation claim weakens. Cheap to settle: decompose all 34 and check for
   collisions before Phase 3 relies on it.
7. **One objective per turn, or per combat?** (§5.4). The event window is offered
   once per turn, which approximates 61.6's one-per-action but is not identical
   for a turn containing several fights. Tightening means a window per combat and
   a per-combat feat scope. Currently rare enough not to matter; worth a decision
   before it is load-bearing.
8. **Distillation corpus** (§6.1). Decisions sampled from champion self-play — but
   on which pool, how many, and refreshed as the student drifts, or fixed? A fixed
   corpus is reproducible; an on-policy one avoids the student being trained only
   where the teacher goes. Not yet specified.

---

## 10. Artifact manifest (prerequisite, not a nice-to-have)

The success criterion and the distillation gate both depend on files that are
not in the repo. Codex is right that this makes them a prerequisite rather than
housekeeping: a baseline nobody else can obtain is not a baseline.

The two kinds of artifact need different answers, and revision 1 conflated them.

**Reproducible — regenerate, do not archive.** The map pools are deterministic
output of committed code:

```sh
cargo run --release --example generate_pool -- \
    --seed 1 --boards 4000 --min 8 --max 12 \
    --out out/pools/full_np8_12_train.json
cargo run --release --example generate_pool -- \
    --seed 777 --boards 1000 --min 8 --max 12 \
    --out out/pools/full_np8_12_holdout.json
```

xorshift64* seeded by `--seed`, no clock, no thread order.

**Verified, not assumed.** Both pools were regenerated from these commands on
commit `635d67d` and hashed: `full_np8_12_holdout.json` reproduces bit-for-bit in
2.5 s, `full_np8_12_train.json` in 8.3 s. A regenerated pool that does *not* match
the checksum below means the corpus moved, and every number measured against it
needs re-reading.

**Not reproducible — must be archived.** Training is stochastic across threads,
so no checkpoint can be regenerated from source. These have to be stored out of
band, and until they are, every r6 number in this document rests on one machine's
disk.

**The manifest** (`docs/evidence/artifacts.md`, checksums verified at use):

| artifact | sha256 (first 16) | recoverable? |
|---|---|---|
| `out/stage2_r6/final10000.json` | `be792a2a207ced25` | **no — archive required** |
| `out/pools/full_np8_12_holdout.json` | `aba33c81aa04cefb` | yes — **verified**, `--seed 777` |
| `out/pools/full_np8_12_train.json` | `106153d4384435b1` | yes — **verified**, `--seed 1` |
| `out/stage1_hacanclone/frozen5000.json` | `0d0fa9e5d7a3f9ce` | **no — archive required** |

**Action before Phase 5:** archive the r6 champions and the Stage-1
`frozen5000.json` somewhere durable, record the location, and add a check that
refuses to run a comparison against a checkpoint whose checksum is not in the
manifest. A gate that silently compares against the wrong file is worse than one
that fails.

---

## 11. What is explicitly not in this branch

- Widening the roster past six (D11) — the architecture supports it; the run
  does not attempt it.
- Extending the horizon past 4 rounds (D13).
- Width sweep beyond 2×256 (D10).
- Replacing the batch-mean baseline in the *linear* pipeline.
- Any hand-authored evaluator, teacher, or preference. The standing constraint is
  straight learning, and it holds throughout: every feature added here reports a
  fact the engine already computes, and every weight is learned.
