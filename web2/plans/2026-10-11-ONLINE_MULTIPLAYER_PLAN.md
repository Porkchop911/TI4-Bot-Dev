# web2: online multiplayer

Status: plan, 2026-10-11. Built: phase 0 (2026-10-11), phase 1a, phase 1b, phase 2, phase 3,
phase 4 and phase 5 (2026-10-12); see "Phase 0: what was built", "Phase 1a: what was built",
"Phase 1b: what was built", "Phase 2: what was built", "Phase 3: what was built", "Phase 4: what
was built" and "Phase 5: what was built". Every phase of this plan is built.

## Goal

web2 has two modes: **Single player** (today's experience, the wasm engine in the page) and
**Multiplayer** (the authoritative `ti4-server` over a websocket). The first multiplayer version
connects as the first seat of an 8-player game; the other seats are random deciders. The bare
address stays the local game; the Dev sheet has the way into multiplayer (decision 5).

The UI (`game/`, `ui/`) must not know which one it is in. The only places that may know are the
entry (`main.tsx`) and the transports in `session/`. Content, rule text and the
catalogue come from the local wasm in both modes.

## Decisions (owner, 2026-10-11)

1. **Derived facts are computed on the server**, per viewer, with the same `ti4-view` code that the
   wasm host uses, and sent in `StateUpdateMsg`. The client never derives them from the redacted
   `state`.
2. **Drafts run on the server.** The wasm cannot run a draft in multiplayer: it has no full game.
   `Transport.runDraft` becomes asynchronous.
   The server gets two script messages (owner, 2026-10-11), and web2 does not use
   the planning runner (`StartPlanning`, `SubmitPlanningChoice`, `ApplyPlanning`). The runner,
   the batch and `web/` can be deleted (owner, 2026-10-11), but not in this plan: see "Postponed".
   - `ClientMessage::RunDraft { game_id, request_id, script }` is answered with
     `ServerMessage::DraftOutcome { request_id, base_version, outcome }`. The server forks the
     planning checkpoint (`session/planning.rs`, `checkpoint`) and calls
     `ti4_view::planning::draft::run_draft`, as the wasm host does. The server keeps no draft:
     the script stays in `localStorage`. It works on the turn of the seat too.
   - `ApplyDraft { nonce, expected_version, script }` sends a draft or a plan to the live game.
     The script is the part that the run of the draft played, in the form of `RunDraft`: the
     action, the system, the steps, and the answers by id that the draft recorded for a card or an
     ability (`{ kind: "choose", option_id }`). So a card that was played in the draft is played
     in the live game, and the player is not asked again. `submitPlan` is the same with the steps
     only. The server answers the questions of the game as a run of a draft does (`Run::answer`
     in `ti4-view/src/planning/draft.rs`: `plan_answer` for a step, the option of that id for a
     `choose`). The result has the shape of `PlanStopped`.
   - The owner does not need a draft that is applied while the page is closed, or a draft on
     another device. So the server keeps no script.
   - What a draft may be asked is the same as in the runner: both use `guard_fork` and
     `audit_offer` of `ti4-view/src/planning`. A card, an ability or a reaction window of the seat
     in the action phase is asked as in the live game (Flank Speed after the activation); its
     answer is an entry `{ kind: "choose", option_id }` of the script. A window in a space battle
     or an invasion ends the draft, in both.
3. **Undo**: every player can undo their own actions back to the point where hidden information was
   revealed, randomness was used, or another player acted. Further undo is host-only (the event
   log, as in `web/`). The event log is not in web2 yet, so host-only undo is **out of scope**.
   Single player keeps its undo with no limit (owner, 2026-10-11): it is a tool for testing.
4. **The server is at the address of the page**, as in `web/`: `/api/...` and
   `ws(s)://<host>/ws/games/<id>`. `web2/vite.config.ts` gets the `proxy` block of
   `web/vite.config.ts` (`/api`, and `/ws` with `ws: true`; the port from `TI4_E2E_BACKEND_PORT`,
   8080 by default). Caddy needs no change. There is no `?server=<url>`.
5. **No mode picker for now.** `/` stays the local game. The Dev sheet has a row "Multiplayer"
   that loads the online address: `?online` starts a game, `?online=<game_id>` is that game. An
   online game has the header link "Local game", as the demo has.
6. **Single player and multiplayer use the same code** wherever they can (owner, 2026-10-11), so
   that a test of single player is mostly a test of multiplayer. A rule or a fact has one
   implementation, in Rust, and both hosts (`ti4-wasm`, `ti4-server`) call it. What this asks for:
   - the facts: one function of `ti4-view` (phase 1a);
   - a draft: `run_draft`, in both;
   - a plan or a draft that is sent: one function of `ti4-view` that both hosts call, the same
     that answers a run of a draft (`plan_answer`, and a `choose` by its id). The wasm host gets an export for it, and the TypeScript `planAnswer` is removed
     (`NEXT.md` has this as open). Phase 0;
   - Never cards: `ti4_engine::reaction_modes::NeverOffer` in both. The wasm host gets an export
     to set the mode of a card, and `filterOffers` of the page is removed. Phase 0;
   - the other seats: `SeededRandom` in both;
   - the page: `useLiveSession` and everything above it has no branch for the mode.
     What stays different, on purpose: the undo (decision 3), the dev tools of a local game, and
     the saved game.
     One suite of tests of the `Transport` contract runs against both transports (phase 2).
     The aim is less code, not more: each of these removes a copy. The two transports end as thin
     adapters over the same operations (an update, an answer, a script that is run, a script or a
     plan that is sent, the mode of a card, an undo): one calls an export of the wasm, the other
     sends a message.

## Non-goals

- Lobby UI for several humans, joining a game, spectating (the transport takes a `viewer` already;
  the screens come later).
- Host-only history (`change_history`), restore, redo, the event log.
- Seat changes (`switchSeat`), stepping and scenarios online: these are dev tools of a local game.
- A mode picker as the start screen, and the installed app in multiplayer.
- Any new layout. Pattern 10 of `AGENTS.md` holds: live, draft and history are one screen.

## Findings that shape the plan

- `Transport` (`session/transport.ts`) is already the seam, and its comments name the websocket
  (`StateUpdateMsg`, `SubmitChoice`). `useLiveSession(transport)` is the only consumer.
- `SessionUpdate` (`session/wire.ts`) = `StateUpdateMsg` fields + six facts that "the wasm host
  fills": `tactical`, `exploration`, `action_cards`, `secret_objectives`, `score`, `agendas`. The
  server sends none of them (`NEXT.md` says the same for `exploration` and the catalogue).
  `project_session_update` (`ti4-view/src/projection.rs`) sets all six to `None`. They are filled
  by hand in three places: `ti4-wasm/src/lib.rs` (two) and `ti4-view/src/planning/draft.rs`.
- `runDraft` is synchronous and is consumed inside a `useMemo` (`useLiveSession.ts` ~l.528).
- The server's planning runner does not fit the draft of web2. It keeps the draft on the server
  (`PlayerPlan`) and takes one answer for each message, with an `AttemptIdentity`. It refuses the
  seat that has the turn (`session/mod.rs`, `PlanningError::ActivePlayer`); web2 drafts on its
  own turn too. Its answer (`PlanningEnvelope`) has no `SessionUpdate`, no `tactical` facts and
  no `movement`, `invasion` or `production` of a `DraftOutcome`.
- The batch of the server is HTTP (`POST /api/games/{id}/batch`, header `x-ti4-player-session`),
  and `ApplyPlanning` sends the draft that the server keeps. Neither fits
  `applyDraft(nonce, { choices, plan })`.
- The server has no random seat. `SeatController` is `Human`, `BotFirstOption`,
  `BotAlwaysDecline`, `BotScripted`; `ti4_engine::choice::SeededRandom` is there and not used.
  `add-bot` of the lobby starts another process behind a password. `POST /api/games` makes a
  lobby: the game starts with `ready` and `start`.
- The server's `change_history` is host-only. There is no player undo yet.
- `Transport` mixes game play with dev tools: `setStepping`, `step`, `switchSeat`; the Dev sheet is
  "in a local game" only.
- Never cards: wasm mode filters offers in the page (`filterOffers`), the server keeps them itself
  (`SetReactionMode`, `reaction_modes` in the update). `NEXT.md` already lists this.
- `web/src/protocol/` (`client.ts`, `decode.ts`, `planning.ts`, `types.ts`) is a working client of
  the same protocol. Read it; do not import it (`web/` is another app). web2's `wire.ts` names only
  the part that web2 reads.

## Phases

Each phase leaves single player working and green (`npm test`, `e2e`, `boundaries.test.ts`).

### Phase 0: the shape of the seam (web2 and ti4-wasm, no behaviour change)

1. **Async draft.** `Transport.runDraft(script)` returns `DraftOutcome | null` now. Change the
   contract to a subscription: `draft(script)` is asked, the outcome arrives as a
   `TransportEvent` `{ kind: "draft", script, outcome }`, with the script echoed so the page
   drops a late answer for an older script. `useLiveSession` holds
   `outcome` in state (`pending` while waiting) instead of a `useMemo`.
   - The wasm transport answers in the same tick (a microtask). This keeps one code path.
   - Check what the draft screens show while an outcome is pending: keep the last outcome until
     the new one arrives (no flicker), and keep the main button disabled while the outcome is for
     an older script.
2. **Capabilities.** Replace the dev methods by a `capabilities` object on the transport:
   `{ stepping, switchSeat, scenarios, save }`, each absent online. `GameSession` exposes it; the
   Dev menu entry shows from `capabilities`, never from a mode flag. `LocalApp` keeps the saved
   game and the scenarios; they are not part of `Transport`.
3. **Never cards move into the engine of both hosts** (decision 6). `useLiveSession` stops
   filtering offers. The wasm host wraps the seat in `NeverOffer` and has an export to set the
   mode of a card; `filterOffers` is removed. The websocket transport reads `reaction_modes` and
   sends `SetReactionMode`. The page's `cards.never` is a view of the transport. Check that the
   saved game and the undo of a local game still hold (`AGENTS.md`, Action cards): the decline
   of a window is then no saved answer.
4. **Undo as a fact, not a method.** `canUndo` stays in the update event. Its meaning becomes
   "this seat may take back its last answer" in both modes (see phase 3).
5. **One answer to a plan** (decision 6). A function of `ti4-view` answers a parked question from
   a script (`plan_answer` for a step, the option of that id for a `choose`, as `Run::answer` of
   `draft.rs` does), and the wasm host has an export for it that is called
   while the game is parked. `submitPlan` and `applyDraft` of the wasm transport use it; the
   TypeScript `planAnswer` is removed.

Tests: the existing `wasmTransport.test.ts`, `useLiveSession` tests and `neverOffer.test.tsx` pass
unchanged in meaning; add a test for a late draft outcome that must be dropped.

#### Phase 0: what was built (2026-10-11)

- `Transport` (`session/transport.ts`): `draft(script)` with the event `draft`;
  `setReactionMode(card, mode)` with the events `cards` and `passed`; `capabilities`
  (`stepping`, `switchSeat`, `scenarios`, `save`). `runDraft`, `setStepping`, `step` and
  `switchSeat` are gone. `applyDraft` and `submitPlan` kept their arguments.
- One answer to a script: `script_answer` in `ti4-view/src/planning/draft.rs`. A run of a draft
  uses it, and the wasm host has `ti4_plan` for a plan or a draft that is sent. `planAnswer`
  and `matches` of the page are removed, with their tests; the tests of the rules are in Rust.
  `ApplyDraft` of phase 1b calls the same function.
- Never cards: the wasm host wraps its seats in `NeverOffer` (`ti4_set_reaction_mode`,
  `ti4_passed`). `filterOffers` and `withoutNever` are removed.
- **Not as item 3 says: the decline of a window stays a saved answer.** The check found that a
  saved game breaks when the decline is no answer: the player sets a card to Never or back, and
  the game that is played again asks one window less or one more than the saved answers have.
  So the saved game is played below the wrapper, as a replay on the server is (`ti4_replay`),
  and the engine tells the host each window that it passed, which the transport keeps with the
  answers. An undo goes back over such a window to the answer of the player before it, in the
  engine (`Host::again`) and in the transport.
- While a draft waits for its outcome, the screen is of the outcome before and its main button
  is disabled (`DraftInfo.waiting`). Before the first outcome the draft is open with nothing to
  decide ("The game checks the draft."), so that nothing of the live game can be sent from it.
- Tests: `liveDraft.test.tsx` (a late outcome is dropped; the game changes under a draft),
  `neverOffer.test.tsx` (the page over a fake transport), three tests of the Never cards against
  the real engine in `wasmTransport.test.ts` (scenario "Late game", seat d, Nav Suite), a test
  of `script_answer`, and a part of the host test of `ti4-wasm`.

### Phase 1a: one place fills the facts (Rust, wasm only, no behaviour change)

1. `ti4-view` gets `FactsContext { content, sources, galaxy }` and one function that fills the
   six facts of a `SessionUpdate` (`ti4-view/src/{tactical,exploration,action_cards,secret_objectives,score,agendas}.rs`).
   `project_session_update` takes the context and calls it. The three places that fill the facts
   by hand use it. Do not keep two copies (see the note on `plan_answer` in `NEXT.md`).
2. Guard: `wasmTransport.test.ts` and the recorded fixtures of web2 do not change.

#### Phase 1a: what was built (2026-10-12)

- `FactsContext { content, sources, galaxy }` and `fill_facts` in `ti4-view/src/projection.rs`:
  the one place that fills the six facts. `galaxy` is optional, as a host has it: without the
  map an update has no `tactical` and no `exploration`. `tactical` is of the choice in the
  update, so only the seat that is asked has it (as the wasm host did).
- `project_session_update` takes `Option<&FactsContext>` as its last argument and calls
  `fill_facts`. The three places use it: `show` and the last update of `ti4_play` in
  `ti4-wasm/src/lib.rs` (`facts_context`), and `run_draft`. No host calls a `project_*_facts`
  function itself any more.
- **The context is optional because of the server.** `ti4-server` gives `None`, so its messages
  are as before. Phase 1b gives the context there (it needs the `Galaxy` of the session, not
  only the `GalaxyLayout`) and sends the fields; `None` can then go.
- **One change of behaviour, in a draft.** `run_draft` filled `tactical` and `exploration`
  only. Its updates have all six facts now, so the Cards, Secrets, Objectives and Agendas
  sheets have their facts while a draft is open; they are of the copy that the draft played. An
  outcome has up to four updates, so it is larger; `DRAFT_CAPACITY` (1 MiB) was not reached in
  the tests.
- Guard: all six fixtures were recorded with their commands before and after the change, and
  the recordings are the same byte for byte. `wasmTransport.test.ts` is not changed.
- **Found, not changed:** the fixtures in the repository are older than the recorder. They have
  no `action_cards`, `secret_objectives`, `score` and `agendas`, and `update-production.json` is
  of the late-game scenario as it was before. A fixture is not edited; record them again when a
  test needs these facts (phase 2 replays fixtures).
- Tests: `the_facts_are_filled_with_a_context_and_only_then` in `projection.rs`, and the facts
  of a draft update in `a_draft_goes_through_activation_and_movement_and_leaves_the_game_as_it_was`.
  `cargo fmt`, `cargo l-web`, `cargo t-web`, `npm test`, `e2e/local.spec.ts` and
  `e2e/phone.spec.ts` are green with the wasm built again.

### Phase 1b: the server sends the facts, plays the other seats and runs a script (Rust)

1. **Facts.** `StateUpdateMsg` and `InitialSnapshotMsg` (`projection.rs`) carry the six facts, per
   viewer, from the **full** game state, through `FactsContext` of phase 1a
   (`project_session_update` with the context; the snapshot calls `fill_facts` or goes through
   the same projection). They are optional fields (`skip_serializing_if`), so
   `web/` and the fixtures keep parsing. `GET /api/content/catalog` exists; web2 uses the wasm one.
2. Redaction tests, in the style of `tests/projection_redaction.rs`: an opponent view and a
   spectator view contain no deck order, no other seat's hand, no secret objective of another
   seat in any of the new fields, nor in `held_action_cards`, `held_secret_objectives` and
   `scored_secret_objectives` of `PlayerView`.
3. Golden fixtures: add the new fields to the actor fixture; keep `web/`'s fixture test green.
4. Write down the size of one update of an 8-seat game with the facts. web2 does not read
   `state`; act only if the size is a problem.
5. **Random seats.** `SeatController::BotRandom` with `SeededRandom`, seeded from the seed of the
   game and the seat. `fill_seats: "random"` on `CreateGameRequest` fills the empty seats, and
   readies and starts the lobby in the same request. Test: after a restart of the server the
   game is the same (the answers of the seats are in the decision log, or come from the seed
   again: read `prior_decisions` first). The e2e of phase 2 needs this.
6. **Script messages** (decision 2): `RunDraft` and `ApplyDraft`, each with an integration test
   that plays the scripts of the tests in `ti4-view/src/planning/draft.rs`.
7. A test of `run_draft` for a card of the seat in a window: Flank Speed after the activation,
   played and declined, as `tests/it/planning_runner.rs` has for the runner. `ti4-view` has no
   such test yet.

#### Phase 1b: what was built (2026-10-12)

- **Facts.** `StateUpdateMsg` and `InitialSnapshotMsg` have the six facts as optional fields.
  Both go through `project_session_update` with `facts_context` of `ti4-server/src/projection.rs`;
  the session keeps the `Galaxy` for it (`SessionShared.galaxy`). The context is no longer
  optional (`project_session_update(…, &FactsContext)`): a message of a session without a map
  has four facts and no `tactical` and no `exploration`, as the fixtures have. The facts are made
  from the full state; `state` of a message stays redacted. `PROTOCOL_VERSION` is still 3: every
  change adds an optional field or a message.
- **Redaction** (`tests/it/projection_redaction.rs`): for every seat and a spectator, on the
  late-game scenario of eight, the facts and the three hand fields of `PlayerView` name no
  action card and no unscored secret objective of another seat, no card of the action card,
  secret objective or agenda deck, and the exploration decks sorted. `tactical` is sent to the
  seat that is asked only (`tests/it/draft_scripts.rs`).
- **Fixtures.** The three snapshot fixtures have `action_cards`, `secret_objectives`, `score` and
  `agendas` (their game has no map). The fixture test of `web/` is green.
- **Size.** One update of the late game of eight is 80 kB: the view 35 kB, `state` 38 kB, the
  facts 5 kB, the layout 2 kB. The facts of an open movement add 0.6 kB (six seats, round 1).
  No problem; nothing was done.
- **Random seats.** `SeatController::BotRandom`: one `SeededRandom` for each seat, seeded from
  the seed of the game and the place of the seat. The answers of the seats are in the decision
  log as every answer is. A recovery replays the log and, for each replayed answer of a random
  seat, draws from its stream and drops the draw, so the seat draws next what it would have
  drawn (`RandomSeat` in `session/worker.rs`). `tests/it/random_seats.rs`: a game that is
  recovered from its log goes on as the game that was never stopped.
- `fill_seats: "random"` of `POST /api/games` fills the other seats, readies every seat and
  starts the game in the request (`create_player_game_with_random_seats`); the response has
  the lobby as running. The seats of a game are in `init.json`, so a recovered game has them. A
  seat that the server plays cannot be taken over. **Not done:** the lobby view does not mark a
  random seat (it reads as a player "Random 2" who is not connected); web2 has no lobby screen.
- **`RunDraft`** `{ game_id, request_id, script }` is answered with `DraftOutcome`
  `{ request_id, base_version, outcome }`. The copy is the planning checkpoint of the session
  (the game at its last completed step), also in the strategy phase and on the turn of the
  seat. A connection runs one draft at a time and only the newest request waits: an older one
  gets no answer. A session that cannot be copied answers with a failed run. A script has at
  most 512 entries, and a client message 64 kB now (it was 4 kB).
- **`ApplyDraft`** `{ game_id, nonce, expected_version, script }`. The answer is not a field of
  an update: it is a message of its own, `DraftApplied { nonce, stopped? }`, to the seat.
  `stopped` is `{ applied, remaining, interrupted, reason }` as `PlanStopped`, counted in
  entries of the script (the transport takes off the answers before the steps, and adds
  `destination`). Without `stopped` the game played the whole script. A stale nonce or version
  is `ActionRejected`, as for `SubmitChoice`.
  - **Refused whole, with no undo.** The script is first played on a copy of the game
    (`check_script` in `session/script.rs`): the checkpoint, the decisions since it, then the
    script, with the dice of the live game. When the game would refuse an entry, nothing is
    sent, and `stopped` has `applied: 0` and `interrupted: false`. The copy cannot tell what
    another seat decides. So after such a decision a script can still stop with its earlier
    entries in the game: `interrupted: true`, the rest staged again. When the copy does not
    come to the open choice, the script is sent with no check.
  - While a script answers, the seat gets no update for the choices in between. `DraftApplied`
    comes before the update of the choice that is open after it.
  - A script ends when the game asks its seat something that it does not answer, when another
    seat that a person plays is asked (a random seat does not end it, as in a local game),
    when the action is over, or when the turn passes.
- Tests: `tests/it/draft_scripts.rs` plays the scripts of the tests of `draft.rs` against a
  session and over a websocket; `session/script.rs` has the cases of a script that stops. The
  test of Flank Speed is in `draft.rs`
  (`a_card_of_the_seat_is_asked_after_the_activation_and_played_or_declined`).
- Phase 2 reads from this: `DraftOutcome.outcome` needs no mapping; `DraftApplied` becomes
  `plan` of the next update; the handshake field for the content version is still to add.

### Phase 2: the websocket transport (web2)

New `session/wsTransport.ts` implementing `Transport`:

- `connect(gameId, playerSession)`, at the address of the page (decision 4): `Subscribe`, then `InitialSnapshot` and `StateUpdate` map
  to `TransportEvent` `update`. `PROTOCOL_VERSION` is checked; a mismatch is an `error` event with
  the versions in the text.
- `submitChoice(nonce, optionId)` sends `SubmitChoice` with `expected_version` from the last
  update. `ActionRejected` (stale version, stale nonce) becomes the next update (a refreshed
  snapshot) and a toast; it is never silent.
- `submitPlan` and `applyDraft` send `ApplyDraft`. Its result becomes `plan` of the next update
  (`PlanStopped`). The batch of `session/batch.rs` is not used, so its missing step for the
  landings does not matter here.
- `draft(script)` sends `RunDraft` with a new `request_id`; the answer is the `DraftOutcome`, with
  no mapping. An answer for an older `request_id` is dropped. When `game_version` changes while a
  draft is open, the transport sends the script again.
- Not read for now, and named in `wire.ts` as such: `events` and `Event`, `auto_resolved`,
  `history`, `current_path`, `PendingChoice`, `TurnStatus`.
- The keys of `localStorage` (`planKey`, `draftKey`, `cardsKey`) are per `game_id` online. The
  staged movement is kept under the nonce of its choice.
- Reconnect: exponential backoff, resubscribe with the stored `player_session`, the first update
  after it is a full snapshot (`game_version`). While disconnected, an `error`-free
  `{ kind: "offline" }` state is shown in the toolbar (one signal, pattern 6).
- `catalog`: the same wasm catalogue as in single player (`Engine.catalog`), so the transport takes
  the `Engine` too.
- **Handshake check**: the server's content/engine version must equal the wasm's. Add a field to
  the snapshot (hash of `ti4-content` as built in) and compare it with `Engine.hash`'s content
  part; refuse to play on a mismatch. Without this, a stale CDN bundle shows wrong card text.

Tests: a fake websocket in vitest that replays the fixtures of `ti4-server/fixtures`
(actor, opponent, stale submission, game over); an e2e against a real `ti4-server` started by
`playwright.config.ts` (see how `web/e2e` does it), through the `proxy` block of decision 4,
playing the first turns of a seeded game with random seats.

One suite of tests of the `Transport` contract (decision 6) runs against the wasm transport and
against the websocket transport with a real server: an update after an answer, a refused plan, a
plan that another question stops, a draft, a draft that is applied, a Never card. A case that
only one mode has (undo, stepping) is outside the suite.

#### Phase 2: what was built (2026-10-12)

- **`session/wsTransport.ts`**: `createWsTransport(engine, { gameId, playerSession }, play?)`.
  It subscribes, and `InitialSnapshot` and `StateUpdate` become the `update` event (the fields
  of a `SessionUpdate`; `state` is dropped). `capabilities` is empty, `canUndo` is always false
  and `undo()` does nothing (phase 3). The messages are named in `wire.ts` (`ServerMessage`,
  `ClientRequest`, `PROTOCOL_VERSION`), with what is not read.
- **Two events are new**, for both transports to tell and only this one does:
  `{ kind: "offline" }` and `{ kind: "rejected", reason }`.
  - Offline is the status of the toolbar ("Offline · reconnecting"), and the panel says "The
    game cannot be reached." where it says who the game waits for. No choice is open while
    offline (`openChoice`), so nothing is sent; the next update ends it.
  - `ActionRejected` is a toast with the reason, and the last update is told again, so the
    main button is free again. After a stale answer the transport waits one second for the
    update that explains it; when none comes it subscribes again on a new socket, and the
    snapshot is the game as it is. A protocol error after the first update is a toast too;
    before it (the server does not know the player) it is an `error` event.
- **A plan and a draft** are sent as `ApplyDraft`. `DraftApplied` becomes `plan` of an update:
  a refusal (`interrupted: false`) is told at once with the last update, because the server has
  no new one; a stop (`interrupted: true`) is told with the next update. `applied` and
  `remaining` are counted in steps: the answers by id before them are taken off.
- **A draft** is `RunDraft` with a number as `request_id`; only the answer to the newest
  request is told, with its script.
  - **Not as the plan says: the transport does not send the script again when `game_version`
    changes.** The page does (`useLiveSession` asks again with each update, in both modes), and
    `Transport` has no way to say that a draft is closed, so the transport would run a closed
    draft for ever. A draft that is asked while offline is not sent; the update after the
    reconnect makes the page ask again.
- **Never cards**: `SetReactionMode` is sent, and `cards` is told from `reaction_modes` of the
  next message. The server tells the game again for such a change; the transport tells no
  update for a message with the version and the nonce that it has, so what is staged stays.
  **One field is new on the server**: `AutoResolvedNote.passed`, the cards of a window that
  was declined for a Never card. The transport tells `passed` from it. (`auto_resolved` is
  read for this only.)
- **Reconnect**: a lost socket is `offline`; the next try is after 0.5 s, then twice as long
  each time, up to 15 s; at once when the server replaced the game (close code 4001). A ping
  every 10 s keeps the player present.
- **Handshake**: `InitialSnapshotMsg.content` is the digest of the corpus of the server
  (`ti4_content::provenance::embedded_digest`), and `Catalog.content` is the same of the wasm
  (`ti4_catalog`). The transport refuses to play when they differ (an `error` event with both,
  no reconnect), and so when the protocol version of any message is another. `Engine.hash` is
  of the whole file and has no content part, so the catalogue has the field. The digest is of
  the parsed records: it does not see the groups that are written by hand in `ti4-content`.
- **`localStorage`**: `onlineKeys(gameId)` (`ti4.online.<game_id>.session`, `.movement`,
  `.draft`). The Never cards are kept by the server, so there is no key for them.
- **The proxy** of decision 4 is in `vite.config.ts` (`/api`, `/ws`).
- **The least of phase 4, because the e2e needs a page** (`dev/OnlineApp.tsx`, `main.tsx`):
  `?online` asks the server for a game of eight with random seats and goes to
  `?online=<game_id>`; that address opens the game as the player that the browser kept, or as
  a spectator. The header has "Local game". Still phase 4: the row of the Dev sheet, the
  nickname, the seed and the map, and what the page says of a game that the server no longer
  knows (it says "Connecting to the game…" and tries for ever).
- **Tests.**
  - `wsTransport.test.ts`: a socket of the test replays the five fixtures of
    `ti4-server/fixtures` and the script messages; fake timers for the reconnect.
  - `transportContract.test.ts`: the same five cases against the wasm transport and against a
    real server (`target/debug/server`, started by the suite; skipped when it is not built):
    an update after an answer, a refused plan, a plan that the landings stop, a draft and a
    draft that is applied, a Never card. The games are not the same game, so a case finds its
    system and its ship in the facts. The Never case is the setting only: the server cannot
    start the late-game scenario that has the window; `tests/it/reaction_modes.rs` has the
    window on the server.
  - `liveOnline.test.tsx`: the page over a fake transport, for `offline` and `rejected`.
  - `e2e/online.spec.ts`: `playwright.config.ts` starts the server with `cargo run`. A seeded
    game through the proxy (the first turn, a draft with a reload, Apply, four sheets),
    `?online`, and a connection that the test cuts.
- **Found by the contract suite, not changed: a plan that the action ends before.** A plan
  with a step after the end of its action (a movement, then a purchase where nothing is
  produced) is refused whole by the server (`check_script` plays it on a copy) and is played
  by a local game, which then says "interrupted" with the movement in the game. The server is
  right; the wasm transport has no copy to check on. The page does not send such a plan.
- **Found, not changed:** the seats of a game on the server have ids like `player_1`, and the
  page names a seat by its id ("Seat PLAYER_1", cut to "Seat PLAY…" in the table). The
  nickname is in the lobby view only. This is for the lobby screen.

### Phase 3: undo for the seat (Rust + web2)

Rule: a seat may undo its own decisions back to, but not past, the first of: a hidden
information reveal (a card drawn, an exploration or a secret objective shown), a random result
(a roll, a shuffle), or a decision of another seat.

1. **Server**: a per-seat undo point. The decision log (`DecisionLog`, replay in
   `session/replay.rs` `replay_session`) already records every decision. A recorded decision is
   a _barrier_ when one of these three holds:
   - **The engine latched it** (`barrier: "random"` or `"revealed"`). The engine has a mark for
     this already: `ExecutionObservation` (`ti4-engine/src/observation.rs`), with the bits
     `randomness` and `hidden_information`. `GameRng::stream` (`ti4-engine/src/rng.rs`) sets
     `randomness` for every die and shuffle. `GameRng::observe_hidden_information` sets the
     other; its callers are the exploration draw (`exploration.rs`) and the check of the secrets
     of a seat that lost its home (`invasion.rs`). `activity_stop`
     (`ti4-view/src/planning/fork.rs`) is how a draft reads the latch.
   - **The view of the seat gained a card** (`"revealed"`). A plain draw sets no bit:
     `action_cards::draw` and `secrets::draw` take the top of a deck that is in order already.
     Only a comparison of the view of the seat before and after the decision finds it. Use the
     part of `validate_knowledge` (`ti4-view/src/planning/audit.rs`) that compares cards,
     relics, attachments and revealed objectives. Its rules for technologies and for an open
     combat are for a draft, not for undo: research of the seat itself is not a barrier.
   - **The actor is another seat** (`"other_seat"`). A decision of a random seat is a decision
     of another seat.

   A latch is an event, so a view cannot show it: a roll with no hit and a shuffle leave the
   view as it was. A view cannot show a short reveal either (below). This is why both checks
   are needed.

   **One change to the engine**: `hooks_cards::reveal` (`ti4-engine/src/factions/hooks_cards.rs`;
   `reveal_hand` calls it) sets `hidden_information`. This is where a seat is shown the cards
   of another seat (Mageon Implants, Spy Net, `action_cards::look_at_hand_and_take`). It writes
   a row `cards:reveal:<scope>|<source>|<viewer>|<owner>|<kind>` to `GameState::faction_marks`
   (`ti4-model/src/view.rs` `mark_visible_to`), but a reveal of `RevealScope::Choice` is cleared
   when its choice closes, and the view of the planning checks does not have these rows. So
   without the latch a seat could look at a hand and undo it. The line does not change what the
   game does, and a draft that looks at a hand now ends there too, which is right. Add no other
   mark to the engine.

   **Not a barrier**: the commander of the Naalu (`SeatObservation::agenda_deck_ends` and
   `naalu::may_view_promissory_hand`, read by `SeatObservation::revealed_cards`). The seat sees
   these cards all the time, from the position, and no decision reveals them (owner,
   2026-10-12).

   **Not a barrier**: the other reasons that end a draft (`StopReason::UnsupportedOffer`,
   `UnsupportedSegment`, `UnsupportedParticipation`, `StepLimit`, `MovementComplete`). They say
   what a draft can show, not what a seat learned. So `audit_offer` has no part in undo.

   How to find the barrier: the latch does not reset, and `Game::fork` drops its binding. So the
   replay binds a new `ExecutionObservation` (`Game::bind_observation`) before each decision and
   reads it after, and so knows which decision latched.

   `canUndo` for a seat is "the last decision is its own and lies after the last barrier". Undo
   replaces the session like the host history does (`session/registry.rs` `HistoryAction`), but
   is allowed for the seat and bounded by the barrier.

2. **Protocol**: `ClientMessage::Undo { protocol_version, game_id, expected_version }`; the update
   carries `can_undo` per viewer (extend `HistoryStatus` with a bool; do not reuse `cursor`, which
   is the host's). A rejected undo says why (`barrier: "revealed"|"random"|"other_seat"`).
3. **web2**: `wsTransport.undo()` sends it; `canUndo` of the update drives the button, as now.
   **The server returns the steps that the undo took back** (owner, 2026-10-11), as `undone`
   with the next update, in the shape of `PlanStep[]`, as the wasm transport does.
   - Why the page needs them: one undo takes a movement back whole, and so the landings. The
     movement is then staged again as it was sent, so that the player can change a part of it
     and send it again (`AGENTS.md`, Local play). Without the steps the step would be empty.
   - Why the server: it has the decision log, so it knows which answers the undo took back. This
     holds after a reload too. A page that keeps what it sent loses it with a reload, and would
     have to guess how many of its plans one undo covers.
   - It is the same in both modes (decision 6): the websocket transport fills `undone` as the
     wasm transport does, and the page has one path.
4. The wasm transport keeps its undo with no barrier (decision 3). So single player undoes
   further than multiplayer: this difference is wanted.

#### Phase 3: what was built (2026-10-12)

- **The track of a session** (`ti4-server/src/session/undo.rs`, `UndoTrack`): the decisions
  after the last barrier, each with the choice that was asked and the option that answered
  it. They are all of one seat that a person plays. It is kept while the game is played and
  while its log is replayed, so a recovered game and the session after an undo know the same.
  - Every seat's decider is wrapped (`Watched`, outside the replay): it tells the track each
    answer. An answer of another seat, or of a seat that the server plays, empties the track
    (`other_seat`).
  - The track looks at the game where it asks a seat (`Table::on_observed_offer`) and after
    each step. There it reads the latch (`random`, `revealed`), and compares the view of the
    seat with its view at the look before (`revealed`).
  - `can_undo` of a viewer: the track is of its seat, the game waits for the seat, and no
    answer or script is on its way.
- **Not as the plan says: the latch is read and cleared, not bound again.** The game asks
  several decisions in one step, and nothing can bind a new `ExecutionObservation` between
  them. So `ExecutionObservation::take` reads the bits and clears them (`ti4-engine`,
  `observation.rs`). It is no mark; `Game::bind_observation` is now called on the live game
  of a session too.
- **Not as the plan says: the line of the engine is at three callers, not in
  `hooks_cards::reveal`.** `reveal` has the state only, and the latch is of the `GameRng` of
  the game. The callers have both: `action_cards::show_action_card`,
  `action_cards::look_at_hand_and_take` and the commander of the Yssaril
  (`factions/yssaril.rs`) call `observe_hidden_information`. The hero of the Yssaril shows
  cards through `show_action_card`. Nothing else of the engine changed, and no test of a
  draft changed.
- **Not as the plan says: the undo ends where the game asks another seat**, also before that
  seat answered. That a seat is asked can tell what it holds (a window of a card), so a seat
  could play, see who is asked, and take it back. With random seats it makes no difference:
  they answer at once.
- **The view of the seat**: `learned_identity` in `ti4-view/src/planning/audit.rs` is the
  part of `validate_knowledge` that compares the cards, the relics, the attachments and the
  revealed objectives; `validate_knowledge` calls it and keeps its rules for technologies,
  counts and an open combat.
- **What one undo takes back** is what one undo of a local game takes back (`insideMovement`
  of the wasm transport): the last answer, and then on while the choice that is open again is
  in a movement, in the landings or in a purchase. It is decided on the track, from the
  subtype of each choice and the kind of each answer, with no replay. A window that the Never
  cards of the seat decline now is no answer of the player: the undo goes back over it, as
  the game would decline it again. The setting of now counts, not the one of then.
- **The undo replaces the session**, with the code of the host's history
  (`GameRegistry::rewind`, taken out of `change_history`; `GameRegistry::undo_player`). The
  decisions that are taken back wait as the redo of the host.
- **Protocol** (`PROTOCOL_VERSION` is still 3: fields and a message are added):
  - `ClientMessage::Undo { game_id, expected_version }`.
  - `HistoryStatus.can_undo`, per viewer, absent when false.
  - `RejectionReason::UndoBarrier { barrier }` (`revealed`, `random`, `other_seat`) in
    `ActionRejected`. A game that does not wait for the seat is `NoPendingChoice`, a version
    that is not the game's `StaleVersion`.
  - `InitialSnapshotMsg.undone`: the steps (`PlanStep::of_option` of `ti4-view`, the inverse
    of `matches_option`). **Not with an update:** the session is replaced, so the server
    closes the sockets of the game as replaced (4001), and the steps are in the first
    snapshot of the seat after it. They are told once, as a local game tells them: a second
    connection and a reload do not get them.
- **web2** (`wsTransport.ts`): `canUndo` is `history.can_undo`; `undo()` sends the message,
  one at a time; `undone` of the snapshot is told with its update and to no listener that
  comes later. The close of the socket after an undo of this side is not told as `offline`.
  A refusal is the `rejected` event with the barrier in words. Nothing above the transport
  changed: the Undo button was there for a local game.
- **Tests.**
  - `session/undo.rs`: what one undo takes back, a Never window, each barrier, and a card of
    the seat against a card of another seat.
  - `tests/it/undo.rs`: a game with random seats through the registry (a movement that is
    sent, three undos back to the turn menu, a barrier before it, and the game goes on); a
    seat that passes for 120 decisions (the undo is offered only after a decision of its own,
    a card that is drawn ends it, and a game that is recovered from its log plans the same
    undo); the messages over a websocket. `protocol_roundtrip.rs` has the wire forms.
  - `wsTransport.test.ts` (a socket of the test), one new case of
    `transportContract.test.ts` on both transports (a movement is taken back whole and its
    steps are told), and `e2e/online.spec.ts` (Undo after "Apply to Live": the movement is
    staged again and sent again).
- **Not tested against the real engine: the latch after a decision of the seat itself.** A
  seat that passes rolls nothing. The latch reaches the track (seen with the random seats of
  the test game), and the unit tests set it by hand.
- **Found, not changed:** an undo plays the game again twice from its first decision, as a
  change of the host's history does. The cost in a late game is not measured (`NEXT.md`).

### Phase 4: the entry

1. **Entry** (decision 5): `main.tsx` routes `?online` and `?online=<game_id>` to the online
   app. The Dev sheet has the row "Multiplayer"; an online game has the header link "Local
   game". `/`, `?local=…`, `?scenario=…`, `?example=…` and `?gallery` are untouched.
2. **Multiplayer start** (`?online`): `POST /api/games` with `player_count: 8`, the nickname, a
   seed, the map and `fill_seats: "random"` (phase 1b); keep the `player_session` in
   `localStorage` per `game_id`; set the address to `?online=<game_id>`; open the websocket
   transport as seat 1.
3. **Resume**: a reload with a stored `player_session` reconnects to the same game; a
   `game_id` that the server no longer knows says so and offers the local game.

#### Phase 4: what was built (2026-10-12)

Phase 2 built the routing of `main.tsx`, `dev/OnlineApp.tsx`, the kept `player_session` and the
header link. This phase built the rest. Only web2 changed.

- **The start** (`dev/onlineStart.ts`): `?online&seed=3&name=Ada&map=random`. Each part can be
  left out: the nickname is then "Player", and the server chooses the seed and the map. `map` is
  "random" (sent as `map`) or the alias of a map of the server (sent as `map_template`). The
  page goes to `?online=<game_id>` with none of it, so a reload starts no other game.
- **The row "Multiplayer" of the Dev sheet** (`dev/LocalApp.tsx`), after "Start from": it
  loads `?online&seed=<seed of the page>`, and says the seed and the seats. What it does is
  behind "?". It is shown in every local game, not from a capability: it is no tool of the
  game that is open.
  - **A choice of this phase, not of the owner: the row sends the seed of the page.** A game
    that is started from the Dev sheet is then the same game each time (the random seats
    decide from the seed of the game), as a local game of that address is. The bare `?online`
    lets the server choose. The board is not the board of the local game of that seed: the
    server builds its own map.
- **A game that the server no longer knows.** The server refuses the socket of such a game
  with 404, and a browser does not tell the page why a socket did not open. So when a socket
  closes before it opened, the transport asks `GET /api/games/<id>/map` (`OnlinePlay.known`;
  it finds the game as the socket does). Only the answer 404 ends the game: the `error` event
  "The server no longer knows this game." (`GAME_UNKNOWN`), and no further try. A server that
  cannot be reached answers something else or nothing, and the transport keeps trying. The
  page shows the text in place of the game, with "Local game" in the header. The keys of the
  browser for that game are kept.
- **Not built: a control for the nickname and the map.** They are in the address only. The
  nickname is shown nowhere yet (phase 2, "Found, not changed").
- **Tests.** `dev/onlineStart.test.ts` (the address and the request); two cases of
  `wsTransport.test.ts` (a game that is gone, and a game that the server has while the socket
  does not open); `e2e/online.spec.ts` (the request of a start with every part, and a game
  that is gone); `e2e/local.spec.ts` and `e2e/local.phone.spec.ts` open the row and come to
  the game on the server, and the phone case checks the menu of that game.

### Phase 5: the draft of a secondary (Rust + web2)

The owner needs it (2026-10-11). It is built for single player first; with decision 6 the online
draft is then the same code.

1. `run_draft` takes a scope: `PlanScope::Tactical` (as now) or `PlanScope::Secondary { primary,
   card }`. The checks are there and the server's runner uses them (`audit_secondary_offer`,
   `Game::prepare_hypothetical_secondary`, `StopReason::SecondaryComplete`); read
   `ti4-server/src/planning/runner.rs` for how it prepares the copy. `RunDraft` and the wasm
   export carry the scope.
2. web2: the row of the viewer in the seat list of a strategy card opens the editor as a private
   draft (`AGENTS.md`, Strategy cards: "✎ Private draft", "✎ Draft ready"). The script is kept
   as the tactical draft is.
3. "Draft ready": when the live game asks the seat for its secondary, the page sends the script
   (`ApplyDraft`). The page must be open; the server keeps nothing (decision 2).
4. A secondary can be drafted from the moment its card is played (owner, 2026-10-12), not only
   after the primary is resolved. The runner does the same: the engine calls
   `Game::on_strategic_action_chosen` inside the step that resolves the primary, and
   `PlanningCoordinator::announce_primary` (`ti4-server/src/session/planning.rs`) opens the round.
   `prepare_hypothetical_secondary` needs no open window. What follows from this:
   - While the primary is resolved, the copy is the last completed step: it has nothing of the
     primary yet. When the primary is done, the game has a new version and the page runs the
     script again (as for every draft).
   - Trade is the exception, as in the runner: its draft opens with the window, because the
     primary changes what the secondary costs and gives.
   - The page must know that a card was played before the window is open. Both hosts set the
     hook and send the primary and the card as a fact of the update.
     A draft before the card is played (a card that another seat holds) is not in this plan.

#### Phase 5: what was built (2026-10-12)

- **The fact.** `ti4-view/src/secondary.rs`: `SecondaryFacts` is the seventh fact of an update
  (`secondary`): who played which card, whether the primary is resolved (`open`), the followers
  in their order with what each did, and `draft` for the viewer. The state of a game does not
  say this, so a host keeps a `SecondaryWatch` for its game: the engine tells it the card
  (`Game::on_strategic_action_chosen`), and after each step it reads the window
  (`Game::open_secondary`, and the new `Game::secondary_resolutions`). `fill_facts` takes it
  from `FactsContext::secondary`. The wasm host has one watch (`SECONDARY`), bound again after
  an undo; the server has it in `SessionPlanning`, beside the round of the runner.
- **The run.** `run_draft(base, player, scope, script)`. `PlanScope` is on the wire
  (`{ kind: "tactical" }`, `{ kind: "secondary", primary, card }`); `RunDraft` and the request
  of `ti4_draft` have `scope`, and without it the script is of a tactical action. A secondary
  is prepared with `prepare_hypothetical_secondary` and asked through `audit_secondary_offer`,
  as in the runner. It is refused (`Failed`) when the watch does not say that this seat may
  draft this card now: Trade before its window, a seat that is recorded, another card. A
  script with an entry left when the secondary is over is `Refused` at that entry.
- **Sending.** Nothing new: `ApplyDraft` (`Transport.applyDraft`) with the answers by id.
- **The page.** `session/secondaryDraft.ts` is the draft (the answers with their labels, and
  `ready`), kept under `secondaryKey`. `useLiveSession` asks for one run at a time: the
  tactical draft while the Draft tab is open, else the secondary. `select/strategy.ts` is the
  list of the seats (the `seats` block of the demo), and the row of the viewer is the draft or
  the live question. The rules of the screen are in `AGENTS.md` (Strategy cards).
- **Not as the plan said.** Item 2 names the editors of the strategy cards: they are not built
  in a game of the engine, so the row opens into the question as a list. A draft that is not
  ready when the seat is reached is not sent by itself; "Send draft" sends it.
- **Protocol.** `PROTOCOL_VERSION` stays 3: `scope` and `secondary` are new and optional, and
  `web/` reads neither.
- **Tests.** `ti4-view` (`planning/draft.rs`): a secondary is drafted before the window, only
  of the card that is played, and the watch follows the window. `ti4-server`
  (`session_secondary_planning.rs`): three people, the followers are told of the card while
  the primary is resolved, a script drafts and answers the secondary. web2:
  `transportContract.test.ts` (both transports), `wasmTransport.test.ts` (with steps),
  `liveSecondary.test.tsx` (the page over the real engine: ready and sent, not ready and
  "Send draft", a draw), `e2e/local.spec.ts` and `e2e/local.phone.spec.ts`.
- **Open.** `NEXT.md` has the list.

Then a screen for the lobby of several humans is the next plan, not this one.

## Postponed: delete what web2 replaced

**Postponed (owner, 2026-10-11) until every UX feature of `web/` is in web2.** That is after
this whole plan. Nothing of it is done in phases 0 to 5: `web/`, the planning runner and the
batch keep working, and a phase of this plan must not break them.

What can go then:

1. `ti4-server`: the planning runner (`planning/runner.rs`, `session/planning.rs`), the batch
   (`session/batch.rs`, `POST /api/games/{id}/batch`), their messages (`StartPlanning`,
   `ResetPlanning`, `EditPlanningMovement`, `ApplyPlanning`, `SubmitPlanningChoice`,
   `SecondaryPlanning`, `PlanningUpdate`, `PlanningStatus`, `PlanningResult`) and their tests.
   About 4000 lines of source; no crate outside `ti4-server` uses them. Raise `PROTOCOL_VERSION`.
2. `web/`.

Before that, list what `web/` has and web2 does not (the host history, the event log, the map
picker, the advisor, the MLP bot, and what else is found), and bring each to web2 or ask the
owner. The lobby is in web2 since 2026-10-14 (`2026-10-12-LOBBY_PLAN.md`, phases 1 to 3 and 5
to 7): the table, its setup, the factions, the nicknames in the game, the table of a running
game and the take-over of a seat.

## Checks that every phase must pass

- Rust (phases 1a, 1b, 3): `cargo c-web`, `cargo t-web`, `cargo fmt --all -- --check`, `cargo l-web`.
  Server integration tests: `cargo test -p ti4-server --test it <file>::`. Do not build other crates.
- Rebuild `session/ti4.wasm` (`scripts/build-wasm.sh`) after a change of a fact in `ti4-view`.
- A changed screen is checked on desktop and phone; phase 2 adds a toolbar state ("offline") and
  phase 4 a row of the Dev sheet and a header link, so both layouts, 8 seats and the long names
  apply (`AGENTS.md`, patterns 6, 13).
- Phase 4 adds a row to the Dev sheet. Add a line for it to `AGENTS.md` "Local play", and open
  the row once in `e2e/local.spec.ts`, in the same change. `/` stays the local game.

## Risks

- **A draft against a moving game.** In multiplayer the other seats act while this seat drafts its
  next turn. The base of the draft changes. The transport sends the same script again for the
  new `game_version`; a step that is no longer legal ends the run as "refused" or "stopped",
  which the draft screen already shows. The script is kept. Do not drop the staged work silently.
- **Latency in the draft loop.** Today every change re-runs the draft in the same tick. Over a
  network each change is a round trip. Keep the last outcome visible (phase 0) and send only the
  latest script; if the server cannot cope, add a debounce in the transport (not in the page).
- **Two copies of the plan rules** (`planAnswer` in TypeScript, `plan_answer` in `ti4-view`) already
  drift. Online play uses `plan_answer` on the server (`ApplyDraft`) and not the TypeScript copy,
  so no third copy is made.
- **Hidden-information leaks** are a server property. The redaction tests of phase 1 are the guard;
  review any new field against `ti4-server/tests/projection_redaction.rs` before it ships.
- **Version skew** between the page bundle, the wasm and the server: the handshake check of phase 2.

## Order and size

Phase 0 and phase 1a are independent and can run in parallel (web2 and Rust). Phase 1b needs 1a.
Phase 2 needs 0 and 1b. Phase 4 can start with a fake transport once phase 0 is in. Phase 3 is
last of the first version and may ship after the first playable multiplayer: without it, the
Undo button is simply not shown online. Phase 5 needs phase 0 only for single player, and
phase 2 for online.

| Phase                                           | Where                                                                         | Size                                                        |
| ----------------------------------------------- | ----------------------------------------------------------------------------- | ----------------------------------------------------------- |
| 0 seam (built)                                  | web2, ti4-wasm                                                                | large (the async draft; plans and Never cards move to Rust) |
| 1a facts in one place (built)                   | ti4-view, ti4-wasm                                                            | small                                                       |
| 1b facts, random seats, script messages (built) | ti4-server                                                                    | large                                                       |
| 2 transport (built)                             | web2, ti4-server and ti4-view (two fields)                                    | medium                                                      |
| 3 undo (built)                                  | ti4-server, ti4-engine (a latch that is cleared, three lines), ti4-view, web2 | medium                                                      |
| 4 entry (built)                                 | web2                                                                          | small                                                       |
| 5 secondary draft                               | ti4-view, ti4-wasm, ti4-server, web2                                          | large                                                       |

## Open

Nothing. Every decision of this plan is made.

## Questions and answers

The owner asked these on 2026-10-11. They explain decisions 2 and 6.

### How a draft works

**When does the page talk to the server?** At three times: after every change of the draft,
whenever the live game changes, and at Apply. A draft is not local. The page keeps only the list
of the answers of the player (the script). After each change the engine plays that list, and the
screen is made from what the engine says. Single player works like this already
(`Transport.runDraft`): there the engine is the wasm in the page, online it is the server.

**What happens, step by step?**

1. The player opens the draft. The page sends the script `[tactical]`. The server plays it on a
   copy of the live game and says which systems can be activated. The page shows them.
2. The player activates system 27. The page sends `[tactical, 27]`. The server says what is
   next: a question of a card, or the ships that can move.
3. The player moves a cruiser. The page sends `[tactical, 27, move cruiser]`. The server says
   how the board is after the move, and what can still move.
4. The same for each move, each landing and each purchase: one message, one answer.
5. The player sends the draft with Apply. Now the server gives the answers to the live game,
   one after the other, and every seat gets the update.

The server discards the copy after each run. It keeps nothing of the draft.

**What is local?** The script itself (`localStorage`), and what is staged in one step before it
is recorded (a build that is not paid). The rules, the legal moves, the windows of the cards and
the board after a move come from the engine.

**Another player acts in the live game. How does the draft show it?** The live game has a new
version, and the page gets its update as always. The page then sends the same script again, and
the server plays it on a copy of the new game. If every answer is still legal, the draft is the
same on the new board. If one is not (ships of the other player are in the system now), the run
stops at that answer: the screen says which part does not fit, and the script is kept. A draft
is never brought up to date; it is always the script on the newest game.

### Cards and abilities in a draft

**The answers go as a list. How does the server know which cards to offer?** It does not look
at the list for this. The game asks its questions one after the other, also on the copy, and it
has the hand of the player. For each question the server takes the next answer of the list. A
question that this answer fits (a move, a landing, a purchase) is answered, and the run goes
on. A question that the list has no answer for (a window of a card) stops the run, and the page
shows that question.

**Example: Flank Speed (+1 move after an activation).** The script is `[tactical, 27]`. On the
copy the game opens the window "after you activate a system", because the player holds the
card. The script has no answer, so the run stops there and the page shows "Flank Speed" and
"Decline". The player chooses. The page puts the answer into the script and sends it again:
`[tactical, 27, flank speed]`. Now the copy plays the card, and the ships that can move have +1.
A card that triggers at a production works the same way, before the first purchase.

**Do the other seats play cards in a draft (Sabotage)?** No. `guard_fork`
(`ti4-view/src/planning/fork.rs`) prepares the copy, for the server's runner and for `run_draft`
alike. On the copy, an optional reaction of another seat is not asked, and nothing of its hand
is read. The abilities of the player and every ability that is not optional run as in the game.
A decision that another seat must make ends the draft there. So a draft shows what happens if
nobody stops the player. At Apply the live game asks the other seats; if one of them reacts, the
plan stops there and the rest is staged again.

**Is the player asked again in the live game for a card of the draft?** No. Apply sends the
script that the draft played, with the answer of the card in it, and that answer is given when
the live game opens the window. The local game does this already (`sent` in `useLiveSession.ts`
is a part of the script, and `planAnswer` answers a `choose` by its id). The player is asked
only when the live game asks something that the draft did not: a window that opens because
another seat reacted, or a question after a roll.

**What does not work?**

- A window in a space battle or an invasion ends the draft. An action card as the action
  itself is not drafted: the menu of a draft has the tactical action only. Both are limits of
  the checks in `ti4-view`, so the server's runner has them too.
- A question in the middle of a step has no place in the script of the page yet (`NEXT.md`). A
  question before a step has one.

### Why the script, and not the drafts that the server has

**What can the server's drafts do that the script cannot?** All of it comes from one fact: the
runner keeps the draft on the server, the script is in the browser only.

- A draft that is applied while the page is closed. Not needed (owner).
- A draft on another device. Not needed (owner).
- A draft that a host undo keeps.
- The draft of a secondary. Needed (owner): phase 5 adds it to `run_draft`.

Nothing of this is lost for good. A script is a short list, and the server can keep one when a
feature needs it.

**What is harder with the script?**

- Each change plays the whole script from its start. The runner keeps a worker at the last
  question. A tactical action has some tens of answers and the wasm plays them in one tick;
  the server with 8 drafting seats is not measured yet.
- Each answer is a whole update with the facts, and up to three more (movement, landings,
  production).
- A client can send many scripts: the server needs a limit per seat and runs only the newest.
- A draft of more than one action would grow the replay. This plan has none.

**What is easier?** A change of an early step (the run checks the later steps; the runner has
messages and state for this), and a game that moved, a reload and a new connection (the script
is sent again; the runner needs an identity on each message).

**Why the script, then?**

- The draft of web2 is a script already: its screens, its undo and redo, its saved state and
  its tests.
- The runner would need a new draft in web2, or an adapter that plays the whole script into it
  on each change. That adapter is the script with more messages.
- The runner must change in any case: it refuses the seat that has the turn, and its answer has
  no facts for the screens.
- Single player and multiplayer then run a draft with the same function (decision 6).

### The same code in both modes

**Is this more code?** No. The aim is one copy of each rule. Today three things exist twice:
once in TypeScript for single player, once in Rust for the server. Each change of decision 6
removes the TypeScript copy and lets the wasm call the Rust one.

**What does "Never cards move into the engine" mean?** No new rule. In single player the page
reads each window and answers "decline" when it offers only Never cards (`filterOffers`). On the
server a wrapper of the seat does this before the player is asked (`NeverOffer` of `ti4-engine`).
The wasm host is to use that wrapper too; then the page only says "never offer this card", in
both modes, and `filterOffers` is deleted.

**Why is phase 0 large, when nothing is added?** "Large" is the work, not the amount of code.
Single player has logic of the game in TypeScript, and it must go to Rust before the modes can
share it:

1. A plan that is sent: `planAnswer` in TypeScript and `plan_answer` in Rust do the same. The
   wasm needs an export that takes the plan while the game is parked. This is the largest part;
   the export for a draft shows that it can be done.
2. Never cards, as above. The decline of a window is a saved answer of a local game now, and is
   not after the change: check the saved game and the undo.
3. The result of a draft arrives later, not in the same tick: a change in the middle of
   `useLiveSession`.

**What is the end state?** The two transports are thin: an update, an answer, a script that is
run, a script or a plan that is sent, the mode of a card, an undo. One calls an export of the
wasm, the other sends a message. Everything above them is one code.

## Done when

- Single player behaves as before: all tests and e2e green, no new import from `session/` into
  `game/` or `ui/` (`boundaries.test.ts`).
- Multiplayer: a browser starts a game as seat 1, plays tactical actions with drafts, the Cards,
  Secrets, Objectives and Agendas sheets show the same facts as in single player, a reload resumes,
  and an e2e plays the first round against a real server.
- The Undo button shows exactly when the server says `can_undo`.
- No component in `game/` reads a mode flag.
