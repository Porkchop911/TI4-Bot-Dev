# Twilight Imperium 4 — Web Client

A minimal browser client and test suite for the authoritative `ti4-server` engine, built with React 19, TypeScript, Vite, Vitest, and Playwright.

---

## Quick Start (Interactive Play)

To test the client interactively across multiple players in your browser:

### 1. Start the Rust Backend Server

In a terminal from the repository root:

```bash
cargo run -p ti4-server --bin server
```

- Starts the HTTP and WebSocket authoritative server on `http://127.0.0.1:8080`.
- Start on the home page to create a lobby with 2–8 positions. Share the
  resulting game URL to invite other players or spectators.

### 2. Start the Frontend Dev Server

In a second terminal:

```bash
cd web
npm install
npm run dev
```

- Starts Vite at `http://127.0.0.1:3000`.
- API requests (`/api/*`) and WebSocket connections (`/ws/*`) are automatically proxied to `http://127.0.0.1:8080`.

### 3. Open in Browser

1. Navigate to [http://127.0.0.1:3000](http://127.0.0.1:3000).
2. Choose the player count and create a lobby. The creator becomes its host.
3. Copy the game URL and open it in another browser context to **Join game**
   (next open position) or **Watch** without claiming a position. A disconnected
   participant can be explicitly taken over from another computer after the
   presence grace period.
4. Players mark themselves ready; the host may reorder the lobby and start when
   every position is occupied and ready. Session credentials stay in tab-scoped
   storage, never in invitation URLs. Reconnect in the same tab to resume.
5. Choices and private cards are hidden from opponents and spectators.
6. The lobby host can use **Undo** and **Redo** in the event drawer, or select
   **Undo to here** on an earlier log entry. This restores the authoritative
   engine state after that entry for everyone in the game. Redo remains available
   until a new choice is made; connected tabs reconnect to the restored timeline.

### 4. MLP Bots (Optional)

The server plays the trained MLP policies itself, inside the game worker, on `ti4-infer` (no
libtorch, no extra process). Point it at a model directory:

```bash
TI4_MODEL_DIR=/path/to/models cargo run -p ti4-server --bin server
```

The directory holds an `index.json` (`{"models": [{"name": "...", "sha256": "...", "file": "..."}]}`)
and the packed `.ti4m` files it lists. Every model becomes a bot kind `mlp:<name>` of the lobby
(`bot_kinds`, `POST /api/games/{id}/lobby/bots` with `{"kind": "mlp:<name>"}`). Without
`TI4_MODEL_DIR` no MLP kind is offered. A packed model is made with `ti4-pack` (`ti4-infer`). The
lobby of this client offers the rule bot only.

---

## Testing

### Unit & Protocol Conformance Tests (Vitest)

Runs wire fixture validations (the messages of `../fixtures/protocol/`, written by the Rust tests),
React component tests, and invariant checks:

```bash
npm test
```

### End-to-End Invariant Tests (Playwright)

Runs real-server end-to-end tests in isolated browser contexts verifying:

- Zero console errors / JavaScript runtime exceptions.
- Live strategy card draft synchronization across multiple tabs.
- Strict DOM privacy redaction (opponents/spectators never receive private card DOM nodes).
- Actionability and decision-making invariants.
- Disconnect and clean reconnection to a live game session.

```bash
# Headless run (builds/starts the Rust backend and Vite automatically):
npm run test:e2e

# Headed run (visible browser windows):
npx playwright test --headed

# Server startup diagnostics:
DEBUG=pw:webserver npx playwright test --reporter=line
```

The E2E servers use per-run ports and a short presence grace/heartbeat interval to keep
takeover tests fast. Set `TI4_E2E_BACKEND_PORT` and `TI4_E2E_FRONTEND_PORT` to use
specific ports. The backend's `TI4_DEV_PRESENCE_GRACE_MS` and Vite's
`VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS` are optional development overrides; ordinary
server and UI startup retain the normal 30-second grace and 10-second heartbeat.

---

## Project Structure

```text
web/
├── e2e/
│   ├── lobby_lifecycle.spec.ts       # Create, join, watch, leave, and start
│   └── multiplayer_invariants.spec.ts # Playwright multiplayer invariant suite
├── src/
│   ├── components/
│   │   ├── Board.tsx                   # Interactive SVG galaxy board with planets & units
│   │   ├── TurnStatusBar.tsx           # Round, phase, speaker, and live turn status bar
│   │   ├── PlayerSheet.tsx             # Player resources, VP, tokens, and private hand
│   │   ├── PendingChoiceModal.tsx      # Accessible dialog for player decisions
│   │   ├── EventLog.tsx                # Collapsible event log drawer
│   │   └── Lobby.tsx                   # Admission, readiness and host reorder
│   ├── hooks/
│   │   └── useGameSession.ts           # WebSocket connection hook with state sync & heartbeat
│   ├── protocol/
│   │   └── types.ts                    # the wire types generated from Rust (web-shared), what web narrows, the lobby
│   ├── test/
│   │   └── invariants.test.ts          # State integrity and privacy invariant checks
│   ├── App.tsx                         # Main app container
│   └── main.tsx                        # Entry point
├── playwright.config.ts                # Playwright configuration with auto-spawning dev servers
└── vite.config.ts                      # Vite configuration with proxy to ti4-server
```

### Shared with web2

The engine driver of offline play (the Worker, `Host`, `HostRunner`, the engine file hash), the
save store, the wire types (generated from Rust) and their checks (`decodeServerMessage`, the map
answers: `wire/decode.ts`) are in `../web-shared/`, which web and web2 share. This app imports it as `@ti4/shared/...` (the alias is in
`vite.config.ts` and `tsconfig.json`). Read [`../web-shared/AGENTS.md`](../web-shared/AGENTS.md)
before you change it: some changes there need the agreement of both maintainers.

The engine file of offline play (`src/offline/local/engine/ti4_wasm.wasm`, not in the repository)
is built by `npm run build:wasm` (`scripts/build-wasm.mjs`, also the `prebuild` of `npm run build`).
It runs the build that web2 runs too, `../web-shared/scripts/build-wasm.mjs`, so both apps get the
same file from one cargo build. `TI4_WASM_FILE=<file>` copies a prebuilt file, `TI4_SKIP_WASM=1`
builds without "Play offline", `TI4_REQUIRE_WASM=1` makes a failed build an error.

### The catalog of the printed cards

Every text of a card, technology, unit, faction, leader, objective or planet that web shows comes
from the engine's catalog (`Catalog.records`): the same JSON from `GET /api/catalog` of the server
and from the engine's `{"query":"catalog"}`. `CatalogProvider` (`src/components/CatalogProvider.tsx`)
loads it once, before the app renders: an online page from the server, an offline page
(`/offline`) from its engine (`src/protocol/pageEngine.ts`); each falls back to the other, and with neither
the page says that the cards could not be loaded and offers Retry. It fills the tables of
`src/protocol/catalogStore.ts`, which the content wrappers (`contentCatalog.ts`, `factionInfo.ts`,
`technologyData.ts`, `unitStats.ts`, `cardDatabase.ts`, ...) read. The unit tests fill them from
`../fixtures/catalog/catalog.json` (`src/test/setup.ts`), the engine's answer written by
`cargo wire-web`.

### The odds of a combat

The win odds of a space combat (`SpaceCombatOverlay`) and of a landing (`InvasionLandingTray`)
come from the engine, online and offline, with no server: `spaceOdds` and `groundOdds` of
`@ti4/shared/engine/odds.ts` (`{"query":"space_odds"}`, `{"query":"ground_odds"}`, on
`ti4_calc::odds`). `OddsEngineProvider` (`src/components/OddsEngine.tsx`) gives the game screen
its engine: in an offline game the engine that plays it (`LocalTransport.queryEngine`, in its
Worker); online the page's own engine (`src/protocol/pageEngine.ts`: the wasm file fetched and
compiled once, after the first render, and one instance on the main thread, which also answers
the catalog). Until it is there the space combat shows a rough fleet estimate ("simulation
pending") and the landing "Calculating…"; in a browser without WebAssembly JSPI they stay at the
estimate and "Odds unavailable".
