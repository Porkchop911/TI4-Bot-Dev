# web2: the odds of a space combat in the movement, and the details of a system on every click

Status: built, 2026-10-14. Phases 1 to 4 are built.

## Goal

What the owner asked for (2026-10-14):

1. **The odds of a space combat from `ti4-calc`, in the movement step of a tactical action**, in two places:
   - the summary of the panel (the card "Arrives in …");
   - the sheet of the active system that opens with a click on the map (the inspector with "Arrives").
2. **Only these values:** Win, Draw and Loss in %, and the ships that are expected to survive.
3. **The details of a system are not shown on a click** in the activation and in the movement. This is fixed in the same way on every screen.

## Decisions (owner, 2026-10-14)

1. **Survivors:** the ships that are expected to survive are shown by kind, for both sides ("Dreadnought 1.8 / 2").
2. **The fleet sheet holds the details of its system.** When a click opens the fleet sheet of a system in the movement, the sheet has the same details as the inspector: planets with `res/inf`, `PlanetTraits`, the owner, and the forces of other seats.
3. **The rule for every screen:** a click on a system shows what is in it. That is the inspector, or the sheet of the task at that system, which holds the same details. In the activation the click also chooses the system.

## Non-goals

- The odds of a ground combat.
- Action cards in the odds.
- Odds in the activation step.
- The demo (`src/mock/`): it keeps its own fake odds.

## What is there now

- **`ti4-calc`** (`crates/ti4-calc/src/odds.rs`) depends on ti4-model, ti4-content and serde only, so it builds for wasm.
  - `resolve_side` reads a side from a `GameState`: the ships in the system, the damaged ones, and the space cannon on its planets and in adjacent systems.
  - `space_odds` gives a `BattleResponse`: the win rates, mutual destruction, and the expected survivors and the fielded ships by unit id.
  - The method is Monte Carlo with a seed: 2000 rollouts by default, the same numbers for the same seed.
- **The real game has no odds.** `OddsView` and `BattleTable` with odds exist only for the demo (`src/mock/engine.ts` `E.odds`).
- **The movement facts do not include the other seats.** `MovementFacts` (`ti4-view/src/tactical.rs`) has the asked seat's ships outside the active system only. The staged movement exists only in the page while in Live.
- **Both pages already load `ti4.wasm`:** LocalApp, SingleApp and OnlineApp (`loadEngine`). A stateless export can be asked with the `ask` pattern of `wasmTransport.ts`, as `setupPreviewOf` is.
- **Why a click shows nothing:**
  - In the activation, `toggleSystem` (`useLiveSession.ts`) only stages the system. It never sets `inspected`.
  - In the movement, an origin opens `OriginSheet`, and `select/board.ts` (the `inspector` decision) drops the inspector on purpose.

## How the odds are computed

The odds are computed in the page, through a stateless export of the engine. The same code runs online and locally, because both pages already have the engine.

- **The engine says what only the state knows**, as a fact of the movement (`MovementFacts.battle`):
  - the defender;
  - the ships of both sides that are already in the active system, and which are damaged;
  - the space cannon of both sides.
- **The page adds the ships that the staged movement brings** to the attacker, then asks `ti4_space_odds`.
- **The seed is fixed**, so the numbers do not change while nothing changes.
- **Draw** is mutual destruction plus the rollouts that do not end.

## Phases

### Phase 1: the details of a system on every click (web2 only)

1. **Activation.** `toggleSystem` and `findSystem` stage the system **and** set `inspected` to it, as the demo does. Closing the inspector does not take the chosen system back.
2. **A shared component.** Pull the body of `SystemInspector` (`game/board/Board.tsx`: facts, notes, rows with `PlanetTraits`) into `game/board/SystemDetails.tsx`.
3. **The fleet sheet.** `select/board.ts` gives the rows of `inspector(...)` to the fleet sheet as `OriginView.details`, in place of dropping them. `OriginSheet` shows them under its sticky heading and cargo row. They scroll with the ships.
4. **The production sheet** gets the same details. The landing sheet already lists every planet with its traits.
5. **`AGENTS.md`, Board section:** add the rule of decision 3.

Tests:

- `e2e/local.spec.ts` and `e2e/local.phone.spec.ts`:
  - a click in the activation shows "Close system details" with the planets;
  - an origin in the movement shows its planets;
  - screenshots at 1920×1080 and on a Pixel 9.

### Phase 2: the odds in Rust (`ti4-calc`, `ti4-view`, `ti4-wasm`)

1. **`ti4-calc`:**
   - Split `resolve_side` into `gather_side(state, system, galaxy, player, …) -> SideInput` and the building of the `Side`. The behaviour does not change.
   - Derive `Serialize` on `SideInput` and `UnitCounts`.
   - Add `space_odds_of(content, sources, attacker, defender, options)` for sides given without state.
   - Test: `gather_side` followed by building gives what `resolve_side` gives.
2. **`ti4-view`:**
   - Add `MovementFacts.battle: Option<BattleFacts { defender, attacker: CombatSide, defender_side: CombatSide }>`, filled from `gather_side`. It is set only when another seat has ships in the active system.
   - A draft gets it from the same function.
   - A late-game scenario (`scenario.rs`) gets a case where a seat can move into a fleet of another seat, with a test of the fact.
3. **`ti4-wasm`:** add the export `ti4_space_odds(len)`. The request is `{attacker, defender}` and the response is `BattleResponse`. Add a test.
4. **`.cargo/config.toml`:** add `-p ti4-calc` to `c-web`, `l-web`, `t-web` and `b-web`. In the Rust builds section of `AGENTS.md`: eight crates.

Gates: `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web`, then `scripts/build-wasm.sh` and `npm test`.

### Phase 3: the odds in the session (web2)

1. **`wire.ts`:** add `BattleFacts`, `CombatSide`, `MovementFacts.battle?` and `SpaceOddsResponse`.
2. **`Transport.odds(request)`:**
   - `wasmTransport.ts`: `spaceOddsOf`, with one instance kept for each module, not one for each question.
   - `wsTransport.ts`: the same function on the module of the engine of the page.
   - A case in `transportContract.test.ts`.
3. **New file `session/movementOdds.ts`** (pure). `oddsRequest(facts, staged)` returns null with no `battle` or when no ship arrives. Its key is its JSON. Unit tests over a recorded fixture with a fleet of another seat.
4. **`useLiveSession.ts`:**
   - When the movement (Live or draft) has a request, ask `transport.odds` and keep `local.odds = {key, response}`.
   - An answer to an old key is shown as pending.
   - `fresh` clears it.
5. **Model:** `SpaceOddsView { defender, win, draw, loss, rollouts, pending, sides: [{seat, ships: [{unit, expected, fielded}]}] }`.
   - Whole percentages that sum to 100.
   - One decimal for an expected count.
   - `MovementView.odds` and `InspectorView.arriving.odds` are built in `select/movement.ts`.

### Phase 4: the odds on the screen (web2)

1. **New component `game/combat/SpaceOdds.tsx`:**
   - "Space combat · ‹seat›", then "Win **72%** · Draw 3% · Loss 25%";
   - one line per side with the icon and "1.8 / 2" for each kind (the name on hover);
   - the rollouts and what the calculation leaves out behind a `Hint`;
   - while pending, the old numbers are dimmed and the line has "…".
2. **Where it is shown:**
   - a row in `Arrival` (`game/steps/movement/parts.tsx`);
   - the "Arrives" block of `SystemInspector`.
3. **`AGENTS.md`, Movement section:** where the odds are shown, what is shown, and that `ti4-calc` computes them in the page.
4. **Tests:**
   - a component test of `SpaceOdds`;
   - e2e on the scenario of phase 2: stage a ship into the fleet of another seat, then read Win, Draw, Loss and the survivors in the panel and in the inspector, on both layouts;
   - the panel at 1920×1080 must not scroll;
   - the whole `npm run e2e` before the commit.

## Order and size

- Phase 1 stands alone, and it is small.
- Phases 2, 3 and 4 depend on each other, in that order. Phase 2 is the largest.
- Each phase is one commit, after its gates.
