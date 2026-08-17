# Results: algorithm-independent efficiency work

Date 2026-08-17. Branch `codex/stage1-parity-fixes`, commits `46404a0..HEAD`.
Companion to `plans/PLAN_2026-08-17_ENGINE_EFFICIENCY.md`, which this supersedes on numbers.

---

## Headline

**3.29× on the production configuration**, measured on exactly one training update.

| save52 pool, 96 games = one update | baseline `46404a0` | now |
|---|---|---|
| one update | 2.11 s | **0.642 s** |
| per game | 21.98 ms | **6.69 ms** |
| 1,000 updates | 35.2 min | **10.7 min** |

Later work on the per-option path (`8c60d33`, `cb28495`) took this from 2.96× to 3.29×; see
"Per-option cost" below.

(The recorded run `learning 15100..16100` took 2616.6 s = 43.6 min for 1,000 updates, against
35.2 min measured here for the same work — the live run also checkpoints and competes with
whatever else the machine was doing. Applying the ratio to the observed figure gives ~14.7 min.)

On the probe configuration used for attribution throughout this document — Rust varied maps,
90 games — the same work is **2.61×**.

## Attribution: two changes did nearly all of it

| stage | probe ms/game | step | share of the gain |
|---|---|---|---|
| baseline `46404a0` | 20.807 | — | — |
| ten bit-identical commits (mimalloc-dominated) | 12.415 | 1.68× | **54%** |
| feature vectors keyed by hash | 8.212 | 1.51× | **43%** |
| naming (buffer, part-hashing, zip) | 7.978 | 1.03× | 3% |

Shares are log-space, so they compose. **`mimalloc` and hash-keying account for ~95% of the
total**; the other ten perf commits together are ~5%. Several of those ten measured within
run-to-run noise individually and are kept because they are strictly less work, not because they
were shown to pay.

Split of the remaining 7.978 ms: policy 4.094 (51%), record ~1.6 (20%), reduce ~1.4 (18%),
engine 0.77 (9%).

The first ten commits (up to `0789519`) reached 1.68× and were **bit-identical**. The eleventh
(`f7b2716`, feature vectors keyed by hash rather than by name) took that to 2.53× and is the one
change that gives up bit-identity — deliberately, on instruction, and with the evidence recorded
in §"Keying features by hash" below.

## The bit-identical portion (1.68×)

Measurement protocol, identical at both ends: 15 seeds × 6 rotations = **90 games**, trained
profiles from `out/run_pure_u5000.json` (286,431 weights), `FULL` sources, four rounds, release,
single-threaded, **min of five runs** (min rather than mean: it is far less sensitive to
scheduler noise, which was swamping 2% effects under a mean).

| | baseline `46404a0` | after `0789519` | |
|---|---|---|---|
| play (rollout) | 15.056 ms/game | 8.228 ms/game | **1.83×** |
| reduce (gradient statistics + merge) | 5.751 ms/game | 4.187 ms/game | **1.37×** |
| **full update** | **20.807 ms/game** | **12.415 ms/game** | **1.68×** |

**Stats digest `d821a383e110b21e` at both ends.** An FNV-1a hash over every accumulated value in
the reduced batch — hashed by *raw f64 bits*, in deterministic order — is identical before and
after all ten of those commits. Nothing about what the trainer computed changed across them.

## The two verification gates used throughout

1. **Byte-identical decision trace.** `single_game_trace` on seed 83000001 over four rounds
   produces a 726,492-byte trace; every inference-path change was required to leave it unchanged.
2. **Bit-level statistics digest.** Every gradient-path change was required to leave the digest
   unchanged over 141,855 accumulated f64 values.

Gate 2 earned its keep. Applying a shared helper to `Statistics::merge` moved the digest, which
exposed a real defect in a commit already made: replacing `entry(k).or_insert(0.0) += v` with
`insert(k, v)` is **not** an identity, because IEEE 754 gives `0.0 + -0.0 == +0.0`. A slot whose
first contribution was a negative zero would have stored `-0.0` where it used to store `+0.0`.
Nothing downstream could see it — the two compare equal, sum alike and serialise the same — and
no test could have caught it. Fixed in `20349a5`.

## What was done

| Commit | Change | Effect |
|---|---|---|
| `255b92c` | **mimalloc global allocator** | **1.89×** on the rollout path |
| `8944648` | Tokenise a choice's prompt once, not once per option | within noise |
| `038ef17` | Compute per-seat facts once per choice (`controlled_planets` scanned the board per option) | within noise |
| `47b92a7` | Stop storing the chosen option's features twice per decision | ~3.5% |
| `97000b7` | Point-lookup a planet instead of building the 110-entry catalogue | ~4.6% |
| `445a80d` | Don't lowercase an already-lowercase string | within noise |
| `ebde17f` | Stop cloning slot names in the gradient reduction | **~23% of reduce** |
| `20349a5` | Preserve negative zero; extend the helper to `Statistics::merge` | correctness |
| `b5fb5d0` | Build the softmax without a cloned copy of every option id | within noise |
| `0789519` | Find every homeworld system in one corpus pass | **`map_filler` 287 → 21 µs (13.6×)** |

Workspace: **1261 tests pass**; clippy at 21 warnings, all pre-existing.

## Negative results — measured, so nobody re-treads them

| Idea | Measurement | Verdict |
|---|---|---|
| `-C target-cpu=native` (Zen 5, AVX-512) | 22.71 → 22.48 ms (+1%) | **Skip.** Workload is not compute-bound. |
| Hash index over the weight table instead of `BTreeMap<String,f64>` | scoring is **12%** of policy; a hash index cuts policy by **6%** | **Skip.** Needs a per-head index and a serialisation-skip field for 6% of one slice. |
| Checkpoint I/O → bincode | load 190 ms, save 18 ms (JSON), 8 ms (bincode) | **Skip.** Negligible per run. Note the recorded "~38 s per run" in `STAGE2-SCHEDULING-WAVES.md` is not reproducible here. |
| Profile deep-copy → `Arc` | 28.7 ms per batch call, ~2.6% of an update | **Deferred.** Ripples through the public rollout API for 2.6%. |

## Keying features by hash — the one change that is not bit-identical

`FeatureVector` was `BTreeMap<String, f64>`: an allocation per key, a string comparison per tree
level, for ~22 features on each of ~5.7 options at every one of ~450 decisions per game. It is now
keyed by an FNV-1a hash of the name.

**Why a hash and not a counter.** Handing out 0, 1, 2… as names are first seen makes the order
depend on which seeds a run happened to play first, so two runs of one configuration drift apart
in the low bits for reasons nobody can reconstruct. A hash is a pure function of the name, so
iteration order is fixed by the corpus rather than by history.

**What it cost before it paid.** The first version took a read lock on the shared name table per
registration — 340,000 registrations per game across 32 workers sharing one reader count. That
made the rollout **22% slower** (8.228 → 10.073) even as the reduction got 4.3× faster. A
thread-local set of already-recorded keys removes the lock from the hot path and recovers it.

**Evidence, in place of bit-identity.** Summation order changed, so dot products differ in their
low bits and the digest gate no longer applies. Measured instead:

- One traced game (seed 83000001, 1,115 decisions): **zero** differing choices, zero differing
  prompts, zero differing option counts. The 91-byte trace delta is entirely printed score digits.
- 720 games on the 96M validation panel: per-faction mean VP and clearance **identical to four
  decimal places** for all six factions; table total 13.7125 both ways.

Checkpoints are unaffected — weights are stored by name, and always were. A `FeatureKey`
serialises as its name, never its number.

## Where the cost is now

Of the 12.415 ms full update: **reduce ~34%, feature construction ~48%, trajectory recording
~10%, engine ~9%.**

Feature construction is measured at **88% of policy cost** (8.29 µs/option against 1.13 µs for
scoring, at 22 features/option) — consistent with the project's own instrumented 5.5:1. There is
no redundant work left in it. The cost *is* the representation: `format!` (65 ns) plus a
`BTreeMap<String, f64>` insert (50 ns) per feature, ~22 features per option.

**Done, in `f7b2716`** — see the section above. What this section originally said, and why it was
wrong to treat it as a blocker:

> Interning the feature namespace is the only remaining large lever — and it cannot meet the
> verification standard used here. Feature names appear dynamically during training, so ids
> cannot be assigned in name order, so the gradient sums would accumulate in a different order
> and produce different floats.

The reasoning about *ids* was right; the conclusion was too quick. Hashing the name instead of
counting removes the dependence on history entirely, and the resulting order — while still not
alphabetical — is a fixed function of the corpus. So the loss is smaller than "identity is gone":
it is "identity against the *old* ordering is gone, and reproducibility between runs is kept".

The evidence gate changed accordingly, from a digest to distributional agreement, and the result
came back stronger than the standard required: zero differing decisions in a traced game, and
per-faction VP and clearance identical to four decimal places over 720 games.

## Feature naming, and where it stopped

After keying by hash, three further passes at the naming cost:

| Change | Effect |
|---|---|
| `add_named` takes `fmt::Arguments` into a reused buffer instead of `&format!` | policy 4.508 → 4.289 (4.9%) |
| Reduce pairs probabilities with vectors by iteration, not by id lookup | within noise |
| Hot families hash from their **pieces**, never building the string | policy 4.289 → 4.094 (4.5%) |

The last one rests on FNV-1a being a *streaming* hash: folding
`["prompt-option:", p, ":", o]` gives bit-for-bit the same key as hashing the joined string, so
the name is built only on the first sighting of a key. The decision trace is byte-identical
across it.

**And that is where naming stops paying.** A census over 3,953 real options (94,436 features,
23.9 per option) shows the five converted families are **92.6% of all features**:

| family | share | |
|---|---|---|
| `prompt-option` | 35.4% | converted |
| `state-kind` | 27.5% | converted |
| `prompt-kind` | 14.2% | converted |
| `option` | 11.4% | converted |
| `kind` | 4.2% | converted |
| `target` | 6.8% | still formats |

So removing `format!` from essentially every feature bought 4.5% of policy — not the ~60%
predicted by microbenchmarking `format!` in isolation. That prediction was wrong, and it is
recorded here so the next reader does not act on it again. Naming is no longer the bottleneck;
what remains is the map inserts, the hashing, tokenising, and the board queries in
`structured_features`.

## Per-option cost — where the money actually was

Two failed predictions (see the naming section and the inert-feature proposal) had one cause: a
model in which cost is proportional to *feature count*. Instrumenting the real training path
over 562,200 options replaced the model with a measurement:

| region of feature construction | share |
|---|---|
| **`structured_features`** | **67.3%** |
| prompt loops | 15.9% |
| tokens + token set | 5.9% |
| payload | 2.7% |

(Absolute values were inflated ~7× by six shared atomics across 32 workers; only the ratios are
usable, and they are decisive enough.) The cost is **per option**, not per feature — which is why
dropping 22.6% of features moved nothing.

Two fixes inside `structured_features`, both byte-identical on the trace:

| Change | Effect |
|---|---|
| Read a system's planets from its own record instead of scanning every planet for a matching `tileId` (2,327 ns per call, one to three calls per option) | 0.709 → ~0.660 s (~7%) |
| Count a system's units in one pass rather than four, each doing its own content lookup per unit | 0.660 → 0.642 s (2.8%) |

### `system_reachable` — examined, and not a lever

Flagged as the obvious next target because it walks the whole board per option. Two attempts,
both reverted:

- Hoisting `galaxy.distance` out of the per-unit loop **made it slower** (0.650 against 0.642):
  the distance was moved ahead of the unit check, so it then ran for every non-pinned system
  including the many holding none of the player's ships — which the original ordering
  short-circuited.
- A lazy version, taking the distance on the first own unit seen, measured flat (0.644 against
  0.642).

`galaxy.distance` is two coordinate lookups and hex arithmetic, not the graph search the shape of
the code suggests. Reverted both rather than bank a neutral change.

## Measurement note

Two samples cannot resolve a 3% effect on this machine; three tight ones can. Between sessions
there is also **~1.2% drift** — the same commit measured 0.642 s mean at one point and 0.650 s
mean an hour later — so an A/B is only trustworthy when both arms are run back to back. That puts
the practical resolution floor around 3%, and everything above it in the per-option path has now
been taken. An earlier reading
of 0.636 s was a lucky low sample against a true level of ~0.660, and one commit message
overstated its change as 10% before `cb28495` corrected it to ~7%. Every A/B from that point uses
three samples per arm and reports them individually.

## What is left

Of the 8.212 ms update: reduce ~17%, and the rest is rollout — feature construction still the
largest single block within it, now paying a hash per feature rather than an allocation.

Remaining known items, none large:

- **Profile deep-copy → `Arc`** (~2.6%): ripples through the public rollout API.
- **Compositional keys** — build a `FeatureKey` from `(family, token, token)` without ever
  formatting the name. `format!` was measured at 65 ns of the ~134 ns naming cost, and it is now
  the largest remaining component. This is a bigger change to `features.rs` than the keying was,
  because every feature family needs a tag.
- **Utilisation** (63% at `--rollout-depth 4`): re-measure now that the allocation pressure that
  made depth 8 regress is much lower.
