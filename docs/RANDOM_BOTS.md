# Random bots

A random bot is a seat that the game server answers by itself: at every question it picks one of
the engine's legal options at random. It is meant for testing a table (alone, on a phone, or with
friends), not for strong play.

## Using it

1. Create a lobby as usual. You are the host.
2. In the lobby press **Add random bot** (one seat) or **Fill empty seats with random bots**.
   No password is needed: your own session authorises it. Bots show a **Random bot** badge and
   are ready from the start; **Remove bot** (the x on the seat) frees a seat again.
3. Press **Start game** once the people are ready. The host is always a person; every other seat
   can be a bot.

Bots wait a moment before each decision so you can follow the game (default 600 ms, jittered
between 60% and 140%). Their moves appear in the event log like anyone's.

## Settings

| Variable | Meaning |
|---|---|
| `TI4_RANDOM_BOT_DELAY_MS` | Think delay per bot decision in milliseconds. Default `600`, `0` disables it (tests, soaks), capped at 30000. Read when a game session starts. |

Nothing else is needed: no `TI4_BOT_PASSWORD`, no advisor service, no libtorch, no child process.
Random bots do not count against `TI4_MAX_ACTIVE_BOTS`.

## API

`POST /api/games/{id}/lobby/bots` with the host's `x-ti4-player-session` header:

```json
{"kind": "random"}                          // one bot
{"kind": "random", "count": 2}              // two bots
{"kind": "random", "fill": true}            // every open seat
{"kind": "random", "nickname": "Robo"}      // one bot with a name (default "Random bot N")
```

The response is the lobby view. It lists `bot_kinds` (`["random"]`, plus `"mlp"` when the MLP bot
service is configured; a client that finds no `bot_kinds` must assume neither) and marks bot
seats with `"bot": "random"` (these also report `ready: true`, `connected: true`,
`can_take_over: false`).

| Status | When |
|---|---|
| 200 | the bots were seated (all or none) |
| 400 | unknown `kind` (`mlp` is added with `/lobby/add-bot`), `fill` together with `count`, a bad nickname, a nickname with more than one bot |
| 422 | unknown JSON fields (for example a `password`) |
| 403 | no or unknown credential, or not the host |
| 404 | unknown game |
| 409 | the game has started, or fewer open seats than requested (including none) |

`POST /lobby/remove-bot` (existing) frees a bot seat. The seat count comes from the lobby, whose
size the map template fixes at creation, so the player-count limits are the lobby's own.

## How it works

- `SeatController::BotRandom` is stored in the lobby and in the saved game, so a restart brings
  the bots back. The decider runs inside the game worker (`session/random_bot.rs`), so its
  decisions are ordinary recorded decisions: history, replay, recovery, undo and redo, turn redo,
  splice and batches see nothing special. On a replay or recovery the recorded answers are used
  and only new decisions are random.
- **Determinism.** Each pick is seeded from (game seed, seat id, decision index), where the
  decision index counts every decision answered in the game (replayed ones included). The same
  position gets the same choice, in the same process and after a restart. Consequently, a bot
  that is asked the same question at the same index gives the same answer again.
- **Undo.** Because of that determinism, an undo that would start with a bot's decision would be
  answered identically at once. History rewinds therefore walk back to the nearest decision of a
  person: Undo takes back your move together with the bots' moves that followed it.
- **Presence.** A bot has no websocket and no usable credential (its session is never handed out
  and authentication skips it). It is always shown as present and cannot be taken over.
  Bluff holds, "never offer" reaction modes and the step snapshot only concern people.
- **Think delay.** Sleeps in 20 ms slices without holding any lock, so a stop (undo, shutdown) is
  noticed immediately.

## Compared with the other bots

| | Random bot | MLP bot | Nightly browser `random` policy |
|---|---|---|---|
| Runs | in the game worker | separate `ti4-bot-agent` process over a websocket | in a Playwright browser, clicking the UI |
| Needs | nothing | `TI4_BOT_PASSWORD`, advisor service, libtorch | a browser and a running server |
| Chooses | among the engine's legal options | by the model's scores | among clickable controls |
| Reproducible | yes (game seed, seat, index) | no (temperature) | by click seed, per browser run |

The browser policy clicks a random enabled control and leans on helpers to finish multi-click
workflows (confirm a complete payment, confirm hits, confirm command tokens, propose a listed
trade). The engine already offers each of those as one question with the legal answers as
options, so a pick among them is the same behaviour one level down. What is mirrored: uniform
choice; opening a transaction is down-weighted to 0.1 like `trade-opt-` / `propose-trade-btn`
(otherwise a table bargains forever). The "steer" policy's weights (tactical action 30, end turn
20, pass 0.3, ...) are *not* applied: the nightly `random` policy does not use them.

An all-bot game takes about 900 to 1500 decisions to the end of round 9 (the nightly random runs
take 800 to 1200), and about 200 decisions per second on a debug build at delay 0.

## Tests

`crates/ti4-server/tests/random_bots.rs` (whole games, determinism, recovery, undo, turn redo,
delay) and `tests/random_bot_lobby.rs` (HTTP). The soak is
`cargo test -p ti4-server --test random_bots soak -- --ignored --nocapture`.
