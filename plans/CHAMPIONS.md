# The best policies, and how to get back to them

Every checkpoint below is copied into `out/champions/`, which nothing regenerates. The sweep
directories they came from (`out/checkpoints/waste-*`, `out/checkpoints/mixed`) are **deleted and
rewritten** by their scripts, so the originals are not durable and the copies are the record.

`out/` is gitignored, so this file is the only version-controlled trace. If the copies are lost, the
provenance below is enough to regenerate them.

## Measurement convention

Everything here is **greedy clearance** (`--temperature 0.001`) on the **Validation** pool, seeds
900000000 upward, via `clearance_eval`. Numbers taken at 600 seeds = 21,600 seat-games, ±0.32 on the
table and ±0.6–1.0 per faction.

Two other conventions were used earlier in the session and are *not* comparable to these: the
in-training clearance table (sampled at the training temperature, on the Train pool) and 200-seed
evaluations on a different, easier seed range. Where a figure looks 0.5 too high, that is usually why.

## The policies

| directory | table | what it is best at |
|---|---|---|
| `out/champions/table-best-93.88_mixed-epoch14` | **93.88%** | the best table average; the policy everything else was trained from |
| `out/champions/xxcha-best-99.22_waste-p8` | 93.23% | kept for its **waste rate**: 1.46% of seat-games, against 56.93% for the champion. Its directory name claims an Xxcha result that the variance study does not support |
| `out/champions/jolnar-best-95.86_waste-p0` | 93.65% | the zero-penalty control. Its name likewise claims a per-faction result that is inside the noise floor |

### Per faction: removed

There was a per-faction table here. It is gone, because a three-replicate variance study of one
fixed arm measured a per-faction spread of up to **5.39 points** between runs that differ only in
their rollout seed base. Every cell of that table was one sample, so it recorded noise in a shape
that looked like a property of each policy — and it was read that way, including by me.

The per-faction figures live in git history if they are ever needed with that caveat attached. The
table averages below are the numbers worth comparing, and even those carry a 1.54-point training
floor: see `plans/STAGE2_TRANSFERABLE_LESSONS.md`.

## Provenance

| | |
|---|---|
| `mixed-epoch14` | `corpus_train`, from `cloned2/epoch-15`, replay bundle `sweep-A-250/checkpoint-14476`, ordinary + rescued corpora at 50/50, 20 epochs, lr 1e-5, balanced across factions |
| `waste-p0` | `ppo_update` 500 updates from `mixed/epoch-14`, T=2.5, movement-entropy 0.05, lr 3e-4, **waste penalty 0** (the control) |
| `waste-p8` | as above with **waste penalty 8**, charged as a reward at the close of each wasted tactical segment |

The lineage back from `mixed/epoch-14`: `sweep-A-250/checkpoint-14476` (PPO, the long-standing
champion at 93.58%) → `cloned2/epoch-15` (behaviour cloning on the ordinary corpus, +0.45 by paired
map-cluster bootstrap) → `mixed/epoch-14` (cloning on ordinary + rescued).

## What the waste penalty did

Greedy, same 21,600 seat-games:

| arm | clearance | tactical/seat | any-waste | waste/tactical |
|---|---|---|---|---|
| mixed-epoch14 | 93.88% | 3.510 | 56.93% | 0.277 |
| waste-p0 | 93.65% | 2.485 | 35.13% | 0.182 |
| waste-p3 | 92.72% | 1.789 | 1.98% | 0.017 |
| waste-p8 | 93.23% | 1.873 | **1.46%** | **0.011** |

Waste falls 39-fold and the table average falls with it, because the policy satisfies the penalty by
taking fewer tactical actions (3.510 → 1.873) rather than by activating better. But the per-faction
split shows that is an average of a large gain and several losses, not a uniform cost.

## Corpora, which are also not regenerated for free

| | |
|---|---|
| `out/corpus/positive` | 103,665 clearing trajectories, Train pool, temperatures 0.25/0.5/0.75, ~7.5 min to rebuild |
| `out/corpus/rescued` | 3,620 clearing lines from starts the champion fails, ~20 min to rebuild |

Both store trajectory *specifications*, not features, and each records the temperature it was
generated at — without which a replay faces different opponents and the line does not exist.
