# How web-game undo and redo work

**Short answer:** the server saves the game's **starting position and the choices
players made**. To undo, it starts a new Rust engine game at that starting
position and plays the saved choices again, stopping before the choice being
undone. Redo plays that choice again. It does **not** keep a copy of the game
state after every choice.

For example, if the accepted choices are `A → B → C`, undo produces a new game
by replaying `A → B`. Choice `C` remains available for redo. If a player makes
a different choice instead, the saved redo future is discarded.

## What data is saved?

| File | Contents | When used |
|---|---|---|
| `init.json` | Initial `GameState` (including the game RNG seed and any decks already set up), seating, game seed, and map information | Starting point for every replay |
| `decisions.jsonl` | One `DecisionRecord` per accepted choice: player, chosen option ID, prompt, offered IDs, and optional decision context | Original append-only choice history |
| `events.jsonl` | Event-log entries (e.g. decision resolved or phase changed), visibility, and a decision count | Displays history; maps a log entry to an undo target |
| `snapshot.json` | **One** periodically overwritten `GameState`, with its decision count | Pre-undo recovery validation, not an undo point |
| `history.json` | After the first rewind: active decisions and events, redo decisions and events, event-ID counter, version, and generation | Authoritative timeline for subsequent play and recovery |

On the original, append-only timeline, `snapshot.json` is written after **32
more accepted decisions** than the previous snapshot. It is replaced, not
accumulated; there is no snapshot per step. After a rewind, recovery reads the
bounded, checksummed `history.json` instead of the old append-only logs and
ignores the old `snapshot.json`. The previous files are left on disk, but no
longer describe the active branch. `history.json` is atomically replaced as
play continues and is limited to 64 MiB. Games without a file store have only
in-memory history during that server session.

The saved **events are not state snapshots**. An event's `decision_count` says
how many choices had been accepted when it was recorded. Older events without
this field are mapped by counting `decision_resolved` events in order.

## How is randomness handled?

The initial state records `rng_seed`. When replay starts, `Game::with_table`
initializes a fresh `GameRng` from that seed. The engine's random streams use
the pinned ChaCha8 generator, separated by purpose (dice, decks, map, and so
on). Each stream starts from a SHA-256-derived seed. The initial state also
contains setup results such as decks already shuffled; a seeded galaxy can be
reconstructed from the saved game seed and player order.

**Individual rolls, RNG positions, and RNG snapshots are not saved for undo.**
Replaying the same choices through the same engine and content consumes the
same random values in the same order, including rolls inside automatic steps.
Changing rules or random-draw order can change a replay; the server checks
reconstructed decision records/hashes and refuses a detected divergence. A
seed alone is not a substitute for the saved player choices.

See [`game.rs`](../../crates/ti4-engine/src/game.rs) (`Game::with_table`) and
[`rng.rs`](../../crates/ti4-engine/src/rng.rs).

## What about sub-steps?

`Game::step()` can resolve a choice **or** progress the game automatically
without asking anyone. Combat, timing windows, phase changes, and other
follow-up work can involve several engine steps and several separate choices.
The saved cursor counts **accepted choices**, not calls to `step()`.

Undo reconstructs the game through the chosen choice boundary, then the
engine runs any automatic work needed to reach its next pending choice. It
does not stop halfway through a combat roll, a timing effect, or an automatic
phase transition. **Undo to here** on an event-log row means the decision
boundary associated with that row; it is not an arbitrary save point inside
the event. Entries produced in the same engine step cannot be rewound
independently. Undo is available when a human choice is waiting or the game
has finished, not while a submitted choice is in flight.

## What happens when the host presses Undo?

1. The browser sends `POST /api/games/{game_id}/history` with the current
   `game_version` and its player-session credential. The server verifies that
   the credential belongs to the **current host**. Actions are `undo`, `redo`,
   `undo_pipeline`, or `restore` with an event ID. **Undo action** rewinds to
   before the latest action-phase choice, including its movement, combat, and
   other follow-up decisions; outside an action it undoes one choice. Its
   remaining decisions are available for redo. A stale version or invalid
   target is rejected. If the worker is still advancing toward the next
   decision, the browser refreshes the snapshot and retries briefly, but only
   while the decision cursor has not changed.
2. The server replays the desired decision prefix from `init.json` in a
   temporary game and checks the resulting decision hashes. It then stops the
   old worker, persists the new active/redo split, and starts a replacement
   worker. The replacement replays those choices and generates a **new**
   pending-choice nonce. Only an actual new choice clears redo; automatic
   steps do not.
3. The host receives a replacement snapshot. Other connected browsers have
   their old WebSockets closed, reconnect, and receive that same authoritative
   timeline through their own visibility-filtered snapshots. Browser event
   lists are **replaced**, not appended to; staged decision UI is cleared.

The operation is implemented in
[`session/registry.rs`](../../crates/ti4-server/src/session/registry.rs),
[`session/worker.rs`](../../crates/ti4-server/src/session/worker.rs), and
[`storage.rs`](../../crates/ti4-server/src/storage.rs). The browser side is in
[`client.ts`](../../web/src/protocol/client.ts) and
[`App.tsx`](../../web/src/App.tsx). The focused server test is
[`tests/history.rs`](../../crates/ti4-server/tests/history.rs).
