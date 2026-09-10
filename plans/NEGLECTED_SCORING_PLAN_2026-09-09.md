# Improving scoring in neglected areas

Date: 2026-09-09. Status: proposed experiment plan; implementation and training have not started.

## Objective and starting point

Increase total candidate VP at the existing four-round horizon by learning additional profitable
scoring routes. Improving a particular objective's conversion rate is a diagnostic, not the acceptance
criterion: pursuing it can consume resources, actions, or the status scoring slot needed for another
point. Longer-horizon play is a separate experiment after the four-round comparison.

Start from `out/checkpoints/arm-vponly/checkpoint-43992`. Retain its exact current reward recipe,
including the remaining opening terms; the name does not mean the return is pure VP. Use a continued
training control from these same weights, not just the unchanged checkpoint.

Evidence: `VP_SOURCES_2026-09-09.md` and `TEMPERATURE_EVAL_2026-09-09.md`. The census covers 20 seed
clusters × six rotations × six candidate positions, all at temperature 0.001 against frozen 473312
opponents. It is exploratory, correlated within seed, and already used to choose these targets.
Low conversion establishes an investigation priority; it does not establish cause or recoverable VP.

## Priority order

| Order | Route | Measured vponly conversion | Working hypothesis and reason to test |
|---|---|---:|---|
| 1 | Improve Infrastructure; overlap with Build Defenses and Fuel the War Machine | Infrastructure 25.0%; Build Defenses 71.3% | Better placement may turn the existing construction habit into additional public/secret points. Test this before encouraging more construction. |
| 2 | Explore Deep Space, Make History, Populate the Outer Rim, Intimidate Council | 0.0%, 8.6%, 18.1%, 26.6% | Existing ships may earn points through deliberate positioning. First establish legal reachability and opportunity cost; empty-system occupation can cost expansion or leave ships exposed. |
| 3 | Develop Weaponry and Diversify Research | 6.2%, 16.7% | Learn specific technology sequences when a revealed objective justifies them. Develop Weaponry regressed from 20.1%. Prerequisites and spending may make some four-round plans unattractive. |
| 4 | Feasible secrets: Form a Spy Network, Mine Rare Metals, structure secrets | Secret VP roughly unchanged at 0.62/game | Improve retention and preparation for achievable cards. Final held counts alone do not establish failures: some cards were drawn late. |
| 5 | Raise a Fleet; other expensive military secrets | Raise a Fleet 6.5% | Test only after checking whether concentration/build cost produces net points. Do not treat generic aggression or larger fleets as the intervention. |

These priorities are hypotheses about efficient gains, not a ranking of proven uplift. Run the
zero-score Explore Deep Space audit immediately alongside the infrastructure audit; it may reveal a
correctness or observation issue that takes priority over training experiments.

## Stage 1 — establish why points are missed

### 1A. Check legality and what the policy can observe

For infrastructure, empty systems, spatial requirements and unit upgrades, trace:

1. Content requirement → engine counting/scoreability predicate.
2. Relevant legal movement, production, construction and technology options.
3. Observed progress and existing option facts → checkpoint vocabulary columns → actual actor input.
4. Completed requirement → legal scoring option → successful VP award.

Exercise focused positive, just-below-threshold and invalid cases. Existing tests such as
`deep_space_counts_systems_without_planets` are a starting point, not proof that a normal rollout can
reach the state. Trace at least one legal, reproducible route to each priority objective using Train
maps. For Explore Deep Space, explicitly verify the number and reachability of planetless systems,
ship availability, movement range, activation tokens and distinct-system counting.

Classify the result as an engine/choice bug, an observation limitation, missing experience, or costly
but legal strategic choice. Report uncertain cases as uncertain. If a correction changes observations,
options or scoring behavior, isolate it from the training comparison; do not silently fold it into a
new baseline. Observation or vocabulary additions require a separately approved surface change.

### 1B. Record the complete opportunity funnel

Extend the inert diagnostic with per-candidate, per-card records:

- Reveal/draw round and phase; removal/discard time; status/Imperial windows while the card is visible
  or held. Account for early game termination, agenda changes, and cards revealed after scoring.
- Requirement progress before actions and scoring windows; best progress reached; home-system gate;
  actual affordability for spend cards and technology/command-token constraints where relevant.
- Eligible, offered, selected and successfully awarded are separate fields. Record competing eligible
  objectives and whether the public/secret allowance has already been used.
- For secrets, retain draw and discard history so early-held failures are separated from recent draws.

Report conversion by reveal round, faction and area as well as the pooled rate. A round-one reveal
and a round-three reveal provide different preparation time. Treat first-seen reveal counts alone as
insufficient evidence of a remaining scoring window.

For each of the first three routes, inspect 12 Train-pool failed exposures, spread across factions and
reveal times, plus successful examples where available. Label whether a plausible low-cost completion
exists. Replays or bounded alternative-action probes can provide examples, but must not use unseen
future draws or opponent private information when proposing a decision. Successful probes are not
population-level uplift estimates.

**Exit:** an evidence table with failure categories, representative replay IDs, observation/legality
findings and at least one legal completion route for each training target. A route blocked by a
correctness issue does not proceed to training until separately resolved.

## Stage 2 — one conservative learning intervention

First test a whole-episode curriculum that provides more experience with a target area while keeping
the observation surface, action set and reward recipe fixed. Start with infrastructure; use spatial
positioning first instead if Stage 1 establishes clearly cheaper attainable gains there.

- Generate and classify a reusable pool of Train-only game setups by their initially revealed public
  objectives. Use 75% ordinary training setups and 25% setups containing the target family among the
  initially revealed objectives. Predeclare this mixture; do not repeatedly tune it against evaluation.
- Draw from many maps and seeds, balance faction rotations, and let normal gameplay generate all
  later states. Log the actual sampling distribution and duplicate frequency.
- Collect fresh trajectories under the current policy and recorded behavior probabilities. Keep whole
  trajectories and normal PPO accounting. Do not oversample successful choices from an old buffer or
  relabel historical actions as on-policy.
- Do not add a generic bonus for construction, technology, fleet size, or merely occupying empty
  systems. Actual VP remains the primary reason to choose those actions.

Curriculum sampling deliberately changes the training setup distribution; it does not change the
rules or options for a fixed setup. It may improve early-revealed objectives and fail to generalize to
late reveals, so ordinary evaluation and reveal-round breakdowns are mandatory.

If the audit finds that fresh rollouts almost never discover a legal completion, stop after the
bounded screen. The next isolated proposal is a small imitation warm-start from verified Train-only
successful routes, followed by ordinary PPO. Specify its separate loss, example provenance and
weight before running it; do not mix demonstrations into PPO as if sampled by the current policy.
An auxiliary progress-prediction head is a later architecture experiment, not part of this first arm.

## Stage 3 — controlled experiment and acceptance

### Training control

Compare A: continued current recipe on ordinary Train setups, against B: the same recipe with the
75/25 curriculum. Both start from checkpoint-43992. Run three paired training-seed replicates. Each
pair uses the same declared RNG seed bases and update budget; curriculum setup differences are the
intended treatment. Freeze all other settings, including rollout temperature, PPO settings, horizon
and opponent schedule. Do not change training temperature because greedy evaluation scored higher.

Initial budget: 300 updates per arm per replicate, matching the earlier arm scale. Record decisions,
games and wall time as well as updates; different policies generate different trajectory lengths, so
equal updates do not imply equal experience or compute. Compare VP gained per wall hour as a
secondary efficiency result. Restore identical optimizer state when available, otherwise initialize
the optimizer identically for both arms and explicitly label the cold restart.

Run one CUDA training job at a time. Check for an existing trainer before starting and avoid concurrent
CPU evaluation when collecting uncontended training timings. Freeze executables and input manifests
before A/B execution. This document does not launch jobs.

### Evaluation

- Use temperature 0.001, four rounds, six rotations and six candidate positions against five frozen
  checkpoint-473312 opponents, matching the existing metric.
- Keep the old 20-cluster census as a development diagnostic only. Before training, reserve a fresh
  screen cohort and a disjoint sealed confirmation cohort under existing artifact-role conventions.
  A new seed on a reused map is not necessarily a new map: record actual map identities, avoid
  Train leakage and account for reuse when grouping uncertainty estimates.
- Screen fixed-budget final checkpoints on 30 reserved clusters. Do not select the best checkpoint
  from a long sequence of noisy measurements. If total VP trends down or the target route does not
  improve, return to the diagnosis rather than adding more simultaneous interventions.
- Confirm a promising recipe on 250 reserved clusters per trained replicate, with paired baseline
  runs. This is a provisional budget: previous paired-difference SD around 0.27 suggested roughly
  230 independent clusters for 80% power at a 0.05-VP effect. Recheck variance and map independence;
  this estimate does not account for training-run variation.
- Report each paired training replicate and an aggregate that accounts for both training replication
  and map clustering. Do not pool thousands of seat games as independent samples.
- Confirm direction against a second frozen opponent panel, including checkpoint-43992 opponents.
  Keep those evaluations separate because their baselines differ. Report VP source changes to detect
  dependence on Support transfers or a single opponent habit.

**Predeclared promotion rule:** mean total-VP uplift at least +0.05 over continued-training control;
the aggregate paired 95% interval excludes zero; positive direction in at least two of three training
replicates; and positive mean direction on the second opponent panel. Route-specific conversion
should improve in the intended direction. A rise in the target route accompanied by lower total VP
fails acceptance. Uncertainty that remains too large is an inconclusive result, not a passing one.

Report source substitution explicitly: which extra points were gained and which established sources
were lost. Opening clearance, waste, faction splits and reveal-time splits are diagnostics. No inference
errors, illegal actions or silent truncations are allowed. A material new failure pattern must be
explained before promotion even if the aggregate VP threshold is met.

## Stage 4 — expand only from a successful intervention

Apply the winning method to the next audited area with a new isolated control/treatment comparison.
Test the combination of successful curricula against the best single curriculum; combined gains
cannot be assumed additive. Keep a majority of ordinary setups so existing economy and expansion
skills remain useful across the normal distribution.

For secrets, use the Stage 1 draw/retention audit to decide whether the next experiment should target
selection or completion. Do not prescribe action-card hoarding, faction technologies or combat from
final-hand counts alone. Treat longer episodes and alternative exploration temperatures as separate
arms only after the first route experiment is interpretable.

## Constraints, deliverables and next action

- Preserve seeded RNG behavior, observed facts, option IDs/order, vocabulary and BTreeMap iteration.
  Inert instrumentation must pass ordered-choice, ordered-event and final-state hash comparisons for
  fixed weights/seeds. New trained weights are expected to change games; byte equality across trained
  policies is not the acceptance criterion.
- Preserve all shared-tree work. Check status before editing; report overlapping dirty files instead
  of overwriting them. Do not stage, commit, reset, stash, clean or regenerate unrelated fixtures.
- CPU inference remains the specification. No GPU-inference or precision/kernel change is included.
- Keep raw diagnostic data, replay identifiers, frozen executable/bundle/input hashes, training
  manifests, per-cluster evaluation results and an explicit accepted/rejected/inconclusive decision.

Next action: perform Stage 1A for Improve Infrastructure and Explore Deep Space, then add the
opportunity funnel and inspect the bounded Train examples. Do not start a new training campaign until
that audit establishes which route is learnable through the existing surface and worth targeting.
