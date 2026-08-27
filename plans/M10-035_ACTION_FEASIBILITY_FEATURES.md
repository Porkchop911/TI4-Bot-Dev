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
