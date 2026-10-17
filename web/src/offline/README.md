# Offline UI shell (T2)

Everything the user sees around an offline (local, one human vs rule bots) game. It is built against
`OfflineLauncher` in `types.ts`, not against any transport, so it does not depend on T1's
`GameTransport`. A dev-only in-memory launcher (`mockLauncher.ts`, registered by `devMock.ts` when the
page sets `window.__TI4_OFFLINE_MOCK__` or the URL has `?offline-mock`) stands in until T5.

## What T5 must plug in

1. Implement `OfflineLauncher` and call `registerOfflineLauncher(launcher)` at startup. With nothing
   registered the "Play offline" button is absent and `/offline` shows "not available".
   - `capabilities`: use `OFFLINE_CAPABILITIES` (splice preview, turn redo, multiplayer, instant
     feedback all off). T1's transport should report the same flags; the banner and `useCapability`
     read them.
   - `supported()`: false with a reason when JSPI is missing (Chrome/Edge 137+ only in v1).
   - `list()` (`updatedAt`/`createdAt` are unix seconds; `resumeEstimateMs` from the last measured
     replay), `create`, `resume` (call `onProgress` with `loading-engine`, then `replaying` with
     `done`/`total` as often as is cheap; honour the `AbortSignal` by rejecting), `remove`,
     `exportGame` (seed, answers, engine hash as JSON).
   - `feedback`: an `OfflineFeedbackQueue`. `list()` must return a stable array reference until the
     content changes (it feeds `useSyncExternalStore`); notify subscribers on every state change.
     The queue persists (IndexedDB), sends in the background (`POST /api/feedback` with
     `{category, text, context}`, plus `local` for triage), retries with backoff, and never blocks the game.
2. Game screen: render the real game from `renderGame` of `OfflineRoutes` (`/offline/games/<id>`,
   currently a placeholder) inside T1's transport provider and `CapabilityProvider`, and show the banner
   (`OfflineGameRoute` passes it as the second argument).
3. While a game is on screen call `setActiveOfflineGame({ gameId, local })` (and `null` on leave). The
   feedback sheet then routes through `routeFeedback`: the report goes to `launcher.feedback.enqueue`
   with `local()` (seed, the human's answers, engine hash, table size, seat) and the sheet says
   "Will be sent". Outside an offline game feedback is unchanged.
4. Hide degraded features with `useCapability("turnRedo" | "splicePreview" | "multiplayer")`
   or `<IfCapable name=...>`. The shell does not touch GameShell, the redo hook or the map
   picker; T5 wires those flags in.
5. Server side (built): `/api/feedback` accepts `context.local` (shape `OfflineFeedbackLocal` in types.ts) and an optional `report_id`; a retry with the same id is stored once (`duplicate: true`). Treat 200 as sent.

Not done here: import of an exported file, switching an offline game to online, the setup's map and
strategy-card-set choices (the setup form has only players, seat, faction, nickname, dev seed).


## T5: what plugs in now (`local/`)

The real launcher is registered at startup by `local/register.ts` when the build has the engine file
(`src/offline/local/engine/ti4_wasm.wasm`, copied there by `scripts/build-wasm.mjs`; `npm run build`
runs it as `prebuild`, and a build without cargo or the wasm target just has no "Play offline"). The dev mock
(`?offline-mock`, the DN screenshots) keeps the slot when asked for.

| File | What |
|---|---|
| `web-shared/src/engine/host.ts` | TypeScript port of `crates/ti4-wasm/js/host.mjs` (`Host`: JSPI, envelopes, save types); shared with web2 (`web-shared/AGENTS.md`) |
| `web-shared/src/engine/worker.ts`, `runner.ts`, `protocol.ts` | the Web Worker: one game at a time, save attached to every question, history restarts in a new instance |
| `LocalTransport.ts` | `GameTransport` on an `engineLink` (`web-shared/src/link/`) over a `HostPort` (the Worker, or a fake in tests); decodes and reduces like `GameSessionClient`; keeps the batch, history, write-ahead of saves, view-only and divergence handling (the link's `gate`) |
| `session.ts` | lazy chunk: fetch and compile the wasm once, spawn the Worker |
| `launcher.ts`, `register.ts` | `OfflineLauncher`; `register.ts` is the only part in the main bundle (flags, JSPI check, feedback queue, a lazy proxy) |
| `OfflineGameScreen.tsx` | synthesises the roster (the human is the host of a table of bots), marks the game active for feedback, renders the app's game view |

## Saves and the feedback queue (`storage/`)

The save store lives in `web-shared/src/storage/` and is shared with web2: the IndexedDB database
`ti4-offline`, the records, autosave, the limits, import and export. Its rules are in
`web-shared/AGENTS.md` ("Save store"). `storage/index.ts` re-exports it together with web's own
parts:

| File | What |
|---|---|
| `shell.ts` | the store's records as `OfflineGameSummary` (the person's seat is the first of the humans mask), and the text of the storage warning (`StorageNotice`) |
| `engineCheck.ts` | the data of the "saved with another engine" dialog (`engineMismatchPrompt`, `replayDivergedPrompt`) |
| `feedbackQueue.ts` | the durable feedback queue (`createDurableFeedbackQueue`, `httpFeedbackSender`) on the store's `feedbackBackend` |
| `ImportGameButton.tsx` | the "Import a game" button |

The launcher writes the faction of every seat that the board has shown (null before the first
question); a faction once known stays. The store keeps milliseconds; `offlineSummary` gives the
shell unix seconds.

The list ("My offline games", `OfflineGamesList.tsx`) is the one save list of the origin: it shows
the games of web2 too, and web2 shows these. A game resumes from its save (`{resume: save}`): the
engine plays every seat's `decisions` again, so no bot decides again. Each row names the faction of
every seat and marks the person's seats. A game that several people play at one screen (web2's
local game, `summary.humans` with more than one seat) is listed with `playable: false`: export and
delete only, since the game screen shows one seat; `resume` refuses it (`NOT_PLAYABLE_HERE`). Games
of 2, 7 and 8 players resume like the others. The engine names a game by the id it was set up
with; `LocalTransport` gives its messages the store's id, so an imported game and a game of web2 open. Saves of an older version are not read: the list
counts them (`storage.status().unreadable`) and offers to delete them (`storage.removeUnreadable`).

Feedback queue: every report gets a `reportId` at enqueue, sent as `report_id` with every attempt,
so the server stores a re-sent report once (`duplicate: true` counts as sent). Backoff from 30 s,
doubling to 1 h with jitter; 12 attempts, then `failed` until `retry()`; an `online` event makes
everything due. A crash while "sending" reverts to "queued". Caps mirror the server (4000
characters, 200 KiB), 50 waiting reports.

Tests: `storage/feedbackQueue.test.ts` (the queue, the sender, the engine check); the store's own
tests run in web2 (`web-shared/AGENTS.md`, "Tests").

Capabilities: `LocalTransport.capabilities` (`splicePreview`, `multiplayer` off,
`turnRedo` and `feedback` on) and the shell's `useCapability` both gate the UI: the
bluff selector (multiplayer). There is no splice preview or map picker in the game screen of this branch,
so nothing else needed hiding.

Odds: `LocalTransport.queryEngine` asks the game's engine in its Worker (`EngineLink.query`); the game
screen hands it to `OddsEngineProvider`, so the odds of `SpaceCombatOverlay` and `InvasionLandingTray`
are the same online and offline.

Tests: `*.test.ts` in `local/` (fake worker), `contract.test.ts` (LocalTransport and GameSessionClient give the
same state; the recorded stream runs with `TI4_HOSTED_STREAM`, see `hostedStream.test.ts`), and
`npm run test:e2e:offline` (production build only, network blocked, about 20 decisions, refresh mid-game,
resume from the list, export and import, a save of an older version, and feedback queued offline that reaches a
real server once: `spawnTestServer()` of `web-shared/test/server.ts`, with `TI4_E2E_SERVER_BIN` or the debug or
release `server` of the target dir).
