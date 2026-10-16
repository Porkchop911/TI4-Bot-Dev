# Remove the demo of web2 (`src/mock/`): the rest of the work (2026-10-16)

Status: in progress.

- Done (2026-10-16): the picker and the generic decisions, the reaction window, every strategy card (with the variants of the card sets) and the payment of a research, the cleanup of the editors, and every draft example. The steps are in `git log --oneline 8b86649e~1..HEAD`.
- Combat slice 6a: done (2026-10-16, `82a7d4b2`).
- Open: the rest of combat (6b to 6g), the Technology sheet, the removal of the demo, and the full regression.

## Context

`web2/src/mock/` is a demo (`?example=<id>`, `src/mock/exampleList.ts`). It feeds hand-built view models to the real components of `src/game/`. `src/session/` builds the same view models from the engine. Five examples are left: `draft-combat`, `live-combat`, `live-combat-full`, `live-invasion` and `summary`. The demo goes when the real game shows what serves the engine's options.

## Decisions (owner, 2026-10-16)

- **The goal is that every option the engine gives is in the UI, with a good UX.** The demo is one example of a good UX, not a list of features. Build generic shapes driven by the question kind and its details: a pick on the map, a pick of a seat in the player table, a pick of a unit, the reaction band (`interruptOf`), counters, payment on the map. A screen for one card, or a rule of a card written in web2, is not built. A card-specific screen is only allowed where the engine sends the data for it.
- **The engine is the source of truth, also against the demo.** The demo is hand-written and is wrong in places (Construction, for example).
  - Where the engine lacks data, the feature is left out and noted in `NEXT.md`. An additive, display-only fact of the engine is the way to add it.
  - Where the engine plays a rule wrongly against the printed card, web2 does not work around it: it is noted in `NEXT.md`. The rule bugs found so far are left for later.
- **An editor is chosen by the question kind and `Choice.details`, never by the id or the number of a card.** The engine plays four sets of strategy cards (pok, base_game, base_game_codex1, te), and every variant gets the behaviour of the editor whose questions it shares.
- **Step 7 of the first plan is skipped**, except the Technology sheet: no completed-action view and no past steps, no toasts, and no Log. The `summary` example is removed with no real counterpart. The Log sheet is not built and not deleted: the owner decides later.
- **The Technology sheet answers "Who has what"** (the rules of a sheet are in `docs/agents/sheets.md`).
- **Combat:**
  - improve the UX over the demo and over web/;
  - not too dense: what the current choice needs, in game values, one signal per state; the details (each die, the modifiers, the breakdown of the odds) behind hover, a click or a disclosure;
  - every fact of the desktop at most one tap away on a phone. A unit shown only as an icon with its name as a tooltip is not enough on a phone.
- **Units are named as their card prints them** (`unitLabel`, `Catalog.records.units`). The numbers of a combat come from `ti4-calc`, through the engine query of the page.

## How the work is done

- **One subagent and one commit for each unit of work** (one strategy card, one combat slice). The units run one after another, never in parallel. Each subagent gets the brief below and the section of its step.
- **Each subagent runs only the tests of its feature:**
  - typecheck, lint, format:check
  - the unit tests it touched or added
  - `npm run e2e -- -g "<its tests>"`, and each changed demo or entry test on its own
  - with Rust: `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web` filtered to what it touched, and `scripts/build-wasm.sh` when the engine changed
  - with a change of a wire type: `cargo wire-web`, then web/'s `npx tsc --noEmit` and `npm test`
- **The full regression runs once, at the end** (step R). The rules for each commit in `AGENTS.md` ("the whole `npm run e2e` before the commit") are relaxed for this plan by the owner.
- **A failing or flaky e2e test:** a separate fix subagent fixes it before the next step. If the fix is hard, the test is skipped with its reason and reported.
- **No fetch, rebase or push** unless the owner asks. The local `dev` and `origin/dev` drift apart while another session works on `dev`.
- **Rust builds:** only the aliases (`cargo l-web`, `t-web`, `b-web`, `wire-web`), never `-p`, one build or suite at a time.

### The brief of a subagent

Each subagent reads `web2/AGENTS.md`, the topic files of `docs/agents/` for its feature, and the `CLAUDE.md` of each directory it touches. In addition:

- The rules of the Decisions above.
- Each screen is reached in a real game: a scenario (`crates/ti4-host/src/view/scenario/`; three-seat helpers `strategy_card()`, `deal`, `release` in `three_seats.rs`) or a seeded local game.
- e2e go in `e2e/local.spec.ts` and `e2e/local.phone.spec.ts`, with the two layout checks: no vertical scroll of the open step at 1920×1080, and nothing cut at the side on a Pixel 9. Reuse the helpers: `e2e/actions.ts` (`activateInLive`), and `pageErrors`, `seatRow`, `mapPlanet`, `playStrategyCard`.
- The demo examples that the work replaces are removed: from `exampleList.ts`, with their mock-only code and their shot tests. The rest of the demo keeps working.
- The topic doc and `NEXT.md` are updated, as decisions and not as history.
- The commit message follows the recent commits and ends with the `Co-Authored-By` line of the session.
- The report says: the commit, what a player gets, the demo examples removed, the findings in the engine, what was left out and why, and the tests that were run.

## What is there to build on

- **Picker and decisions:** `session/select/turn.ts`, `action.ts` (the picker, the picks on the map and in the player table).
- **Reactions:** `select/reaction.ts` (`interruptOf`: the reaction band).
- **Strategy cards:** `select/editors.ts`, `editor.ts`. They are sent with `Transport.applyDraft`, or with the `pool`/`purchase` plan steps for Leadership.
- **Payment on the map:** `select/payment.ts`, `PaymentBar.tsx`.
- **Rolls:** `session/rolls.ts` (the public rolls of the action), `select/movementRolls.ts`.
- **Odds:** `groundOdds.ts`, `movementOdds.ts`, `CombatOdds`.
- **Battle components:** `game/combat/BattleTable.tsx`, `BattleSteps.tsx`.
- **Draft:** `draft.ts` (`reactivate`, `reviewOf`, `removeEntry`), `select/review.ts`.
- **Facts of the engine:**
  - `MovementFacts.unloadable` and `.cannon`
  - `MoveFact.wormholes`
  - `PublicRoll`, with the unit, the origin, the cargo and the seat fired at
  - `GroundOddsResponse`: the whole outcome, with the rolls
  - `DraftStop::Refused` with its entry and code
  - `fleet_standing`
  - `PlanetView.resources` and `.influence`
- **The data of a combat, already sent:**
  - `BoardView.combat` (`CombatView`) and `BoardView.invasion` (`InvasionView`)
  - the question kinds: `assign_casualty`, `combat_sustain`, `retreat`/`retreat_to`, `anti_fighter_barrage`, `space_cannon`, `bombardment_target`, `assign_ground_casualty`, `fight_ground_combat_round`
  - web/ reads them in `SpaceCombatOverlay.tsx`, `InvasionOverlay.tsx` and `CombatResultSummary.tsx`.

## Plan (one subagent and one commit per step, in order)

### Step 6a: the space combat, read only

Done (`82a7d4b2`).

- **What it built:** the "Space combat" tab of the tactical action, for every seat (read only for the seats that do not fight). It has one battle table from `round_start` (printed names, counts, hits on, damaged), the round and the stage, and who the game waits for. The viewer's question is answered as a list above the table, and a reaction window is the band.
- **Where to see it:** scenario `ongoing_combat`. For a watching seat: Dev "Other seats: Step", then "Play as" Seat B.
- **The pieces for 6b to 6e:**
  - `session/select/combat.ts`: `selectCombat(update, combat, flow, asked, printed)`, `roundTable`, `sideRows`, `stageOf`, `waitingFor`.
  - `select/tactical.ts`: `CombatStep {kind: "combat", combat, flow, asked}` and `TacticalStep = TacticalFacts | CombatStep`.
  - `select/shell.ts` builds the `CombatStep` when `board.combat` is set, there are no step facts and no draft is open.
  - The question of the viewer is `CombatView.decision {prompt, blocks}` (`model/action.ts`), shown by `Blocks` in `SpaceCombatStep` (`game/steps/BattleSteps.tsx`).
  - Reaction windows come through `flow.interrupt`.
  - `useLiveSession.ts` `selectStep` lets the Movement tab be read during a combat.
- **Engine findings:**
  - Before the roll there is no hit value after the modifiers: a row shows the printed one. The engine computes it in `fleet_groups`; adding it to `CombatView` fixes this (additive).
  - The engine rolls the ships of one value together, and a die names all of them (`unit: "sol_carrier+destroyer+fighter"`). So dice cannot be put on single rows.
- **Not checked:** 8 seats with the long names, because no combat scenario has them.

### Step 6b: dice and hits

The dice of a round, from `CombatView.dice_rolls` and the public rolls.

- The engine rolls by group of hit value (see 6a), so the dice are shown for each roll group and fill `BattleRowView.dice/hits/scored`. If a die per row is wanted, the engine names the unit of each die (additive).
- The engine adds the hit value after the modifiers to `CombatView` (from `fleet_groups`), so that a row shows it before the roll. `assign_casualty` and `combat_sustain` become `row.assign`, with a `stageHit` handler in `useLiveSession`, and leave `CombatView.decision`. The assignment of hits is on the rows: a row is the control, with "now → after" and the count of hits that are left. Every casualty option of the engine can be chosen, also sustain damage and the units that an ability protects.

### Step 6c: before round 1

Space cannon offense against the combat and anti-fighter barrage, from `barrage_start`, `barrage_dice` and `barrage_hits`, with the hits assigned as in 6b. They are records before the round in `records` (anti-fighter barrage, and the space cannon offense with `cannonTable` of `select/movementRolls.ts`).

### Step 6d: every option in a combat

Every option that the engine offers during a combat is available, with a good UX, driven by the question kind and its details: the reaction windows of action cards and abilities (the reaction band of step 2), and the retreat (picked on the map when its options are systems, through `boardPick`). There is no screen for one card.

- The combat windows get their words in `triggerText` (today it words them by the name of the event).
- `CombatView.offers` holds the notes and the offers of the open stage.

### Step 6e: outcome and draft boundary

- **The outcome:** `CombatView.outcome`, `round_start` against the units that are left.
- **In a draft:** the combat tab is where the draft stops ("Preview ends here", not "skipped", `tactical.ts`), with a preview of round 1 from `BattleFacts` and `spaceOdds`, as the `odds` of the record. `draft-combat` leaves the demo.
- **Together with 6a to 6d:** `live-combat` and `live-combat-full` leave the demo.

### Step 6f: the invasion

Bombardment (`bombardment_target`), space cannon defense, the rounds of ground combat (`assign_ground_casualty`) and the control of a planet become `InvasionPlanetView`.

- `last_step` holds only the latest step, so the session keeps the earlier steps from update to update.
- If that is not enough, the engine adds the list of the steps (additive).
- The ground odds of `ti4-calc` (the whole outcome) are there.
- `live-invasion` leaves the demo.

### Step 6g: the strength of a fleet

The combat strength of each fleet for the map view "Space combat" (an additive fact of the engine or of `ti4-calc`; `strength: 0` today in `session/select/board.ts`). web2 does not compute it.

### Step 7: the Technology sheet

A reference sheet that answers "Who has what": every technology by colour and tier against the seats, so that players are compared at a glance (who has Gravity Drive, who can reach the War Sun). The seats are the filter, as in the Objectives sheet. The data is the `research` fact and `Catalog.records.technologies`. The rules of `docs/agents/sheets.md` hold; the owner looks at it on a phone.

### Step 8: remove the demo

- **The code:** `src/mock/`, `src/dev/DemoApp.tsx`, `?example` in `src/main.tsx`, the "Demo" link, and the fallback to `<DemoApp notice>` in `LocalApp.tsx` and `SingleApp.tsx` (replaced by a plain error).
- **The tests and the config:**
  - the layer `mock` of `boundaries.test.ts`
  - `e2e/shots.spec.ts`, `e2e/phone.spec.ts`, and the demo cases of `e2e/entry.spec.ts` and `localGame.test.ts`
  - the projects `shots` and `shots-phone`, and `DEMO_OR_PHONE`, in `playwright.config.ts`
  - `npm run shots`, and `shots/web2` and `shots/phone`
- **First, these checks of the phone move to `local.phone.spec.ts`:** the three panes, the pinch zoom, the reference menu, the link to the other app, and "installable".
- **What stays dead is removed:** the past steps, `uncommitted`, `toast`, `reveal`, `announcement`, and the generic `cards` sections. Every real reference sheet and the Log sheet stay.
- **The docs:** `AGENTS.md` (pattern 13 and the shots projects), `docs/agents/testing.md` and `NEXT.md`.

### Step R: the full regression

- **What runs, once:**
  - web2: `npm test`, `npm run e2e`, `npm run e2e:slow`
  - Rust: `cargo t-web`, `cargo t-web-slow`
  - web/: `npx tsc --noEmit`, `npm test`
- **A fix subagent for each failure.**
- **Known failures:**
  - the slow test "a game of eight is played through the decision list": it answers each screen by its first row, so it fails on the editors since the pool editor (Leadership's pools, Politics' two placements)
  - the slow test "a reload goes on… @slow": 24 saved answers instead of 20, because of Leadership's pools
  - the server test `lobby_settings::only_the_host_changes_the_settings_and_only_before_the_start`, which is flaky in a full `t-web`

## Left for later (in `NEXT.md`)

- **The rule bugs of the engine:**
  - the base game Diplomacy (`base2`) is played as Codex I
  - the order of two agendas on one side of the deck cannot be chosen
  - the secondary of Construction (pok and base) does not place its token
  - Construction of Thunder's Edge offers PRODUCTION in any system
  - Trade replenishes the chosen players instead of giving them the secondary for free
  - Warfare of pok skips the redistribution without a token on the board
  - guns fire at a system where the moving seat has no ships
  - The Triad is counted as money but never offered in a payment
- **The engine changes that would most simplify the UI:** the section in `NEXT.md` written by the cleanup.
- **Scenario setup:** `ongoing_invasion_coexistence` and `invasion_draft` give Sol generic units.

## Verification

- **After each step:** the screen of the removed examples is in a local game or a scenario, with the rules of the engine where they differ from the demo. It is checked at 1920×1080 with 8 players and the long names, and on a Pixel 9. The tests of the feature are green.
- **At the end:**
  - `grep -r "mock/" web2/src web2/e2e` is empty
  - the full regression of step R is green on desktop and phone
  - web/'s gates pass
