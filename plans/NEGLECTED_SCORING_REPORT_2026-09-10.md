# The neglected-scoring problem was an observation bug — 2026-09-10

For Astra. `NEGLECTED_SCORING_PLAN_2026-09-09.md` treats low conversion on Explore Deep Space,
Make History and Improve Infrastructure as a training problem and proposes a curriculum. It is not
a training problem. **33 of the corpus's 40 public objectives could never have any option linked to
them**, and fixing that moved four cards before a single gradient step.

Two things in here matter more than the fix, and both are corrections you should read before
trusting anything measured in this tree:

1. **`git_commit` in checkpoint manifests is not build provenance.** `arm-control/checkpoint-42728`
   records `bd5486840c1d`. That binary was built from that commit *plus* ~630 uncommitted lines in
   `reward.rs` and `ppo_update.rs`, because `GIT_COMMIT` is an exported environment variable, not a
   derived hash. Every arm from 2026-09-09 has this property. The committed tree at `bd54868` does
   not even have `--fleet-weight`, `--tech-weight` or `--strategy-diversity-weight`; those flags
   exist only in the working tree. A binary built from a clean checkout silently ignores them —
   the arg parser drops unknown flags without complaint — and the only tell is a missing `shaping`
   line in the run header. I lost a run to exactly that before spotting it.
2. **Per-card conversion rate is the wrong target metric**, and the plan's priority table is built
   on it. Section 4 below is the measurement.

---

## 1. What was broken

The option-side feature that answers "does this action advance a revealed objective?" is a
difference of `Observed::revealed_objective_progress_gaining`, whose contract was explicit:

> Only planet control is imagined. Requirements about units, technologies or map shape read the
> real state on both sides and cancel out of the difference.

So every objective counted in units, structures, technologies or spending cancelled to zero on
both sides, and no option ever carried a gain toward it. Worse, on the caller side in
`features.rs` the guard was `if !uncontrolled.is_empty()` — an empty system offers no planets, so
activating one skipped the objective block outright.

That is the whole of Explore Deep Space's **0-for-660** across two independent measurements. The
requirement was visible the entire time (`objective-need:planetless_systems:3` is a live,
non-OOV column in `checkpoint-241428`'s vocabulary); the qualifying action was not; nothing
joined them.

`plans/STAGE1A_AUDIT_2026-09-09.md` had already ruled out the alternatives — the predicate is
correct, the map has 2.19 planetless systems within two hexes of an average home, movement has no
restriction on ending in one, and only 0.3% of seats have none within three hexes. Reachability
was never the problem.

## 2. The audit

`crates/ti4-mlp/examples/objective_signal_audit.rs` (isolated worktree) reveals all 40 public
objectives at once and hands the seat every planet it does not control — a maximal counterfactual
no real option can exceed — then reports which cards' progress moves.

| | before | after |
|---|---:|---:|
| Linked — an option can carry a gain | **7** | **27** |
| Invisible to every option | 33 | 10 |
| Already at the bar (no headroom) | — | 3 |
| No progress record at all | 0 | 0 |

The 7 that worked were all pure planet-counting families. Everything else was dark.

Two of the remaining 10 are correct (capital-ship cards: imagined presence is a generic ship, not
a flagship). Two more are correct (trade-good costs: planets do not yield trade goods). The real
remainder is `cost_all_three` (2 cards, needs the plan re-run rather than an offset) and
`distinct_rival_home_reaches` (1, wired but unverified in fixture).

## 3. The fix

Engine — `Position` gained three imagination axes beside planets (systems, technologies,
structures) behind a new `Imagined` struct; `revealed_objective_progress_imagining` takes it, with
the old planet-only entry point kept as a wrapper. Accessors (`systems_holding_units`,
`systems_with_ships`, `technology_types`, `structures`) union the imagined state in, so every
predicate reading through them picks it up. Three predicates that bypassed the scoring view were
rewired: `planetless_systems_count`, `attached_planets_count`,
`distinct_rival_home_reaches_count`. Spending got `bought_progress_at`, adding imagined planets'
resources and influence to affordability.

Policy — activation imagines presence in the target system; structure placement (whose option id
is `{unit}|{system}|{planet}` and which sets no `system` payload, so it reached *none* of the
system facts) imagines the structure at the chosen planet.

Existing feature names are reused throughout, so `checkpoint-241428` benefits with no new
vocabulary columns and no retraining required to see the change.

**Tests: 1,244 engine + 239 policy, all passing**, including two new ones pinning that activating
an empty system shows a gain toward a revealed Explore Deep Space *and* shows none when the
revealed objective is planet-counting — presence must not manufacture signal.

One bug I introduced and the suite caught: my first pass read planet locations from the corpus.
Two tests failed and were right — they place corpus planets into fixture systems via
`set_control`, so a planet's system must come from the board. Reading the corpus would have
silently relocated every planet to its canonical system.

## 4. The result, and the part that matters

All numbers: `checkpoint-241428` as candidate against five frozen `checkpoint-473312`, temperature
0.001, four rounds, 300 held-out seeds × 6 rotations = 1,800 candidate-games. Seeds drawn from a
scan range disjoint from training's, verified 0 overlap.

**The fix alone, with zero training:**

| card | before fix | after fix |
|---|---:|---:|
| Make History | 13.3% | **28.7%** |
| Improve Infrastructure | 14.1% | **28.8%** |
| Populate the Outer Rim | 26.5% | **34.8%** |
| Intimidate Council | 22.0% | 25.5% |
| Explore Deep Space | 0.0% (0/480) | 0.6% (3/480) |
| mean VP | 3.8572 | 3.9506 (+0.093, ~1.1σ) |

**Then 1,200 updates on the corrected surface** (full 2026-09-09 recipe, non-economy openers,
evaluated at `checkpoint-166236`):

| card | after fix | +1,200 updates |
|---|---:|---:|
| Improve Infrastructure | 28.8% | **58.4%** |
| Expand Borders | 48.7% | 64.8% |
| Make History | 28.7% | 37.1% |
| Build Defenses | 65.1% | 73.3% |
| Explore Deep Space | 0.6% | 3.8% |
| Push Boundaries | 58.9% | **51.7%** |
| **mean VP** | **3.9506** | **3.9272** |

Conversions rose by up to 30 points. **Total VP did not move** (−0.023 against se 0.0816).

The explanation, measured rather than assumed: **total objectives scored per game went 1.896 →
1.962.** Conversion rates jumped enormously and the number of things actually scored barely
changed. That is source substitution, near-exactly.

**Scoring is slot-limited, not eligibility-limited.** Roughly one public per status phase, ~1.6 per
game against a nominal ceiling of 4. Satisfying three cards at a window instead of one still
scores one point. Raising per-card conversion mostly reshuffles which card fills a slot that was
already being filled.

`VP_SOURCES_2026-09-09.md` said this in passing — *"The main gap is reaching the scoring window
with an eligible unscored card, not the final button press"* — but its priority table ranks by
conversion rate, and the plan inherits that ranking. By the plan's own acceptance rule (*"A rise
in the target route accompanied by lower total VP fails acceptance"*), this run fails at 1,200
updates. It is flat, not harmful.

**Where the headroom actually is:** round-by-round public selections in the census were
35 / 266 / 412 / 423. Round one is nearly empty. The lever is satisfying one card *earlier*, so
more of the four windows have something eligible — not satisfying more cards at once.

## 5. What is not done

- **Research options are not wired.** `develop` and `diversify` show linked in the audit but no
  option supplies a technology. Wiring it needs a new feature prefix; the six that exist are all
  system-scoped. A new name is out-of-vocabulary on every current bundle and would be inert until
  a new vocabulary generation, so I left it rather than fake it.
- `cost_all_three` (Amass Wealth, Hold Vast Reserves) still needs the plan re-run.
- `raise_fleet` barely moved (2.1% → 5.8%). Presence is one generic ship and cannot carry a
  five-ship card. Expected from the chosen semantics.
- The +0.093 pre-training VP gain is ~1.1σ. Not a result.
- This changes play, so fixture and ordered-choice hashes will move. Deliberate, not incidental.

## 6. Provenance

Everything is in an isolated worktree, `D:\Projects\ti4-engine-rs-neglected-scoring-audit`, built
from a snapshot of the shared tree's uncommitted state (`git diff`, sha256
`63ec03e05e9052c31b8cf9f843514c2f1773d3bd570401d89e02672aa96f41a3`, plus 15 untracked sources).
That snapshot is sound: every training-path file was last modified 09-07/09-08 while the arms ran
09-09 13:45–15:13, so the tree had not drifted. Before the feature change, the rebuilt trainer
reproduced `arm-control`'s update 0 at **exactly 138,162 decisions**, which is what establishes the
reconstruction was faithful. After the change it no longer does, and should not — different
features, different play.

The shared tree was never modified. This file is the only thing written to it.

Training is stopped at update 1201; the last checkpoint is `checkpoint-181056`. Resuming
cold-starts Adam, per the known schema-7 limitation.
