# Offline play

Status: operator and user documentation for the offline single-player mode. Every statement here
was checked against the code in `web/src/offline/` and the end-to-end test
`web/e2e/offline_local.spec.ts` (`cd web && npm run test:e2e:offline`).

## What it is

"Play offline" starts a game against rule bots entirely in the browser. The rules engine is
compiled to WebAssembly (`crates/ti4-wasm`, built on the shared host crate `ti4-host`) and runs in
a module web worker. After the first load there is no server request: you can switch the network
off. Games are stored in the browser (IndexedDB).

* One human, 3 to 6 players, every other seat is a rule bot (`ti4-rule-bot`). No ML bots.
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

1. Start page, "Play offline". Choose the player count, your seat and faction (or random) and a
   nickname. The first start downloads the engine (6.5 MB unpacked, 1.6 MB with gzip, 1.1 MB with
   brotli) and compiles it; a progress bar shows this. Later starts use the browser cache (the
   file name contains a hash and is served `immutable`).
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

* A save is the host's `save_state` object (seed, setup, the human's answers, redo list, the
  decisions hash, engine build and content digest), not a board snapshot, plus the sha-256 of the
  wasm file that played it. Size is small (about 60 KiB at 5000 answers). Limits: 8 MiB per game,
  100 games, 128 MiB in all.
* **Export** writes a JSON file (`kind: "ti4-offline-game"`, `version: 1`) with that save. Keep it
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
presence, takeovers, the access password. Also hidden in v1: the map splice preview, turn redo
(undo, rewind to an earlier decision and redo of undone decisions stay), the advisor (win odds,
combat odds). The replay download of an offline game is its save file, not the server's replay
format. The UI hides these through the transport capabilities rather than showing broken buttons.
Known engine gaps apply (7 and 8 players are not offered; Creuss and Antimass limits as in the
engine).

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
