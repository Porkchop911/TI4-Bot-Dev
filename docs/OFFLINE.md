# Offline play

Status: operator and user documentation for the offline single-player mode. Every statement here
was checked against the code in `web/src/offline/` and the end-to-end test
`web/e2e/offline_local.spec.ts` (`cd web && npm run test:e2e:offline`).

## What it is

"Play offline" starts a game against rule bots entirely in the browser. The rules engine is
compiled to WebAssembly (`crates/ti4-wasm`, built on the shared host crate `ti4-host`) and runs in
a module web worker. After the first load there is no server request: you can switch the network
off. Games are stored in the browser (IndexedDB).

* One human, 3 to 6 players, every other seat is a rule bot (`ti4-rule-bot`). No MLP bots: the
  page of `web2/` plays those (`mlp:<name>`, `ti4_host::bots`).
* Same engine and same rules as the server; a game is fully determined by its seed and your answers.
* The server is needed only to load the page the first time and to deliver feedback.

## Browser requirement

Chrome or Edge **137 or newer** (tested on desktop Chromium only; Android Chrome of the same version has JSPI too but is not tested here). The engine blocks while it waits for your
decision and the browser suspends it with WebAssembly JSPI (JavaScript Promise Integration),
which shipped in Chrome 137. There is no fallback in v1: Firefox and Safari (all iOS browsers
are Safari underneath) cannot play offline and the page says so instead of failing later. Online
games work in every browser as before. No special server headers are needed (no
COOP/COEP, no SharedArrayBuffer).

## Using it

1. Start page, "Play offline". Choose the player count, your seat and faction (or random), the
   map and a nickname. The first start downloads the engine (6.5 MB unpacked, 1.6 MB with gzip, 1.1 MB with
   brotli) and compiles it; a progress bar shows this. Later starts use the browser cache (the
   file name contains a hash and is served `immutable`).
   **Map**: the row says "Recommended layout" until you touch it (the game then gets the layout
   the engine recommends for the table size, as every offline game did before the picker). "Choose
   map" opens the same sheet as the online lobby (`OfflineMapPicker`, the lobby's `MapCard` and
   `MapPreviewBoard`): a strip of the layouts that build for this table size, Random, and a large
   picture with a badge per seat. The first use loads the engine (so the download can happen there instead of at Start) and asks it (`describe` for the
   layouts, `map_preview` for the pictures; `web/src/offline/local/mapSource.ts`, one short-lived
   worker that ends 20 s after the last question). The picture is an example of the choice, not the
   exact board: seats, factions and the filler tiles are drawn from the game's seed at Start.
   "Another example" redraws it. A layout made for another table size is dropped when you change
   the player count. If the engine cannot answer the sheet shows the error with Retry and the form
   still starts with the recommended layout.
2. Play as in an online game. The position at every question is written to the browser
   (IndexedDB) before your answer goes to the engine, so closing the tab loses at most the
   answer you were giving. The page also asks the browser to keep its data
   (`navigator.storage.persist()`), which Chrome may grant or refuse by itself.
3. "My offline games" lists the saved games with round, phase and an estimated resume time.
   Resuming replays all your answers through the engine, which takes up to about 14 s late in a
   long game; a progress bar shows it and the tab stays responsive.
4. The strip above the game says which engine runs, `engine <first 8 hex digits of the sha-256 of
   the wasm file>`. The same text is in the build footer of the start screen once an offline game
   was opened in that page. Name it in a bug report.

## Saves, export and import

* A save is the host's `HostSave` (`SAVE_FORMAT = 2`, shared with `web2/` through
  `web-shared/src/storage`): the game as the engine was given it (`game`: one of `setup`, `table`,
  `local`, with every open choice filled in), the seed, the number of players, the option id of
  **every decision of every seat** (`decisions`, so a replay asks no bot again), the person's
  answers, the seat switches, the redo list, per-seat staged plans, the decisions hash, the rule
  bot version (`bot_version`, also for `table`/`local` games, which have no `setup` to hold it),
  engine build and content digest, plus the sha-256 of the wasm file that played it. Size is small
  (about 60 KiB at 5000 answers, more with the decisions of all seats). Limits: 8 MiB per game,
  100 games, 128 MiB in all. One IndexedDB (`ti4-offline`, version 2) serves both apps.
* **Starting technology** (Winnu, Argent Flight, Council Keleres): a new game asks for it before the
  first strategy card. The save records it as `game.setup.starting_tech: true` (or
  `game.table.setup.starting_tech`); a game of bot version 2 or less (every save made before the
  choice) has no such key, is made without the choice and replays as it was played. See
  `docs/STARTING_TECH_CHOICES.md`.
* **Format 2 replaces format 1.** Saves of format 1 (the old `web/` store, and web2's formats 1 to 3)
  are not read and not migrated: the page lists them as "older version" and offers to delete them.
  Exported files changed too (an export of the old format is refused on import). Testers lose their
  offline games once; see `docs/OFFLINE_SAVE_RESET_NOTE.md` for the notice.
* The chosen map is `game.setup.map` of the save (a template alias, or `"random"`). The engine
  fills it in when the person left it open, so a game always stores its layout, and the same
  board replays because the alias is part of what the seed and the answers are applied to. The
  picker asks the shared Worker (`query`: `describe`, `map_preview`) for layouts and pictures.
  Export, import and feedback carry it with the save.
* **Export** writes a JSON file (`kind: "ti4-offline-game"`, `version: 2`) with that save. Keep it
  as a backup or move a game to another browser.
* **Import** ("Import a game" next to "My offline games") checks the file (shape, version, size)
  and adds it under a new id. If its engine differs from the one in this page, it asks what to
  do when you open it (update policy below).
* Clearing site data, a private window or a browser reset deletes the saves. If the browser
  refuses to store (no IndexedDB, quota, a failed write) the game keeps running from memory and a
  notice at the top says "Games are not being saved on this device"; export before closing.
* Offline games are separate from online "My games"; nothing is migrated between them.

## Feedback

The Feedback button works offline. The report is put in a queue in IndexedDB and sent to the real
server in the background; the game keeps going and is never blocked. The queue survives closing the
tab and is sent on the next visit (the queue starts when the page loads, game or not).

A report is `POST /api/feedback` with the usual `category`, `text`, `context`, a top-level
`report_id` (made when the report is queued and kept for every retry) and `context.local`:
`{seed, setup, answers, engine: {build, content_digest}, wasm_sha256, save}`, where `save` is the
host's save at the moment of the report. That is enough to replay the game
(`tools/public/list_feedback.sh -s <id>`, then `cargo run -p ti4-wasm --example replay_save`).
The server stores a report with a known `report_id` once and answers `duplicate: true` to a
retry; the client counts any 200, duplicate included, as sent. So a lost answer never makes a
second file.

A failed send is retried with a delay of 30 s doubling up to 1 h (with jitter), 12 attempts, then
the report stays `failed` until retried by hand; a 4xx other than 408, 429 and 401 fails at once.
When the browser says it is online again, everything waiting is sent at once. The strip above the
game shows "N feedback reports will be sent", "... could not be sent yet" or, for ten minutes after
delivery, "... sent". Operators read these like any other report (`tools/public/list_feedback.sh`,
`docs/FEEDBACK_BUTTON_NOTES.md`).

## What is disabled offline

Everything that needs a server or other people: lobby, invitations, other seats, spectators,
presence, takeovers, the access password. The map picker is available (see "Using it"). Also hidden in v1: the map splice preview and turn redo
(undo, rewind to an earlier decision and redo of undone decisions stay). The combat odds come from
the engine of the page. The replay download of an offline game is its save file, not the server's replay
format. The UI hides these through the transport capabilities rather than showing broken buttons.
Known engine gaps apply (7 and 8 players are not offered; Creuss and Antimass limits as in the
engine).

## Notifications, event log and undo (single player)

* The notification log is an icon-only button (`toast-history-toggle`, bell, small unread count) in
  the row of the Technologies / Objectives buttons (phone: the Players / Objectives / Events
  buttons, `toast-history-mobile-toggle`). Its list is newest first. Games with other people keep
  the "Recent notifications" pill under the corner toasts.
* Corner toasts can be swiped away (touch or mouse drag, sideways), or dismissed with a click or
  Escape/Delete; a dismissed toast stays in the log. In a single-player game the toast column sits
  below the turn bar, its menus and dialogs, so it never covers a control.
* The event log lists the newest entry first (new entries appear at the top) and has an
  **Undo last decision** button: the engine's live undo (`history` action `undo`), i.e. exactly the
  human's last answer; the bots' decisions after it are replayed from the seed. It is enabled while
  `history.can_undo` is set (the last 32 checkpoints), announced through a polite live region, and
  the autosave follows (the save holds seed + the remaining human answers).
* The browser/system **Back** button does the same thing (`hooks/useBackUndo.ts`): one sentinel
  history entry sits on top of the game's entry; Back with something to undo undoes one decision
  and re-pushes the sentinel (the player stays on the game screen, a short note says so). Back
  with nothing to undo goes back once more, i.e. leaves the game for the previous page (no trap;
  the game is autosaved). A reload keeps the sentinel of its entry, so none is pushed twice.
  Forward onto a sentinel is ignored; nothing is redone.

## Size and performance

* wasm: 6.5 MB raw, 1.6 MB gzip, 1.1 MB brotli (`--profile wasm-release`, `opt-level = "z"`, no
  `wasm-opt`); the CI size gate (`scripts/wasm-size-gate.mjs`, `crates/ti4-wasm/size-budget.json`)
  allows 7.0 / 1.8 / 1.3 MB. The file is loaded only after "Play offline" (dynamic import), never
  for online games, and hashed (sha-256) while it compiles.
* The server hands out `assets/ti4_wasm-<hash>.wasm` with `Content-Type: application/wasm`, a
  one-year `immutable` cache and, when the release has the files, `.br` and `.gz` variants chosen
  by `Accept-Encoding` (deploy.sh creates them). Caddy compresses what is left (zstd, gzip).
* A step takes 3 to 6 ms of engine time in Node (`crates/ti4-wasm/README.md`); a full late-game
  resume is the slow case.

## Update and engine-hash policy

A save names the engine that played it by three values: the engine version (`build`), the digest of
the game data (`content_digest`) and the sha-256 of the wasm file. Determinism holds per build: a
changed engine can answer the same question differently and make an old answer list diverge.

* All three equal: resume silently.
* Any differs: before anything is replayed the page asks "This game was saved with another version
  of the engine" with three choices. **View only** replays the game with the current engine and
  opens it with moves refused ("View only" in the strip); nothing is written to the saved game.
  **Try anyway** replays and plays on; from then on the saved game is overwritten with the new
  engine's run. **Delete this game** (asks once more) removes it.
* After the replay of a game whose engine looked the same, or after "try anyway", the engine
  reports whether it reached the saved decisions (`resume_check`). If not, the same three choices
  are asked again with "This game no longer replays the same way". Old wasm builds are not kept in
  v1; export a game you care about before you let an update through.
* The page names its wasm by hash and each release has only its own file. A tab that was opened
  before an update and loads the engine afterwards finds the server's start page where the wasm
  should be and says "This page is out of date ... Reload the page"; an engine that is already
  loaded keeps running.
* Operators: deploy updates normally (see `deploy/public/README.md`, "How to deploy safely").
  Tell testers in the alpha notice when the engine changed, since their offline saves are
  affected even though no server data is.

## Operator notes

* CSP (set in `deploy/public/Caddyfile`): `script-src 'self' 'wasm-unsafe-eval'` (needed for
  `WebAssembly.compile`; plain `unsafe-eval` stays off) and `worker-src 'self'`. The worker is a
  same-origin file, so `blob:` workers are not allowed.
* No COOP/COEP headers are required; do not add them, they would break third-party embeds and the
  Cloudflare layer for no benefit.
* `deploy.sh` builds the engine with `web/scripts/build-wasm.mjs` (`npm run build:wasm`, also the
  `prebuild` hook of `npm run build`) with `TI4_REQUIRE_WASM=1`: a missing wasm32 target or a
  failed cargo build stops the deploy instead of shipping a release without "Play offline", and so
  does a `web/dist` without a `.wasm`.
* The nightly smoke sweep can play offline games instead of (or beside) the proctored server games:
  `NIGHTLY_MODE=offline|both`, see `tools/nightly-smoke/README.md`, "Offline mode". It plays full games with the
  production bundle and no server, reports findings in the usual night report, and keeps each game's save blob
  (`trace/save.json`) so `tools/nightly-smoke/offline_replay.sh` can replay it natively.
* Check headers after any proxy change: `deploy/public/check-headers.sh` starts the server and a
  throwaway Caddy with the repo Caddyfile on loopback and asserts content type, cache, br/gzip,
  CSP and the absence of COOP/COEP.

## Troubleshooting

| Symptom | Likely cause | What to do |
|---|---|---|
| "Play offline" says the browser is not supported | not Chrome/Edge 137+ | update the browser or use an online game |
| Stuck on "Loading engine", console shows a CSP error naming `wasm-eval` | proxy CSP lacks `'wasm-unsafe-eval'` | fix the Caddyfile `Content-Security-Policy`, run `check-headers.sh` |
| Worker fails to start, CSP error naming `worker-src` | CSP lacks `worker-src 'self'` | same |
| `WebAssembly.compileStreaming` fails with a MIME error | wasm served as `text/html` (SPA fallback for a missing file) or `application/octet-stream` | check the file exists in `dist/assets` and the type is `application/wasm` |
| Huge download every time | proxy strips `Cache-Control`, or the file is not under `/assets/` | headers on `/assets/*.wasm` must be `immutable` |
| Game list is empty after a browser update | site data cleared | restore from an exported file ("Import a game") |
| "Games are not being saved on this device" | private window, IndexedDB blocked, quota, or a database written by a newer app | export the games you want to keep; use a normal window |
| "Engine changed" prompt on an old save | the app was updated | choose "View only" to look without changing anything, or export first and then "Try anyway"; see the policy above |
| "This page is out of date" when starting | the server was updated after this tab was opened | reload the page |
| Feedback stays "will be sent" | browser offline, or server unreachable or rate limited (burst of 10, then 10 per hour) | it retries by itself and when the browser comes online; after 12 failed attempts it says "could not be sent yet" |
| Resume takes very long | late game, many answers | expected up to about 14 s on a desktop, longer on a phone |
