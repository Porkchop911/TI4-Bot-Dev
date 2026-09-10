# Stage 1A audit: Improve Infrastructure and Explore Deep Space

Date: 2026-09-09. Status: partial — the trace steps below are complete; the opportunity-funnel
instrumentation and the 12-example Train-pool inspections `NEGLECTED_SCORING_PLAN_2026-09-09.md`
§1B calls for are not started. No training was launched. No file outside this one was touched.

## What this is

Stage 1A of `NEGLECTED_SCORING_PLAN_2026-09-09.md`, for its two named routes: "perform Stage 1A for
Improve Infrastructure and Explore Deep Space." Four steps per route: content requirement → engine
predicate; legal options that can satisfy it; whether progress reaches the actor input; completed
requirement → legal scoring option. Read-only tracing throughout — no code was run against a live
game beyond the two existing unit tests cited below.

## Improve Infrastructure (`infrastructure`, priority 1)

**1. Content → predicate.** `content/public_objectives.json`: "Have structures on 3 planets outside
of your home system," 1 point, Status phase, POK. `objectives.rs:917` routes it through
`counting_progress` to `CountFamily::StructuresAway`, threshold 3, `structures_away_count`
(`objectives.rs:654`). The predicate collects `(system, planet)` for every unit this player owns
that `is_structure()`, keeps only those whose system is not `home_system()`, and dedupes on
**planet** via a `BTreeSet` before counting. That matches the card text exactly: it counts distinct
planets, not raw structure units, so a space dock and a PDS on the same away planet do not double
count, which is correct and not a source of undercounting.

No bug found in the predicate.

**2. Legal options.** Structures are placed only through `place_structure` (`strategy_cards.rs:956`),
shared by Construction's primary (two placements) and secondary (one), and by any law that grants the
same ability. `structure_options` (`strategy_cards.rs:921`) offers **every controlled planet**,
home or away, filtered only by "not a space station" and `production::structure_allowed` /
`supply::allowed`. There is no away-from-home restriction, no extra cost, and no separate gate — a
structure secondary on an away planet is exactly as legal and exactly as cheap as one at home.

No bug found in legality. The route is not blocked; it is merely not the placement the policy
currently prefers.

**3. Observation.** `objective_facts` (`ti4-policy/src/features.rs:524`) reads
`Observed::revealed_objective_progress`, which iterates `state.revealed_objectives` and calls
`counting_progress` per card — so this fact exists only once Improve Infrastructure (or Protect the
Border, same family) is actually revealed, which is the correct condition to measure conversion
against. Progress is recorded per `(family, threshold)` pair, not merged across cards sharing a
family: `objective-progress:structures_away:3` (Infrastructure) is a separate feature from
`objective-progress:structures_away:5` (Protect the Border) and from `objective-progress:structures:4`
(Build Defenses). `projection.rs:124` classifies the whole `objective-progress` family as
`Transferable`, so it is not suppressed on the way into the MLP's dense input.

Checked directly against the checkpoint this plan starts from
(`out/checkpoints/arm-vponly/checkpoint-43992/slots.json`): `objective-progress:structures_away`,
`:3` and `:5` are all present as **assigned, non-OOV columns** (85 total lines matching
`objective-progress:structures_away`). The signal is not missing from the vocabulary.

**This rules out the most likely observation bug.** The policy has, in principle, an exact per-decision
read on how many away planets currently carry a structure, distinct from the easier all-planets count
it is visibly already acting on (Build Defenses converts at 71.3% against Infrastructure's 25.0%).

**4. Scoring.** `scoreable()` awards the card once `counting_progress` reports the threshold met and
the status-phase scoring option is offered; this path is exercised by
`infrastructure_needs_structures_outside_home` and neighbouring tests and was not separately
re-derived here.

**Working conclusion.** No engine-legality or observation-vocabulary bug found for Infrastructure.
The gap between Build Defenses (71.3%) and Infrastructure (25.0%) is not explained by the policy
being blind to the requirement — it can see exact away-planet progress whenever the card is revealed.
The leading hypothesis is the plan's own: the policy has learned "place structures" but not "place
this one somewhere that is not already the easiest planet," which is a **missing-experience /
opportunity-cost** question, not a correctness one. That is a hypothesis, not a proof — the
opportunity funnel (§1B) is what would confirm it, and it has not been run.

## Explore Deep Space (`deep_space`, priority 2)

**1. Content → predicate.** "Have units in 3 systems that do not contain planets," 1 point, Status,
POK. Routed to `CountFamily::PlanetlessSystems`, threshold 3, `planetless_systems_count`
(`objectives.rs:691`): counts systems on the board with `!units_of(player).is_empty()` whose
catalogue entry has no planets. Correct reading of the card; no bug found.

**2. Legal options / reachability.** Not fully traced. What is confirmed: the static corpus has
**134 of 231 systems with no planets** (`content/systems.json`) — wormholes, anomalies, asteroid
fields, and plain "Empty System" tiles — so planetless space is not scarce in the tile pool. What is
*not* confirmed here: how many of those land on the specific Train-pool maps this experiment would
use, their distance from a typical home system, and whether normal movement/command-token play
reaches three of them inside a four-round horizon without diverting from expansion. The plan's
0.0% measured conversion is the strongest single signal in the whole priority table and deserves
that reachability trace before anything else in this route.

**3. Observation.** Same mechanism as Infrastructure: `objective-progress:planetless_systems`,
`:3` and `:5` are present and non-OOV in the same checkpoint's `slots.json`. Not a missing-vocabulary
bug either, conditional on reveal.

**4. Existing test is not evidence of a reachable route.** `deep_space_counts_systems_without_planets`
(`objectives.rs:3239`) proves the *predicate* is correct by injecting cruisers directly into three
planetless systems' unit lists — it never moves a ship, spends a command token, or checks activation.
The plan calls this out by name as "a starting point, not proof that a normal rollout can reach the
state," and the code confirms that reading: there is no legal-play trace here at all, only a counting
trace. **This is the one open question worth resolving before anything else in this route**: is 0.0%
"the policy never bothers" or "the policy is never offered three reachable planetless systems inside
a Train map's fleet-supply and movement-range envelope"? Those have different fixes and this audit
does not yet distinguish them.

## What Stage 1A this covers, and what it does not

Covered: step 1 (predicate correctness) and step 3 (vocabulary presence) for both routes, and step 2
(legality) for Infrastructure. Both routes clear the two failure modes an observation/vocabulary
gate would predict — the checkpoint can see the exact fact it would need.

Not covered, and each is real work rather than a formality:

- Explore Deep Space's movement/activation reachability on the actual Train map pool (step 2).
- §1B's opportunity funnel: reveal round, requirement progress at reveal and at scoring windows,
  eligible/offered/selected/awarded as separate fields, competing-objective and allowance state.
  This needs new per-decision instrumentation on recorded trajectories, not static code reading.
- §1B's 12 inspected Train-pool failures per route, spread across factions and reveal times, each
  with a proposed low-cost completion checked against legality/observation at that state — the part
  of Stage 1 that would actually confirm or refute "missing experience" against "engine cannot get
  there."
- Build Defenses / Fuel the War Machine overlap, and the priority-3/4/5 routes, are untouched.

Per the plan's own gate: **do not start a training campaign** on either route until the funnel and
the 12-example inspection are done. Nothing here clears that gate yet.

## Files touched

None outside this one. All 56 pre-existing dirty/untracked paths in `git status` at the start of
this audit were read, where relevant, but not modified, staged, or reverted.
