# Agent handoff: isolated pitched-battle arena

## Authoritative user intent

Build an isolated environment starting directly at a **pitched battle** between
two supplied fleets. Vary composition, damage, upgrades and factions; repeat with
different dice seeds to learn how fights resolve. Optionally extend it to randomly
dealt action-card hands and combat-card play to measure their effects.

No expansion, production, movement planning, diplomacy or strategy phase needs to
be played through. Construct the minimum valid engine context, place the fleets,
and fight. Only combat decisions remain: casualties, sustain, relevant abilities
and cards, and optionally retreat. Begin with controlled combat policies.

This clarification **supersedes narrower scope and sequencing in the earlier plan
and design**. Ordinary no-upgrade/no-faction fights are smoke fixtures, not the
intended final domain. A movement-input audit is downstream integration work, not
a prerequisite for building the isolated arena. The earlier five-type/20-input
predictor is a toy baseline, not a fixed production contract.

## First implementation task

Implement a bounded, reproducible arena harness using the existing Rust combat
engine. Deliver a runnable example/CLI, matchup configuration, focused tests and a
small measured report. Do not begin a large dataset or PPO-training campaign.

1. Inspect repository instructions and combat/timing APIs.
2. Prove the harness with hand-checkable ordinary-fleet fixtures.
3. Extend the scenario schema and tested execution to upgrades and faction-specific
   combat behavior. Include actual upgrade and faction-effect comparisons in the
   first report. Declare supported coverage and reject unsupported effects; do not
   silently resolve special ships as ordinary ones.
4. Repeat each matchup over explicit dice seeds. Report outcomes, survivor
   distributions, completion/failure counts, reproducibility and fights/second.
5. Provide an extension point for hands and card policies. Card-enabled generation
   is a subsequent bounded extension unless existing timing support makes it small.
   A hand attached to state does not establish that the engine offered or used it.

## Inputs and labels

Inputs include both factions and attacker/defender roles; unit types/upgrades and
damage/markers; relevant technologies and public effects; readiness/resources
needed for combat abilities; rules version; both combat policies; and dice seeds.
Make relevant faction setup explicit instead of accidentally inheriting unrelated
defaults. Represent factual unit properties and effect identity, not only labels.

Initial pitched-battle mode explicitly disables retreat. A later retreat mode
must provide legal destinations and distinguish escaping from destruction.
Optional card scenarios specify hands and a dealing seed separate from dice seeds.

Record per fight: completed outcome, surviving units by type and damage, losses,
combat rounds, choices/cards/abilities actually used, and failure details. Aggregate
win/loss/mutual-destruction rates, sample counts and uncertainty. Never turn an
engine error or completion cap into a draw or silently discard difficult cases.
Specify the settlement boundary, including capacity consequences after carrier
loss, and apply it consistently.

For comparisons, change one factor while holding unrelated context fixed. Faction
comparisons must apply actual special units and rules, not just a faction label.
Combat policies select legal options semantically with deterministic tie-breaks.
Label odds as conditional on those policies, not optimal-play probabilities.

## Engine and information boundaries

Reuse `ti4-engine/src/combat.rs`, especially `CombatWindow`, shared fixtures and
validated choice delivery. `combat::resolve` lacks an outer Game timing/scoring
context; do not assume it exercises all faction abilities or action cards. Use
the retained window and actual timing machinery where required, with minimal
valid game context. Do not write another combat-rules simulator.

Keep simulator state separate from actor-visible inputs. The simulator can know
both hands; the main model must see public facts and its own private information.
Neither inputs nor support flags may reveal an opponent's hidden cards. Distinguish
controlled odds for specified hands from actor-usable predictions averaged over a
declared distribution of unknown hands. Random-hand experiments must record the
dealing distribution and card-play policy.

Split fleet/faction/upgrade scenario families before generating dice repetitions.
Do not put the same matchup in train and test under different seeds. Keep relabels
and related variants within their assigned family/split.

## Main-model integration to preserve

The arena teaches **how fights resolve**. The main PPO model learns **whether the
fight helps win**, given objectives and opportunity costs, maximizing VP by round 4.
Do not replace the VP critic target or add a full-game battle-win reward.

The proposed first integration is a small frozen predictor packaged with the main
policy. Its factual/predicted battle features enter the existing option MLP. Arena
and live prediction share input semantics; predictor weights/schema/continuation
policy must be saved with checkpoints. Actual integration must describe candidate
committed fleets, not every friendly ship that could reach a destination.

Preserve these contracts while building the arena, but do not modify PPO, vocabulary
or checkpoint schemas in the first harness unit. The design document covers later
materialized PPO features, zero-initialized migration and CPU-local inference.
Do not narrow the intended faction/upgrade domain merely to fit its toy predictor.

First success: trustworthy, inexpensive pitched-battle data. Later success: held-out
prediction improvement and better round-4 VP at equal total compute. Neither has
been established yet.

## Execution bounds and acceptance

- Read `AGENTS.md`, scoped permissions and current execution state. Use the existing
  `diplomacy-worktree`; preserve all other edits. No new worktrees, cleanup, commits,
  remote writes or production checkpoint changes are requested.
- Proposed ownership: tensor-free contracts in `ti4-policy`, generation/example in
  `ti4-training`, predictor later in `ti4-mlp`. Reuse engine APIs; any missing timing
  adapter needs a bounded explicit change and tests.
- CPU simulation first. Do not revive the slower GPU rollout queue for this task.
- First measured panel: at most 128 scenarios × 32 dice seeds, five-minute execution
  cap, bounded logs in an existing ignored output directory. Start smaller to prove
  mechanics. Record complete commands, source/binary hashes and actual coverage.
- Test seeded repeatability across workers, upgrades/faction effects taking effect,
  damage, legal decisions, failure propagation and survivor accounting. Card support
  additionally requires tested timing, eligibility, consumption and actual effects.
- Four rounds caps full-game training/evaluation. One battle can require more than
  four combat dice rounds; retain the engine's bounded combat resolution.
- Update evidence and `plans/EXECUTION_STATE.md`; report coverage gaps and the next
  exact command. Obtain required independent review for implemented critical changes.

## Read next / current status

This handoff governs intent and the first bounded task. Consult
`BATTLE_ARENA_PLAN_2026-09-16.md` for training/transfer gates and
`BATTLE_ARENA_DESIGN_2026-09-16.md` for later integration, subject to corrections above.
The retained-combat-window policy test in `crates/ti4-policy/src/bot.rs` is a useful
engine-driver reference.

Current status: documentation/design only. This task has not implemented an arena,
generated its dataset, trained a predictor or run arena training.
