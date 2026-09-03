# Stage 2 complete decision contract

## Purpose

Give the learned actor enough legally observable information to make every engine decision without
irreducible observation aliasing. This is not a request to expose `GameState`, add authored utility,
or predict hidden cards. It is a versioned information contract:

```text
actor input = lawful information state
            + typed decision context
            + typed facts for one legal option
```

The engine remains the sole source of legality. The actor ranks only the options the engine offers.
"Complete" means that two decision situations which are strategically different for reasons the
actor can know do not silently collapse to the same input within a declared, tested abstraction.
Every deliberate abstraction collision is recorded; corpus coverage alone is never proof that two
states are equivalent.

## Current position

`STAGE2-OBS-001` substantially improves the option-invariant snapshot. The MLP now sees phase and
initiative context, ready economy, public standing, strategy/technology readiness, objective
progress, aggregate opponent state, board pressure, production/fleet headroom, Mecatol, and public
promissory relationships. Existing tactical work also describes activation, movement, cargo, and
landing candidates.

That is necessary but not yet a complete decision contract:

- `Choice` contains only `player`, free-text `prompt`, and options. It does not carry a stable typed
  reason, timing window, source effect, parent action, or accumulated obligation.
- some consequential engine sites still call the position-less `Decider::choose` path through
  `Table::ask`; an MLP cannot consume even the existing surface there. Setup, tests, and genuinely
  position-free callers must be distinguished from live learned-policy decisions;
- structured option features specialize strategy-card drafting and the main tactical candidates;
  most of the engine's roughly 91 option kinds still rely on generic kind/id/payload words;
- opponent facts are mostly aggregates, so two rivals with different scores, reach, relationships,
  and turn status can exchange properties without the actor noticing;
- the global board is summarized, while only a few option types receive candidate-centred topology,
  composition, reach, defence, and objective-consequence facts;
- the acting seat's own secret objectives are represented, but other private holdings and public
  rule modifiers are not systematically available outside the prompt that directly offers them;
- multi-prompt decisions do not have a common representation for amounts already paid, capacity
  already consumed, units already committed, hits remaining, votes committed, or choices still
  required;
- the critic predates `STAGE2-OBS-001` and is materially narrower than the new actor surface;
- no coverage gate proves that every consequential decision kind is separable and state-sensitive.

## What a bot needs

### 1. Lawful information state

The actor needs all current public facts and all private facts belonging to its seat that can affect
the value of present or future options. The contract should cover:

- round, phase, subphase, timing window, active seat, speaker, initiative, passed seats, and
  once-per-turn/round/activation uses;
- own resources, planets and readiness, command pools, reinforcements, unit composition, technology
  and exhaustion, strategy cards, action cards, promissory notes, relics, fragments, leaders,
  breakthroughs, exploration holdings, and secret-objective progress;
- public laws, agendas and their current outcomes, attachments, tokens, wormholes, anomalies,
  frontier/ingress/breach state, discarded or played public information, and public deck counts;
- each opponent's public score, economy, tokens, technologies, strategy-card state, controlled
  territory, visible forces, passed status, and relationship to the actor;
- actor-relative board topology: home, Mecatol, neighbours, objectives, production sites, activated
  systems, movement paths, enemy response reach, space cannon, capacity, fleet supply, and invasion
  support.

The boundary must be capability-based through `SeatObservation`. Opponent private identities never
enter the actor or critic; only information the rules make public may be represented.

### 2. Typed decision context

The prompt is display text, not a model contract. Every non-forced choice needs bounded, typed
metadata describing why the question exists:

- canonical decision kind and subtype, with a visibility/redaction class for every field;
- source category and transferable source identity where appropriate (strategy card, technology,
  law, ability, action card, agenda, objective, combat step, or generic rule);
- timing window and current action pipeline step;
- parent action/continuation identity without using replay-unstable sequence numbers as features;
- target role (origin, destination, planet, player, outcome, unit, card, or pool);
- outstanding constraint: amount owed, hits remaining, production resources/capacity remaining,
  cargo capacity remaining, votes remaining, picks remaining, or min/max selection count;
- optional/mandatory status and whether declining ends only this window or the enclosing action.

This should be a typed engine structure serialized beside `Choice`, not inferred from prompt tokens.
Stable option IDs remain unchanged. Because canonical fingerprints hash serialized decision records,
adding context is a replay-schema change: it needs a new canonical decision-hash version, backward
reader/default rules, migration fixtures, and an explicit decision about whether each context field
participates in the hash. It must not be described as replay-neutral.

The new actor contract emits no feature derived from free-text `prompt` or `label` wording. The
legacy schema-4 extractor may remain compatibility-frozen, but the versioned MLP projection must
suppress `prompt-kind` as well as the already suppressed unbounded prompt crosses once typed context
is populated. Rewording a prompt or substituting a display-only player/planet/system name must leave
new-contract vectors and logits bit-identical. Because current extraction merges stable `option.id`
and display `option.label` tokens into one `option:` family, migration must first separate their
provenance: approved bounded ID/corpus semantics remain, while label-derived tokens are removed.

### 3. Option semantics and immediate consequences

Every legal option needs facts that answer three questions: what does it consume, what does it
change immediately, and what opportunities does it preserve or close? These must be computed from
the same rules helpers used by legality/application, not duplicated policy logic.

At minimum, cover these clusters:

| Cluster | Required candidate facts |
|---|---|
| Turn/strategy | action category, strategy-card effect/initiative, token cost, pass consequences, remaining usable actions |
| Movement/cargo | exact reachable path properties, movers left behind, capacity before/after, fleet-supply result, transported force, interception/space-cannon exposure |
| Invasion/combat | attacker/defender composition and factual dice distributions, sustain state, hits outstanding, bombardment/AFB/space-cannon context, retreat destination state, planet gain/loss |
| Production/payment | unit stats and counts, marginal resources and capacity, production limit remaining, fleet/capacity result, current debt, overpay, payment flexibility left, objective reserve impact |
| Technology/objectives | prerequisites met/remaining, exhausted discounts, upgrade replacement, exact objective progress delta, point availability and timing limit |
| Trade/promissory | counterpart-relative standing, complete proposed terms, net public inventory delta, transaction limits, Support relationship, binding/nonbinding consequence |
| Agenda | agenda/law semantics, outcome target type, current public votes and predictions, own spend and remaining influence, speaker/tiebreak context |
| Abilities/cards/exploration | source identity and timing, costs, legal targets, once-use state, factual immediate delta, decline consequence |
| Tokens/transit/placement | source and destination state, pool totals after choice, command/fleet constraints, adjacency and ownership consequences |

Prefer before/after deltas and normalized counts over opaque entity IDs. Corpus identities such as a
technology or action-card alias are legitimate bounded vocabulary; transient player, planet, system,
and unit-instance IDs are not.

For stochastic effects, describe the lawful information about the distribution rather than a sampled
future: dice count/faces/modifiers, outcome categories and exact probabilities where public rules
determine them, or draw count and publicly knowable deck composition. Hidden deck order, hidden card
identity, and the eventual random result remain unavailable. Irreducible chance is explicit rather
than encoded as a zero immediate delta.

### 4. Continuation state

A sequence of prompts is one decision process. The actor must know what earlier answers in the same
process committed. Introduce typed public/seat-redacted continuation summaries for:

- tactical action: activated system, movement origins used, units moved/loaded, capacity remaining,
  invasion commitments, and remaining pipeline steps;
- combat: sides, round/step, rolled effects that remain relevant, hits to assign, retreats announced,
  sustain/repair/use flags, and pending destruction;
- production/payment: source, production limit, resources owed/paid, discounts applied, selected
  units, capacity/fleet result, and whether cancellation is still legal;
- strategy cards and component actions: primary/secondary source, selections already made, costs
  already paid, and remaining selections;
- agenda/trade: revealed agenda, public vote ledger, current outcome/target, complete offer/counter,
  promises, and remaining transaction allowance.

If an earlier public event affects rational play but is not reconstructible from current state, add a
bounded public-history summary or persist its consequence in state. Do not add recurrence merely to
recover information the engine could expose directly.

## Representation rules

1. Use one immutable `DecisionObservation` assembled by the engine at ask time from a
   `SeatObservation`; policy code must not regain arbitrary `GameState` access. Every consequential
   live learned-policy ask must use this route. A closed registry identifies genuinely viewless
   setup/offline choices and tests that none can silently expand into live play.
2. Separate namespaces for state, decision context, and option consequence. Old bundles route new
   names through family OOV slots until a new immutable vocabulary generation is published.
   Free-text prompt and label tokens are not an input family in the completed contract.
3. Represent opponents in deterministic actor-relative slots, not player IDs. Candidate ordering:
   relationship first (Support/neighbor/combat counterpart), then initiative rank, then seating
   offset. Test permutation equivariance under a table relabeling.
4. Represent the board candidate-centrically: target plus bounded rings/paths and relevant
   relationships. Do not flatten every system identity into dense columns. `OBS-002` must map any
   strategically relevant distant fact that this abstraction omits and either add a bounded summary
   or record and test the remaining abstraction collision.
5. Use exact engine arithmetic for payments, capacity, production, fleet supply, votes, hits, and
   objective progress. Features describe facts and deltas, never authored desirability.
6. Missing context is explicit (`known`, `not-applicable`, or `unavailable`), never silently encoded
   as factual zero.
7. Actor and critic inventories are separately specified. A centralized/full-information critic is
   an optional ablation, never a way to leak information into actor inference.

## Work packages

Each implementation row should be split further if it exceeds the repository's atomic-package
limits. Timing, hidden-information, schema, and training rows require Tier-C review.

| ID | Objective | Depends | Principal result |
|---|---|---|---|
| OBS-002a | Decision producer and delivery audit | OBS-001 | Inventory every emitted non-forced choice by head/kind/subtype/source and every call to `choose`/`ask`; classify live seat-bound versus genuine setup/offline viewless exceptions. |
| OBS-002b | Rule-dependency and aliasing matrix | OBS-002a | Map every state read by legality/application to state, context, option, hidden, stochastic, or irrelevant; record missing typed payloads and every intentional representation collision. |
| OBS-003a | Typed choice-context schema | OBS-002b | Define versioned `DecisionContext`/`OutstandingConstraint`, field visibility, and canonical serialization without migrating all producers. |
| OBS-003b | Replay/hash migration | OBS-003a | Bump the canonical decision-hash contract, add backward reader/default rules and old/new replay fixtures, and pin which context fields participate in fingerprints. |
| OBS-003c | Seat-bound delivery migration | OBS-002a, OBS-003a | Route all consequential live choices through `DecisionObservation`; enforce a closed registry for genuine viewless setup/offline exceptions. |
| OBS-003d | Tactical/combat context producers | OBS-003b–c | Populate typed context for tactical, invasion, combat, transit, and placement producers without changing legal sets or option IDs. |
| OBS-003e | Economy/strategy context producers | OBS-003b–c | Populate typed context for production, payment, turn, strategy, technology, token, and scoring producers. |
| OBS-003f | Trade context producers | OBS-003b–c | Populate typed context for offers, answers, counters, promises, replenishment, and transaction limits. |
| OBS-003g | Agenda context producers | OBS-003b–c | Populate typed context for agenda outcomes/targets, votes, predictions, speaker, quash, and tiebreak decisions. |
| OBS-003h | Reaction/content context producers | OBS-003b–c | Populate typed context for reactions, cards, abilities, exploration, relics, leaders, breakthroughs, and remaining content producers; use the audit to split by crate if this exceeds five files. |
| OBS-003i | Prompt-free MLP projection | OBS-003d–h, OBS-008a–i | Separate stable option-ID tokens from display-label tokens, suppress all free-text prompt/label input in the new versioned contract, retain the frozen legacy extractor, and prove prompt-rewording/display-identity invariance. |
| OBS-004 | Complete actor-owned inventory | OBS-002b | Extend `SeatObservation` with actor-private holdings and public rule modifiers, with identity/readiness/count features and opponent-mutation leak tests. |
| OBS-005 | Relational public table state | OBS-002b | Replace aggregate-only opponent blindness with deterministic actor-relative opponent slots and relationship/threat facts; prove relabeling equivariance. |
| OBS-006 | Candidate-centred board state | OBS-002b, OBS-005 | Add reusable topology, composition, reach, response, capacity, production, and objective-location facts for board-targeting options. |
| OBS-007a | Preview contract | OBS-002b, OBS-003a | Define bounded factual before/after summaries, stochastic outcome descriptors, unknown/unavailable states, and a fail-closed API that cannot mutate the real game. |
| OBS-007b | Deterministic preview foundation | OBS-007a | Share rules helpers for exact deterministic costs, limits, and immediate deltas; prove preview agrees with application and failed preview is atomic. |
| OBS-007c | Stochastic preview foundation | OBS-007a | Represent public probability/outcome support and irreducible unknown draws without reading hidden deck order or consuming RNG. |
| OBS-008a | Tactical continuation and options | OBS-003d, OBS-006, OBS-007b | Complete activate/move/load/land/transit/placement semantics and tactical commitments. |
| OBS-008b | Combat and invasion options | OBS-003d, OBS-006, OBS-007b–c | Complete combat step, casualty, sustain, retreat, bombardment, and invasion semantics. |
| OBS-008c | Production and payment options | OBS-003e, OBS-004, OBS-007b | Complete marginal build, limit, discount, debt, overpay, payment-flexibility, and post-choice constraint facts. |
| OBS-008d | Strategy, technology and scoring | OBS-003e, OBS-004, OBS-007b–c | Complete turn, primary/secondary, research, token, pass, and objective timing/consequence facts. |
| OBS-008e | Trade options | OBS-003f, OBS-004–005, OBS-007b | Carry complete offer/counter terms, counterpart-relative state, transaction limits, and inventory/relationship consequences. |
| OBS-008f | Agenda options | OBS-003g, OBS-004–005, OBS-007b–c | Complete agenda identity/target/outcome, vote commitment, public vote ledger, prediction, speaker, quash, and tiebreak semantics. |
| OBS-008g | Reactions, abilities and action cards | OBS-003h, OBS-004, OBS-007b–c | Complete timing-source, target, cost, once-use, decline, and stochastic semantics for reactions, faction abilities, and action cards. |
| OBS-008h | Exploration and relic options | OBS-003h, OBS-004, OBS-007b–c | Complete draw/support, deck-information, target, fragment/relic cost, exhaustion, purge, and random-outcome semantics. |
| OBS-008i | Leaders, breakthroughs and remaining content | OBS-003h, OBS-004, OBS-007b–c | Complete leader/breakthrough availability, cost, target, once-use and consequence semantics; close every residual audit row or split it into a named package. |
| OBS-009 | Information-history audit | OBS-004–008i | Identify strategically relevant lawful facts not reconstructible from current state; persist bounded sufficient summaries. Only then decide whether recurrence/belief features merit an ablation. |
| OBS-010 | Critic alignment | OBS-004–009 | Rebuild the option-free critic inventory from the completed actor information state; retain option/legal-set invariance and hidden-information boundaries. |
| OBS-011 | Vocabulary and corpus migration | OBS-003–010 | Discover and publish a new append-only vocabulary, bump the observation contract, update corpus manifests/checksums, and prove old bundles either load with declared OOV semantics or fail with a clear version error. |
| OBS-012 | Decision completeness qualification | OBS-011 | Run counterfactual, separability, hidden-information, equivariance, performance, distillation, and PPO ablations; publish remaining exceptions rather than claiming blanket completeness. |

## Verification gates

### Static contract gate

- 100% of non-forced choice producers declare a typed context and subtype.
- 100% of consequential learned-policy choices are delivered through a seat-bound
  `DecisionObservation`; every viewless exception is named, synthetic/setup/offline only, and tested.
- the completed MLP projection emits no prompt/label-derived feature; changing free-text wording or
  display-only identities leaves features and logits bit-identical.
- 100% of consequential `(head, kind, subtype)` rows have an approved option descriptor or an
  explicit reviewed reason that generic facts are sufficient.
- Every feature is classified as public, actor-private, derived-from-lawful-input, or forbidden.
- Every field read while applying an option is mapped to a represented fact, an engine-only legality
  fact, hidden/stochastic uncertainty, or a documented non-decision-relevant field.
- Serialized decision-context fields have explicit actor visibility and redaction tests before they
  are admitted to records, hashes, corpora, or features.

### Counterfactual gate

Build paired fixtures by changing one lawful fact while holding the legal option IDs fixed. The
expected state/context/option feature must change for at least:

- strategy-card used versus ready;
- opponent passed versus active;
- payment debt and already-paid amount;
- production capacity remaining;
- cargo capacity remaining;
- hits remaining and sustain state;
- public law/agenda outcome;
- own held card/relic/leader availability;
- opponent identity-relative score/reach/Support relation;
- objective progress delta for the offered option.

Conversely, a classified mutation suite must leave both actor and critic vectors bit-identical when
it changes any opponent-private action card, secret objective/progress, facedown promissory note,
private exploration/content holding, private continuation field, or hidden deck order while
preserving every public count and fact. The serialized redacted `DecisionContext` must be invariant
under the same mutations.

For every lawful field classified as decision-relevant by `OBS-002b`, generated counterfactual or
property fixtures must exercise each affected decision class. Corpus observations supplement this
gate; absence from a corpus does not establish equivalence. Deliberately bounded summaries require a
named collision case and an argument/test that the lost distinction is accepted.

### Empirical separability gate

On a fixed, checksummed full-game corpus:

- report every head/kind/subtype, frequency, mean option count, identical-vector groups, distinct
  feature count, OOV rate, and state/context/option sensitivity;
- zero unexplained identical vectors among strategically non-equivalent options;
- zero consequential kinds absent from the corpus without a synthetic fixture;
- no head may rely only on prompt text or transient entity IDs;
- no new-contract vector contains a prompt/label-text family at all;
- compare teacher regret before/after each option cluster, not only aggregate game score.
- stochastic decision classes report whether options have distinct lawful distributions/support;
  draw, exploration, dice, and random-target fixtures must not inspect sampled futures.

Identical vectors are allowed only when a rules-based equivalence test proves the options have the
same immediate state transition and the same continuation state up to stable renaming.

### Learning and performance gate

Pre-register ablations in this order:

1. OBS-001 baseline;
2. + typed decision/continuation context;
3. + actor inventory and relational table state;
4. + complete option consequences;
5. + aligned critic;
6. optional bounded history, centralized critic, recurrence, or entity/graph encoder.

Use the same seeds, rotations, corpus, vocabulary generation, teacher, and training budget. Require
improved held-out action ranking/teacher regret for the targeted heads and no regression in legality,
replay determinism, hidden-information tests, or fixed-workload throughput. Do not move to recurrence
or a graph network until the explicit contract has passed and residual aliasing is measured.

## Recommended execution order

Begin with `OBS-002a/b`, not vocabulary generation or PPO. The audit should produce the decision
contract matrix, ask-path inventory, lawful-field map, and a ranked histogram of real decisions.
Then implement the `OBS-003a–c` schema/hash/delivery foundation, `OBS-004`, and `OBS-007a/b` before
the production/payment slice in `OBS-008c`: recent bugs show that accumulated costs and remaining
capacity are exactly the sort of multi-prompt facts a snapshot misses. Add `OBS-007c` before combat
or other random-effect packages. Follow with tactical/combat, then strategy/scoring, then the
separately reviewable trade, agenda, and content/reaction clusters. Land `OBS-003i` after all typed
context producers exist and before vocabulary publication. Publish the vocabulary only after the
feature families stabilize.

The architecture should remain the current per-option MLP for this sequence. Its shape already
matches the problem; its input contract is incomplete. A set/graph encoder becomes justified only
if the completed candidate-centred representation is too large or residual board aliasing remains
after `OBS-012`.
