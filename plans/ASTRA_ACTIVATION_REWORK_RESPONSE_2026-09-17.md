# Astra review: activation packages and fleet strength

Reviewed proposal: `ASTRA_ACTIVATION_REWORK_2026-09-17.md`.
Source: `codex/diplomacy-v1`, HEAD `7b22aff81284d0d2a9abad317f47a665be8d7899`.
This is an advisory code/design review. I inspected the proposal, predictor evidence,
activation/movement/projection code, combat guards, PPO records and strength probe.
I did not replay the cited reviewer capture or rerun the 40-million-fight calibration.
Reported benchmark/pilot figures remain the authors' measurements, not new verification.
The accepted user decision allowing the lean arena simulator is retained.

## Verdict

**Yes to evaluating a concrete fleet before activation and experimenting with
plan-level decisions. Revise the package generator and execution contract before
implementing the proposal as written.** Adding odds only during movement leaves
the irreversible destination decision uninformed. However, this does not prove
that all movement and landing decisions should become a fixed script immediately.

The best next experiment shares one legal package generator between two arms:
activation informed by candidate-fleet summaries with step-wise execution, and
joint system/package selection with planned movement/cargo execution. This separates
better information from benefits or regressions caused by action abstraction.
Keep existing arena-v4 PPO as the baseline. All full-game runs end at round 4.

## 1. The package menu is a policy, not just an optimization

The model cannot select a fleet the generator omits. "Everything", "90% win" and
"invasion" bake in strong preferences before PPO evaluates anything:

- A hard 0.9 threshold can exclude the best available risky scoring attack in round
  4, or a cheap favorable attack where raising success from 85% to 90% is wasteful.
- Full cargo can empty a necessary home defense or consume capacity better used
  for fighters. Two packages with identical ships but different cargo/origins have
  different opportunity costs.
- The globally cheapest decisive fleet is not guaranteed by an index shortlist.
  Call it the cheapest *found* feasible candidate unless search is exhaustive.
- No enemy ships does not imply simple peaceful expansion. Rival ground forces,
  PDS, enemy-controlled planets and own systems used for production remain relevant.
- Activating an owned system to produce or reinforce is valid even when no new
  hull can move in. Do not filter legal engine activations by "some hull can reach".

Start with a small diverse budget, for example 4–6 candidates per system: low-cost
commitment; combat-efficient commitment; objective/capture-oriented transport;
strong feasible commitment; and origin-preserving variants when distinct. This is
a suggested search budget, not evidence that six is optimal. Thresholds may help
generate diversity but must not exclude every below-threshold choice.

Deduplicate by actual executable commitment including origins, cargo, boosts and
intended allocation. Prune dominated candidates only under explicit comparable
resource/context dimensions, not solely win probability and fleet purchase price.
Retain an explicit step-wise/manual branch for uncovered objectives and unsupported
predictor contexts. This is different from silently abandoning an executed plan.

Avoid a subtle probability bias: a flat softmax over all system/package pairs gives
systems with more candidates more probability mass at equal scores. Prefer an
explicit hierarchical distribution P(system) × P(package | system), or define and
test another deliberate count correction. Do not accidentally inherit a prior from
how many templates survived deduplication. Treat the combined choice probability
consistently in PPO and in its entropy term.

## 2. Individual reachability is not joint feasibility

`tactical::movable_into` enumerates hulls individually. Each query can find Gravity
Drive available; `technology::use_gravity_drive` spends it once per activation.
Two ships that each require that same boost cannot both use it. Ionian availability,
cargo, supply, capacity and origin defenses similarly require a joint commitment.

Consequently "every ship that can reach" is not necessarily an executable package.
Generate packages against a shared resource ledger and verify them with actual
engine legal decisions. Track movement boosts and their consumers explicitly.
Ship/cargo selection must use stable semantic identities, not stale vector indices
after earlier moves remove units. The engine can retain authority over every step
while the policy carries the plan; no rules redesign is required.

Keep distinct factual concepts: legal activation, individually reachable hull,
jointly feasible package, and remaining token/access budget. Unify their rules
implementation, not their meaning into one boolean.

## 3. Plan execution needs an interruption contract

A chosen plan is a contingent commitment. Activation reactions, cards, combat
casualties and changed transport capacity can invalidate subsequent steps.
"Fallback to the model for anything else" is too underspecified to audit.

Give the plan an activation identity, remaining commitments and explicit states:
executing, normally complete, invalidated by an observed event, or generation error.
Continue through legal prompts, revalidate after relevant events, and terminate or
replan at a documented boundary. Log the reason and new learned decision when one
occurs. A scenario that requires an impossible move absent an intervening event is
a generator defect, not a normal fallback. Never answer an unrelated window merely
because its option ID resembles a planned choice.

Keep landings learned in the first pilot. Space combat changes surviving carriers,
ground cargo and bombardment capability; planet allocation also depends on ownership,
objectives and defense. A package may express a landing intention, but should not
blindly execute a precombat allocation after losses. Landings can become a second
small postcombat planning decision once its own evaluation passes.

## 4. PPO can support this, but the policy contract changes

The existing `Step` can hold package feature rows, sampled index, actual behavior
log probability and pre-choice value. That does not make the integration automatic:

- Build and sample a policy-internal package choice, record its actual alternatives,
  then return the corresponding legal engine system option. Do not let the existing
  system-choice sampler/recorder overwrite the package likelihood.
- Deterministic plan-following steps are not additional independent policy samples.
  Preserve their outcomes in episode rewards/progress and event traces. Combat,
  intentional replanning and manual execution remain learned when applicable.
- Freeze/store the exact sampled menu and generator/predictor identity. Never
  regenerate a different candidate menu during PPO epochs.
- Test the package log-probability ratio before the first optimizer step; unchanged
  weights should reproduce the recorded distribution within declared numerics.
- The macro decision's critic must be captured before selection. If later decisions
  depend on retained plan state, include the needed context in actor/critic inputs
  or explicitly terminate the plan before those decisions.
- Fingerprint and save generator, execution and probability-factorization semantics
  with the bundle and resumed run. New zero feature rows cannot preserve behavior
  across a changed action space; the proposal is correct about this.

Fewer decisions also change the number of training rows, minibatches, entropy
contributions and relative head weights. Keep the first comparison otherwise fixed
and report these changes. Do not interpret fewer rows as automatically better
credit assignment or learning. With seven combat decisions in the cited action,
"about three learned decisions" is not a general consequence; count actual recorded
non-forced decisions over a panel.

## 5. Use the index as a proposal heuristic; do not refine it yet

F × D is a useful cheap descriptor to test. The opening-adjusted version is
**matchup-dependent**: `square_after_opening(own, enemy)` depends on enemy cannon,
barrage and one's fighter screen. It is not one intrinsic number per fleet.
Keep intrinsic descriptors (firepower, durability, fighters, barrage, cannon,
transport) separate from a matchup-adjusted comparison. Retain composition facts.

The probe's "favorite right" metric excludes all simulated win rates in [0.4, 0.6].
It therefore does not establish 97% correctness on arbitrary or close battles.
The predictor's comparison uses a different held-out set, as the proposal notes;
that is not a matched accuracy comparison. The probe splits sampled pairs in half,
without an explicit canonical fleet-family holdout.

The important shortlist metric is **candidate recall/regret**, not favorite accuracy:
on small exhaustively enumerable legal positions, does the shortlist retain a
cheap strong fleet, useful invasion force and an origin-preserving alternative?
Measure how often its best candidate loses substantially to the larger reference
menu, and how many predictor calls and milliseconds it saves.

Delay fitted unit weights until this test identifies a material index-induced
miss. Repairing flags, sustain and fighter screens are not reliably summarized by
one globally monotone scalar. Use index-per-cost as an optional descriptive
production feature, not an authored instruction to maximize it. Likewise, enemy
threat summaries must consider public mobility/access and origin opportunity costs;
they do not know hidden card-assisted moves or predict that an opponent will attack.

## 6. Whole-action odds require more than today's predictor call

Current `battle::invasion_query` is a **post-bombardment, current-commitment** query.
It is not an activation-time end-to-end invasion predictor. The proposal's space
winner can differ from the planet owner, as in its own Hacan/Jol-Nar example.

Precombat estimates must distinguish P(win space), ground success conditional on
specified landed survivors, and P(achieve the whole capture objective). Losing a
carrier changes cargo and landing feasibility; survivors and damage are correlated.
Feeding mean surviving units into a ground predictor or multiplying unrelated
marginal success rates does not generally recover the whole-action probability.

For the first pilot, show separately labeled conditional estimates and cargo/risk
facts. Do not claim a calibrated whole-action take probability. A later composed
predictor needs joint space/cargo/bombardment/invasion labels with preserved losses
and separate ground defenders, or a validated bounded integration over outcomes.

The predictor uses a fixed casualty continuation while live combat remains learned;
its odds are conditional estimates, not guarantees for that live policy. Evaluate
calibration on selected package fleets, where maximizing predictions can expose
errors unseen in a random arena test set. Include unsupported-case coverage.

## 7. Clean-up and evidence gates

I support cleanup first, with three qualifications:

1. `target:reachable` currently uses printed map distance/base movement while
   `movable_into` uses movement rules/boosts. `target:within-token-budget` subtracts
   map distance from tactic-token count. This is a heuristic, not legal movement or
   package feasibility. Correct/version these inputs and rebaseline both pilot arms.
2. `activate-enemy-ground` indeed counts all planet units, including structures.
   `CombatWindow::sustainers` indeed lacks a ship predicate. Combat-round reaction
   table entries are unguarded; trace eligibility through the complete card path
   before choosing a fix. Use the exact card/participant semantics, not a blanket
   rule that accidentally blocks valid third-party effects. Replayed duplicate
   frames still require diagnosis; source inspection alone does not identify them.
3. Additional label integrity issue: `battle_arena::fight_core` stops after 50 rounds
   and returns `winner: None` both for mutual destruction and for both sides still
   alive. Add an explicit unresolved outcome and count occurrences in the measured
   panels before treating every None as mutual destruction. This review does not
   establish how frequently the cap affected existing labels/calibration.

Before a small training pilot, require:

- Corrected engine/projection baseline with focused regression tests and the
  existing relevant simulation checks. Re-evaluate comparison checkpoints on it.
- Joint package feasibility, stable identities, resource reservation, explicit
  manual branch and tested interruption/invalidation. Include owned-production,
  defended-but-shipless, Fracture, boost-contention and capacity-loss scenarios.
- Exact package recording/probability/temperature checks; deterministic execution
  steps excluded from policy likelihood; reward/progress accounting retained.
- Bounded generation cost and candidate-recall checks, plus a small real rollout
  smoke. No full 40-million-fight rerun is needed merely to launch this pilot.

For broader qualification, the proposed 10,000-position check is useful, stratified
by mechanics. Require zero unexplained generator failures in supported static cases;
separately test valid interruptions. "No fallback ever" is not appropriate for
all stochastic and reactive continuations.

For gameplay compare: existing arena-v4; activation information plus step-wise play;
and package execution. Use matched four-round seeds/rotations/opponents/rewards,
report VP and margin by faction with seed-cluster intervals, and treat 20 blocks
as screening rather than a conclusive gate. Confirm promising results across
training seeds with fixed budgets. Report both equal-update and equal-total-time
learning including candidate search and generation cost.

Do not gate success on reducing attacks below 50% predicted win: that can reward
inaction and reject necessary round-4 risks. Report it descriptively alongside
scoring outcomes, unnecessary losses and missed scoring opportunities. A 10% wall
overhead target is a sensible engineering budget, not a substitute for VP per time.

## Direct answers

1. **Plan-level activation?** Worth piloting; first compare it against preactivation
   candidate information with step-wise execution. Movement-only odds are too late
   for destination selection, but the full macro is not yet proved necessary.
2. **Three packages?** Too restrictive as the sole action space. Use a bounded
   diverse menu plus explicit step-wise choice; measure shortlist regret.
3. **Landings?** Keep them learned/adaptive initially; plan after observing survivors.
4. **Refine the index?** No, until shortlist failures or profiling justify it.
5. **Required gates?** Correct baseline, joint legality/interruption semantics,
   correct PPO likelihood/records, and bounded-cost rollout smoke before training;
   four-round equal-time VP evidence before adoption.

No implementation or benchmark was performed for this review. The next safe work
unit is fixing and testing the identified engine/feature/label defects independently
of the package policy, then specifying the candidate and execution contracts.
