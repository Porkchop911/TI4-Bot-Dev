# Offline save store (T6)

Self-contained persistence for offline games: IndexedDB with an in-memory fallback, autosave,
export/import, engine-mismatch data, and a durable queue for feedback reports. Nothing here
imports the T5 launcher/worker; it imports only `../types.ts` (`OfflineGameSummary`,
`QueuedFeedback*`). Import everything from `./index.ts`.

## Interface (`OfflineSaveStore`, `types.ts`)

| Member | Use |
|---|---|
| `await openOfflineSaveStore()` | Opens (never throws for a storage problem; falls back to memory) |
| `status()` / `subscribe()` | `{persistence, warning, warningKind, persisted, unreadable, unsynced}`. Show `warning` as a visible notice whenever it is non-null |
| `put(id, save, meta)` | Durable write of the T4 `save` object (resolves when committed). Call it **before** the game continues with the answer (write-ahead) |
| `createAutosave(store).schedule(id, save, meta)` | Debounced path (300 ms, max 2 s, newest save wins, flushed on `pagehide` / hidden tab / `flush()`) |
| `list()` / `listSummaries()` | Records without the save / `OfflineGameSummary[]` for T2's list |
| `load(id)` | `SavedGameRecord` incl. `save` and `engine {build, contentDigest, wasmSha256}` |
| `remove(id)` | Delete |
| `exportGame(id)` | `{filename, blob}` JSON file (`kind: "ti4-offline-game"`, `version: 1`) = `OfflineLauncher.exportGame` |
| `importGame(file, currentEngine?)` | Validates shape/version/size, always a fresh id; returns `engineDiffers` |
| `engineMismatchPrompt(saved, current)` | Before replay: dialog data (view only / try anyway / delete) or null |
| `replayDivergedPrompt(resume_check, saved, current)` | After replay: same dialog when `resume_check.ok` is false |
| `requestPersistence()` / `estimate()` | `navigator.storage.persist()` once / usage and quota |
| `feedbackBackend` + `createDurableFeedbackQueue` | See below |

The caller supplies `meta = {nickname, humanFaction, engine}`. `engine.build` and
`engine.contentDigest` are `save.engine.build` / `.content_digest`; `wasmSha256` is the hash of
the wasm file the page hashed. `put` keeps `createdAt` and `resumeEstimateMs` of earlier writes.

## What T5 does with it

T5's placeholder in-memory storage maps like this (the placeholder should keep these four
operations and swap its object for the store):

- `save(gameId, save, meta)` -> `store.put` (or `autosave.schedule` after each answer; `autosave.flush()` on leaving the game)
- `load(gameId)` -> `store.load` (use `record.save` as `ti4_host_start({setup: save.setup, resume: save})`)
- `list()` -> `store.listSummaries()`
- `remove(gameId)` -> `store.remove`

and `OfflineLauncher.exportGame` -> `store.exportGame`; `OfflineLauncher.feedback` -> the queue
below. Wire `ImportGameButton` (a plain button + hidden file input, `ImportGameButton.tsx`) into
"My offline games"; wire `store.status().warning` into a visible banner.

## Feedback queue

```ts
const queue = createDurableFeedbackQueue({ backend: store.feedbackBackend, send: httpFeedbackSender() });
await queue.start();            // loads stored items, sends what is due
launcher.feedback = queue;      // implements OfflineFeedbackQueue (enqueue/list/subscribe/retry)
```

Every report gets a `reportId` (uuid) at enqueue, sent with every attempt (header
`idempotency-key`, and `context.offline.report_id`; `local` goes in `context.offline.local`
because the server rejects unknown top-level fields). States as in `QueuedFeedbackItem`;
backoff 30 s doubling to 1 h with jitter, 12 attempts then `failed` (manual `retry`); an
`online` event makes everything due. A crash while "sending" reverts to "queued" and is re-sent
with the same id. Caps mirror the server (4000 chars, 200 KiB), 50 waiting reports.
**Server dedupe by report id is not built** (out of scope): until then a lost response can create
a duplicate file that triage recognises by `context.offline.report_id`.

## Schema and limits

Database `ti4-offline`, version 1: stores `games` (key `id`, index `updatedAt`), `feedback`
(key `localId`), `meta`. Add a step to `MIGRATIONS` in `backend.ts` and bump `DB_VERSION` for a
change; a database newer than the build falls back to memory with a warning (nothing is
overwritten). IndexedDB transactions are atomic, so a crash during a write leaves the previous
version; there is no second copy. Limits (`LIMITS` in `types.ts`): 8 MiB per save, 100 games,
128 MiB total, 20000 answers, 40-character nickname.

## Failure behaviour

- IndexedDB missing or throwing (private window): memory backend, `warningKind: "unavailable"`.
- Write fails or quota: the record stays in memory and is shown by `list/load`;
  `warningKind: "quota" | "write-failed"`, `unsynced` counts such games; the next successful write clears it. `put` does not throw, so the game goes on.
- Nothing is silently dropped: invalid stored records are hidden and counted in `unreadable`.

Tests: `storage.test.ts` (vitest + fake-indexeddb 6.2.5): `cd web && npx vitest run src/offline/storage`.
