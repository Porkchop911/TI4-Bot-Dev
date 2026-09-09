# Two of your ranked items, implemented and measured — 2026-09-09

For Astra. Your ranks 2 and 4 from `plans/INFERENCE_OPTIMIZATION_2026-09-08.md` are now in `main`'s
branch. Both preserve behaviour, verified. **Neither has a defensible speedup number yet**, and the
reason is a measurement mistake worth reading before you benchmark anything in this tree.

Commits: `6b72017` (rank 2), `05a5fc7` (rank 4), `0cd1bf7` (rank 1, partial).

---

## 1. The measurement mistake — read this first

I reported three timing numbers today and **all three were wrong**. The cause was the same each
time and it is specific to this repository.

**The working tree carries ~32 files of other sessions' uncommitted work**, including
`features.rs`, `critic.rs`, `progress.rs`, `state.rs` and `choice.rs` — files that change the
observation and the engine. A binary built from the working tree therefore contains their changes as
well as mine. Benchmarking it against anything measures the difference between *two different
programs*, only one of whose differences is the patch under test.

Concretely, on a fixed bundle and seed base, update 0 produces:

| build | decisions at update 0 |
|---|---:|
| clean checkout of `d400d53` | **153,918** |
| working tree (`0cd1bf7` + 32 dirty files) | **154,885** |

Same commit lineage, ~1,000 decisions apart. That gap is other sessions' work. I attributed it first
to my patch helping (−2.35%), then to my patch regressing (+14%). Both were noise from a confound I
had already been caught by once today — the v34 behavioural bounds, derived in a dirty tree and
committed wrong before a clean worktree caught them.

**The rollout itself is perfectly deterministic**: the same binary produced 153,918 three times and
another produced 154,885 three times, exactly. Determinism was never the problem. The problem was
comparing two builds that were not the same program.

### The harness that works

Two `git worktree` checkouts at the two commits, each with its own `CARGO_TARGET_DIR`, each clean;
alternate the order; compare **update 0 only**, because it is the last update before a CUDA gradient
step makes later updates irreproducible across processes.

```powershell
git worktree add --detach <dir-before> <commit-before>
git worktree add --detach <dir-after>  <commit-after>
# build each with its own CARGO_TARGET_DIR and LIBTORCH=<the cu128 tree>
# then, alternating B/A, A/B, B/A…:
ppo_update.exe --bundle out\checkpoints\stage2-mlp-shaped-resumed\checkpoint-241428 `
  --stage 2 --rounds 4 --temperature 2.5 --movement-entropy 0.05 --entropy-final 1 `
  --learning-rate 3e-4 --waste-penalty 5 `
  --fleet-weight 0.03 --tech-weight 0.1 --strategy-diversity-weight 1.0 `
  --seed-base 2400084800 --updates 1 --report-every 1000 --device cuda --out <scratch>
```

Identical decision counts on both sides is the equivalence check *and* the proof the workloads are
comparable. If they differ, the timing means nothing.

**Do not use a separate `CARGO_TARGET_DIR` to work around DLL staging without knowing why**: each
libtorch distribution stages its own DLLs beside the binary, and one shared `target/` lets the last
build win. That is how an already-built trainer silently lost CUDA today.

---

## 2. Rank 2 — vocabulary lookup (`6b72017`)

**Implemented as you specified.** `Vocabulary::resolve_key(key) -> (usize, bool)` returns column and
assigned-membership from one `BTreeMap` probe; `MlpBot` memoises `FeatureKey -> (column, assigned)`.

`sparse_from` had been calling `is_assigned_key` then `column_of_key`, walking the same ~15,000-entry
ordered index twice when the first hit already carried the column.

The memo is safe by construction, not convention: `MlpBot` owns its `Vocabulary` by value, nothing
hands out `&mut`, and the MLP path never calls `append` — a published generation is immutable and a
new one produces a new bundle and a new bot. Counters increment per *occurrence*, not per distinct
key, so `assigned`/`oov` read exactly as before.

**Equivalence: verified.** A new test drives all three routes through `column_of` — assigned names,
unassigned names in registered families that must land on the family OOV column, and names in no
registered family, the fallback being where such an equivalence would actually break. A greedy
21,600-seat-game eval reproduced all six factions' clearance to the digit.

**Speed: unmeasured.** My −3.48% came from two 100-update runs that drew different games. It has not
been re-run under the clean harness. Your screened A/B (7.13% median diagnostic rollout reduction)
remains the best evidence that exists for this item; my number should not be cited.

---

## 3. Rank 4 — freeze canonicalisation shortcut (`05a5fc7`)

`Batch::freeze` returns early when the sparse columns already strictly increase: no duplicates to
fold, nothing to reorder, and the loop would rebuild what it started from. An identity case, not a
tolerance.

**Equivalence: verified bit-for-bit.** The test keeps the original body verbatim as a reference and
compares `f32` bit patterns, covering the shortcut's own path and everything it must decline — out
of order, a duplicate, both at once, and equal columns whose fold order is the tie-break.

**Speed: unmeasured**, for the same reason. Your 94.18%-of-options figure remains the argument for
why it should matter.

---

## 4. Rank 1 — decision-local action facts (`0cd1bf7`) — implemented, and it does nothing

You named `action_facts`' per-option construction as the concrete next target. It builds three
things per option that depend only on the decision: the seat's controlled planets, the derived
held-systems set, and `galaxy::all_planets`, which materialises the whole planet catalogue and whose
own doc comment records that "building a hundred-entry map to answer a single question was a third
of a simulated game's running time".

They now live in an `ActionContext` built once per decision.

**Equivalence: verified.** Both clean checkouts produce **153,918 decisions at update 0, on all five
alternating pairs**.

**Speed: no measurable change.** Rollout medians 10.00s vs 10.10s, spread ±0.15s — inside
resolution.

**Why, and this is the useful part.** The hoist moved `all_planets` *out of the activation arm* and
*into every decision*. Activation is ~1,508 calls against ~62,000 decisions in your own head table,
so ~97% of decisions now build a catalogue they never use. The saving on activation decisions is
roughly cancelled by the cost imposed on everything else.

The obvious repair is to make `planets` lazy — a `OnceCell` populated on first use inside the
activation arm — so non-activation decisions pay nothing and a twenty-option activation decision
still builds it once. **Not yet done.** If that also measures flat, the honest conclusion is that
this item does not carry its weight and the commit should be reverted; your own estimate put
action-facts at only ~13.6% of actor feature cost, which is ~1.5% of wall before any of it is
recovered.

---

## 5. What this suggests about the remaining ranks

Your estimates were derived from stage timings in a diagnostic profiler. Ranks 2 and 4 both look
right in principle and neither has yet produced a measurable wall-clock change under a controlled
comparison. That is not evidence they are worthless — it is evidence that **stage-share arithmetic
over-predicts, because removed work is partly replaced by the cost of avoiding it**, as rank 1 shows
literally.

Before rank 3 or 5, it would be worth establishing what the harness in §1 can actually resolve. Five
pairs give ±0.15s on a 10s rollout, so roughly **1.5%**. Anything you expect to land below that needs
more pairs or a longer measured window, and a 1–2% scenario estimate cannot be validated at all with
five pairs of one update.

Ranks 3 and 5 remain untouched; rank 7 stays disqualified on the bitwise logit divergence you
measured.

---

## 6. Repository state

`main` is at `c73cadf` and green. The branch carries the three commits above plus the recovery work.

Your two reports are **now committed** (`d400d53`) — they had been untracked, and since `out/` was
destroyed on 2026-09-08 they are the only surviving record of the measurements behind them. Your
`engine_cost` and `inference_cost` diagnostic examples are committed with them. The frozen
executables and raw logs are gone.

Also restored and verified: all three map pools regenerate bit-for-bit from
`plans/evidence/MLP-ARTIFACTS.md`, the vocabulary generation came back from a committed checkpoint's
`slots.json`, and libtorch was re-downloaded — the CPU manifest re-pinned, a `cu128-20260908`
distribution added beside it with its own manifest.

Two `git worktree` checkouts remain on disk under `%TEMP%` (`ti4-ab-before`, `ti4-ab-after`) with
their target directories. **They have been left deliberately** — `git worktree remove --force` is
prohibited in this repository after it followed a junction and destroyed `out/`. Remove them by hand
if you want the disk back.

Training is paused at effectively update 1,800, resumable from
`out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428`. Best checkpoints: u50 at 93.15%
clearance / 0.33% any-waste, u1700 at 92.12% / 0.10%.

The open training question is unchanged and is not a performance one: **Hacan at 83.53% clearance
with 0.00% waste** is the binding constraint on the 95% target — it is not wasting, it is failing to
cross the bar. See `plans/TRAINING_APPROACH_2026-09-09.md` §8.
