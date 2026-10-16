# web-shared: rules for agents

The code that `web/` and `web2/` share: what lies below each app's UI-facing transport. The plan
is `web2/plans/2026-10-14-SHARED_TRANSPORT_PLAN.md`.

## Ownership and changes

- Both maintainers own `web-shared/`. An app imports only its public modules.
- Some changes need the agreement of both maintainers:
  - a stored format: the save, the DB schema and its migrations, or the export file;
  - the shape of `GameLink`/`LinkEvent`, or of the generated wire types (that is, a protocol
    change in Rust);
  - removing or renaming an export;
  - a new dependency.
- Other changes need no agreement: additive changes, fixes and tests, as long as the gates of
  both apps are green.
  - web2: `npm run typecheck`, `npm run lint`, `npm run format:check` (it also checks the format of
    web-shared), `npm test`.
  - web: `npx tsc --noEmit`, `npm test`.
- Never in `web-shared/`: React, CSS, view models, text shown to a player, or the state model
  of an app.
- The code follows both apps' toolchains:
  - explicit `.ts` import extensions;
  - ES2022;
  - no vitest globals;
  - no `import.meta.glob` and no `vite/client` types.

## How the apps use it

- An app imports `@ti4/shared/<dir>/<file>.ts`. The alias is in each app's `vite.config.ts`
  (`resolve.alias`, and `..` in `server.fs.allow`) and `tsconfig.json` (`paths`, and
  `../web-shared/src` in `include`).
- `web-shared/` has no `package.json` and no `node_modules`. `vitest` resolves through the
  `paths` of each app's `tsconfig.json`, and `fake-indexeddb` (a dev dependency of web2 only)
  through web2's.
- In web2 only `src/session/` imports it. `web2/src/boundaries.test.ts` checks that, and that
  nothing here imports an app or a package other than `node:` and `vitest` (and
  `fake-indexeddb` in a `*.test.ts`).
- A Worker is started with a relative `new URL(".../web-shared/src/engine/worker.ts",
  import.meta.url)`: vite bundles a Worker only from a literal path, not from the alias.

## Layout

```
src/engine/  host.ts      Host: one wasm instance plays one game (JSPI), Envelope, HostSave, StartConfig
             jspi.ts      hasJspi(), on its own so that a page checks it without loading the host
             runner.ts    HostRunner: one game at a time in the Worker, a save at each question; a restart
                          starts as the game was started, with the stepping and the modes changed since; a
                          query goes to the instance of the running game, else to a new one; the files of
                          the MLP models (`{t: "models"}`) are loaded into each instance of a game before
                          its start (`Host.loadModel`, `ti4_model_load`), checked against their SHA-256
             worker.ts    the Worker entry; it gets a compiled WebAssembly.Module, not the file
             protocol.ts  ToWorker, FromWorker, HostPort (the Worker or a fake)
             port.ts      workerPort (a HostPort on a Worker that the app made), inlinePort (the engine in
                          this thread, for Node and the tests)
             identity.ts  sha256Hex/sha256 of the engine file, EngineIdentity {build, contentDigest, wasmSha256}
             odds.ts      spaceOdds, groundOdds (queries of the engine of the page), pageQuery (an engine on the
                          main thread)
             fleet.ts     fleetStanding (a query of the engine of the page: fleet supply and capacity of counts)
src/net/     socket.ts    Socket, openSocket, pageSocket, connect (reopens a lost socket), backoff, HEARTBEAT_MS
             api.ts       the HTTP API of the server: the paths, PLAYER_SESSION_HEADER, postJson, lobbyPost,
                          joinLobby, playerOf, createTable (ApiError), isGone/gameGone
src/link/    link.ts       GameLink (send, request?, subscribe, close), LinkEvent, HostCommand; a game at the level of
                           its messages: `ClientMessage` (or a command of the host) in, `ServerMessage` out
             socketLink.ts socketLink: the server over `connect` (subscription on each open, heartbeat, backoff,
                           `again`); web2's `wsTransport` runs on it
             engineLink.ts engineLink: the engine over a HostPort (start, replies, the next question, a gate, saves,
                           the files of the models of the game: `models`, posted before the start);
                           web's `LocalTransport` and web2's local game (`wasmTransport`) run on it
src/storage/ types.ts     HostSave (format 2), SavedGameRecord, GameMeta, StoreStatus, StoreError, LIMITS,
                          OfflineSaveStore, FeedbackBackend
             backend.ts   Backend: IdbBackend (DB `ti4-offline`, DB_VERSION, MIGRATIONS), MemoryBackend
             saveStore.ts openOfflineSaveStore: the games, a memory fallback, import and export
             autosave.ts  createAutosave: debounced writes, flushed on pagehide and a hidden tab
             validate.ts  validateSave, the record and the export file (EXPORT_KIND, EXPORT_VERSION),
                          engineDifferences
             index.ts     the public exports
src/wire/    index.ts     the wire types: import them from here
             generated/   one file for each type, and index.ts with every type and PROTOCOL_VERSION; written
                          from Rust by `cargo wire-web`, never by hand
             decode.ts    decodeServerMessage (checks a message before an app reads it), decodeInitialSnapshot,
                          isStaleServerMessage, the display parts of an update (trigger, auto-resolved notes,
                          reaction modes), and the map answers of the server (decodeMapChoice, decodeMapList,
                          decodeMapPreview)
src/content/ catalog.ts   loadCatalog (from the engine of the page, else `GET /api/catalog`), CatalogError
             names.ts     humanizeId, the faction ids of a table (base game, Prophecy of Kings)
src/identity/ nickname.ts NICKNAME_KEY (`ti4.nickname`), validNickname (the names that the server takes)
test/server.ts            spawnTestServer, testServerBinary: a real ti4-server for the tests of both apps (Node only)
scripts/build-wasm.mjs    the build of the engine file of both apps
```

The engine file stays where each app keeps it (`web/src/offline/local/engineAsset.ts`,
`web2/src/session/ti4.wasm`); `scripts/build-wasm.mjs` builds it for both ("The engine build").

## The engine build

`node web-shared/scripts/build-wasm.mjs <out.wasm> [--dev]` builds `crates/ti4-wasm` for the
browser and copies the file to `<out.wasm>`. Both apps call it with their file, so they run one
cargo command and share one build: web's `web/scripts/build-wasm.mjs` (`npm run build:wasm`, the
`prebuild` of web's `npm run build`, and so of `deploy/public/deploy.sh`) and web2's
`web2/scripts/build-wasm.sh`.

- The command: `cargo build -p ti4-wasm --target wasm32-unknown-unknown --profile wasm-release`,
  in the repository; `--dev` builds `--profile wasm-dev` (not optimized, incremental). It is not
  one of the native aliases: the wasm target has its own build. The file is taken from
  `$CARGO_TARGET_DIR` (default `target/`).
- `TI4_WASM_FILE=<file>` copies a prebuilt file and builds nothing. `TI4_SKIP_WASM=1` builds
  nothing and removes `<out.wasm>`.
- A failed build (no cargo, no wasm32 target) warns, removes `<out.wasm>` and exits 0, so that
  web's `npm run build` works without the engine. `TI4_REQUIRE_WASM=1` makes it an error (exit 1,
  the old file kept): web2's script and the deploy set it.
- The size gate is a separate step: `node scripts/wasm-size-gate.mjs <file.wasm>` of the
  repository (budgets in `crates/ti4-wasm/size-budget.json`).

## Wire types

The types of what the server and the engine send and take, generated from the Rust types with
`ts-rs` (`crates/ti4-wasm/src/wire_ts.rs`). Import from `@ti4/shared/wire/index.ts`.

- The roots: `ClientMessage` and `ServerMessage` (`ti4_host::protocol`, with the views, the
  status, the staged work and the planning wire), `Catalog`, the setup of a table
  (`SetupOptions` and `MapPreview` as the server gives them, `TableOptions`, `TablePreview`,
  `TableDraw`, `GameRequest`), the odds of a space combat (`SpaceOddsRequest`, `BattleResponse`)
  and of a ground combat (`GroundOddsRequest`, `GroundOddsResponse`), and the save (`HostSave`). Every type that they name is generated with them.
  `PROTOCOL_VERSION` is generated too. The types of `ti4-server` (the lobby of a table) are not.
- The doc comments are those of the Rust types: a comment for a page is written there. A field
  that Rust leaves out when it is empty is `name?: T`. A field that a page may leave out of what
  it sends (serde `default`) is `name?: T` with `#[cfg_attr(feature = "ts", ts(optional))]` on an
  `Option`, and `ts(optional = nullable)` on another type (`TableSetup`, `SeatRequest`, `scope`
  of `run_draft`). `u64` is `number`. The engine's `GameState` is `Record<string, unknown>`.
- Each app imports them through its own module, which re-exports them and adds only what the app
  narrows or keeps of its own: `web2/src/session/wire.ts` and `web/src/protocol/types.ts` (with
  the lobby of the server, and web's `PendingChoiceDto`). A field that Rust does not send is not
  read: a type of an app is never a looser copy of a generated one.
- `wire/decode.ts` checks a message of the server or the engine (`decodeServerMessage`): the
  envelope and what an app relies on, not every field. A link (`GameLink`) types its messages as
  `ServerMessage` but does not check them; web decodes each one (`GameSessionClient`,
  `LocalTransport`). Its errors are for the developer.
- A change of a Rust type of the wire: run `cargo wire-web` (it writes `src/wire/generated/`
  and removes the files of types that are gone, and writes `fixtures/protocol/`) and commit the
  files. `cargo t-web` fails while they are stale (`wire_types::generated_wire_types_are_current`).
- The derives are behind the `ts` feature of `ti4-model`, `ti4-engine`, `ti4-calc`, `ti4-host`
  and `ti4-wasm`. Only the tests enable it (the dev-dependency of `ti4-server` on `ti4-wasm`), so
  the wasm file and the server binary of a deploy have none of it. A new type that a root names
  gets `#[cfg_attr(feature = "ts", derive(ts_rs::TS))]`; a field that cannot be typed gets
  `#[cfg_attr(feature = "ts", ts(type = "..."))]`. Two types with one name fail the generator:
  rename one with `ts(rename = "...")`.
- The generated files are not linted or formatted (`biome.json`, `.oxfmtrc.json`).
- `PROTOCOL_VERSION` is also named in `index.ts`: Node (web's e2e) loads this dir as CommonJS,
  and an ES module does not see a value that only `export *` passes on.

## Catalog

What is printed on the cards, from `ti4-content` through the engine: `Catalog`
(`ti4_host::view::components`). A page asks for it once; an update names cards by id only.

- The engine gives it to `{"query":"catalog"}`, the server at `GET /api/catalog`: the same
  JSON. `content` is the digest of the corpus; a page compares it with `content` of a snapshot of
  the server.
- `exploration` and `components` are the cards of the sources of a game (Prophecy of Kings):
  web2 reads them.
- `records` (`ContentRecords`) is every record of the corpus that a page names by id, of every
  source: 15 kinds (strategy cards, secret and public objectives, action cards, technologies,
  explores, planets, attachments, factions, units, leaders, abilities, promissory notes, relics,
  breakthroughs). Each kind is a list in the order of the corpus, each record with its `id`; a
  field that a record does not print is left out. web's content wrappers read these, as tables by
  id (`web/src/protocol/catalogStore.ts`).
- `loadCatalog(source)` (`@ti4/shared/content/catalog.ts`) loads it. With `source.query` (a
  query of the engine that the page has loaded: `EngineLink.query`, or `Host.query`) it asks the
  engine, `{"query":"catalog"}`; without it the server (`source.fetch`, `source.url`, default
  `/api/catalog`). With `source.content` it refuses a catalog of another corpus. It throws a
  `CatalogError` with a `kind` (`engine`, `http`, `invalid`, `digest`): the app shows its own text.
  - web2 asks its engine when it loads it (`loadEngine` in `wasmTransport.ts`, `engineQuery`).
  - web loads it before its app renders (`web/src/components/CatalogProvider.tsx`): an online
    page from the server, an offline page (`/offline`) from its engine in a short-lived Worker;
    each falls back to the other.
- `fixtures/catalog/catalog.json` of the repository is the answer of the engine, for the tests of
  web (`web/src/test/setup.ts` loads it). `cargo wire-web` writes it; `cargo t-web` fails while it
  is stale (`catalog_fixture::catalog_fixture_is_current`).
- `content/names.ts` holds the pure helpers about ids, with no text of their own:
  - `humanizeId`, the one capitalization of both apps: underscores to spaces, each word in upper
    case, also after a hyphen ("jol-nar" → "Jol-Nar"). web's `contentCatalog.ts` and web2's
    `select/names.ts` re-export it.
  - `FACTION_IDS` (what a table can seat, the engine's `seating::LOBBY_FACTIONS`, base game
    first), `BASE_FACTION_IDS`, `POK_FACTION_IDS`, `POK_FACTIONS`. A test checks them against
    the Rust source. web's `OFFLINE_FACTION_IDS` and `POK_FACTIONS` are these.

## Odds

The odds of a space combat and of a ground combat, from the engine of the page
(`ti4_calc::odds`): the same numbers online and offline, with no server. Import from
`@ti4/shared/engine/odds.ts`.

- `spaceOdds(query, request)` asks `{"query":"space_odds", ...SpaceOddsRequest}` and gives a
  `BattleResponse` (2000 rollouts from a fixed seed: the same sides give the same numbers). The
  sides name their ships by unit id (`dreadnought2`, `sol_flagship`: the upgraded and faction
  units are read by their ids; the faction only shifts the rolls). `in_progress: true` is a
  combat that has begun: no space cannon and no anti-fighter barrage come before its round (web's
  combat window); without it the combat is about to start (web2's movement).
  `groundOdds(query, request)` asks `{"query":"ground_odds", ...GroundOddsRequest}` and gives a
  `GroundOddsResponse`. Sides that the engine does not read reject with its error (`space odds:
  …`, `ground odds: …`, `query: …`): the app shows its own text.
- `query` is the engine of the page:
  - while a game runs in a Worker, `EngineLink.query`: the Worker asks the instance of the
    running game, also while it waits at a question (web2's `wasmTransport.ts`; web's
    `LocalTransport.queryEngine`);
  - else `pageQuery(module)`: one instance of the module on the main thread, kept for the page
    and made again after a trap (web2's online game, `wsTransport.ts`; web's online game and its
    catalog, `web/src/protocol/pageEngine.ts`, loaded after the first render). It needs JSPI, as
    every engine of a page.
- A query waits while the game in the Worker computes (a bot decides); it is answered when the
  game waits for the page again.

## Server API

The HTTP calls of the server that both apps make, from `@ti4/shared/net/api.ts`. Each call takes
the `fetch` to use as its last argument (default: the page's), so a test gives its own.

- The paths: `GAMES_PATH`, `gamePath(id)`, `lobbyPath(id, route?)`, `mapsPath(players)`,
  `SETUP_OPTIONS_PATH`, `tableSocketPath(id)` (`/ws/tables/<id>`).
- `PLAYER_SESSION_HEADER` (`x-ti4-player-session`) and `sessionHeaders(session)`. `postJson(url,
  body, {session, signal})` posts JSON, or no body and no content type when `body` is undefined;
  `lobbyPost(id, route, …)` is a call of the lobby, `joinLobby(id, request, …)` its join (a new
  player or a takeover with no session; with a session, who that player is: `playerOf`).
- `createTable(request)` posts `/api/games` and gives the answer (`Admission`: `game_id`,
  `player_session`, `player.id`, and the lobby, untyped here). A refusal throws an `ApiError`
  with the status and the text of the answer; the app words it (web reads the list of a 429
  `too_many_games` from it).
- `isGone(response)` / `gameGone(id)`: the server no longer has the game when it answers 404,
  not when it cannot be reached.
- The lobby types of the server (`LobbyState`, `TableServerMessage`, the join answer) are not
  generated: each app keeps its own, and the answers here are typed only on the fields read here.
- The way an app follows a lobby stays its own: web polls (`useLobbySession`, a join with the
  session each 2 s), web2 has the socket of the table (`web2/src/session/table.ts`).
- The calls of a game (`/snapshot`, `turn-redo`, history, splice) stay in web's
  `GameSessionClient`; they use the paths and the header from here.

## Nickname

The name of a player at a table of the server, from `@ti4/shared/identity/nickname.ts`.

- `validNickname(value)` is what the server takes (`ti4-server` `storage::validate_nickname`): a
  string, trimmed, not only whitespace, at most `NICKNAME_MAX_BYTES` (64) bytes of UTF-8, with no
  control or formatting code point. It gives no reason: the app words its own refusal (web's
  lobby says why, web2 marks the input and does not send the name).
- `fixtures/nickname/cases.json` of the repository has names that are taken and names that are
  refused. `src/identity/nickname.test.ts` and the server's test
  `storage::player_record_tests::nickname_validation_matches_the_shared_cases_of_the_pages` both
  check them: a change of the rules changes both and the file.
- `NICKNAME_KEY` (`ti4.nickname`) is the localStorage key of the name that the player gave
  last, for both apps (web's `protocol/nickname.ts`, web2's `session/onlineKeys.ts`).
- The seat tokens stay with each app: web keeps one player session per tab, web2 several
  players of a game per browser (`onlineKeys.ts`).

## Protocol fixtures

`fixtures/protocol/` of the repository holds messages for the tests of both apps, written by the
Rust tests; never edit one.

- `actor_snapshot.json`, `opponent_snapshot.json`, `spectator_snapshot.json`,
  `stale_submission_rejected.json`, `terminal_game_over.json`: `ServerMessage`s of the server
  (`ti4_server::fixtures`, `crates/ti4-server/tests/it/fixtures_verification.rs`).
- `update-*.json`: the `SessionUpdate` of one question of a game that the hosted engine plays
  (`crates/ti4-wasm/tests/it/protocol_fixtures.rs` names the game and the answers that reach it).
- `event_visibility.json`: three `GameEvent`s, checked to round-trip by a test of `ti4-host`.
- `cargo t-web` fails while a file differs from what Rust writes; `cargo wire-web` writes them
  again. A change of the engine can change a recorded game: then a test of an app is brought up
  to date, or sets on its copy what it needs.
- They are read through the generated types: `src/wire/decode.test.ts` decodes each message and
  reads each update, web's `src/protocol/fixtures.test.ts` decodes the messages, and the tests of
  web2 import the updates as `SessionUpdate` (`web2/src/boundaries.test.ts` lets a test import
  them).

## Save store

The single player games of both apps, in one IndexedDB database of the origin. Import from
`@ti4/shared/storage/index.ts`.

- `await openOfflineSaveStore()` never throws for a storage problem. Without IndexedDB (a
  private window), with a database newer than the build, or when a write fails, it keeps the
  games in memory and says why in `status().warningKind` (`unavailable`, `newer-version`,
  `quota`, `write-failed`). The app shows a visible notice with its own text.
- `put(id, save, meta)` resolves when the write committed; call it before the game goes on
  (write-ahead). `createAutosave(store).schedule(...)` is the debounced path (300 ms, at most
  2 s, the newest save wins).
- `meta = {nickname, factions, engine, slot?, createdAt?, resumeEstimateMs?}`. `factions` has one
  entry per seat, null where the app does not know it. `engine` is `{build, contentDigest,
  wasmSha256}`: the first two from `save.engine`, the hash of the wasm file that the page loaded.
  A later write keeps `createdAt`, `resumeEstimateMs` and `slot` when it does not give them.
- `list()` (newest first, no save), `load(id)`, `findSlot(slot)` (the newest game with the slot),
  `remove(id)`, `removeUnreadable()` (deletes the records that are not read: the saves of an
  older version), `exportGame(id)`, `importGame(file, currentEngine?)` (always a fresh id, and
  no slot; `engineDiffers` says whether the file's engine is another one; later writes keep
  `imported`).
- `createAutosave` flushes on `pagehide`, but a write of IndexedDB that starts there does not
  end when the page is gone (a reload, a navigation). An app that must not lose the last save
  keeps it at once elsewhere: web2 puts it into localStorage and into the store on the next
  load (the saved game in `web2/docs/agents/local-play.md`).
- web2 keeps its local games and its single player game here (`web2/src/session/saves.ts`), with
  the slots below; web/ its offline games.
- **The record** (`SavedGameRecord`, schema 2): `id`, `slot?`, `nickname`, `createdAt`,
  `updatedAt`, `status`, `summary {playerCount, humans, factions, seed, round, phase, answers}`,
  `engine`, `resumeEstimateMs`, `imported`, `sizeBytes`, `save`.
- **The slot** is the fixed address of a game in an app: `"single"`,
  `"local.<seed>.<players>.<mask>"`, `"scenario.<id>.<mask>"`. A game found by its id only has
  none.
- **The save** is the engine's `HostSave`, format 2 (`crates/ti4-wasm/README.md`). `validateSave`
  checks what the store and the lists rely on (2 to 8 players, the humans mask, every seat's
  decisions, the limits); the engine checks the rest when it resumes. A record that fails
  (an older save format or record schema, a broken row) is not shown and is counted in
  `status().unreadable`.
- **The export file** is the one export format of both apps: `{kind: "ti4-offline-game",
  version: 2, exportedAt, game: {nickname, factions, createdAt, engine, save}}`.
- **The schema.** Database `ti4-offline`, version 2: `games` (key `id`, indexes `updatedAt` and
  `slot`), `feedback` (key `localId`), `meta`. A change adds a step to `MIGRATIONS` and bumps
  `DB_VERSION`; an old step is never edited. A database newer than the build is not touched (the
  memory fallback).
- **The limits** (`LIMITS`): 8 MiB a save, 100 games, 128 MiB in all, 20000 answers, 200000
  decisions, a nickname of 40 characters.
- **Feedback.** The object store `feedback` belongs to web's feedback queue
  (`web/src/offline/storage/feedbackQueue.ts`), through `store.feedbackBackend`. It is part of
  this schema because it is one database; the queue's code and its rows are web's.

## Tests

- The tests here (`src/**/*.test.ts`) run in web2's vitest, in its `unit` project (node, no DOM):
  `cd web2 && npm test`.
- web2's biome lints this dir: `web-shared/biome.json` extends `web2/biome.json`, and web2's
  `npm run lint` runs it here too. Not `src/wire/generated/`.
- `cargo t-web` checks that `src/wire/generated/`, `fixtures/protocol/` and `fixtures/catalog/`
  are current.
- The storage tests (`src/storage/storage.test.ts`) run on `fake-indexeddb`, a fresh
  `IDBFactory` for each test.
- The odds tests (`src/engine/odds.test.ts`) ask web2's engine file (`web2/src/session/ti4.wasm`)
  when it is built, and skip that part otherwise.
- web type-checks this dir with its own `tsconfig.json` (`npx tsc --noEmit`), without the
  `*.test.ts` files; its vitest does not run the tests here.
- web's `npm run test:e2e:offline` and web2's `npm run e2e` play games through the Worker of
  `src/engine/`. web2's `transportContract.test.ts` checks its transport over both links (the
  engine on `inlinePort`, and a real server).
- **The test server** (`test/server.ts`, Node only; outside `src/`, so no app bundles it and web's
  `tsc` does not check it): `spawnTestServer({binary?, env?, timeoutMs?})` starts a real
  `ti4-server` on a free loopback port with a temp data directory (`TI4_DATA_DIR`), and resolves
  once it printed `Listening on` (`{url, port, dataDir, stop}`). `stop()` kills it, waits for its
  exit and removes the directory; a server that does not start is stopped and removed before the
  call throws, and one left running is killed when the test process exits.
  `testServerBinary()` is `TI4_E2E_SERVER_BIN`, else the debug and then the release `server` of
  `$CARGO_TARGET_DIR` (default `target/`; `cargo b-web` builds the debug one), else null: the
  caller skips. Its users: web2's `tableContract.test.ts` and `transportContract.test.ts` (a test
  imports it by its relative path; `boundaries.test.ts` lets it), web's `e2e/offline_local.spec.ts`.
  The servers of the Playwright configs and web's `resume_real.spec.ts` (a fixed port and a data
  directory that outlives a restart) start their own.
