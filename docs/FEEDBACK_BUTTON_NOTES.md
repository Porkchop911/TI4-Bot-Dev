# In-game feedback button: BUILT (design notes kept below)

**Status: implemented** in commits `171d9721` (server) and `52c851bd` (web UI), 2026-10-12, and
running in the public alpha. It is on `main`, `consolidated-final-2026-10-11`,
`public-playtest-2026-10-12` and `felix-merge-2026-10-12`, but not on every older branch.

What exists:

- Web: `web/src/feedback/` (`FeedbackLauncher`, `FeedbackSheet`, `context.ts` builds a
  viewer-safe context, so no hidden cards). Category is bug / confusing / idea plus free text.
- Server: `POST /api/feedback` (`crates/ti4-server/src/http/feedback.rs`,
  `crates/ti4-server/src/feedback.rs`), per-IP rate limit (`public/ratelimit.rs`).
- Storage: one JSON file per report at `<data_dir>/feedback/YYYY-MM-DD/fb_<unix>_<hash>.json`
  (public alpha: `/var/lib/ti4-public/feedback/`, owned by `ti4`, readable as root). Each holds
  category, text, build id and context (game id, faction, phase, pending decision, recent public
  events). The game id points at `/var/lib/ti4-public/game_*`, so a report can be replayed.
- Names (added 2026-10-13): each report says who sent it, inside `context`. `context.sender` has
  `nickname` (the roster nickname of the viewer's seat), `position`, `lobby_nickname` (the nickname
  this browser remembers from the lobby, only when it differs) and `typed_name` (the optional "Your
  name" field, shown only when there is no nickname, for example a spectator). With "Include game
  info" ticked, `context.players` lists every seat: `seat`, `position`, `nickname`, `bot`,
  `faction`, `is_sender`, so "np gave me a note" maps to a person. The sender block is sent even
  with the box unticked (the sheet's privacy line says so); the table is not. Nothing secret:
  the seat id is public to the table, the seat token (`playerSession`), nonces and passwords are
  never read. Older reports have none of these keys and still load and list. Names are
  clipped to 300 characters by the client and stripped of control characters by the lister.
- Reading it: `tools/public/list_feedback.sh [-n N] [-v] [DATA_DIR]` or
  `server feedback-list --data-dir DIR --limit N --verbose`. Both print a line under each header,
  `from Alex | table: Alex* (sol), np (letnev)`, when names are present (`*` marks the sender).
  The jq fallback of the script needs jq, which is not installed on every box.

Offline games (added 2026-10-14, branch `offline-t10-feedback-local`): the single-player game that
runs in the browser has no server game, so its reports carry what replays it. The web queue
(`web/src/offline/feedbackRoute.ts`, `feedbackRequestBody`) stores a report on the device and posts
it in the background; the contract with `POST /api/feedback` is:

- `report_id` (optional, 8 to 64 characters of `A-Za-z0-9_-`): generated once per report by the
  client and kept across retries. The server stores a report with a given id once; a second send
  (the answer was lost, the client retried) gets the first report's `id` back with
  `"duplicate": true`, even after midnight or a server restart (the memory is the file
  `feedback/.ids/<report_id>`, written after the report; a marker without its report is ignored).
  Without an id every send is a new report. A retry still counts against the per-IP rate limit.
  The stored file has `report_id` next to `id`.
- `context.local` (optional object, any report): `seed`, `setup`, `answers` (the human's option
  ids), `engine {build, content_digest}`, `wasm_sha256`, and `save`, the host's `save_state`
  object (`seed`, `setup`, `answers`, `redo`, `never`, `event_times`, `event_versions`,
  `generation`, `version`, `decisions`, `decisions_hash`; further keys are kept). `seed`, `setup`
  and `answers` must equal the save's. Malformed `local` is a 400 with a message.
- Size caps: text 4000 characters and the rest of `context` 200 KiB as before; `context.local` has
  its own 2 MiB cap (a long game's save is well under 1 MiB: a few thousand answers, one time and
  version per event), at most 50 000 answers. The route's body cap rose from 256 KiB to 3 MiB to
  fit both; the check on the ordinary context is unchanged, so the larger body only buys room for
  `local`. The other routes keep 8 KiB.
- Listing: `tools/public/list_feedback.sh` and `server feedback-list` print
  `source: offline | seed 7 | build abc | decisions 118 | answers 40 | replay: feedback-save <id>`
  under the header of an offline report; `-v` leaves the save, answers and setup out of the context
  dump. Reports without `local` print as before.
- Extracting the save: `tools/public/list_feedback.sh -s <fb_id or report_id> -o save.json [DATA_DIR]`
  (jq, or with `TI4_SERVER_BIN` set the server's `feedback-save --data-dir DIR --out FILE ID`).
  The deployed alpha binary predates this; use a binary built from this branch or jq.
- Reproducing the position: `cargo run -p ti4-wasm --example replay_save -- save.json
  [--snapshot snapshot.json]` plays the save with the host crate the browser runs (the file may also
  be the stored report). It prints whether decisions and their hash came back the same
  (`resume_check`), the round/phase and the pending question, and exits 1 when this build decides
  otherwise (then check `engine.build` and `content_digest` in the report against the build you
  replay with).

Not built from the options below: no Discord/Telegram/GitHub forwarding (the file on the server
is the only copy, so the team needs server access or someone to relay it), no confirmation
reference id for the player. The first alpha batch (39 usable reports, 12-13 Oct, build
`23cfb192`) was triaged on 2026-10-13.

The text below is the original 2026-10-06 plan and is kept for the open questions.

Idea: a "Give feedback" button in the web UI. The player writes feedback, and it is sent
together with the game history. Collected 2026-10-06 for later; nothing is built yet.

## Constraint

Not everyone on the dev team has access to the server, so storing feedback only on the server
is not enough. It has to land somewhere the whole team can see.

## What the server already has

- Every game is persisted under `data/games/<game_id>/` (`decisions.jsonl`, `events.jsonl`,
  `history.json`, `init.json`, `lobby.json`). The history therefore does not need to be uploaded
  by the UI: the endpoint can attach it from disk.
- Caveat seen in the nightly runs: the persisted copy can lag behind the live game, so the
  endpoint should flush or snapshot the session first.

## Receiving options

| Option | Setup | Notes |
|---|---|---|
| Discord channel webhook | create a webhook URL, put it in a server env var | simplest; no bot; file attachments up to about 25 MB, enough for a game history; whole team sees it |
| Telegram group bot | create a bot, bot token in an env var, add it to a group | about as easy; files up to 50 MB |
| GitHub issue | fine-grained token on the server | durable and linkable to fixes; the API cannot attach files to issues, so the history would go in a secret gist linked from the issue |
| Email | SMTP credentials or a mail service | most setup (deliverability, spam handling) for the same result |
| File on the server | none | keep as a fallback so a failed webhook never loses feedback; not shared with the team on its own |

Slack incoming webhooks cannot attach files, so Slack is a poor fit.

## Leaning

A chat webhook (Discord or Telegram, depending on what the team uses) that posts the feedback
text plus the history as an attached file, with the plain file on the server as a fallback.

## Open questions

- Which chat does the team use, Discord or Telegram? (Decides the first implementation.)
- Rate limiting per game to stop spam; the webhook secret lives in the server environment.
- What to include besides the text: game id, player seat, game version, browser/user agent,
  and the current screen state or a screenshot?
- Privacy: whether private cards in the history may be sent, since the history contains hidden
  information (hands, secret objectives).
- Should the feedback also be visible to the player afterwards (a confirmation with a reference id)?

## Rough shape if built

1. `POST /api/games/{game_id}/feedback` with `{ text, ... }` and the player session header;
   validates the seat, applies the rate limit, snapshots the game history.
2. Writes `feedback-<time>.json` next to the game's files (fallback, always done).
3. Posts to the configured webhook with the history attached; failures are logged, never
   shown as lost feedback to the player.
4. A button and dialog in the web UI (`web/src/components`), with a Vitest test and a server
   test for the endpoint.
