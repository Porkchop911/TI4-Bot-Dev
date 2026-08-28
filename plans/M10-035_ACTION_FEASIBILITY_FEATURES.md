# M10-035 — action-feasibility features for the opening

**Author:** Claude Opus 5 — not eligible to review. **Status:** proposed.
**Origin:** codex's throughput/quality review, plus the reachability measurement below.

## The finding this rests on

Four independently trained policies converge on 86–91% held-out clearance. The operator states
that every faction can clear the Stage-1 bar 100% of the time under the current map restrictions,
regardless of opponents. So the entire remaining gap is policy.

The reachability search says something sharper about *what kind* of policy gap:

| search | recovery of failures | mean replays to first success |
|---|---|---|
| 40 replays at T=2.5 | 64% | 6.3 |
| 150 replays at T=3.5 | 65% | 26.3 |

Nearly quadrupling the budget bought one point. The searcher saturates at ~65%. Temperature only
explores within the policy's support, so for roughly a third of failures the clearing line has
effectively zero probability at several decisions in a row. **The policy is not mis-ranking the
right option; it is placing no mass on it.**

That is what an inability to *see* the option would produce.

## Why it cannot see it

`features::explicit_option_features_with` builds an option's vector from tokens of its `id` and
`label`, then drops every all-digit token. Its own comment records why:

> system ids are numbers, so `option:72` was dropped, while `option:archonren` sailed through

Planet identities are dropped deliberately and correctly — "so that the policy learns about planets
rather than about Archon Ren". The consequence is that an activation option carries the *kind* of
move and nothing about its destination. `tactical::activation_options` builds each option with
`system.to_string()` as the id, so the target is present and discarded.

The seat-level opening features added in `50468b0` tell a seat *"you hold two systems and need a
third"*. Nothing tells it **which offered move supplies one**.

## Plan

### Phase 0 — settle the baseline (~30 min)

Evaluate run-010's checkpoints 5,750 and 6,000 on identical held-out seeds and select on that, not
on the on-policy window. The window favours 5,750 (90.35% against 90.00%) while checkpoint 6,000
scored 90.7% held-out — above its own window. Selecting on the noisier signal is how a worse
checkpoint gets promoted.

Add explained variance to the critic telemetry. Critic loss has been reported at 25–45 all run and
is uninterpretable at this reward scale; without it we cannot say whether the critic helps.

### Phase 1 — resolve an option's target (~1 h)

A spike, because everything after it depends on the answer. Deliver
`projection::option_target(choice, option) -> Option<Target>` covering activation, movement,
invasion and production, with a test per head.

**Gate:** if a head's target cannot be resolved from the `ChoiceOption`, that head gets no features.
Guessing a target would produce a plausible feature describing the wrong square.

### Phase 2 — the features (~2 h)

Per option, only where a target resolves. Properties, never identities — the same rule that keeps
`option:archonren` out:

- `option-new-planets` — planets at the destination this seat does not control
- `option-adds-system` — would this be a system it holds no planet in
- `option-deliverable-ground` — ground forces that could actually land there this activation
- `option-capacity-left` — carrying capacity remaining after the movement
- `option-strands-ground` — would this leave ground forces without transport
- `option-closes-unit-deficit` — production that builds while preserving transport

Each connects "I need one more planet" to "this action obtains one", which no aggregate seat fact
can do.

### Phase 3 — prove they discriminate (~1 h)

Non-vacuity is the gate, not compilation. For a real position:

- the feature must **vary across the options of one choice** — a feature constant within a choice
  cannot inform that choice, and would look identical to a working one from outside;
- a probe that zeroes it must change at least one decision.

This is the failure class this project keeps finding, and a feature family is exactly where it
hides.

### Phase 4 — vocabulary generation and blank bundle (~15 min)

New names shift the column map, so a new generation and a fresh blank bundle are required. The
existing checkpoints become evaluable only at their own commits, as run-009's already are.

### Phase 5 — train (~2 h)

6,000 Stage-1 updates from blank, fresh seeds, **`--movement-entropy 0.01`**. Entropy returns to
default so exactly one thing differs from run-010: the features. run-010 changed two things at once
and bought +0.3 held-out for it.

### Phase 6 — judge against predictions registered now

Stated before the run, so the result can fail:

1. Held-out clearance above run-010's **90.7%** on identical seeds.
2. **Reachability recovery rises above 65%.** This is the sharper test. Clearance can move for many
   reasons; recovery rising means specifically that the clearing lines have entered the policy's
   support, which is the mechanism claimed.
3. No faction regresses by more than one point.
4. The failure mix shifts away from planets, currently 77.8% of failures.

**If 1 passes and 2 does not, the gain is not from the stated mechanism** and the feature family
should not be credited with it.

Reproduce across a second training seed before promoting, per codex's evaluation protocol.

## Deferred, with reasons

- **Reward shaping per deficit.** The Stage-1 potential is already per-component and capped, and
  rewards are already potential differences, so most of the proposal restates what exists. The
  `unit_weight` A/B changed nothing measurable against a matched control.
- **Curriculum on failing seeds.** Distorts the training distribution; worth trying only after a
  representation fix, and only with evaluation seeds kept untouched.
- **Larger models, longer runs, hyperparameter sweeps.** run-010 against run-009 was +0.3 held-out
  for 1,500 extra updates and a hotter movement head. This is not where the gap is.

---

## Result: the hypothesis failed

run-011, 6,000 Stage-1 updates from blank on vocabulary `1b2221be`, movement entropy back to 0.01
so the features were the only change from run-010. Judged against the predictions registered above,
on identical held-out seeds.

| prediction | required | measured | |
|---|---|---|---|
| 1. held-out clearance | > 90.7% | **90.2%** | ✗ |
| 2. reachability recovery | > 65% | **56%** | ✗ |
| 3. no faction regressing > 1 point | — | xxcha −1.6, jolnar −3.6 | ✗ |
| 4. failure mix shifts off planets | < 77.8% | **85.9%** | ✗ |

All four fail. The action-feasibility features as built do not help, and the feature family is not
adopted.

Prediction 2 is the informative one. Recovery **fell** from 65% to 56%: the clearing lines did not
move into the policy's support, they moved further out of it. Whatever the nine columns changed, it
was not the thing the plan claimed they would change.

The failure mix moved the wrong way on every axis. Failures missing two or more parts of the bar
rose from 53.9% to 73.9%, and systems-missing from 51.0% to 77.6%. The policy is not making
near-misses it could be nudged out of; it is failing more comprehensively.

### The defect that most likely explains it

`move-free-planets` and `move-adds-system` are computed from the **active system**, because movement
in a tactical action moves into the already-activated system. Every movement option in a choice
therefore receives the *same* value for both. They cannot discriminate between moves — only between
choices — which is the opposite of what the family exists for.

Two of the five movement facts are provably non-discriminating within a choice, and movement is
where the diagnosed failure lives. They occupy columns, train weights, and add gradient noise while
carrying no per-option signal.

This is the vacuity failure `an_action_fact_tells_two_activations_apart` was written to catch, and
it did not catch it: **the test covers activation, and I assumed the property held for movement.**
A per-head version of that test would have caught it before the run, in seconds rather than in two
hours of training.

### What survives

The three movement facts that do vary per option — `move-capacity`, `move-ground-at-origin`,
`move-carries-ground` — and the activation and commit facts. Five of nine are sound; the
experiment cannot say whether they help, because it ran them alongside two that could not.

### Disposition

Best Stage-1 remains **run-010/checkpoint-151528 at 90.7% held-out**, which predates this family.

Before another run: give every head its own discrimination test, delete the two non-discriminating
facts, and replace them with per-option ones — what *this* move would add, not what the destination
already is.
