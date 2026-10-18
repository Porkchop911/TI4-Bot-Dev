# Engine and content: from a card to a legal decision

[Repo guide](REPO_GUIDE.md) · [Bots](REPO_BOTS.md) · [Interfaces](REPO_INTERFACES.md)

Use this route when a rule behaves incorrectly, an option is missing, or you need to understand how state changes. Source scope: `rule-bot`, inspected 2026-10-15; links are navigation evidence, not conformance certification.

## The three foundations

| Crate | Responsibility / first files |
|---|---|
| [ti4-model](../../crates/ti4-model/src/lib.rs) | Typed IDs, state, units, schema contracts and redacted views. Begin with [state.rs](../../crates/ti4-model/src/state.rs), [id.rs](../../crates/ti4-model/src/id.rs) and [view.rs](../../crates/ti4-model/src/view.rs) |
| [ti4-content](../../crates/ti4-content/src/lib.rs) | Corpus loading, provenance and referential validation. Read [loader.rs](../../crates/ti4-content/src/loader.rs), [validator.rs](../../crates/ti4-content/src/validator.rs), then the relevant [content record](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-content/content) |
| [ti4-engine](../../crates/ti4-engine/src/lib.rs) | Game orchestration, rule modules, decisions, timing and transitions. Begin with [game.rs](../../crates/ti4-engine/src/game.rs), [choice.rs](../../crates/ti4-engine/src/choice.rs) and [timing.rs](../../crates/ti4-engine/src/timing.rs) |

Component text and execution are different sources. A technology record describes content; the module that offers and resolves its use determines implemented engine behavior. The [wiki component index](components.md) helps locate stable IDs and corpus text; the [rules reference](RULES_REFERENCE.md) states its official-rules limits.

## Reading route: a tactical action

Start with [tactical.rs](../../crates/ti4-engine/src/tactical.rs) for orchestration. Follow the part relevant to the symptom:

| Question | Source to inspect |
|---|---|
| Can the fleet reach this system? | [movement.rs](../../crates/ti4-engine/src/movement.rs), [fleet.rs](../../crates/ti4-engine/src/fleet.rs) |
| What happens in space combat? | [combat.rs](../../crates/ti4-engine/src/combat.rs), [dice.rs](../../crates/ti4-engine/src/dice.rs) |
| Which ground forces can land and fight? | [invasion.rs](../../crates/ti4-engine/src/invasion.rs) |
| What can be built and paid for? | [production.rs](../../crates/ti4-engine/src/production.rs), [payment.rs](../../crates/ti4-engine/src/payment.rs), [supply.rs](../../crates/ti4-engine/src/supply.rs) |
| When does an ability or reaction apply? | [timing.rs](../../crates/ti4-engine/src/timing.rs), [trigger.rs](../../crates/ti4-engine/src/trigger.rs), [reactions.rs](../../crates/ti4-engine/src/reactions.rs) |

Trace the offered choice and its resolution together. [choice.rs](../../crates/ti4-engine/src/choice.rs) contains `Choice`, `ChoiceOption`, `Decider`, `Table` and seat-observation boundaries. A UI label or bot score can be wrong even when the underlying legal options are correct, so continue into [interfaces](REPO_INTERFACES.md) or [bots](REPO_BOTS.md) when the error begins outside rule generation.

## Other rules paths

- Scoring and public/secret requirements: [objectives.rs](../../crates/ti4-engine/src/objectives.rs), [secrets.rs](../../crates/ti4-engine/src/secrets.rs), [status.rs](../../crates/ti4-engine/src/status.rs).
- Research and strategy cards: [technology.rs](../../crates/ti4-engine/src/technology.rs), [strategy_cards.rs](../../crates/ti4-engine/src/strategy_cards.rs).
- Deals and diplomacy: [transactions.rs](../../crates/ti4-engine/src/transactions.rs), [diplomacy/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-engine/src/diplomacy), [promissory.rs](../../crates/ti4-engine/src/promissory.rs).
- Agendas and laws: [agenda.rs](../../crates/ti4-engine/src/agenda.rs), [agenda_effects.rs](../../crates/ti4-engine/src/agenda_effects.rs), [laws.rs](../../crates/ti4-engine/src/laws.rs).
- Faction and leader behavior: [factions/](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-engine/src/factions), [faction_abilities.rs](../../crates/ti4-engine/src/faction_abilities.rs), [leaders.rs](../../crates/ti4-engine/src/leaders.rs).
- Expansion-specific mechanics: [breakthroughs.rs](../../crates/ti4-engine/src/breakthroughs.rs), [synergy.rs](../../crates/ti4-engine/src/synergy.rs), [fracture.rs](../../crates/ti4-engine/src/fracture.rs), [coexistence.rs](../../crates/ti4-engine/src/coexistence.rs).

## State copies and checks

Read [the repository fork rule](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/AGENTS.md#fork-rule-new-per-game-state-must-be-copied-by-gamefork) before adding state. `Game::fork`/snapshots, [the server's step snapshot](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/src/session/step_snapshot.rs), and [fork equivalence tests](https://github.com/Porkchop911/TI4-Bot-Dev/blob/592e571668239ebc25d3b579b0ec46c1ce01f1eb/crates/ti4-server/tests/fork_equivalence.rs) matter when a live game and a simulated branch disagree. Tests live both beside implementation and in each crate's `tests/`; see [verification routes](REPO_WORKFLOWS.md).
