# ti4-host

The part of the game host that does not depend on how the process is hosted. `ti4-server`
(threads, files, sockets) and the browser host (`ti4-wasm`, a Worker) both build on it, so the
wire messages, the event journal and the history rules are one piece of code.

Contract (checked by CI for T4):

- `cargo check -p ti4-host --target wasm32-unknown-unknown` passes.
- No tokio, no thread spawn, no `Instant::now()`, no `SystemTime::now()`, no `rand`. The only
  place that touches the system clock is `clock::SystemClock`, which is `cfg(not(wasm32))`.
- Decision-pure: nothing here changes what the engine decides. After every change, run
  `cargo run -p ti4-server --example hash_survey -- <COPY of /var/lib/ti4-public>` before and
  after; the output must be byte-identical.

## What is in the crate

| Module | Contents | Used by the server in |
|---|---|---|
| `protocol` | `ClientMessage`, `ServerMessage` and every message struct (`StateUpdateMsg`, `InitialSnapshotMsg`, `PendingChoiceMsg`, `TurnStatusMsg`, `ActionAcceptedMsg`/`ActionRejectedMsg`, `GameOverMsg`, `Event`...), `GameEvent`/`GameEventKind`/`EventVisibility`/`SeatDecisionDetail`/`MovementFact`/`PublicRoll` and their builders (`public_roll`, `decision_facts`, `verified_decision_facts`, `decision_grouping`, `action_id_for`, `current_log_path`), `HistoryStatus`, `AutoResolvedNote`, `PROTOCOL_VERSION`, `parse_client_message`/`parse_server_message` | everywhere (`ti4_server::protocol` re-exports it) |
| `projection` | `project_initial_snapshot[_with_map]`, `project_state_update[_with_map]`, `redacted_state`, `project_turn_status`, `project_pending_choice`, `facts_context` (thin layer over `ti4-view`) | `session/worker.rs`, `ws` |
| `planning` | `PlayerPlan`, `AttemptIdentity`, `PlanningEnvelope`, `PlanningUpdate`, `Progress`, `FailureCategory`, `SubmissionError` + the recorders of `ti4-view::planning` (`RecordingDecider`, `ReplayDecider`, `RecordedDecision`) | `planning/runner.rs` (the thread that runs a plan stays in the server) |
| `journal` | `reconcile_with_redo`, `build_event`, `build_roll_event`, `roll_is_known`, `roll_cursor`, `consume_or_fork_redo`, `EventDraft`, `Reconciled`, `RedoOutcome` | `SessionShared::{record_and_broadcast_event, record_rolls, consume_or_fork_redo}` |
| `state_update` | `StateUpdateInputs`, `state_update_for`, `queue_auto_resolved`, `answer_reach` | `SessionShared::{broadcast_state_update, note_passed_window}`, `session/reach.rs` |
| `rng_force` | `RngMarks`, `RngForce` (forced random positions of a timeline, the "marks" of a history) | storage, turn redo, replay |
| `replay` | `replay_session`, `replay_session_forced`, `replay_session_with_copy`, `ReplayReport` (deterministic re-execution of a decision log, hash comparison) | storage, recovery |
| `step_snapshot` | `StepSnapshot` (copy of the game at a step boundary, fork for fast resume) | worker, batch |
| `batch` | `BatchRequest`, `MovementPlan`, `MovementStep`, `simulate[_with]`, `replay_boundary[_state]`, `replay_budget`, `ReplayBasis` (trait) | registry `submit_batch` |
| `clock` | `Clock`, `Deadline`, `NonceSource`, `SystemClock` (native only), `ManualClock`, `CounterNonce` | all of the above that need time |
| `preset`, `card_set`, `map`, `maps` | Game setup: start presets, the strategy-card sets, `create_game_with_factions` (what a lobby start calls), the map catalogue and previews (`maps::catalog`). Moved from `ti4-server` unchanged (it re-exports them); the browser host builds the same opening state from the same seed. | lobby start, `/api/maps` |
| `batch::LiveBatch` | A batch applied to the live game one question at a time (the same decider as `simulate`), for a host with no network between the request and its answers | (browser host only) |

### Injected time and nonces

```rust
pub trait Clock: Send + Sync {
    fn monotonic_ms(&self) -> u64;   // deadlines; never goes back
    fn unix_secs(&self) -> u64;      // event timestamps ("HH:MM:SS" UTC via utc_time_string())
}
pub trait NonceSource: Send + Sync { fn next_nonce(&self) -> String; } // 16 hex digits
```

- Server: `SystemClock`, `session::decider::RandomNonce` (uses `rand`).
- Browser (T4): implement `Clock` over two imports, `host.now_unix_ms()` (`Date.now()`) and
  `host.now_monotonic_ms()` (`performance.now()`), and use `CounterNonce::new(seed)` (nobody
  else sees the token offline, so unguessability is not needed; uniqueness is).
- `batch::simulate*` and `replay_boundary*` read the deadline through `ReplayBasis::clock()`.
  The basis is a trait so the host does not need the server's `SessionConfig`:

```rust
pub trait ReplayBasis {
    fn state(&self) -> &GameState;                 // opening state
    fn galaxy(&self) -> Option<&Galaxy>;
    fn rng_marks(&self) -> &RngMarks;
    fn history_generation(&self) -> u64;
    fn clock(&self) -> &dyn Clock;
}
```

## The host API for T4/T5: commands in, `ServerMessage` JSON out

One game, one human seat (`ViewerRole::Player(seat)`), no sockets. The wasm export (T4) is
`host_command(json_in) -> json_out_array` (or a push callback); both ends are the existing
serde types, so the web client's `decode.ts` and `reduceServerMessage` need no changes.

Input is `ClientMessage` JSON (`{"type": "...", "protocol_version": 3, ...}`; unknown fields
are an error, `parse_client_message` validates the version). Output is a list of
`ServerMessage` JSON objects (`{"type": "...", ...}`), in the order the server would have sent
them to that one connection. The mapping, with the server code that produces each answer today:

| Command (`type`) | Output (`ServerMessage`) | Built by |
|---|---|---|
| `subscribe` | `initial_snapshot` (state, `galaxy_layout`, `map_tiles`, history status, pending choice with nonce) | `projection::project_initial_snapshot_with_map` |
| `submit_choice {nonce, expected_version, option_id}` | `action_accepted` or `action_rejected {reason}` (stale version, wrong nonce, unknown option), then after the engine step: `event` (0..n), `state_update` (every step), `pending_choice` / `turn_status` when asked again, `game_over` at the end | `journal::build_event`, `state_update::state_update_for`, `projection::project_turn_status`, `broadcast_game_over` |
| `undo {expected_version}` | `action_rejected {reason: undo_barrier \| no_pending_choice \| stale_version}` or, on success, a fresh `initial_snapshot` (offline: replay to the earlier position, same game id, `history.generation` + 1) | `StateUpdateInputs.may_undo`, `rng_force`, `replay` |
| `query_reach {request_id, system_id}` | `reach_into {origins, refused?}` | `state_update::answer_reach(game_id, request_id, system_id, version, Some((&state, &galaxy)))` |
| `preview_secondary {request_id, card, primary, answers}` | `secondary_preview` | `ti4_engine::secondary_preview` (server glue: `session/preview.rs`, stays out of the host for now; `ti4_draft` with `scope: secondary` is the wasm equivalent) |
| `run_draft` / `apply_draft` | `draft_outcome` / `draft_applied` (+ `state_update`) | `ti4-view::planning` (`script_answer`, `ti4_plan`) |
| `start_planning`, `reset_planning`, `edit_planning_movement`, `apply_planning`, `submit_planning_choice`, `secondary_planning` | `planning_status`, `planning_update`, `planning_result` | `planning::PlanningEnvelope` (the runner thread is the server's; offline uses the draft path above) |
| `set_reaction_mode {card, mode}` | `state_update` with `reaction_modes` | `StateUpdateInputs.reaction_modes` |
| `set_reaction_intent`, `pass_reaction_hold` | none (bluff holds are for several humans; dropped offline) | n/a |
| `ping {sequence}` | `pong` | trivial |

Batches (`BatchRequest` and the movement/production basket as one request) are decoded by
`batch::BatchRequest` / `MovementPlan` and checked by `batch::simulate`; the batch HTTP/WS
glue is in `ti4-server/src/session/registry.rs::submit_batch`. Offline it can instead be
mapped to a `PlanStep` script for `ti4-view::planning::script_answer`, see the plan, 2.2.

### What the host crate does not do: the session core

The state machine that owns the decision log, the event log, the redo stack, the batches and
`game_version` is still `ti4_server::session::worker::SessionShared` (fields are coupled to
`mpsc` subscribers, the file store, bluff books and planning threads). The browser has its own,
small one: `ti4_wasm::host_session::HostSession`, built out of the pieces above (see
`crates/ti4-wasm/README.md`, "The hosted game"). It produces the same events and decisions as
`SessionShared` for the same seed and answers (`crates/ti4-wasm/tests/it/server_parity.rs` checks
that against a real server session). The sequence that `SessionShared` performs after each engine
step is its specification, and it only calls host functions for the logic:

1. For each newly recorded `DecisionRecord`: `protocol::server::verified_decision_facts(record,
   selected, active_system, &state)` gives `(detail, movement, seat_detail)`.
2. `journal::reconcile_with_redo(...)`; unless `Duplicate`/`Reattach`, increment
   `event_counter` and `journal::build_event(&clock, game_id, counter, &decision_log, EventDraft {..})`.
   Append to the log; send `ServerMessage::Event(GameEventMsg { entry: entry.for_viewer(&viewer)? })`.
3. `journal::consume_or_fork_redo(&mut redo_decisions, &mut redo_events, &record)`; on
   `Forked`, also drop `batches` past the index and `rng_marks.truncate_to(index)`.
4. New dice: `protocol::server::public_roll(index, roll, active)` for each unseen roll of
   `game.rolls()`, then `journal::roll_is_known`/`roll_cursor`/`build_roll_event`.
5. `game_version += 1`, then one `state_update::state_update_for(&inputs, &inputs.path(), &viewer)`
   per viewer (offline: one). `auto_resolved` notes queued with `state_update::queue_auto_resolved`
   are drained by that update.
6. Game end: record `GameEventKind::GameFinished { winner }` and send `GameOverMsg`.

A save is `{seed, decisions: Vec<DecisionRecord>, rng_marks: RngMarks, events, redo..., engine hash}`;
`replay::replay_session_with_copy` re-executes it, and `replay::ReplayReport.hashes_match`
tells whether the engine still decides the same.
