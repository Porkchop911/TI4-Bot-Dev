# Results: algorithm-independent efficiency work

Date 2026-08-17. Branch `codex/stage1-parity-fixes`, commits `46404a0..HEAD`.
Companion to `plans/PLAN_2026-08-17_ENGINE_EFFICIENCY.md`, which this supersedes on numbers.

---

## Headline

**2.53× on a full training update.**

| | baseline `46404a0` | after `f7b2716` | |
|---|---|---|---|
| play (rollout) | 15.056 ms/game | **6.791** | 2.22× |
| reduce (gradient statistics + merge) | 5.751 ms/game | **1.421** | 4.05× |
| **full update** | **20.807 ms/game** | **8.212** | **2.53×** |

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
after all eleven commits. Nothing about what the trainer computes has changed.

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

Workspace: **1255 tests pass**; clippy at 21 warnings, all pre-existing.

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

**Done, in `f7b2716`.** The original text of this section read:

> Interning the feature namespace is the only remaining large lever — and it cannot meet the
> verification standard used here. `BTreeMap<String, _>` iterates in lexicographic name order;
a `Vec` indexed by interned id iterates in id-assignment order. Feature names appear dynamically
during training, so ids cannot be assigned in name order, so the gradient sums would accumulate
in a **different order** and produce different floats. Mathematically equivalent, bit-wise not.

That is a genuine trade-off for the operator, not a technical obstacle:

- Keep bit-exactness → the remaining ~1.5–2× stays on the table.
- Accept "statistically equivalent but not bit-identical" → interning is available, and the
  verification argument has to change from *identity* to *distributional agreement over N seeds*,
  which is a much weaker and more expensive thing to establish.

Everything committed so far sits on the identity side of that line, which is why each commit
could be verified rather than argued.
