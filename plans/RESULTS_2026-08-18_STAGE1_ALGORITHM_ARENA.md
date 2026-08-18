# Stage-1 algorithm arena: PPO is 3.3–4.0× more sample-efficient

Date 2026-08-18. 5 arms × 3 seeds × 3,000 updates = **4.3 million games**, 2h45m on 32 cores.
Trained from **blank weights** on the Python map pool; evaluated on a fixed 200-seed panel
(200 boards x 6 rotations = 1,200 games per faction), identical for every arm and every seed.

**What the panel does and does not hold out.** A seed fixes three things at once: the map
(`pool.draw(seed + 20,000,000)`, one of 8,192 arrangements), the deck shuffles, and each seat's
sampling stream. The six rotations of a seed share the map *and* the decks, so the panel is 200
distinct boards played six ways, not 1,200 independent situations.

The evaluation *seeds* are disjoint from training. The evaluation **maps are not**: each training
run consumes 48,000 seeds and therefore sweeps all 8,192 arrangements about 5.86 times, so **all
200 evaluation boards were seen in training**, roughly six times each, paired with different decks
and sampling. This is a test of generalisation across shuffles and sampling, **not across boards**.
Nothing here should be read as a clean generalisation result. A genuinely map-held-out panel needs
the pool partitioned, which is a protocol change rather than a re-analysis.

For the same reason the across-seed ranges below measure **training stochasticity, not map
variability**: all three seeds train on the identical set of 8,192 maps and differ only in the
order they arrive and in which deck and sampling stream each is paired with. That is the right
control for comparing arms, and it is narrower than "run-to-run variation" sounds.

---

## The headline

| arm | algorithm | final clearance (3 seeds) | range | shortfall |
|---|---|---|---|---|
| S0 | REINFORCE lr 0.03 | 0.7329 · 0.7258 · 0.7153 | [0.7153, 0.7329] | 0.547 |
| **S1** | **PPO K=4, clip 0.2** | **0.8681 · 0.8972 · 0.8364** | **[0.8364, 0.8972]** | **0.269** |
| S2 | PPO K=2, clip 0.2 | 0.8019 · 0.8104 · 0.7733 | [0.7733, 0.8104] | 0.403 |
| S3 | REINFORCE lr 0.12 | 0.8186 · 0.7885 · 0.7775 | [0.7775, 0.8186] | 0.414 |
| S4 | REINFORCE γ=0.97 | 0.7418 · 0.6987 · 0.7260 | [0.6987, 0.7418] | 0.526 |

**S1's across-seed range is disjoint from S0's and from S3's.** Both comparisons clear the
pre-registered bar, and the shortfall metric agrees with clearance on every one of them.

## Sample efficiency — games to reach a clearance threshold

| threshold | S0 | **S1 PPO K=4** | S2 PPO K=2 | S3 lr 0.12 | S4 γ=0.97 | S0/S1 |
|---|---|---|---|---|---|---|
| 0.20 | 48,000 | **14,400** | 24,000 | 14,400 | 48,000 | 3.3× |
| 0.40 | 91,200 | **24,000** | 48,000 | 28,800 | 91,200 | 3.8× |
| 0.50 | 134,400 | **33,600** | 72,000 | 43,200 | 144,000 | 4.0× |
| 0.60 | 182,400 | **48,000** | 100,800 | 62,400 | 182,400 | 3.8× |
| 0.70 | 264,000 | **72,000** | 139,200 | 81,600 | 273,600 | 3.7× |
| 0.75 | — | **86,400** | 182,400 | 115,200 | — | — |
| 0.80 | — | **105,600** | — | — | — | — |
| 0.85 | — | **196,800** | — | — | — | — |

PPO K=4 is the **only arm to exceed 0.75**, and it reaches 0.85 while three of the four others
never reach 0.75 at all within 288,000 games.

## The confound was tested, and PPO survived it

The clip fraction is ~0.002 — the trust region essentially never binds — so K=4 is four nearly
unconstrained steps on one batch, which is difficult to distinguish from one step at four times the
rate. **S3 exists to test exactly that**, and the arms are ordered the way an effective-step-rate
story predicts: S1 (4×) > S3 (4×) > S2 (2×) > S0 (1×).

But the story is not sufficient. S1 separates from S3 with disjoint ranges, and the mechanism is
visible in the late curve — **clearance gained over the final 57,600 games**:

| | S0 | S1 | S2 | S3 | S4 |
|---|---|---|---|---|---|
| late gain | +0.0525 | +0.0115 | +0.0310 | **+0.0062** | +0.0524 |
| final | 0.7219 | 0.8675 | 0.7931 | 0.7966 | 0.7200 |

**S3 has stalled** at 0.797 while still moving in large steps. A raw learning-rate increase buys the
same early speed and then stops paying; the ratio-weighted update keeps going. Worth stating
plainly: this is *not* the clip doing it, because the clip never binds. Something else about
weighting each decision by its importance ratio is responsible, and this arena does not identify
what.

## Where PPO does not win: wall clock at modest targets

PPO costs 3.301 s/update against REINFORCE's 2.194 (1.5×), so sample efficiency and wall-clock
efficiency are different questions:

| threshold | S0 | S1 PPO K=4 | S3 lr 0.12 |
|---|---|---|---|
| 0.40 | 35 min | 14 min | **12 min** |
| 0.60 | 69 min | 28 min | **25 min** |
| 0.70 | 101 min | 41 min | **33 min** |
| 0.80 | — | **61 min** | — |

**If a moderate clearance is all that is wanted, turning the learning rate up is the cheaper
route.** PPO's wall-clock advantage appears only above 0.75, where it is the sole arm that arrives
at all. Reporting the sample-efficiency table alone would overstate the case.

## Discounting: a confirmed no-op here, and harmful at Stage 2

S4 (γ=0.97) is indistinguishable from S0 — 0.7200 against 0.7219, ranges nearly coincident. At
Stage 2 the same change cost **−0.680 table VP** with disjoint ranges. A one-round horizon leaves
little to discount, which is the expected direction; the Stage-2 result is the one that matters.

## Why this contradicts the Stage-2 PPO pilot, and which to believe

The same arm, tested at Stage 2, was a flat null: PPO K=4 at −0.020 table VP, the whole 12-run
field spanning 0.375. Both results are sound; they measure different regimes.

* **Stage 2 resumed from a converged champion.** Near a local optimum, more or larger steps buy
  nothing — and the thing that *did* pay there was fixing the advantage itself (the round baseline,
  +1.294 VP with disjoint ranges).
* **Stage 1 trained from blank.** Far from any optimum, with abundant signal, taking more effective
  steps per game is worth 3–4× the data.

The reconciled reading: **optimiser choice matters most where the policy is far from converged.**
That makes PPO a tool for *reaching* a good policy quickly, not for improving one that has already
arrived — which is a claim about when to spend the 1.5× per-update cost, not about whether PPO is
"better".

## Caveats, stated rather than buried

1. **This measures efficiency, not the asymptote.** S0 was still climbing fastest at the end
   (+0.0525 over the final 57,600 games, against S1's +0.0115). Given enough games it may well
   close the gap. Every claim here is about the path, not the destination.
2. **Three seeds.** Enough for disjoint ranges, thin for anything subtler. An earlier reading at
   update 300 had S1 and S3 overlapping, and the separation only appeared later — early-curve
   rankings are not stable, and this one was checked at the end rather than called early.
3. **The clip never binds**, so nothing here validates the trust region. A PPO variant without the
   clip was not run and would be the natural next control.
4. **`--round-baseline` is untestable at Stage 1** (one round, one bucket), so the largest measured
   effect in the whole programme could not be included as an arm here.

## What this changes

* **PPO is worth using for Stage-1 training and for any run starting from blank or near-blank
  weights.** 3.3–4.0× on the resource the programme is short of.
* **It is not worth using to polish a converged Stage-2 champion**, where it is a measured null and
  costs 1.5× per update.
* The next control worth running is **PPO K=4 with the clip removed**, which would say whether the
  importance ratio or the clip is doing the work — currently unresolved, and the clip fraction says
  it is not the clip.
