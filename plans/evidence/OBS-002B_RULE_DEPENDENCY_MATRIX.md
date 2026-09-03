# OBS-002b — rule-dependency and aliasing matrix

Maps every decision producer to the state its legality and application read, and records which of
those classes reach the actor's observation. Companion to the empirical census in
`OBS-002B_ALIASING_CENSUS.md`, which measures where the observation collapses distinct situations;
this says what it would have to carry not to.

## Method

The producer universe is OBS-002a's reviewed registry: **80 producers across 25 modules**, pinned by
`decision_delivery_inventory`. For each, the function body was extracted and its state reads
classified mechanically, then the consequential classes were verified by reading the code.

Mechanical extraction is used rather than assertion so the matrix can be regenerated when producers
move, and so its coverage is the registry's coverage rather than whatever came to mind. Its limits
are stated at the end.

## What the producers read

| class | producers | what it means |
|---|---:|---|
| board | 44 | systems, planets, control, units |
| actor-own | 34 | the acting seat's holdings, tokens, economy |
| laws-effects | 14 | active laws, agenda outcomes, elected targets, custodians |
| rules-content | 10 | static corpus lookups |
| hidden-other | 9 | action cards, secrets, promissory notes |
| phase-timing | 8 | phase, round, initiative, who has passed |
| stochastic | 4 | dice and random draws |
| objectives | 2 | scoreable and scored objectives |

14 producers read nothing classified: they construct options from their arguments alone.

## What the observation carries

`ChoiceContext` is `seat_facts` (8 values), `own_units`, `objective_facts`, `ability_facts`,
`opponent_facts`, and the tokenised prompt, crossed into per-option features.

| class | in the observation? | finding |
|---|---|---|
| laws-effects | **NO** | `features.rs` contains **zero** occurrences of `laws` or `custodians`. Fourteen producers read this state and none of it reaches the model. |
| phase-timing | **PARTIAL** | `round` is in `seat_facts` and `card:initiative` exists for strategy cards. Phase, initiative order, and which seats have passed are absent. |
| board | **PARTIAL** | `own_units` names the systems holding the actor's units; per-option features describe the option's own target. There is no general composition, adjacency, or opponent-position state. |
| actor-own | **PRESENT BUT LOST** | `seat_facts` carries round, tactic/strategic/fleet tokens and economy. The census proves they do not survive the option-crossing on the `tokens` head: seven distinct seat states arrive as one input. |
| objectives | PRESENT | `objective_facts`, including held-secret progress. |
| rules-content | PRESENT | static, reachable through option ids and kinds. |
| hidden-other | CORRECTLY ABSENT | see below. |
| stochastic | **NO** | four producers roll or draw; no outcome distribution is represented. This is OBS-007c's subject. |

## Hidden information: no leak found

Nine producers touch `action_cards`, `secret_objectives` or `promissory_notes`. Every one was
checked against the acting seat:

- `secrets::enforce_hand_limit`, `action_cards::enforce_hand_limit`, `thunders_edge::pay`,
  `faction_abilities::perform_component`, `game::action_options`, `reactions::slot`,
  `strategy_cards::politics_primary`, `agenda_effects::resolve_with` — all read
  `state.player(player)`, the actor's own holdings, which the actor must know.
- `relics::codex` reads `state.discarded_action_cards`, the public discard.

**No producer builds an actor-visible option set from another seat's hidden state.** The automated
scope heuristic over-flagged seven of these — it matched a field access whose player binding sat on
the previous line — so the clearance is by reading, not by the scan.

## Missing typed payloads

`Choice` carries no typed source or subtype. The learned router therefore falls back to free prompt
text, and `other` is a catch-all conflating scoring, agenda riders, exploration, transit, faction
abilities and card effects. Two decisions with different semantics and different correct answers
reach the same head with the same shape.

This is the direct input to OBS-003a, and it interacts with the census: the prompt reaches the
option-invariant state key in only 1,753 of 3,678 decisions, so the one channel that currently
distinguishes these questions is itself unreliable.

## Intentional collisions

- Option ids are stable identifiers and are deliberately reused across contexts (`decline`,
  `done_moving`); the head plus context is what should separate them, and today the context is the
  prompt.
- `state_cross` deliberately crosses per-seat facts into options, because a linear softmax cannot
  read an option-invariant feature. That crossing is intentional; the census shows it is also where
  the `tokens` head loses its pool counts, so the intent and the effect diverge.

## Ranked consequences

1. **laws-effects, 14 producers, zero features.** The largest hard gap in the matrix.
2. **actor-own present but lost.** 108 of 164 proven aliases. The information exists and is computed;
   the delivery discards it.
3. **phase-timing partial.** Round survives; phase, passed and initiative do not.
4. **stochastic unrepresented.** Four producers.
5. **board partial**, and the largest by producer count, but per-option targeting covers much of what
   those producers actually decide.

## Limits

- The classifier is lexical. It reports what a function body mentions, not what the rules require;
  a producer could read state through a helper and be undercounted.
- Coverage is the OBS-002a registry. A producer absent there is absent here.
- "In the observation" is judged against the current feature families. A family present but inert
  after crossing counts as present in this table and is caught only by the census.
