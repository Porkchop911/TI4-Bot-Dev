# Overnight plan: testing learning approaches on a free machine

Date 2026-08-17. Budget: one night (~10 h) on 32 cores, unattended.
Constraint: **straight learning — no heuristic teacher, no authored policy as a target.**

---

## What changed since the last arena plan

Three things, all measured, and together they make an overnight programme possible where the
earlier plan needed eight days:

1. **Training is 3.42× faster.** 1,000 updates: 35.2 min → **10.3 min**.
2. **Evaluation is now nearly free.** 720 games ≈ 5 s. The gate's statistical-power problem —
   SE 0.072 at 32 seeds against a 0.03–0.05 effect — is solved by throwing seeds at it: a
   **500-seed panel gives SE ≈ 0.018**, which resolves a 0.04 gain at 2σ. Panels stop being the
   bottleneck and become the cheap part.
3. **Concurrency beats sequence.** Measured effective throughput per update:

   | layout | s/update |
   |---|---|
   | 1 process × 32 threads | 0.618 |
   | 6 × 5 | 0.569 |
   | 4 × 8 | 0.542 |
   | **8 × 4** | **0.542** |

   8 arms at 4 threads each is ~12% more total throughput *and* runs eight arms at once. Each
   individual run is slower in wall-clock; the machine finishes more work.

**Budget: ~10 h at 0.542 s/update ≈ 66,000 updates in total across all arms.**

## What the arms are testing

The plateau has one *proved* cause and one *unproven* one, so the programme is a 2×2 rather than a
list of algorithms:

- **Representation.** `activation` is 94% blind — two candidate systems routinely carry identical
  feature vectors, so no weight vector can separate them (`FINDING_2026-08-17_ACTIVATION_IS_BLIND`).
  That is a hard ceiling of 0.681 on the head generating 44.5% of all options.
- **Optimiser / reward.** Unproven, but with one specific suspect: `--clearance-weight 5.0` lands
  in the final reward slot, so via the suffix sum every decision in the episode carries the full
  −5. At 87% clearance that is ~34% of all return variance, and it is constant within a game —
  it discriminates between games, never between decisions inside one.

| | O0 reference | O1 `--clearance-weight 0` |
|---|---|---|
| **R0** current features | **A1** baseline | **A4** |
| **R1** + target-system facts | **A2** | — |
| **R2** R1 + zeros recorded | **A3** | **A5** |

Plus **A6**: R0/O0 at `--learning-rate 0.06`, as a control that the reference is not simply
under-stepped — the cheapest possible check that "the optimiser is fine" is not being assumed.

Six arms × 3 training seeds = **18 runs**, 8 concurrent, ~2,500 updates each
(18 × 2,500 × 0.542 s ≈ 6.8 h), leaving headroom.

## What R1 and R2 actually change

Both stay behind the line `features.rs` draws in its own opening: **observations, never
judgements**. Nothing here says which system is better; each is something the board states.

**R1 — facts about the activation target**, so two systems stop being the same decision:

- distance from the player's nearest own ship, and from their home system
- distance to Mecatol Rex
- total resources and total influence of the system's planets
- wormhole kinds present; anomaly kinds present
- how many *other* players hold units there, and control planets there

**R2 — record a zero rather than dropping it**, for that fixed schema of facts. `add_parts`
returns early on `0.0`, so `target:own-ships = 0` is *absent*, and "no ships here" is
indistinguishable from "this fact does not apply". That collapse is half of why the ten features
in the worked example became three.

## Comparing arms: cross-play, not separate panels

Running arm A as all six seats and arm B as all six seats pairs the *maps and decks* but not the
**opponents** — A's score was earned against A's table. Putting them in the same game pairs the
opponents too, which is the strongest variance reduction available and measures the thing that
actually matters, relative strength.

The faction confound is fatal if ignored (the evolution anchor spans xxcha 4.57 to jolnar 2.08, so
whichever arm draws xxcha wins by default) and completely solved by rotation. This project is
unusually well set up for it: it already trains **six independent per-faction profiles** and
already rotates factions across seats, so every arm owns a profile for every faction. Rotate the
arm→faction assignment as well, and with six arms and six factions each arm plays each faction
exactly once per seed. The gate is already per-faction, so nothing else changes.

**Training stays separate.** Mixing arms at the table during *training* would break two things:
the baseline stops being a baseline once it is learning against five other algorithms, and in
self-play the opponents *are* what is being learned — a more sample-efficient arm pulls ahead
mid-run and thereby changes the environment the others are learning in. That measures an
interaction, not two algorithms.

**But mixed-table training is worth its own arm later.** The six factions currently converge to
within 0.24 VP of each other — six abilities, one indistinguishable policy — which is what you
would expect from every seat training against near-copies of itself. Opponent diversity as a
learning hypothesis is the cheap version of the league-play arm, and needs no frozen pool.

## Two independent success criteria

Keeping these apart matters, because one can move without the other and they mean different
things:

1. **Mechanism** — run `separability` on each representation arm. R1/R2 must drop
   `activation` blind% from **94%** and raise its ceiling from **0.681**. This is a direct test of
   the fix and needs no training at all; it can be checked within minutes of the features
   landing, before any arm is launched.
2. **Outcome** — mean VP per faction on a **500-seed held-out panel** (3,000 games, ~20 s), and
   table total VP against the current champion's 13.71.

A representation arm that fixes the mechanism but not the outcome is still informative: it says
the blindness was not what capped VP, and moves weight to the optimiser hypothesis.

## Schedule

| | |
|---|---|
| **before launch** (~2 h, mine not the machine's) | implement R1 and R2; run `separability` to confirm the mechanism moved; a smoke run of 20 updates per arm to prove every arm starts |
| **wave 1** (~3.4 h) | 8 runs: A1–A6 seed 0, A2/A3 seed 1 |
| **wave 2** (~3.4 h) | 8 runs: remaining seeds |
| **wave 3** (~0.8 h) | last 2 runs + all evaluation panels |
| **morning** | one report: per-arm VP, clearance, table total, blind%, and the per-seed spread |

All runs write to `out/arena/<arm>-s<seed>.json` and log to `out/arena/<arm>-s<seed>.log`, so a
crashed arm costs one run rather than the night.

## Decision rules, pre-registered

Written now so a marginal morning result cannot be argued into a conclusion.

- **The representation hypothesis is supported** if any of A2/A3/A5 beats A1 on table total VP by
  ≥0.3 with non-overlapping across-seed ranges, *and* its `activation` blind% fell.
- **It is refuted** if blind% fell to near zero and VP did not move by ≥0.15. That is the
  informative negative: the features were blind, fixing them was free, and the blindness was not
  the binding constraint.
- **The reward-variance suspicion is supported** if A4 beats A1 by ≥0.15, and separately if A5
  beats A3 by a similar margin.
- **The optimiser is under-stepped** if A6 beats A1 — in which case none of the above is
  interpretable and the reference configuration needs re-tuning first.
- **Nothing moves** — then neither hypothesis survives at this budget, and the next step is a
  different policy *class* (bilinear or a small MLP over the same features), not more of this.

Across-seed spread is reported for every arm. Three seeds is few; a difference smaller than the
spread is not a result, and will be labelled as such.

## Method note from the smoke run

The first launcher put `--clearance-weight 5.0` in the shared flag block and let arms override it.
They could not: the argument parser takes the **first** occurrence, so A4 and A5 silently trained
with 5.0 and would have produced two arms that were copies of A1 and A2 under different names. The
six-update smoke run caught it because A4's own banner still printed `-5.00 per uncleared opening`.
Anything an arm overrides is now passed per-arm and never in the shared block.

That is the whole reason for a smoke run: the failure was silent, produced plausible output files
of the right size, and would have cost the night.

## What is deliberately not in this

- **No teacher of any kind.** The behaviour-cloning route is dropped: the only policies stronger
  than the student are heuristic, and that is ruled out.
- **No PPO, no value baseline, no ExIt.** Each needs implementation time that would eat the night,
  and all of them are optimiser-side — worth doing *after* A1–A6 says whether the optimiser is
  where the problem is. Building them first would be guessing.
- **No `--rollout-depth` variation.** It is scheduling, not learning, and was measured at ~8%.
