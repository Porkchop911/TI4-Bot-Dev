# Online multiplayer architecture

## Decision

Online play uses a Rust authoritative server and browser clients. The server is the only owner of a
mutable `ti4_engine::Game`; clients submit only an option id that the server validates against the
engine-generated legal choice.

Do not use peer-to-peer state replication or action synchronization. It would require every client
to hold hidden state, exactly reproduce the rules/RNG/content, resolve desynchronization, and handle
host migration. Those are all worse fits for a long-running, private-information, turn-based game.

The first implementation uses the current synchronous `Table`/`Decider` boundary. A server-side
`RemoteHumanDecider` publishes a choice to the owning browser and waits for that seat's answer. This
works for all current nested choices without changing the engine's decision flow. A resumable,
non-blocking engine API is a later scale/recovery improvement, not a prerequisite for online play.

## Current seams

| Existing seam                      | Multiplayer use                                                                                                                           |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------- |
| `ti4-engine::Game`                 | Canonical mutable rules state and transitions; one session worker owns it.                                                                |
| `Game::step()`                     | Drives automatic transitions and invokes the decider when a player must decide.                                                           |
| `Table::ask_seeing`                | Authenticates the acting seat by selecting its decider, binds a `SeatObservation`, validates the returned option, and records the choice. |
| `Choice` / `ChoiceOption`          | Server sends the generated choice only to its owner; clients return one offered option id.                                                |
| `ti4-model::view::view_for`        | Basis for redacted state projection. It is not itself the browser protocol.                                                               |
| `DecisionContext::visible_to`      | Redacts actor-only outstanding constraints from other seats.                                                                              |
| `DecisionLog` and canonical hashes | Foundation for audit, diagnostics, deterministic replay, and durable history.                                                             |
| `ti4-bridge`                       | TTS command/telemetry integration, not the online game authority or browser choice protocol.                                              |
| `ti4-review`                       | Source of board/presentation requirements and SVG export semantics.                                                                       |

`Table::ask_seeing` is central to hidden-information safety. It binds the private observation only
after looking up the decider for `choice.player`. A server must preserve this path rather than
constructing a private view from a client-supplied player id.

## Session model

Each live game has one session worker and one authoritative `Game`. The worker may block while a
human decides; the WebSocket runtime continues accepting connections and routes the answer to that
session's per-seat inbox. A blocked game cannot mutate, so one accepted decision remains atomic.

```text
browser WebSocket submission
  -> server authenticates account and seat ownership
  -> session worker's pending-choice inbox
  -> RemoteHumanDecider returns the engine-owned ChoiceOption
  -> Table validates and appends DecisionRecord
  -> Game applies the transition and generates the next choice
  -> server sends redacted updates to every connected viewer
```

The server serializes a different projection for each recipient:

- the actor receives its full generated `Choice`, own private holdings, and private decision context;
- opponents receive public/redacted state and a waiting status, never another seat's choice/options;
- spectators receive the public projection only;
- a referee/debug role, if added, is explicit and separate from ordinary spectators.

Every pending decision has an opaque nonce and a monotonically increasing game version. The server
accepts a submission only when its game id, authenticated seat, nonce, version, and option id match
the currently pending choice. Rejected, stale, duplicated, or wrong-seat submissions make no state
change.

TI4 reaction opportunities are modeled as ordered timing windows, not concurrent state mutations.
The engine issues one choice to one actor, applies or declines it, then recomputes the next eligible
choice. The UI may collect non-binding drafts, but it must never commit two reactions based on the
same stale state.

## Delivery steps

### 1. Define the multiplayer protocol and redacted projections

Create an explicit versioned JSON protocol before networking or UI code. Keep the initial Rust DTOs
inside the future `ti4-server` crate; do not create a shared crate until a second Rust consumer needs
one.

The protocol needs, at minimum:

- an initial per-viewer snapshot;
- a versioned state update or replacement snapshot;
- `pending_choice` with an opaque nonce, prompt, legal options, and actor-visible context;
- `submit_choice { game_id, nonce, expected_version, option_id }`;
- clear stale, unauthorized, malformed, disconnected, and engine-error responses;
- a public waiting/turn status that does not disclose another player's legal reactions.

**Crates to update**

- Add `crates/ti4-server/` to the workspace, initially containing protocol DTOs and serialization
  tests only.
- Read `ti4-engine` and `ti4-model`; do not change game rules in this step.
- Do not modify `ti4-bridge` or `ti4-review`.

**Works after this step**

- Protocol fixtures exist for actor, opponent, spectator, stale submission, and terminal-game views.
- Browser and server implementation can agree on stable message shapes without treating internal
  `GameState` serialization as the wire contract.

**Tests**

- JSON round-trip and unknown/version rejection tests for each message type.
- Projection tests proving an opponent payload contains no action-card or secret-objective identity,
  no actor-only outstanding constraints, and no legal options belonging to the actor.
- Snapshot-size and message-size bounds tests.

**Status: COMPLETED (PROTOCOL & REDACTED PROJECTIONS)**

- **Wire Protocol DTOs (`crates/ti4-server/src/protocol/`)**:
  - Implemented versioned JSON protocol (`client.rs`, `server.rs`, `choice.rs`, `status.rs`, `view.rs`, `error.rs`).
  - Strict serde configuration (`deny_unknown_fields`) enforces schema conformance.
- **Redacted State & Event Projection (`crates/ti4-server/src/projection.rs`)**:
  - `project_for_viewer` generates role-specific snapshots (`Player` vs `Spectator`).
  - Strictly redacts opponent/spectator private cards (secret objectives, action cards, promissory notes).
  - Outstanding constraints and pending choice options are isolated exclusively to the deciding actor.
- **Golden Protocol Fixtures (`crates/ti4-server/src/fixtures.rs`)**:
  - Fixtures for actor, opponent, spectator, stale submission, and terminal game.
- **Verification**:
  - Protocol roundtrip tests (`tests/protocol_roundtrip.rs`): 11/11 passing.
  - Projection redaction tests (`tests/projection_redaction.rs`): 5/5 passing.
  - Fixture verification tests (`tests/fixtures_verification.rs`): 6/6 passing.
  - Size bounds tests (`tests/size_bounds.rs`): 4/4 passing.

### 2. Build an in-memory authoritative session vertical slice

Implement a `RemoteHumanDecider` in `ti4-server`. It receives the engine-authenticated `Choice` and
`SeatObservation`, emits an actor-specific pending-choice message, waits on that seat's inbox, and
returns the matching engine-generated option. The session worker owns the `Game`; no mutex lets a
second request mutate it while the decider waits.

Use one bounded worker thread per active game for this initial version. This keeps synchronous nested
decisions working without converting every existing `Table::ask_seeing` call site into a resumable
state machine.

**Crates to update**

- `ti4-server`: session worker, pending-choice registry, `RemoteHumanDecider`, in-memory game
  registry, and test-only transport adapters.
- `Cargo.toml`: add the new workspace member and only the minimum runtime/channel dependencies.
- `ti4-engine`: no behavior change expected. Add a focused integration test only if a public API
  needed by the server is genuinely missing.

**Works after this step**

- A process can start a seeded game with remote human seats.
- A test harness can receive a generated choice, submit one legal option, and drive the game through
  automatic steps and nested choices.
- Bots and humans can occupy different seats because both are `Decider` implementations.

**Tests**

- A scripted end-to-end game containing at least one nested timing/reaction choice.
- Wrong seat, stale nonce, duplicate submission, unknown option, and malformed message tests assert
  identical pre/post state and decision-log length.
- A disconnect test proves an unanswered game remains pending rather than choosing a default option.
- Deterministic replay test: identical seed and accepted option-id sequence produce identical
  decision and event hashes.

**Status: COMPLETED (AUTHORITATIVE SESSION VERTICAL SLICE)**

- **Session Architecture (`crates/ti4-server/src/session/`)**:
  - `RemoteHumanDecider` coordinates blocking choices with thread-safe pending choice registry and response channels.
  - `GameSession` runs an authoritative `ti4_engine::Game` on a dedicated session worker thread.
  - `GameRegistry` manages active sessions and thread-safe routing.
  - Supports hybrid human and bot seats via `SeatController::Human` and `SeatController::BotFirstOption`.
- **Verification**:
  - Scripted 6-round multi-stage game with secondary strategy follows (`tests/session_e2e_game.rs`): 1/1 passing.
  - Rejections preserve state and decision log length (`tests/session_rejections.rs`): 1/1 passing.
  - Client disconnect preserves pending decision without defaulting (`tests/session_disconnect.rs`): 1/1 passing.
  - Deterministic replay produces bit-identical hashes and event logs (`tests/session_deterministic_replay.rs`): 1/1 passing.

### 3. Add HTTP, WebSocket, and a minimal browser client

Expose the session vertical slice through a Rust HTTP/WebSocket server. HTTP supplies health, lobby,
and one-shot initial snapshot endpoints; WebSocket carries subscription, choice submission, and live
updates. Use bounded messages, per-connection outbound queues, and explicit backpressure behavior.

Create `web/` as a TypeScript React application. Render the board with SVG and use DOM components for
player sheets, pending choices, payment controls, logs, and accessibility. Do not use Canvas as the
primary renderer: the TI4 map is small and SVG keeps systems, labels, tooltips, and click targets
accessible and inspectable.

**Crates/directories to update**

- `ti4-server`: HTTP routes, WebSocket transport, connection lifecycle, and session-to-connection
  routing.
- `Cargo.toml`: add only the selected Rust server/runtime dependencies.
- `web/`: React application, TypeScript protocol types generated or checked against fixtures, SVG
  board, and browser integration tests.
- `ti4-review`: unchanged initially; use it as a reference for board semantics, not as a dependency.

**Works after this step**

- Players can create/join an in-memory game in a browser, see their permitted state, submit legal
  choices, reconnect during a running process, and see updates made by other players.
- Spectators can observe a public-only game view if that role is included in the first UI scope.

**Tests**

- WebSocket integration tests for subscribe, submit, reconnect, stale version, and backpressure.
- Browser end-to-end test with two seats: one takes an action, the other receives the resulting
  redacted update, and neither can submit for the other seat.
- Browser accessibility tests for keyboard-selectable board systems and choice controls.
- Manual responsive visual checks at desktop and narrow mobile widths.

**Status: COMPLETED**

- **Step 3A (Server Transport & Lobby Lifecycle)**:
  - Added HTTP routes (`GET /health`, `GET /api/games`, `POST /api/games`, `GET /api/games/:game_id/snapshot`).
  - Added pre-game lobby lifecycle endpoints (`POST /api/games/:game_id/lobby/ready`, `POST /api/games/:game_id/lobby/claim`, `POST /api/games/:game_id/lobby/start`) with unguessable capability tokens and seat lease renewal.
  - Added WebSocket protocol handler (`GET /ws/games/:game_id`) with 128-msg outbound queue, backpressure handling, viewer role isolation, and clean unsubscription.
  - Integration tests passing in `crates/ti4-server/tests/ws_lifecycle.rs` (4/4) and `crates/ti4-server/tests/lobby_lifecycle.rs` (5/5).
  - Standalone binary `crates/ti4-server/src/bin/server.rs` running with default demo game pre-seeded and crash recovery.
- **Step 3B (Browser Client)**:
  - React 19 + Vite + TypeScript application in `web/`.
  - SVG hex map board (`Board.tsx`) with planet status, unit counts, highlights, and tooltips.
  - DOM components for player sheets, turn status bar, accessible modal dialogs (`PendingChoiceModal.tsx`), event log drawer, and game lobby.
  - Conformance and invariant tests in Vitest passing.
  - Multi-seat Playwright test (`e2e/multiplayer_invariants.spec.ts`) passing with 2 human seats and 1 spectator: verifying zero console/page errors, live strategy card draft sync, strict DOM privacy redaction, DOM actionability, fuzzing loops, and disconnect/reconnection.
- **No changes made to `crates/ti4-engine` or any other crate**.

### 4. Persist games and make reconnect/restart recoverable

Add a durable session store. Persist an append-only accepted-decision/event history and periodic
versioned snapshots. Write snapshots atomically, validate schema/content versions/checksums before
loading, and rebuild pending-choice state on recovery. Do not persist a `RemoteHumanDecider` or an
open channel; recreate those runtime objects around recovered game state.

Start with one local durable implementation suitable for development and self-hosting. Keep the
storage trait narrow enough that a database-backed implementation can follow without changing rules
code.

**Crates to update**

- `ti4-server`: storage interface, local store, recovery, snapshot scheduling, retention bounds.
- `ti4-engine` only if it lacks a safe versioned snapshot/replay boundary. Do not serialize private
  runtime internals by accident.
- `ti4-sim`: reuse or complete replay utilities only where their contracts fit the durable format.

**Works after this step**

- A server restart can restore a game and re-issue the outstanding choice to its owner.
- A reconnecting client can fetch a current snapshot instead of relying on missed WebSocket updates.
- Administrators can inspect an immutable decision/audit history.

**Tests**

- Crash/restart tests before a choice, while a choice is pending, and immediately after acceptance.
- Corrupted, oversized, unknown-version, wrong-checksum, and partial-write fixtures fail without
  changing the last valid saved game.
- Recovery replay produces the same canonical hashes as the uninterrupted run.

**Status: COMPLETED (DURABLE CRASH RECOVERY & REPLAY)**

- **Filesystem Durable Storage (`crates/ti4-server/src/storage.rs`)**:
  - Implemented `FileGameStore` rooted at `./data/games` (configurable via `TI4_DATA_DIR` or `--data-dir`).
  - Atomic JSON writer (`init.json`) using `.tmp` and atomic replace ensures corruption immunity during power cuts.
  - Append-only `decisions.jsonl` and `events.jsonl` with fsync/`sync_data()` after each accepted decision or event.
  - Corrupt/partial trailing line tolerance on restart (`read_json_lines`).
  - Rewind capability: truncating the last line of `decisions.jsonl` rewinds the game 1 decision on recovery replay.
- **Deterministic Replay Recovery & Live Continuation (`crates/ti4-server/src/session/worker.rs`)**:
  - `ReplayingDecider` steps through all recorded decisions in microseconds to reconstruct exact in-memory engine state (tactical windows, secondary windows, dice RNG, logs).
  - Verifies that replaying decisions produces bit-identical canonical decision hashes (`decision_hash(V1)`).
  - Seamlessly transfers control to live deciders (`RemoteHumanDecider` or bots) for subsequent choices.
- **Automatic Boot Recovery (`crates/ti4-server/src/session/registry.rs`, `crates/ti4-server/src/bin/server.rs`)**:
  - `registry.recover_all_games()` automatically recovers existing games from disk on server startup.
  - Default `demo` game is resumed if present on disk, or created and persisted if not.
  - New games created via HTTP `POST /api/games` automatically save `init.json` and persist all subsequent moves.
- **Snapshot Recovery & Event Log Parity (`crates/ti4-server/src/session/worker.rs`, `web/src/hooks/useGameSession.ts`)**:
  - Monotonic typed `GameEvent` log maintained in server shared session state and persisted to disk;
    visibility is explicit and events are projected per viewer.
  - Reconnecting clients fetch current snapshot (via WebSocket or HTTP), receiving the complete authoritative event log and matching continuous subscribers 1:1.
  - Human-readable action formatting in `crates/ti4-server/src/format.rs` aligns with frontend representations.
- **Verification**:
  - Integration tests in `crates/ti4-server/tests/crash_recovery.rs` pass (1/1).
  - Integration tests in `crates/ti4-server/tests/session_recovery_replay.rs` pass (2/2).
  - All Rust `ti4-server` tests pass (35/35).
  - Web unit/invariant tests (`npm test`) pass (22/22).
  - Playwright multi-seat integration tests pass (1/1).
  - Zero edits made to `crates/ti4-engine` or any rules crate.

### 5. Optional: Add identities, authorization, invitations, and operations

> [!NOTE]
> **Status: OPTIONAL / DEFERRED**
> This step is optional for now and deferred. It is not essential for playing with friends or private play sessions where players coordinate via private server links, direct IP/domain sharing, or LAN. The current seat selection and nonce-based turn authorization is fully sufficient for trusted peer groups. Full OAuth/account registration, database user storage, email invitations, and operational rate-limiting can be revisited if public internet matchmaking or multi-tenant hosting is desired in the future.

Replace development-only seat tokens with authenticated accounts and scoped game permissions. The
authorization decision belongs at the server boundary: an account may read or submit for a seat only
when its game membership permits it. Keep the engine unaware of accounts, cookies, sessions, and
database identities.

Add rate limits, audit logs, structured error reporting, metrics, backups, and deployment
configuration. This step requires a security review before an internet-facing deployment.

**Crates/directories to update**

- `ti4-server`: authentication, authorization, invite flow, rate limits, audit/metrics, deployment
  configuration.
- `web/`: sign-in, invitation acceptance, lobby/game membership, and logout UI.
- No rules changes in `ti4-engine`; no changes to the loopback-only TTS bridge.

**Works after this step**

- Multiple games can be safely shared among authenticated users.
- A user cannot enumerate, view, or submit choices to a game or seat they do not control.
- The service has enough diagnostics and recovery behavior to operate beyond a developer machine.

**Tests**

- Authorization matrix for owner, assigned seat, spectator, revoked user, and unauthenticated user.
- Session fixation, expired token, cross-game nonce, message-size, and rate-limit tests.
- Independent security review of authentication, WebSocket origin policy, storage, logging, and
  private-information boundaries.

### 6. Complete the gameplay UI and add AI seats

> [!NOTE]
> Detailed UI implementation plan: [`web/plans/2026-09-21-GAMEPLAY_UI_COMPLETION_PLAN.md`](2026-09-21-GAMEPLAY_UI_COMPLETION_PLAN.md).
> Covers the Choice Renderer Model, constraint handling (`min_selection`/`max_selection`), and specialized workflow components (payments, tactical moves, combat, agenda voting, trade negotiation).

Expand the browser UI from the vertical slice to all player-facing workflows: timing/reaction
prompts, multi-step payments, transactions, agenda voting, combat assignment, action-card details,
and history. Keep client-side drafts separate from authoritative submissions.

AI seats run server-side through the same `Decider` boundary as human seats. An AI receives only the
choice and seat-bound observation it is entitled to see; it does not gain a privileged state API.

**Crates/directories to update**

- `web/`: complete choice workflows, board details, history, reconnect state, and accessibility.
- `ti4-server`: AI-seat configuration, task scheduling, time limits, cancellation, and audit detail.
- `ti4-policy` and `ti4-mlp`: reused as existing server-side deciders; no browser inference.
- `ti4-review`: optionally extract a renderer-neutral presentation model after a separately scoped
  review. Do not duplicate its renderer blindly.

**Works after this step**

- A full online game can be played by humans, bots, or a mixture of both.
- The UI makes mandatory choices, optional reactions, and waiting states clear without revealing
  private options.

**Tests**

- End-to-end fixtures covering tactical movement, combat, invasion, production, transactions,
  strategy-card follow, agenda voting, and action-card timing.
- Redaction regression tests for every new screen and protocol message.
- AI timeout/failure tests confirm a crash or incomplete inference never becomes an automatic legal
  choice or a false successful game completion.

**Status: COMPLETED (GAMEPLAY UI WORKFLOWS & FRONTEND EXPERIENCE)**

- **Completed Work Packages (`web/plans/2026-09-21-GAMEPLAY_UI_COMPLETION_PLAN.md` UI-01 through UI-08)**:
  - **UI-01 (Choice Renderer Model & Classifier)**: `deriveChoiceRendererModel` (`web/src/presentation/choiceModel.ts`) with typed payload decoders (payment, movement, trade) eliminating raw string parsing.
  - **UI-02 (Bounded Multi-Selection & Search)**: Accessible bounded checkbox cards (`min_selection` to `max_selection`), count badge, and option search filter (`web/src/components/PendingChoiceModal.tsx`).
  - **UI-03 (Economy & Payment Drawer)**: Modeless payment drawer (`web/src/components/PaymentDrawer.tsx`) with planet card toggles, trade good stepper, live debt/credit tally, and pipelined execution.
  - **UI-04a (System Activation & Vector Overlays)**: Board-first spatial interaction (`web/src/components/Board.tsx`) with active system reticles, legal candidate pulsing, and animated green movement vectors.
  - **UI-04b (Tactical Fleet Rally & Semantic Pipeline Runner)**: Docked fleet rally tray (`web/src/components/TacticalMovementOverlay.tsx`) and `usePipelineRunner.ts` executing atomic intent predicates without engine batching.
  - **UI-05 (Combat Resolution Arena)**: Multi-stage space/ground combat arena (`web/src/components/CombatResolutionModal.tsx`) with authoritative dice roll feed, sequential sustain damage vs reaction pause vs casualty assignment, and grouped unit steppers.
  - **UI-06a (Bilateral Trade Desk)**: Structured deal catalog modal (`web/src/components/TradeDeskModal.tsx`, `tradeDecoder.ts`) with tabs for commodity swaps (`cc{n}`), goods exchanges, promissory notes, and mutual support.
  - **UI-06b (Agenda Council Ballot Desk)**: Council desk (`web/src/components/AgendaBallotModal.tsx`) with live vote tallies, multi-planet influence basket, and Speaker tiebreaker gavel.
  - **UI-07 (Reaction Status Bar & Production Builder)**: Floating non-blocking pill (`web/src/components/ReactionStatusBar.tsx`) for fast passing (`Spacebar`), and production cart drawer (`web/src/components/ProductionBuilderDrawer.tsx`) tracking space dock capacity.
  - **UI-08 (GameShell Integration & Layouts)**: Unified top-level dispatcher (`web/src/components/WorkflowShell.tsx`, `GameShell.tsx`) with accessible design primitives (`web/src/primitives/`) and responsive overlays.
- **Verification**:
  - Web unit, component, and invariant test suite (`npm test`): **25 test files / 153 tests passing (100%)**.
  - Multi-seat browser integration test with Playwright (`e2e/multiplayer_invariants.spec.ts`): passing across human and spectator seats.
  - Rust server bot deciders (`SeatController::BotFirstOption`) operational in `ti4-server`. Full neural policy integration (`ti4-policy` / `ti4-mlp`) remains deferred to future milestones.

### 7. Optional: make the engine decision boundary non-blocking

Only undertake this after the blocking session-worker implementation has measured operational limits.
Refactor `Game::step()` and nested decision sites into explicit automatic advancement and resumable
pending choices. This can reduce blocked threads and simplify some recovery paths, but it crosses a
large number of decision producers and risks timing regressions.

**Crates to update**

- `ti4-engine`: `Game`, windows/timing resolvers, and every nested `Table::ask` / `ask_seeing` path.
- `ti4-server`: replace the blocking decider adapter with calls to the resumable engine boundary.
- `ti4-sim`, `ti4-policy`, `ti4-training`, and `ti4-review`: adapt existing synchronous drivers.

**Works after this step**

- One async worker can hold many waiting games without dedicating a blocked OS thread to each game.
- The same pending-choice object can be persisted/restored more directly.

**Tests**

- Differential tests demonstrate that scripted choices produce identical state, decision logs, and
  canonical hashes through the old synchronous and new resumable drivers.
- Full workspace rules, replay, hidden-information, and timing suites remain green.
- Load tests demonstrate a measured operational benefit before claiming an improvement.

## TTS boundary

TTS remains an optional physical-table integration. Its Lua executor polls a loopback bridge for
commands and uploads telemetry; it does not presently act as a secure browser client that submits
engine-generated option ids. Table telemetry also lacks private card identities. Do not make TTS the
online authority or depend on it for browser multiplayer.

Later, an explicitly scoped adapter may mirror authoritative engine events to TTS and audit returned
telemetry. A TTS refusal, delay, or missing upload must remain visible and must never alter the
authoritative online game as though the table action succeeded.

## Non-goals for the first playable release

- Peer-to-peer synchronization, host migration, or client-side rules execution.
- Simultaneous commitment of state-changing reactions.
- TTS as a required client or authoritative source.
- A full non-blocking/resumable engine refactor.
- Production-scale account/deployment infrastructure before the in-memory two-seat browser vertical
  slice is proven.
