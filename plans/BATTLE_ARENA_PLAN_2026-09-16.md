# Battle arena and isolated-mechanics learning plan

> Start with the latest [agent handoff](BATTLE_ARENA_AGENT_HANDOFF_2026-09-16.md).
> User clarified isolated pitched battles: varied fleets, upgrades and factions,
> optionally action cards. Earlier ordinary-fleet scope below is a smoke stage;
> main-model movement integration is downstream, not the arena itself.

Status: proposed implementation plan; no arena dataset or trained model exists yet.
Concrete input, runtime and main-model integration design:
[`BATTLE_ARENA_DESIGN_2026-09-16.md`](BATTLE_ARENA_DESIGN_2026-09-16.md).
Requested by the operator on 2026-09-16. Full games, captured source games and
evaluation stay capped at **four game rounds**. Maximizing VP by that horizon is
the objective. Combat dice rounds inside one battle are a different engine concept;
retain the engine's existing bounded combat resolution and report failures.

## Decision and hypothesis

Build a small engine-backed combat-outcome learning pilot. Advance to policy
integration only if it learns useful held-out information cheaply. Stop GPU rollout
tuning; use parallel CPU simulation and batched GPU model training where appropriate.

The hypothesis is that concentrated mechanics examples teach useful consequences
with fewer full games. This does not imply faster individual PPO updates. Count
generation, pretraining, added inference cost and evaluation in the total cost.

Keep three questions separate:

1. **Prediction:** given two fleets and declared combat policies, what happens?
2. **Combat decisions:** which casualty, sustain or retreat choice helps this seat?
3. **Strategy:** should it attack, reinforce, build something else, or score instead?

Start with question 1. Do not equate winning a battle with gaining VP, or assume
that training the combat decision head improves activation and production.

## Existing implementation to reuse

- `crates/ti4-engine/src/combat.rs`: `CombatWindow`, `combat::resolve`, typed
  outcomes, legal decisions, damage assignment and actual dice resolution.
- `crates/ti4-engine/src/fixtures.rs`: shared board/unit builders for smoke cases.
- `crates/ti4-policy/src/bot.rs`: the retained-combat-window test demonstrates
  validated policy decisions and resumption after scoring pauses.
- `crates/ti4-policy/src/features.rs` and `projection.rs`: the actual actor inputs.
  Unit-option attributes and local combat previews already exist; inspect the
  complete projected vector before claiming that fleet information is missing.
- `crates/ti4-policy/src/vocabulary.rs` and `crates/ti4-mlp/src/lib.rs`: controlled
  input admission and migration; preserve existing checkpoint behavior.
- `crates/ti4-mlp/examples/ppo_update.rs`: downstream experiment and time accounting.

The synchronous combat resolver has no outer Game timing/scoring context. It is
appropriate only for the declared simple subset. Timing-dependent mechanics and
captured mid-combat states require the retained window/game path and its context.
A `GameState` clone alone must not be assumed to reconstruct a suspended game.

## Phase 0 — Verify that the task is learnable and relevant

**Deliverables:** an input audit, a scenario schema, a small frequency report and
fixtures proving important states are distinguishable at the model boundary.

- Inspect a bounded panel of four-round training games. Count battles, battle
  decisions, attack opportunities and invasions by round, fleet size and faction.
  Include opportunities declined by the current policy where capture supports it;
  only collecting actual fights would inherit its avoidance bias.
- Create paired scenarios that hold ship count fixed while changing composition,
  damaged status, upgrades or relevant public modifiers. Compare the complete
  projected actor inputs, vocabulary indices and candidate fleet descriptions.
- Verify which activation/movement/production heads receive the necessary facts.
  If different matchups collapse to identical inputs, implement and test a bounded
  actor-relative fleet representation before training. Do not add redundant inputs
  if the audit establishes that existing ones are sufficient.
- Inputs describe the candidate committed fleet and defending fleet, not every
  friendly ship on the map. Reachable ships are not automatically all committed.
  Encode side, unit kind/count, damaged count, relevant upgrades/public modifiers,
  and an explicit scenario-support flag. Normalize player/system identities away.
- Opposing private cards, future dice and simulator-only state must not leak into
  actor inputs. Initially exclude hidden-effect cases. Later targets must average
  over a declared hidden-state distribution consistent with the observation.

**Gate:** schema distinguishes the required examples; legal/factual information
boundary holds; ordinary-fleet scenarios cover enough observed play to justify a
pilot. Report observed coverage rather than guessing it in advance.

## Phase 1 — Generate trustworthy battle labels

**Initial scope:** ordinary two-sided space combat; legal fleet/capacity setups;
base ship types plus damaged sustain-capable ships; no faction specials, action
cards, leaders, retreats, external space cannon, invasion or pending scoring
windows. Unsupported effects are explicitly rejected, never silently stripped
from a captured real position. Add upgrades and other effects in later strata.

Use a declared, deterministic legal casualty/sustain policy on both sides. Store
its version with every label. These are outcomes under that policy, not optimal
combat odds. Evaluate sensitivity to a second legal policy before relying on the
predictor with a materially different learned combat policy.

For each canonical scenario, vary only the dice seed and record:

- attacker win / defender win / mutual destruction;
- surviving counts by unit type and damage state on both sides;
- losses under an explicitly named resource-cost metric, as a diagnostic only;
- combat duration, completion/error status, policy/rules version and seeds.

Do not collapse unfinished combat or errors into draws. Retain a failure ledger
and refuse to publish a successful dataset if unexpected failures remain.
Later retreat scenarios need separate retreat/escape/control outcomes.

**Sampling and split:** begin with equal-strength, asymmetric and counter-composition
matchups, then combine real training-position frequencies with deliberately sampled
rare/boundary cases. Report separate realistic and challenge-set metrics. Group by
canonical fleet family/context **before** generating dice repetitions. Player
relabels, side swaps and closely related variants stay in the same split. Reserve
unseen fleet compositions and sizes as explicit generalization tests.

**Proposed starting bounds:** smoke 128 configurations × 32 dice seeds (4,096
fights), at most five minutes. If successful, at most 2,048 configurations × 64
seeds (131,072 fights), at most twenty additional minutes and 512 MiB of output.
These are caps, not evidence that generation is this fast. Measure fights/s first;
reduce sample count rather than exceeding bounds. Freeze splits before labels.
Use equal initial repetitions; any extra sampling of uncertain cases uses only
training/validation, with an untouched fixed evaluation panel.

At 64 independent repetitions, estimated 50% win probability has standard error
about 6.25 percentage points. Preserve counts/sample sizes and report uncertainty;
do not present these labels as exact odds or train against falsely precise targets.

**Checks:** identical seed replay, relabel invariance, swapped-side accounting
without assuming attacker/defender symmetry, outcome/survivor consistency,
legal choices, effect exclusions, bounded failure propagation, and split disjointness.
Use tiny hand-checkable cases and engine regression fixtures to validate the harness.

## Phase 2 — Learn consequences before changing the game policy

Train a small separate predictor of outcome probabilities and surviving fleets.
Use aggregated outcome counts (or equivalent per-fight likelihood), retaining sample
sizes; do not label a single stochastic win as certain. Train on CPU-generated
batches with GPU optimization if it measures faster.

Baselines: training-set constant outcome prior, fleet-cost/ship-count model, and
a simple model using expected hits plus available damage absorption. They provide
comparison points, not alternate rule engines. All trainable baselines see the
same train split and no evaluation labels. Direct repeated engine simulation is
the label reference and a cost/accuracy comparator, not the live deployment path.

Report held-out Brier score, log loss, calibration, survival error, results by
scenario stratum, training time and inference cost. Aggregate uncertainty by
scenario family; thousands of dice repeats are not thousands of independent
fleet configurations. Include label uncertainty when interpreting error.

**Proposed screening gate, frozen before training:** at least 5% relative Brier
improvement over the best cheap baseline on the realistic validation panel, with
the paired scenario-cluster 95% interval for improvement above zero; no gross
miscalibration on challenge strata. Inspect the held-out test once after selecting
the model. This threshold is an experiment decision, not an established effect.
If it fails, diagnose input coverage or stop before PPO integration.

## Phase 3 — Give relevant decisions access to the learned result

Start with a **frozen predictor plus bounded additional actor features**, keeping
the full-game VP critic and PPO reward definition unchanged. Predicted battle odds
and losses are information the policy may use; they do not become a battle-win
reward or a hard attack threshold. Use a neutral/unsupported indicator outside
the covered scenario set; do not silently extrapolate into special mechanics.

- Movement/attack choices: compute predictions for the legal proposed committed
  fleet; retain already committed ships and capacity consequences. Incremental
  movement choices need incrementally updated fleets, not a fictitious all-in force.
- Activation: a commitment does not yet exist. Either describe a specific legal,
  feasible candidate fleet under a declared construction rule, or defer activation
  integration. Never present an optimistic union of reachable units as actual odds.
- Combat choices: deferred to a separate experiment conditioning predictions on
  the proposed casualty/sustain/retreat action and the continuation policy.
- Production: deferred until scenarios include budget, transport, mobility and
  competing goals. The strongest immediate combat fleet is not always the best build.

Add inputs through the existing vocabulary/bundle migration route. Zero-initialize
new actor connections and test unchanged old policy outputs before learning. Freeze
the outcome predictor for the initial PPO pilot so its policy-conditioned targets
do not silently change. Measure added rollout cost: aim for no more than 5% per
update at the existing CPU worker count. Batch/cache repeated candidate evaluations
only if needed; no Monte Carlo simulation per live action and no GPU request queue.

Shared-trunk auxiliary loss or combat-policy distillation is a later alternative,
not part of the first experiment. Auxiliary replay must never masquerade as fresh
on-policy PPO records, and combat labels must not replace round-4 value targets.

## Phase 4 — Test transfer to winning within four rounds

Three arms initialized from the same checkpoint:

| Arm | Purpose |
|---|---|
| A: current PPO | Baseline |
| B: PPO plus bounded fleet inputs | Isolate benefits from better observations |
| C: same inputs plus frozen learned battle predictions | Test added arena knowledge |

If Phase 0 proves inputs already sufficient, B becomes the matching existing-input
control and redundant arms are removed before launch. Freeze identical rewards,
opponents, pools, temperature and evaluation protocol. Use training pools for
data, validation for tuning, and reserved evaluation seeds/fleet groups for tests.

First screening pilot: one training seed, at most ten updates per arm. Evaluate
each against the same frozen opponents using 64 held-out seeds × six seat rotations,
with the learner swapped through seats. This is exploratory; it cannot establish
robustness across training seeds. Stop if it fails to outperform the input-only
control or causes clear regression. Include an unseen opponent style as a later
robustness check, without tuning on final evaluation results.

If promising, confirm across at least three training seeds with a fixed budget.
Primary endpoint: round-4 mean VP at equal total wall-clock training budget.
Also report time to a preregistered VP target, strict round-4 VP lead rate with ties
separate, faction breakdown and confidence intervals clustered by game seed.
Count arena generation and pretraining cost once in the candidate's total budget;
give the baseline corresponding additional PPO time. Also report equal-update
results as a diagnostic. Paired seeds do not imply identical trajectories.

Proposed practical success threshold: at least +0.1 mean VP over the input-only
control, paired clustered 95% interval above zero, no faction mean drop exceeding
0.25 VP, and better equal-time performance after all arena costs. Freeze thresholds
and budget before confirmation. If uncertainty is too wide, report inconclusive;
do not keep adding runs until a positive result appears. Combat accuracy alone
does not pass this gate.

## Extension order and other mechanics

These are ranked hypotheses based on engine support and how clearly local outcomes
connect to decisions. Audit actual mistakes/frequency before committing campaigns.

| Priority | Area | Isolated task and engine reference | Useful learning target | Main limitation |
|---|---|---|---|---|
| High, parallel candidate | Objective completion and payment | Small end-of-round positions; `objectives::{scoreable, requirement_for}` and `payment::plans` | Choose actions/payments that secure a score while preserving resources for other known goals | Exact eligibility/affordability already belongs to the engine; opportunity cost depends on goals and remaining actions |
| High | Movement, cargo and expansion | Local origin/destination boards using movement/tactical/cargo legal choices | Deliver the required force or claim planets with minimal committed ships/tokens while retaining useful capacity | Reachability alone is already solved; do not train a duplicate legality filter or ignore the rest of the map |
| High, after space combat | Invasion and planet capture | `invasion::{bombardment, ground_combat}` and retained invasion window | Capture probability, surviving ground forces, transport required, resource/objective consequence | Space victory does not imply planet capture; PDS, shields, infantry/mechs and timing matter |
| Medium | Casualties, sustain, rerolls and retreat | Decision-conditioned continuation from a retained combat window | Rank legal choices by survival/control outcomes under a declared continuation policy | Cheap casualty choices can sacrifice needed transport; retreat requires valid destinations and strategic context |
| Medium | Production and force composition | `ProductionWindow`, budget, fleet supply, transport and a declared mission | Reach mission success with lower expenditure or build a better feasible fleet for representative threats | Optimizing raw damage encourages overspending and weak transport; mission/opponent distribution must vary |
| Medium | Command tokens and strategy secondaries | Short sequences with known objectives, affordability and remaining turns | Preserve ability to perform a required action sequence or score before round 4 | Value spans multiple turns and opponents' timing; one-step resource gains are misleading |
| Later | Technology paths and faction abilities | Goal-conditioned short paths ending by round 4 | Unlock a required move, build or score using fewer actions/resources | Highly contextual prerequisites and faction exceptions; start only after frequent failures are measured |
| Low initially | Diplomacy, promises and agenda voting | Multi-agent subgames with explicit opponent policies and settlement horizons | Value completed exchanges, enforce affordability, forecast commitment settlement | No single correct deal; hidden intent, retaliation and opponent adaptation can invalidate local labels |

**Recommended sequence:** battle prediction pilot; objective/payment puzzles as the
next independent candidate; movement/cargo if the audit shows failures; then linked
space-combat → invasion → objective scenarios. Avoid launching many arenas at once.
Individual battle repetitions may be independent; linked scenarios must preserve
ship losses, capacity, exhaustion, token use and subsequent legal choices.

For deterministic tasks, start with a direct engine-backed solver or exact labels.
Learn only the unresolved choice/value problem. If a cheap exact feature or better
observation solves it, prefer that over a neural predictor.

## Implementation packages and stop points

1. **ARENA-001: audit and generator smoke.** New narrow example under `ti4-training`,
   focused harness tests and evidence; use existing engine APIs. Output split
   manifest, completion ledger, throughput and input-collision report. No PPO edits.
2. **ARENA-002: predictor feasibility.** Bounded dataset and a separate `ti4-mlp`
   diagnostic trainer/evaluator. Publish model/data manifest and baseline comparison.
   Stop on the Phase 2 gate; do not modify the production policy yet.
3. **ARENA-003: input integration.** Scope and independently review projection,
   vocabulary migration, hidden-information and checkpoint compatibility changes.
   Add zero-init invariance tests and measure CPU rollout overhead.
4. **ARENA-004: transfer pilot.** Preregister the A/B/C configurations and budget;
   run the bounded screening and conditional confirmation. Record negative or
   inconclusive outcomes as such; no automatic promotion.

Permission scope when executed: P1 repo source/tests/evidence and bounded P2 local
generation/training. Read only explicitly selected training checkpoints/pools;
write generated artifacts under the existing approved ignored output location.
No remote writes, new worktrees, source cleanup, commits, live-game changes or
checkpoint promotion are part of this plan. Every run needs a manifest containing
source/binary/rules/policy hashes, full arguments, seeds, split IDs, resource limits,
completion counts and measured elapsed time. Complete independent review for
training math, actor information boundaries and performance claims before promotion.

The first executable unit is ARENA-001 alone. Its measurements determine whether
to spend the larger budget, rather than committing to a large arena before seeing
whether the input representation and sample economics support it.
