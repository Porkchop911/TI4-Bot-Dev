# Shared web module (web-shared/): transport, engine, saves, wire types, content, odds (2026-10-14)

Status: done (2026-10-15): steps 1 to 22, and the docs.

## Context

`web/` and `web2/` both stay maintained. Their backends are shared as far as reasonable. Each
UI stays free in its state model and its screens. Changes that affect both apps are agreed by
both maintainers (see "Ownership and changes").

Each app has two backends for a game:

|       | server                                                                                      | single player (wasm engine)                                                                                                            |
| ----- | ------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| web/  | `GameSessionClient` (`src/protocol/client.ts`, WS + HTTP, own reconnect in `resilience.ts`) | `LocalTransport` → Web Worker → `HostRunner`/`Host` (`src/offline/local/`) → wasm `ti4_host_*` = `HostSession`, speaks `ServerMessage` |
| web2/ | `wsTransport.ts` over `socket.ts` (reconnect, backoff, heartbeat), speaks `ServerMessage`   | `wasmTransport.ts` (966 lines) on the **main thread**, bespoke C-ABI (`ti4_play*`, `ti4_draft`, `ti4_plan`, `ti4_journal`, …)          |

One wasm binary has two JS drivers and two wasm protocols. Step 5 of
`web2/plans/2026-10-14-DEDUPLICATE_HOSTS_PLAN.md` moves web2 onto `HostSession` and its
`ServerMessage` decoding.

Several other things are also written twice, or exist in one app only:

- **Saves.**
  - web/ keeps them in IndexedDB `ti4-offline` (`web/src/offline/storage/`). It has a list,
    limits, an autosave with a pagehide flush, and validation. Its `HostSave` format 1 holds
    only the human's answers, for one human seat and 3–6 players.
  - web2 keeps one game per localStorage key (`web2/src/session/savedGame.ts`). Its format 3
    holds every seat's decisions, plus `switches` and `table`, for 3–8 players and a `humans`
    mask. It has no list and no limits.
- **Wire types.** They are hand-written twice: `web/src/protocol/types.ts` (859 lines) and
  `web2/src/session/wire.ts` (1049 lines). The two share 19 core types, and they already
  differ in detail.
- **Content.**
  - web/ reads a generated manifest (`web/scripts/generate-content-manifest.mjs` →
    `generatedContentManifest.ts`, 9858 lines, 15 record kinds).
  - web2 reads `Catalog` from the engine (`ti4_catalog`, and `{"query":"catalog"}` in
    `crates/ti4-wasm/src/host_api.rs`).
- **Combat odds.**
  - web/ asks a server advisor service for them (`web/src/services/advisorService.ts`). That
    service does not work at the moment.
  - web2 computes space odds in the engine (`ti4_space_odds`, `session/movementOdds.ts`), both
    online and in single player.
  - `ti4-calc/src/odds.rs` has `ground_odds`, but no wasm export calls it.

**Not shared: the UI-facing transport interface.**

- web's `GameTransport` is a stateful store (`GameSessionState` via `reduceServerMessage`). It
  has history, splice, turn-redo, the advisor and reach.
- web2's `Transport` is an event stream (`TransportEvent`). It has drafts, plans, `SeatStore`,
  odds and test capabilities.
- Merging them would mean rewriting one UI's state model, with no runtime gain.

**Shared: the layers below it.**

- A message-level _link_. It sends a `ClientMessage` (or a local host command) and receives
  `ServerMessage` objects and the connection status. It has two implementations: websocket and
  wasm engine.
- The save store.
- Wire types generated from Rust.
- The content catalog and the odds, both from the engine.

## Decisions

- **One save list for both apps.**
  - There is one IndexedDB database (`ti4-offline`) with one record format.
  - On one origin (phase 4 of `2026-10-14-SAME_SEAT_TABS_AND_ONE_DOMAIN_PLAN.md`), either app
    lists and resumes every single player game of the browser.
- **A save records every seat's decisions, in order**, as the server keeps a game. A resume
  feeds them all back, so no bot decides again and no seat waits.
- **Old saves are not read.** This covers web/ `HostSave` format 1 and web2 formats 1–3. The
  page says the save is from an older version and offers to delete it.
- **The engine is the one source of the content catalog.** web drops its generated manifest.
- **Combat odds come from the engine in both apps.** web's advisor client goes away.
- **The wire types are generated from Rust** and used by both apps.

## Ownership and changes

This section is also the first section of `web-shared/AGENTS.md`.

- Both maintainers own `web-shared/`. An app imports only its public modules.
- Some changes need the agreement of both maintainers:
  - a stored format: the save, the DB schema and its migrations, or the export file;
  - the shape of `GameLink`/`LinkEvent`, or of the generated wire types (that is, a protocol
    change in Rust);
  - removing or renaming an export;
  - a new dependency.
- Other changes need no agreement: additive changes, fixes and tests, as long as the gates of
  both apps are green.
  - web2: `npm run typecheck`, `npm run lint`, `npm test`.
  - web: `npx tsc --noEmit`, `npm test`.
- Never in `web-shared/`: React, CSS, view models, text shown to a player, or the state model
  of an app.
- The code follows both apps' toolchains:
  - explicit `.ts` import extensions;
  - ES2022;
  - no vitest globals;
  - no `import.meta.glob` and no `vite/client` types.

## Mechanism

`web-shared/` is a new TS source dir at the repo root with no dependencies. Apps import it as
`@ti4/shared/...`.

- Both `vite.config.ts`: `resolve.alias { "@ti4/shared": <root>/web-shared/src }`, and add `..` to
  `server.fs.allow`.
- Both `tsconfig.json`: `paths { "@ti4/shared/*": ["../web-shared/src/*"] }`, and add the dir to
  `include`.
- Tests:
  - The tests of `web-shared` run in web2's vitest: its `include` gets
    `../web-shared/src/**/*.test.ts`.
  - The storage tests use `fake-indexeddb`, a dev dependency of web2 only.
  - web2's biome lints `web-shared` (`biome.json` files).
  - web/ only type-checks what it imports.
- `web2/src/boundaries.test.ts` gets two rules: `session` may import `@ti4/shared`, and nothing
  in `web-shared` imports an app.
- `web-shared/AGENTS.md` says what lives here and how it is tested. It is linked from the Topics
  table in `web2/AGENTS.md` and from `web/README.md`.

Layout:

```
web-shared/src/
  engine/   host.ts (Host, hasJspi, Envelope/HostSave types), runner.ts (HostRunner),
            worker.ts (Worker entry), protocol.ts (ToWorker/FromWorker/HostPort), port.ts (Worker HostPort factory),
            identity.ts (wasm SHA-256, engine identity), odds.ts (space/ground odds)
  net/      socket.ts (Socket, connect, backoff, HEARTBEAT_MS, pageSocket), api.ts (HTTP API of the server)
  link/     link.ts (GameLink, LinkEvent), socketLink.ts, engineLink.ts
  storage/  backend.ts, saveStore.ts, autosave.ts, validate.ts, types.ts, index.ts
  wire/     generated/*.ts (from Rust), index.ts, decode.ts
  content/  catalog.ts (loadCatalog), names.ts (humanizeId, faction ids, expansion sets)
  identity/ nickname.ts
web-shared/test/server.ts   spawnTestServer() (node only)
web-shared/scripts/build-wasm.mjs
fixtures/protocol/          protocol fixtures for both apps, written by Rust tests
```

## Steps

There are five tracks, with one commit per step. The order inside each track is fixed. The
tracks are independent of each other, except where a step says otherwise.

### A. Engine driver and link

1. **Scaffold, and move the engine driver (a pure move).**
   - `git mv` web/'s `hostApi.ts`, `hostRunner.ts` (+test), `hostWorker.ts`, `hostProtocol.ts`
     and `jspi.ts` into `web-shared/src/engine/`. web/ imports them via
     `@ti4/shared/engine/*`.
   - The engine file stays where each app keeps it (`engineAsset.ts` in web/, `ti4.wasm` in
     web2). The Worker gets a `WebAssembly.Module`, so there is no coupling to the asset.
   - `engine/identity.ts`: the wasm SHA-256 and the engine identity
     `{build, contentDigest, wasmSha256}`. It comes from `web/src/offline/local/sha256.ts`, and
     replaces the hashing in `wasmTransport.ts` `loadEngine`.
2. **Move the socket.**
   - `web2/src/session/socket.ts` → `web-shared/src/net/socket.ts`. web2's `wsTransport.ts` and
     `table.ts` import it.
   - web/'s `GameSessionClient` keeps its own reconnect. Its HTTP resume logic is tied to it.
     Moving it onto `connect` is an optional follow-up.
3. **`GameLink`.**
   ```ts
   interface GameLink {
     send(message: object): void; // ClientMessage or a local host command
     request?(command: object): Promise<unknown>; // engine only: save_state, history, queries
     subscribe(listener: (e: LinkEvent) => void): () => void;
     close(): void;
   }
   type LinkEvent =
     | { kind: "message"; message: unknown }
     | { kind: "opened" }
     | { kind: "offline" }
     | { kind: "progress"; done: number; total: number }
     | { kind: "save"; save: HostSave }
     | { kind: "fatal"; message: string };
   ```
   - `socketLink(open, …)` wraps `connect`: the subscription on `opened`, the heartbeat and the
     backoff.
   - `engineLink(port, config)` wraps a `HostPort`:
     - `start`;
     - the `messages` of `ask`/`reply` envelopes become `message` events;
     - a `save` at each ask;
     - `crash`/`fatal` become `fatal`.

     The logic is lifted from `LocalTransport` (`web/src/offline/local/LocalTransport.ts`, its
     port and waiter handling).

   - Tests use a fake `Socket` and a fake `HostPort`, built on the ideas of
     `web/src/offline/local/testKit.ts`.
   - The link stays untyped (`unknown`) until track C step 12. After that it is
     `ClientMessage` in and `ServerMessage` out.
4. **web/ `LocalTransport` on `engineLink`.**
   - It keeps `GameTransport`, `MessageDrivenState`, and the batch, history and divergence
     handling. Only the port plumbing goes.
   - `LocalTransport.test.ts`, `contract.test.ts` and `launcher.test.ts` stay green.
5. **web2 single player on `engineLink`.** This is Step 5 of the dedup plan.
   - Do its Rust part 5.1 first: `HostSession` local commands for:
     - the humans mask and seat switching;
     - rule seats;
     - stepping and wait;
     - scenarios;
     - draft and plan.
   - Then `wsTransport.ts` becomes `messageTransport(link, …)`, used by both modes:
     - online: `socketLink`;
     - single player: `engineLink`, plus capabilities (`stepping`, `switchSeat`, `wait`,
       `scenarios`, `save`) mapped to local commands.
   - Delete the game loop of `wasmTransport.ts` and the bespoke exports (dedup 5.3).
   - Keep `ti4_setup_*`, `ti4_catalog` and `ti4_space_odds` until tracks D and E replace them.
     They are stateless queries.
   - The engine moves to the Worker, so the page's main thread no longer runs the game.

### B. Saves

6. **The save format (Rust, in `HostSession::save_state`).** `HostSave` has `format: 2`. It holds:
   - `seed` and `table` (`TableSetup`);
   - `scenario?`;
   - `humans` (a mask);
   - `decisions`: every seat's option id, in order;
   - `switches`: a decision index and a humans mask each;
   - `finished`, `round?`, `phase?`;
   - `engine`.

   The player count is `setup::PLAYER_COUNTS` (2..=8). A resume feeds every decision to the
   engine, as web2's local play and the server already do. Undo cuts `decisions` at the answer
   it takes back. The host keeps this list itself, so web2 no longer builds it from
   `ti4_journal`, which goes with the other bespoke exports (step 5).
   Needs step 5.1 of the dedup plan.

7. **The save store moves to `web-shared/src/storage/`.**
   - `git mv web/src/offline/storage/*`. The feedback queue code stays in web/. Its object
     store stays in the shared schema, because it is one DB.
   - `validate.ts` checks format 2.
   - In `SavedGameRecord`, `summary` has `humans` and `factions` (in place of
     `humanSeat`/`humanFaction`) and the player count.
   - New field `slot?: string`: the fixed address of a game in an app (`"single"`,
     `"local.<seed>.<players>.<mask>"`, `"scenario.<id>.<mask>"`). Migration to DB version 2
     adds an index on it.
   - The export envelope (`kind: "ti4-offline-game"`) is the one export format of both apps.
   - The limits of `types.ts` stay: 8 MiB a save, 100 games, 128 MiB in all.
8. **web2 saves in the shared store.**
   - `savedGame.ts` load/store/clear become `saveStore` and `autosave`.
   - `LocalApp`/`SingleApp` find their game by `slot`.
   - The `save` event of `engineLink` drives `autosave.put`.
   - Export and Import use the shared envelope.
   - The localStorage keys `ti4.single` and `ti4.local.*` are deleted once, at start. The
     `DEFAULT_GAME` fallback in `StartApp.tsx` goes.
   - The staged work is in the engine's save (`staged`, step 5). localStorage keeps, next to a
     save, what the page reads or writes at once, keyed by the save's id:
     `ti4.save.<id>.<part>`.
     - `cards` (the reaction modes) and `wait` (the wait of a table).
     - `left`: the newest save, with what the shown seat staged, when the page is left. A write
       of IndexedDB that starts at `pagehide` does not end before the page is gone; the next
       page that opens the store puts it there.
     - Removing a save clears them.
   - web2's start screen lists the saves, by web2's own rules. The list is UI and stays per app.
9. **web/ on format 2.** `launcher.ts` and `register.ts` resume from `decisions`. The list
   shows the humans and factions of a game. Its `offline` e2e covers save, resume, export and
   import.

With both apps on one origin, both lists show every save of the browser.

### C. Wire types

10. **Generate them from Rust.**
    - `ts-rs` derives `TS` behind a `ts` feature, so the wasm and server builds do not change.
    - It covers the wire DTOs of `ti4_host::protocol`: `ClientMessage`, `ServerMessage`, views,
      status, staged and the planning wire. It also covers `Catalog`, the setup options, the
      odds request and response, and `HostSave`.
    - An `xtask` command writes `web-shared/src/wire/generated/*.ts`. The files are checked in.
    - A test under `cargo t-web` fails when they are stale.
    - `PROTOCOL_VERSION` is generated too.
11. **web2 uses them.**
    - `wire.ts` re-exports the generated types. Its doc comments move to the Rust doc comments,
      which ts-rs copies.
    - A hand-written type stays only where web2 narrows one.
    - Each drift that typecheck shows is fixed in Rust or in web2.
12. **web uses them.**
    - `types.ts` re-exports the generated types.
    - `decode.ts`/`mapDecode.ts` move to `web-shared/src/wire/decode.ts` and validate the
      generated types. Both apps may use them.
    - `GameLink` gets its types.
    - **Protocol fixtures.**
      - `crates/ti4-server/fixtures/*.json` and `web2/src/session/fixtures/update-*.json`
        become `fixtures/protocol/`, written by Rust tests.
      - The fixture tests of both apps read them through the generated types.

### D. Content catalog

13. **One catalog (Rust).**
    - `ti4_host::view::components::Catalog` gets every record kind that web's wrappers read:
      - strategy cards and objectives;
      - planets, attachments and factions;
      - units, leaders and faction abilities;
      - promissory notes, relics and breakthroughs;
      - plus the action cards, technologies and exploration cards it has.
    - The server serves the same JSON at `GET /api/catalog`, with the content digest.
    - Needs step 10.
14. **`web-shared/src/content/`.**
    - `loadCatalog()` uses the engine query when an engine is loaded, and `/api/catalog`
      otherwise.
    - It also holds the pure helpers that both apps have: `humanizeId`, the faction
      ids and the expansion sets.
15. **web uses the catalog.**
    - It loads the catalog at startup (a provider in web's React tree).
    - `contentCatalog.ts`, `factionInfo.ts`, `technologyData.ts`, `unitStats.ts` and
      `cardDatabase.ts` read the catalog.
    - Delete `generate-content-manifest.mjs`, `generatedContentManifest.ts` and
      `verify:content`.
    - `lobbyFactions.ts` and `offline/factions.ts` take their faction data from the catalog. The
      monograms and hues are UI and stay.
    - web2 moves from `ti4_catalog` to `loadCatalog()`, and the export goes.

### E. Combat odds

16. **Odds queries (Rust).**
    - `host_api::query` gets `space_odds` and `ground_odds`, on `ti4_calc::odds`.
    - The server and web2 use the same request and response types (step 10).
17. **`web-shared/src/engine/odds.ts`.**
    - `spaceOdds(request)` and `groundOdds(request)` run on the engine of the page: in the
      Worker after step 5, on the main thread before it. They work online and offline.
    - web2's `movementOdds.ts` and `wsTransport.ts` call them. The export `ti4_space_odds` goes.
18. **web uses them.**
    - `SpaceCombatOverlay.tsx`, `InvasionLandingTray.tsx` and `TacticalMovementOverlay.tsx`
      call the shared odds.
    - Delete `advisorService.ts`, `advisorTypes.ts`, the `advisor` capability and the
      `advisorUrl` props.
    - web's online page loads the wasm, as web2's does.
    - The server's advisor (`ti4-advisor`, `/api/advisor*`) is not part of this plan.

### F. Small shared pieces (any order)

19. **`net/api.ts`.**
    - Lifted from `web2/src/session/table.ts`, which is pure and takes an injectable fetch.
    - It has the URL builders, the `x-ti4-player-session` header, `createTable`, lobby join and
      the simple lobby POSTs, and the "game gone" check.
    - The lobby transport stays per app: web polls, and web2 uses `/ws/tables`.
20. **`identity/nickname.ts`.**
    - The `ti4.nickname` key and `validNickname` (from `web/src/protocol/nickname.ts`, which
      mirrors the server). web2 validates with it too.
    - Seat tokens stay per app: web keeps one session per tab, web2 several seats per browser.
21. **`web-shared/scripts/build-wasm.mjs`.**
    - One build script with the output file as its argument. It has web's `TI4_*` env flags and
      web2's `--dev`.
    - `web/scripts/build-wasm.mjs` and `web2/scripts/build-wasm.sh` call it with their file;
      `deploy.sh` reaches it through web's `npm run build`.
22. **`web-shared/test/server.ts`.**
    - `spawnTestServer()`: a free port, a temp data dir, and a wait for `Listening on`.
    - It replaces `web/e2e/offlineServer.ts` and the spawn in
      `web2/src/session/tableContract.test.ts`.

### Agreements between maintainers

The steps that change web's code or a shared format are 6, 7, 9, 10, 12, 13, 15 and 18. Each
starts only after both maintainers agree on it.

## Not shared

- Hex geometry: small, and tied to each app's model.
- Scenarios: web reads `/api/dev/scenarios`, web2 reads `ti4_scenarios`.
- The UI-facing transport and state models.
- The layout and settings keys of each app.

## Docs

- `web-shared/AGENTS.md`: ownership and changes, the layout, the tests, the save format, the
  catalog and the odds.
- `web2/AGENTS.md`: a Topics row for `web-shared`.
- `web2/docs/agents/local-play.md`: the Transport; the saved game (IndexedDB, format 2, the save
  list); the wording for stepping.
- `web2/docs/agents/online.md`: the socket path.
- `web/src/offline/README.md`: the storage now lives in `web-shared`. The stale
  `store.ts`/`SessionGameStore` mention goes.
- `web/README.md`: the catalog and the odds; the advisor setup goes.
- `crates/ti4-wasm/README.md`: one host with two consumers, the save format, and the queries.
- The last open question of `2026-10-14-SAME_SEAT_TABS_AND_ONE_DOMAIN_PLAN.md` (two single
  player entries): one save list, answered by this plan.

## Verification

Gates for every step:

- web2: `npm run typecheck`, `npm run lint`, `npm test`.
- web: `npx tsc --noEmit`, `npm test`.
- Rust steps: `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web`, run one at a time. When
  the host or wasm changed, also `scripts/build-wasm.sh`.

Per step:

- **1, 4:** `npm run test:e2e:offline` in web/ (start an offline game, resume from a save).
- **2:** web2 `npm run e2e -- -g online`, with `cargo b-web && target/debug/server`.
- **5:**
  - `transportContract.test.ts` runs against both links.
  - The full `npm run e2e` in web2.
  - In the browser:
    - a `?single` table: hot-seat "Play as", undo, draft + Apply, reload from save,
      `?scenario=…`;
    - `?online&fill=random` still plays.
- **6–9:**
  - A Rust test: save, resume, and the same state with no bot call.
  - The storage tests under `fake-indexeddb`.
  - web2 e2e for single player: play, reload, undo, "Play as" + reload, export/import, the save
    list.
  - web `test:e2e:offline`.
  - On one origin (the dev form of domain phase 4): start in web and continue in web2, and the
    other way round.
- **10–12:**
  - The stale-check test.
  - No hand-written copy of a generated type is left in `wire.ts`/`types.ts`.
  - The shared fixtures parse in both apps.
  - The wasm size gate is unchanged (the `ts` feature is off).
- **13–15:**
  - A Rust test: the catalog has every record kind of `ti4-content`, with the counts.
  - In web, the card, technology and unit texts are the same as before, online and offline (a
    few screens by hand).
- **16–18:**
  - A Rust test: the queries equal `ti4_calc::odds`.
  - web shows space and ground odds online and offline with no advisor.
  - web2's `movementOdds` tests pass.
- **19–22:**
  - Both apps' online e2e (`api.ts`).
  - The wasm builds of both apps and `deploy.sh --help` (build script).
  - `tableContract.test.ts` and web's offline e2e (test server).
