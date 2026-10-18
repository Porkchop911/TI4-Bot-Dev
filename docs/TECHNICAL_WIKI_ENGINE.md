---
type: Component
title: Engine and content
description: Trace component records, legal choices, game transitions, and tactical resolution.
timestamp: 2026-10-18
source_commit: 1f3556d1
confidence: high
resource: docs/TECHNICAL_WIKI_ENGINE.md
citations:
  - crates/ti4-engine/src/choice.rs
  - crates/ti4-engine/src/game.rs
  - crates/ti4-content/src/loader.rs
---

# Engine and content

[Technical wiki](TECHNICAL_WIKI.md) · [Bots](TECHNICAL_WIKI_BOTS.md) · [Interfaces](TECHNICAL_WIKI_INTERFACES.md)

Use this route for a wrong or missing legal option, a transition, or a content mismatch. Source scope: `wp/base-factions` at `1f3556d1`, inspected 2026-10-18. The corpus and the executing rule are different evidence.

| Question | Start here |
|---|---|
| What is stored and visible to a seat? | [model state](../crates/ti4-model/src/state.rs), [IDs](../crates/ti4-model/src/id.rs), [views](../crates/ti4-model/src/view.rs) |
| Where does a component record come from? | [content loader](../crates/ti4-content/src/loader.rs), [validator](../crates/ti4-content/src/validator.rs), [content files](../crates/ti4-content/content) |
| Who offers a choice and validates the answer? | [game](../crates/ti4-engine/src/game.rs), [choice](../crates/ti4-engine/src/choice.rs), [timing](../crates/ti4-engine/src/timing.rs) |
| How is a tactical action resolved? | [tactical orchestration](../crates/ti4-engine/src/tactical.rs), [movement](../crates/ti4-engine/src/movement.rs), [combat](../crates/ti4-engine/src/combat.rs), [invasion](../crates/ti4-engine/src/invasion.rs), [production](../crates/ti4-engine/src/production.rs) |
| Why was payment or supply refused? | [payment](../crates/ti4-engine/src/payment.rs), [supply](../crates/ti4-engine/src/supply.rs), [fleet](../crates/ti4-engine/src/fleet.rs) |
| How do cards, objectives or factions apply? | [strategy cards](../crates/ti4-engine/src/strategy_cards.rs), [objectives](../crates/ti4-engine/src/objectives.rs), [agenda](../crates/ti4-engine/src/agenda.rs), [factions](../crates/ti4-engine/src/factions) |
| Where are combat odds computed separately? | [calculator arena](../crates/ti4-calc/src/arena.rs), [odds](../crates/ti4-calc/src/odds.rs) |

## The path of one decision

1. A setup selects a source set and game state. The [loader](../crates/ti4-content/src/loader.rs) embeds the component files and filters records by source; [model types](../crates/ti4-model/src/lib.rs) hold state and typed IDs. The [validator](../crates/ti4-content/src/validator.rs) checks references used by the game, with explicitly recorded gaps.
2. [Game::step](../crates/ti4-engine/src/game.rs) advances the active phase or window. Its ordering matters: an open window can interrupt and resume another action. It also reconciles derived state, including space station control and map effects, before continuing.
3. A rule constructs a [Choice](../crates/ti4-engine/src/choice.rs) for one player. Options have IDs, kinds, labels and payloads. The ordered option list is the immediate legality boundary; a UI label is not an ID.
4. The table asks a [Decider](../crates/ti4-engine/src/choice.rs). Where a full position is available it calls `choose_seeing` with a `SeatObservation`; elsewhere the default `choose` path applies. A decider does not receive an unrestricted `GameState` through this API.
5. [Choice validation](../crates/ti4-engine/src/choice.rs) accepts an offered ID, or a permitted constructed free-form answer, and rejects an invented one. The rule then resolves the chosen option and the [decision log](../crates/ti4-engine/src/choice.rs) records what happened. Identical asks within one step have a 500-repeat guard so a stalled decision cycle becomes an error.

For a missing option, inspect the rule that **constructed the offer** before the resolver. For an option that is offered but has the wrong effect, follow the resolver and the next `Game::step`. For an option that exists in the engine but is missing on screen, move to [host and projection](TECHNICAL_WIKI_INTERFACES.md). [Choice definitions](../crates/ti4-engine/src/choice.rs), [game driver](../crates/ti4-engine/src/game.rs), and [projection](../crates/ti4-view/src/projection.rs) are separate evidence.

## Content is input, not executable rules

The embedded [JSON corpus](../crates/ti4-content/content) carries components and upstream fields. The [loader](../crates/ti4-content/src/loader.rs) keeps file order where it matters to seeded decks; the [validator](../crates/ti4-content/src/validator.rs) checks references in and out of the active source set. Neither proves the engine interprets every printed effect. When a faction card or objective looks wrong, compare the record and source set with the specific [rule module](../crates/ti4-engine/src/lib.rs), then reproduce the offer and effect. Keep official rule citations in the strategy vault, per [repository instructions](../AGENTS.md#llm-wiki-workflow).

## Tactical example: activation to production

The [tactical orchestrator](../crates/ti4-engine/src/tactical.rs) coordinates substeps. [Movement](../crates/ti4-engine/src/movement.rs) decides what can travel; [combat](../crates/ti4-engine/src/combat.rs) and [invasion](../crates/ti4-engine/src/invasion.rs) resolve fights; [production](../crates/ti4-engine/src/production.rs), [payment](../crates/ti4-engine/src/payment.rs), [supply](../crates/ti4-engine/src/supply.rs) and [fleet](../crates/ti4-engine/src/fleet.rs) constrain the final build. A failed build can therefore be caused by the offer, available spend, capacity, supply, or the transition into production. Check the actual choice and the first failing constraint instead of treating the entire tactical action as one function.

The [calculator](../crates/ti4-calc/src/lib.rs) is a separate odds facility. An odds estimate is advice about a possible combat, not the engine's resolution of a submitted choice.

## When a replay differs

The [game snapshot](../crates/ti4-engine/src/game.rs) carries per-game state; [host step snapshots](../crates/ti4-host/src/step_snapshot.rs) are captured only between steps and are accepted only for a matching history generation and decision-log prefix after RNG sync. Otherwise the host falls back to full replay. Compare offered IDs and order, decision records, final state, events, timing and random stream with the [fork equivalence test](../crates/ti4-server/tests/it/fork_equivalence.rs). Adding mutable state to a rule means checking both the live path and the fork path; see [invariants](TECHNICAL_WIKI_INVARIANTS.md).
