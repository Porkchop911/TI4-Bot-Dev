# Engine completion plan

Bring `ti4-engine` to a complete and correct implementation of a **six-player game of TI4 with
Prophecy of Kings and Thunder's Edge**, and wire the result through `ti4-policy` so a learner can
make connected decisions on it.

Companion documents: `engine-rules-audit.md` (what is wrong or missing, all 109 rule topics),
`plans/evidence/SPACE_STATIONS_AUDIT.md` (the one defect measured end to end).

## Scope

**In.** Base + PoK + Codices 1–4 + Thunder's Edge. Six players. The six trained factions: sol,
letnev, xxcha, hacan, jolnar, l1z1x. All shared content — action cards, agendas, relics, objectives,
exploration, generic technologies and units.

**Out.** Galactic events (a variant, not a mechanic). Twilight's Fall. Franken. Player counts other
than six. Faction content belonging to the other factions.

**Out, but flagged.** Reward shaping for the new mechanics. The chosen wiring depth is engine plus
policy features; `ti4-training::reward` is left alone. §7 records where that will bite.

### What the six-faction scope changes

Restricting faction content to the trained roster shrinks it by roughly four times, and the
remaining faction work is nearly finished:

| area | all factions | the six | implemented (the six) |
|---|---|---|---|
| faction abilities | 73 | 14 | **14 (100%)** |
| leaders | 103 | 19 | 16 (84%) |
| breakthroughs | 31 | 6 | 2 (33%) |
| promissory notes | 40 | 6 + 5 generic | unverified |
| technologies | 102 | 12 + 37 generic | unverified |
| units | 125 | 18 + 22 generic | unverified |

The real weight is in shared content and in the five absent Thunder's Edge mechanics.

## Verification standard

This applies to every phase, and exists because the audit found the opposite pattern repeatedly.

1. **Test against the engine, not against a fixture built to match the implementation.**
   `activate-free-planets` shipped inverted twice past tests whose hand-built `planet_units` encoded
   the bug. The reachability feature is now tested by activating each candidate for real and
   comparing against what the movement step then offers.
2. **Probe every new gate.** Break the implementation deliberately, confirm the test fails, revert.
   A gate that has never been seen to fail has not been shown to work.
3. **Quote the rule.** Every registered effect carries the rule text it implements in a doc comment,
   so a future reader can check the code against the rule without leaving the file.
4. **Coverage is registration, not correctness.** `unimplemented()` counts absent registrations. Each
   phase states separately what was *verified*.

## Phase 0 — Stop the bleeding

Training is paused for the duration of this plan (decided 2026-08-29). No run is comparable across
an engine change of this size, and every clearance figure recorded so far is inflated ~6 points.

- **0.1** Record the re-baseline debt. Champion `run-017/checkpoint-26160` is ~90%, not 95.8%. Mark
  every recorded clearance number in `plans/` as measured under the pre-fix engine.
- **0.2** Land the `coverage_report` example (done) and keep it green as a progress meter.

## Phase 1 — Correct what is actively wrong

Eight verified defects. All are space-station consequences plus their fallout, and all are cheap.
Nothing later depends on them being done first *except* that they change measured clearance, so doing
them first means one re-baseline rather than several.

- **1.1 `Planet::is_space_station()`** in `ti4-content`, reading the `SPACESTATION` planet type. One
  predicate, so the distinction stops being re-derived. `Planet::traits` already excludes the type;
  this generalises what that knew.
- **1.2 Invasion.** `landable_planets` excludes stations (rule 5). Removes the illegal move.
- **1.3 Structures.** Construction (`strategy_cards.rs:474`) and `production::placements` exclude
  stations (rule 5).
- **1.4 Control.** Station control becomes sole-occupancy of the system (rules 2, 2a, 2b): gained
  when a player is the only one with units there, unchanged when they leave, transferred only by
  winning space combat. This is a new control path, not a variation on `planet_control` writes from
  invasion — it must be re-evaluated after movement, after space combat, and after unit removal.
- **1.5 Objectives.** Stations excluded from planet-counting families (rule 7). They remain counted
  for voting (rule 6, already correct) and remain exhaustable for resources and influence (rule 4,
  already correct).
- **1.6 Frontier tokens.** A system whose only planets are stations is planetless for rule 14, so
  tile 117 gets a token.
- **1.7 Opening bar.** Follows from 1.2 and 1.5 with no separate change; re-measure and restate.

**Exit:** the space-station audit table is all-green; clearance is re-baselined on a fixed engine.

## Phase 2 — Coexistence

13 rules. Sequenced here because it changes control, invasion, bombardment and blockade — the
primitives every later phase builds on, and the ones the opening policy exercises every game.

- **2.1 State.** A planet gains a set of coexisting players alongside its controller. `SystemState`
  grows a representation that keeps "controls" and "has units on" distinct — today they are conflated
  in `planet_units` plus `planet_control`.
- **2.2 Control interaction.** Rules 3.1, 3.2, 6: coexisting does not take control; being instructed
  to coexist on a planet you control hands control away and exhausts it; when only one player is
  left, coexistence ends and that player takes control.
- **2.3 Invasion.** Rules 9–12: a controller may start ground combat against a coexister and vice
  versa; a player with no units there who commits must start combat; a winner takes control and may
  then choose to fight the next coexister, repeatedly. This is a new decision loop in the invasion
  window, not a predicate.
- **2.4 Bombardment.** Rules 7, 7.1, 7.2: the bombarding player chooses whose units take the hits,
  independently per bombarding unit, and surplus hits do not spill to another player.
- **2.5 Blockade.** Rule 4: a coexisting structure is always blockaded regardless of ships present.
- **2.6 Objectives.** Rule 13: a coexister counts as controlling **only** for scoring objectives, and
  for nothing else. This is a scoring-only view of control and must not leak into spending, voting or
  the opening bar.

**Exit:** each of the 13 rules has a test quoting it, and the probe passes for the control-transfer
and combat-chain rules.

## Phase 3 — Neutral units

9 rules. Before the Fracture, because the Fracture places them.

- **3.1** A neutral force that is not a player: no turns, no technology, no cards, no retreat
  (rule 6), and no seat in `seating_order`.
- **3.2** Space and ground combat against neutrals (rules 2, 3), with hits assigned by the reference
  card's order (rule 7, 7a) and every usable unit ability always used (rule 5).
- **3.3** Rule 9: neutral units count as another player's ships for other effects, while no neutral
  player exists. This is the rule most likely to break assumptions elsewhere — anything iterating
  `state.players` to find "enemies" needs auditing against it.

## Phase 4 — Breakthroughs and synergy

The six factions' breakthroughs, plus the synergy system they all carry.

- **4.1 Synergy** (6 rules). Two technology colours treated as interchangeable, chosen per technology
  and per specialty, independently and re-chosen freely (rules 2, 3, 4, 6, 8). Affects research
  prerequisites and objective scoring. Needs a resolved-colour query rather than a stored value.
- **4.2 The four missing breakthroughs** for sol, hacan, jolnar, l1z1x. `xxchabt` and `letnevbt`
  already work. Passive and exhaust abilities coexist; the passive and the synergy survive exhaustion
  (rules 4.1, 4.2).
- **4.3 Not a technology** (rule 6): a breakthrough alone satisfies neither a research prerequisite
  nor an objective's technology requirement.
- **4.4 The breakthrough roll** (rule 3) — the d10 that may bring the Fracture into play. Implement
  the roll here; its consequence lands in Phase 5.

## Phase 5 — The Fracture

15 rules. Last of the Thunder's Edge mechanics because it depends on neutral units (Phase 3) and the
breakthrough roll (Phase 4).

- **5.1** Additional systems outside the map, placed on a 1 or 10 (rules 1–5).
- **5.2** Neutral units on its planets and their space areas (rule 6).
- **5.3** Ingress/egress tokens and movement between the map and the Fracture (rules 7, 8, 14).
- **5.4** Ingress placement by the triggering player's synergy, one per technology-specialty colour,
  with the degenerate cases (rules 9–13).
- **5.5** A relic drawn on first gaining control of a Fracture planet (rule 15).

## Phase 6 — Entropic scars

9 rules, independent of Phases 2–5, so it can run in parallel.

- **6.1** Unit abilities suppressed inside the anomaly, text abilities unaffected, and text that
  depends on a suppressed unit ability made inert (rules 2, 2.1, 2.2, 2.3).
- **6.2** Abilities from outside cannot target units inside (rule 4).
- **6.3** Wormhole tokens returned rather than placed (rule 5).
- **6.4** The status-phase faction-technology grant, one command token per scar, prerequisites waived,
  ships only (rules 6, 6.1, 6.2, 6.3).

## Phase 7 — Shared content

The largest block by count, the least structurally risky, and highly parallel.

| item | state | note |
|---|---|---|
| action cards | 0 of 142 | the single biggest gap; self-consistent today because nobody has any |
| agendas | 34 of 63 | 29 missing |
| laws | 36 unenforced | a law that passes and does nothing is worse than one never drawn |
| relics | 5 of 24 | 19 missing |
| public objectives | 30 of 40 | 10 missing |
| exploration | 71 of 80 | 9 missing |
| leaders (the six) | 16 of 19 | 3 missing |

Sequence within the phase: **laws → agendas → action cards → objectives → relics → exploration →
leaders**. Laws first because an unenforced law silently changes every subsequent game; action cards
before objectives because several objectives reference card play.

## Phase 8 — Reaction windows

Eleven windows cannot fire, ten of them from one structural cause: combat resolves in whole steps, so
there is no moment between rolling and assigning hits, no event naming a single destroyed unit, and
sustain is applied inside hit assignment rather than announced.

This is the one phase that is a **redesign, not a gap-fill**: it means decomposing combat into
announced steps. It is placed late because Phase 7's action cards are the main consumers of these
windows, and doing it earlier would mean guessing at what they need.

## Phase 9 — Verify the unverified

80 of the 109 topics have code that this audit did not check against rules text. The space-station
precedent says the defect rate among them is not zero.

Worst-first, by how much the opening and mid-game depend on them: **Invasion, Movement, Transport,
Production, Space Combat, Control, Blockaded, Anomalies, Capacity, Fleet Pool**, then the rest.

Each verified topic moves from `?` to `OK` or produces a defect entry in the audit.

## Phase 10 — Wiring for connected decisions

The engine can be correct and still be unlearnable, because a policy sees only what the option set
and the projection expose. This phase is what makes the rest usable.

### 10.1 The engine-side view

`Observed` is the seat's view of the position and is where new context belongs. Precedent from
M10-036: `movable_into` and `Position::imagining` answer "what would this action produce?" by calling
the engine's own functions on a hypothetical, rather than reimplementing the rule. Every accessor
below follows that pattern.

- Coexistence: who coexists where, and what a commit here would start.
- Station control: who holds each station and what sole-occupancy would mean after this move.
- Breakthrough and synergy: the resolved colour set, so research and scoring options are legible.
- Fracture: reachable ingresses, and what lies beyond them.
- Neutral units: strength and composition, as an opponent that is not a player.
- Entropic scars: which of my abilities are suppressed here.

### 10.2 Per-option policy facts

`ti4-policy::projection::action_facts` currently covers activation, movement and commit. Each new
decision type needs facts that **discriminate between the options in the same choice** — the failure
mode measured in run-011, where two movement facts were computed from the active system and so took
one value for the whole choice.

New heads needing facts: coexist-or-fight, bombardment target selection, synergy colour choice,
breakthrough exhaust, ingress movement, and the research choice once synergy exists.

### 10.3 One vocabulary generation, not eight

Every new fact family shifts feature columns and invalidates every existing bundle. **Batch all
projection changes into a single vocabulary regeneration at the end of this phase.** Doing it per
phase would mean up to eight forced restarts from blank weights.

### 10.4 A discrimination gate

Extend the existing per-head discrimination test into a gate that every option-emitting head must
pass: within one choice, at least one fact must differ between options. This is the test that did not
exist before run-011 and cost six thousand updates.

## Phase 11 — Resume training

- **11.1** Regenerate the vocabulary, rebuild the blank bundle, re-baseline Stage 1 on the corrected
  engine.
- **11.2** Restate every historical clearance figure, or retire it.
- **11.3** Reconsider the reward. Deferred by scope, but Stage 2 now has coexistence, breakthroughs,
  synergy and the Fracture in it, and `Progress` carries none of them — a policy paid only for
  victory points and opening progress will ignore mechanics it cannot see itself using. This is the
  first thing to revisit once the engine is correct.

## Sequencing

```
Phase 0  ──▶ Phase 1 ──┬──▶ Phase 2 ──▶ Phase 3 ──▶ Phase 5
                       │                    ▲
                       ├──▶ Phase 4 ────────┘
                       ├──▶ Phase 6            (independent)
                       └──▶ Phase 7            (independent, parallel)
                                    Phase 8 ──▶ (after 7)
                                    Phase 9  ── (continuous)
                                    Phase 10 ──▶ Phase 11
```

Phases 6, 7 and 9 are independent of the Thunder's Edge chain and can proceed alongside it. Phase 10
must come after every phase that adds a decision, because of 10.3.

## Risks

1. **Coexistence touches everything.** It changes what "control" means, and control is read in
   roughly twenty places. Phase 2 is the phase most likely to produce regressions elsewhere, which is
   why it precedes the mechanics that build on it rather than following them.
2. **Phase 8 is a redesign.** Decomposing combat into announced steps will change event ordering and
   therefore fingerprints and any recorded corpus keyed on them.
3. **The unverified 80.** The plan assumes they are mostly correct. The one topic examined properly
   yielded nine defects. Phase 9 may expand the plan rather than close it, and should be started
   early enough to find that out while there is still time to react.
4. **Content volume.** 142 action cards is the largest single number here and the easiest to
   underestimate; each is a small effect with its own timing window, and Phase 8 exists partly
   because some of them cannot work without it.
