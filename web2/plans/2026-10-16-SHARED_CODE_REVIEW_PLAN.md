# Review of the shared code of the demo removal, and its cleanup (2026-10-16)

Status: in progress, steps 1–7 done.

## Context

The demo removal of 2026-10-16 (`plans/2026-10-16-DEMO_REMOVAL_PLAN.md`) changed `ti4-engine`,
`ti4-calc`, `ti4-host`, `ti4-wasm` and `web-shared` in 18 commits, from `aece5a34` to `6afaefb5`.
Every field that they added is read by web2. This plan says which of these changes to keep as they
are, which to fix, and how to fix them.

The rule behind the review: **the engine is the truth**. A rule is decided in `ti4-engine` (or
`ti4-calc` for odds). `ti4-host` projects what the engine says into typed facts. web2 shows the
facts, and asks the engine of the page when it stages something that is not sent yet.

## What is good, and stays

- **`combat::space_cannon_shots`** (`e85e3d94`). The preview of the space cannon offense
  (`MovementFacts.cannon`) and the roll come from one function, and a test checks that the
  preview matches what is rolled.
- **`fleet::FleetCounts::standing`** (`6afaefb5`). The fleet supply and capacity arithmetic is one
  pure function. web2 adds the units it stages and asks the engine (`fleet_standing` query), so
  rules 16 and 37 (dock fighter support, Fighter II, Naalu halves) live only in the engine. **This
  is the pattern for every staged preview.**
- **The payment of a paid research is the player's** (`8f234e73`). It is asked one planet or trade
  good at a time. A technology is offered only when it can be paid while leaving its prerequisite
  specialty planets ready (`research_payable`, `specialties_spent_by_research`). This also fixes
  auto-pay exhausting a specialty planet that a prerequisite needed.
- **`GroundOddsResponse` has the shape of the space odds** (`639a8724`), with `GroundRoll` for the
  battle table, computed in `ti4-calc`.
- **The status-phase gain is part of the one redistribution** (`cf5af3e0`). Each seat gets one
  question.
- **Look-ahead details on strategy-card questions** (Diplomacy, Politics, Construction, Trade,
  Warfare, Imperial, Technology). The first question tells the client what the later questions
  will offer, so the client stages the whole card on one screen. Tests check the hints against the
  real later questions. The constants pulled out along the way (`DIPLOMACY_READIES`,
  `POLITICS_DRAWS`, `POLITICS_LOOKS`, `secret_limit`) stay.
- **`RefusalCode` / `EntryRefusal`** (`d787e5fd`). `code` drives Remove and the footer in
  `web2/src/session/draft.ts`. `entry` finds the refused row after a payment moved.
- **Small typed facts**: `MoveFact.wormholes`, `ActivationFacts.costs_token`, `ResearchFacts`,
  `PayingPlanet.face`, `ProductionFacts.commodities`, `PlanetView.resources`/`influence`, and
  `PlanStep::Pool`/`Purchase`.
- Scenario additions (`invasion_draft`, the damaged dreadnought and the lone PDS of late-game) are
  e2e fixtures and stay.

## What is bad

1. **The look-ahead details are untyped `serde_json`.** `structures`, `redistribution`, `own`,
   `agendas`, `draws`/`looks`, `ready_hints`/`ready_count`, `secrets_held`/`secrets_max`, and the
   `payment` of a research and of a purchase (which also gained `commodities`) are built with
   `json!` in `strategy_cards.rs`, `strategy.rs` and `tokens.rs`. They have no generated TS type,
   so web2 reads unchecked JSON, and a renamed key fails silently. Meanwhile the typed
   `ProductionFacts` describes the same kind of payment.
2. **The odds disagree on unfinished rollouts.** `space_odds` counts an unfinished rollout (no
   winner) as mutual destruction. `ground_odds` counts it as a defender win. web2 adds
   `unresolved_rate` to `mutual_destruction_rate` for space
   (`web2/src/session/select/movement.ts`, `draw:`), which counts unfinished rollouts twice.
3. **The host decides rules that are the engine's.**
   - `MovementFacts.unloadable` (`ti4-host/src/view/tactical.rs`, `fn unloadable`) re-derives
     "why no ship loads this": the command token of 95.5, capacity on the path, and `NotOffered`
     as "unknown".
   - The production `Unavailable` reasons `OnlyAnother`, `TooCostly`, `Banned` and `Barred`
     (`fn production`) rebuild the order of the production window's checks from loose details
     (`only_unit`, `max_unit_cost`, `by_ability`). `Barred` is a guess
     (`barred_capacity >= limit.amount`).
4. **The status-phase redistribution hides an error.** In `Game::finish_status_phase`, when
   `redistribute_tokens` fails, the gained tokens go to the tactic pool and the error is dropped.
5. **Duplicated types and wrappers.**
   - `FleetCountsFact` / `FleetStandingFact` copy `fleet::FleetCounts` / `Standing` field by
     field, with two hand-written conversions.
   - `production::spend_label` and `production::spendable_commodities` are one-line `pub`
     wrappers around private functions.
   - `combat::space_cannon_offense` builds `RerollEntry` in two almost equal `match` arms.
6. **`PublicRoll` flattens the roll subject.** It has five optional top-level fields
   (`unit`, `damaged`, `origin`, `cargo`, `target`) that only apply to rift and space cannon
   rolls. `RollSubject.target` is a `String` in the engine and a `PlayerId` on the wire.
7. **Hard-coded literals.**
   - `PlanStep::Purchase` matches the option ids `"yes"`/`"no"`.
   - `redistribution_after_gain` writes the pool option ids `"tactic_tokens"`, `"fleet_tokens"`
     and `"strategic_tokens"` as literals.
   - `structure_hints` stops after two placements (`take(2)`).
8. **`value_now` in `ti4-host/src/projection/mod.rs` ignores the game's content sources.** It uses
   `ContentStore::embedded()` and `content_types::FULL`.

## Decisions

- **Look-ahead details are typed.** Each one is a Rust struct in the engine, next to the code
  that builds it (`Serialize`, `Deserialize`, `cfg_attr(feature = "ts", derive(ts_rs::TS))`).
  It is put into `Choice.details` with `serde_json::to_value`, so the decision format and the
  recorded hashes do not change. `ti4_wasm::wire_ts` visits every one, and web2 reads them through
  the generated types. A research and a purchase share one `PaymentHints` struct, with the same
  fields as the payment part of `ProductionFacts`.
  - The engine writes a look-ahead with `Choice::hinted(key, &hints)`, and its test reads it back
    with `Choice::hint::<T>(key)`. The struct of a card is named `<What>Hints`. A part that two
    cards share is a struct of its own, `#[serde(flatten)]` into each (`PaymentHints` in
    `ResearchPaymentHints` and `PurchaseHints`); ts-rs inlines a flattened struct, so `wire_ts`
    visits the shared part as a root too.
  - A look-ahead whose keys are top-level keys of the details (Diplomacy's `ready_hints` and
    `ready_count`, Politics' `draws`, `looks` and `agendas`, Imperial's `secrets_held` and
    `secrets_max`) is one struct of each question whose fields are those keys (`ReadyHints`;
    Politics' `PoliticsHints` of the speaker question, `draws` and `looks`, and
    `AgendaPlacementHints` of each agenda placement, `agendas`; Trade's `TradeReplenishHints`
    of the question of the other seats, `commodities` and `own`; Imperial's `ImperialHints` of
    the primary's question and the window of its secondary, `secrets_held` and `secrets_max`).
    The engine writes it with `Choice::hinted_fields(&hints)`, which puts each field into the
    details, and its test reads it back with `Choice::hint_fields::<T>()`.
  - web2 reads a look-ahead with `hintOf<T>(choice, key)` of `select/editor.ts`, or one of
    top-level keys with `hintFieldsOf<T>(choice, ...keys)`, which names every field of `T`;
    `T` is the generated type, and the fixtures of its unit tests are typed with it.
  - `ProductionFacts` keeps `PayingPlanet` (`planet`, not `id`) and leaves out `commodities` at
    0: reusing `PaymentHints` there would change the wire of a production, so it does not.
- **The four rates of both odds are exclusive and sum to 1**: `attacker_win_rate`,
  `defender_win_rate`, `mutual_destruction_rate` and `unresolved_rate`. An unfinished rollout is
  only in `unresolved_rate`. The client decides what it means: a draw in space, the planet held
  for ground combat. `ti4-calc` documents this on both responses.
- **The engine says why it offers nothing.**
  - `ti4-engine/src/transit.rs` gets a function next to the load offers (`CargoWindow::for_ship`):
    `transit::unloadable`. It returns, for each unit that uses capacity in a system that the
    movement starts in or passes, why no ship loads it (`transit::Unloadable`: a command token,
    a route of Myru Vos, no capacity), from the same checks as the load offers.
    `MovementFacts.unloadable` maps that function's result. There is no `NotOffered`.
  - `ProductionWindow` gets a function that returns, for each unit kind that it does not offer,
    the first check that excluded it (technology, supply, blockade, no place, only another, too
    costly, banned, barred). The host maps it to `Unavailable`. `NotOffered` stays only for kinds
    that the window never considered.
- **A failed status-phase redistribution is an error of the step.** `Game::finish_status_phase`
  returns it like every other failed answer. `COMMAND_TOKEN_GAINED` keeps naming the pools that
  grew; this is documented as display only.
- **One type per concept.**
  - `fleet::FleetCounts` and `fleet::Standing` derive the wire traits. `FleetCountsFact` and
    `FleetStandingFact` are deleted, and `fleet_standing` calls `FleetCounts::standing` directly.
  - The private functions behind `spend_label` and `spendable_commodities` become `pub`, and the
    wrappers are deleted.
  - `space_cannon_offense` builds its `RerollEntry` in one place.
- **`PublicRoll.subject: Option<RollSubject>`** replaces the five flat fields. `RollSubject.target`
  is a `PlayerId`. The wire changes, and web2's `movementRolls.ts` reads `roll.subject`.
- **Option ids come from engine constants.** The engine exports the ids of the yes/no of a purchase
  and of the three pools, and `PlanStep` and `redistribution_after_gain` use them.
  `structure_hints` reports every placement it is given.
- **The board projection takes the game's sources.** `project_board_view_full` gets the content
  and sources from its callers, as the facts do, and `value_now` uses them.

## Steps

Each step is one commit: Rust with its tests, the regenerated wire (`cargo wire-web`), and web2
reading the new shape with its unit tests. The gates are those of `web2/AGENTS.md`: before each
commit `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web`, and, since every step changes
`ti4-host` or `ti4-wasm`, `scripts/build-wasm.sh` and `npm test`; then the web2 checks
(`npm run typecheck`, `npm run lint`, `npm run format:check`) and the e2e of the screen it touches.

1. **Odds rates.** Make unresolved exclusive in `space_odds` and `ground_odds`; update the tests
   that check the rates sum to 1. In web2 the space draw is `mutual + unresolved`, and the ground
   "defender holds" is `defender + unresolved`.
2. **Status-phase error.** Return the error of `redistribute_tokens`; add a test with a decider
   that answers an illegal arrangement.
3. **Fleet types.** Derive the wire traits on `FleetCounts`/`Standing`, delete the copies, and
   point `host_api` `fleet_standing` and `web-shared/src/engine/fleet.ts` at the engine types.
4. **Small cleanups.** Delete the `spend_label`/`spendable_commodities` wrappers, give
   `space_cannon_offense` one `RerollEntry` constructor, export the option-id constants and use
   them, remove `take(2)` from `structure_hints`, and make `value_now` use the game's sources.
5. **`PublicRoll.subject`.** Nest the subject, type `target`, regenerate, and update
   `movementRolls.ts` and its tests and the protocol fixtures.
6. **Typed look-ahead details**, one commit per card: Technology and Leadership (`PaymentHints`),
   Construction (`StructureHints`), Warfare (`RedistributionHints`), Diplomacy, Politics, Trade,
   then Imperial. In each, the engine test reads the struct back from `details`, and the web2
   selector imports the generated type in place of its hand-written interface.
7. **Movement: why nothing loads.** Add the engine function, map it in `MovementFacts.unloadable`,
   and remove `Unloadable::NotOffered` and its text in web2.
8. **Production: why a unit is not offered.** Add the `ProductionWindow` function, map it in
   `fn production`, and remove the `only_unit`/`max_unit_cost`/`by_ability` details if nothing else
   reads them.
9. **Regression.** Full `cargo t-web`, the web2 unit tests, and the e2e of the real game. A failing
   e2e is fixed in its own commit before the plan is done.
