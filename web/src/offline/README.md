# Offline UI shell (T2)

Everything the user sees around an offline (local, one human vs rule bots) game. It is built against
`OfflineLauncher` in `types.ts`, not against any transport, so it does not depend on T1's
`GameTransport`. A dev-only in-memory launcher (`mockLauncher.ts`, registered by `devMock.ts` when the
page sets `window.__TI4_OFFLINE_MOCK__` or the URL has `?offline-mock`) stands in until T5.

## What T5 must plug in

1. Implement `OfflineLauncher` and call `registerOfflineLauncher(launcher)` at startup. With nothing
   registered the "Play offline" button is absent and `/offline` shows "not available".
   - `capabilities`: use `OFFLINE_CAPABILITIES` (splice preview, turn redo, advisor, multiplayer, instant
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
4. Hide degraded features with `useCapability("turnRedo" | "splicePreview" | "advisor" | "multiplayer")`
   or `<IfCapable name=...>`. The shell does not touch GameShell, the redo hook, the advisor or the map
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
| `hostApi.ts` | TypeScript port of `crates/ti4-wasm/js/host.mjs` (`Host`: JSPI, envelopes, save types) |
| `hostWorker.ts`, `hostRunner.ts`, `hostProtocol.ts` | the Web Worker: one game at a time, save attached to every question, history restarts in a new instance |
| `LocalTransport.ts` | `GameTransport` on a `HostPort` (the Worker, or a fake in tests); decodes and reduces like `GameSessionClient` |
| `session.ts` | lazy chunk: fetch and compile the wasm once, spawn the Worker |
| `launcher.ts`, `register.ts` | `OfflineLauncher`; `register.ts` is the only part in the main bundle (flags, JSPI check, feedback queue, a lazy proxy) |
| `store.ts` | `OfflineGameStore` interface, `MemoryGameStore`, `SessionGameStore` (placeholder, `sessionStorage`; T6 brings IndexedDB) |
| `localFeedbackQueue.ts` | placeholder queue (in memory, sends in the background); T10 replaces it |
| `OfflineGameScreen.tsx` | synthesises the roster (the human is the host of a table of bots), marks the game active for feedback, renders the app's game view |

Capabilities: `LocalTransport.capabilities` (`splicePreview`, `turnRedo`, `advisor`, `multiplayer` off,
`feedback` on) and the shell's `useCapability` both gate the UI: turn redo (GameShell prop and hook), the
bluff selector (multiplayer), the advisor requests in `SpaceCombatOverlay` and `InvasionLandingTray`.
There is no splice preview or map picker in the game screen of this branch, so nothing else needed hiding.

Tests: `*.test.ts` in `local/` (fake worker), `contract.test.ts` (LocalTransport and GameSessionClient give the
same state; the recorded stream runs with `TI4_HOSTED_STREAM`, see `hostedStream.test.ts`), and
`npm run test:e2e:offline` (production build only, network blocked, about 20 decisions, refresh mid-game).
