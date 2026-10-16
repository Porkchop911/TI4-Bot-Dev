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
| `protocol` | `ClientMessage`, `ServerMessage` and every message struct (`StateUpdateMsg`, `InitialSnapshotMsg`, `PendingChoiceMsg`, `TurnStatusMsg`, `ActionAcceptedMsg`/`ActionRejectedMsg`, `GameOverMsg`, `Event`...), `GameEvent`/`GameEventKind`/`EventVisibility`/`SeatDecisionDetail`/`MovementFact`/`PublicFact`/`PublicRoll` and their builders (`public_roll`, `decision_facts`, `verified_decision_facts`, `verified_event_facts`, `public_fact::decision_fact`, `state_fact::observe`, `decision_grouping`, `action_id_for`, `current_log_path`), `HistoryStatus`, `AutoResolvedNote`, `PROTOCOL_VERSION`, `parse_client_message`/`parse_server_message`, `check_submission` (nonce, version and option of a `submit_choice`/`apply_draft` against the pending choice; the seat check and the reservation stay with each host) | everywhere |
| `projection` | `project_initial_snapshot[_with_map]`, `project_state_update[_with_map]`, `redacted_state`, `project_turn_status`, `project_pending_choice`, `facts_context`, and the views and the facts they are made of (`project_session_update`, `SessionUpdate`, `FactsContext`; `messages.rs` wraps them in protocol messages) | `session/worker.rs`, `ws` |
| `planning` | The recorders (`RecordingDecider`, `ReplayDecider`, `RecordedDecision`), the draft (`draft::{run_draft, script_answer}`, `steps::PlanStep`), the scripts sent to the live game (`script::ScriptRuns`: one run per seat, `begin`/`answer`/`after_step` report the scripts that `Ended`, which each host sends as `draft_applied`; `OnMismatch::Stop` on the server, `TakeBack` in the browser; `first_answer`, `refused`, `tactical_complete`, `ACTION_OVER`; `check_script`, the check of a script on a copy of the game before it is sent, with its verdict `Check`), the checks of the publication boundary (`audit`, `fork`) and the undo of a seat (`undo::UndoTrack`: the answers that may still be taken back, fed with each answer; `UndoRules::Table` on the server, where another seat, a die, a shuffle and a revealed card cut, `UndoRules::Local` in the browser, where only a change of the page's seats cuts; `start`, `plan`, `can_undo`; `undo::start`, how far one undo goes back) | `session/mod.rs` (`run_draft`, `apply_draft`), `session/script.rs`, `session/ready_secondary.rs`, `session/undo.rs` |
| `staged` | The staged work of a seat (`SeatStaged`: the slots, `put` by compare-and-set on the revision of a slot, `Put`; `SecondaryDraft`/`DraftAnswer`, the draft of a secondary as web2 keeps it) and `staged::ready`, the ready draft of a secondary that a host sends when the game asks the seat (`asked`, `ready_draft`, `judge` of the run of the draft on a copy, `Check`, `unready_value`). The wire types are `protocol::staged`. The server keeps the shared book, its storage and the gate of a live session (`may_send`); `HostSession` keeps the slots in its save | `session/staged.rs`, `session/ready_secondary.rs`, storage, `ws` |
| `preview` | The secondary preview (`ClientMessage::PreviewSecondary`): `eligibility` of a seat at a `Position` (published state, decision log, the open question; refused with `no_strategic_action`, `is_primary` or `already_asked`), `answer` (the engine's `secondary_preview` on a `Basis`, or the refusal, as `SecondaryPreviewMsg`), `PreviewRequest`, `busy`. Each host keeps whether its game is live and the seat that asks; the server also keeps the lock, the copy of the position and the rate limit of a connection | `session/preview.rs`, `ws` |
| `view` | The facts of an update that the UI reads, beside the view: `tactical`, `exploration`, `action_cards`, `secret_objectives`, `score`, `agendas`, `secondary`, `reaction`, `seats`; `reaction_modes::NeverOffer`; the catalogue of the printed components (`components`). It was the crate `ti4-view` until 2026-10-14. | `session/worker.rs`, `ws`, lobby |
| `view::scenario` | The scenarios, the one system of prepared games (`SCENARIOS`, `find`, `start`, `apply`): the late games on eight seats, and the games of one question on three seats (`three_seats.rs`: a research, a production, a battle, an invasion, a score...). A `Scenario` names its seats that people play (`humans`; the rule bot plays the others), whether it plays on the seed of the table (`any_seed`), its map, and its `opening`: the answers of its first human seat that take a new game to its question (`opening`, `Opening`, `OpeningAnswer`; `OpeningRun` answers until the opening stops). A table starts one through `setup` (`start_from`) | lobby start (`registry.rs`, the opening in `session/worker.rs`), `dev` (`/api/dev/scenarios`) |
| `journal` | `reconcile_with_redo`, `build_event`, `build_roll_event`, `roll_action`, `roll_is_known`, `roll_cursor`, `consume_or_fork_redo`, `EventDraft`, `Reconciled`, `RedoOutcome` | `SessionShared::{record_and_broadcast_event, record_rolls, consume_or_fork_redo}` |
| `state_update` | `StateUpdateInputs`, `state_update_for`, `queue_auto_resolved`, `answer_reach` | `SessionShared::{broadcast_state_update, note_passed_window}`, `session/reach.rs` |
| `rng_force` | `RngMarks`, `RngForce` (forced random positions of a timeline, the "marks" of a history) | storage, turn redo, replay |
| `replay` | `replay_session`, `replay_session_forced`, `replay_session_with_copy`, `ReplayReport` (deterministic re-execution of a decision log, hash comparison) | storage, recovery |
| `step_snapshot` | `StepSnapshot` (copy of the game at a step boundary, fork for fast resume) | worker, batch |
| `batch` | `BatchRequest`, `MovementPlan`, `MovementStep`, `simulate[_with]`, `replay_boundary[_state]`, `replay_budget`, `ReplayBasis` (trait), `BatchRecord` (a batch applied to the history, saved with it) | registry `submit_batch`, storage |
| `history` | Rewrites of the past, played in memory: `splice` (the dry run of a removed or changed decision, `splice_preview`, `SpliceBaseline`, the strict matcher of recorded decisions) and `turn_redo` (the redo of a seat's last turn: `find_turns`, `redo_window`, `prepare_tail`, `autoplay` of the round after it with the dice and cards of the old timeline, `TurnRedoRecord`/`AlternateTimeline`, saved while a redo is in flight). The DTOs are `protocol::{splice, turn_redo}`; the gates, the publication and the storage stay in the server | registry (`turn_redo_ops.rs`, splice preview), storage |
| `clock` | `Clock`, `Deadline`, `NonceSource`, `SystemClock` (native only), `ManualClock`, `CounterNonce` | all of the above that need time |
| `setup`, `preset`, `map`, `maps` | Game setup: `setup` is the one path of every host (`TableSetup`, `draw`, `create`, `setup_options`, `SpeakerOrder`, the strategy-card sets `setup::card_set`); a start preset (`preset`) is a field of `TableSetup`. `map` has the board geometry and the game of a map with every faction given (`create_game_with_template_seated`, `rebuild_galaxy`; `create_game_with_template` seats the in-scope factions, for tests and fixtures), `maps` the map templates and their builder (`maps::{template_loader, build}`) and the map catalogue and previews (`maps::catalog`). The server and both browser hosts build the same opening state from the same seed. | lobby start, `/api/maps`, wasm setup |
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

One game, the seats of one page (`ViewerRole::Player(seat)`, the seat that is asked), no
sockets. The wasm exports are `ti4_host_start` and `ti4_host_command` (`crates/ti4-wasm/README.md`,
"The hosted game"); both ends are the existing serde types, so the offline client of `web/`
(`reduceServerMessage`) and web2 (`serverMessages.ts`, shared with its server transport) read the
messages of the server as they are. Both pages play through this one host.

Input is `ClientMessage` JSON (`{"type": "...", "protocol_version": 4, ...}`; unknown fields
are an error, `parse_client_message` validates the version). Output is a list of
`ServerMessage` JSON objects (`{"type": "...", ...}`), in the order the server would have sent
them to that one connection. The mapping, with the server code that produces each answer today:

| Command (`type`) | Output (`ServerMessage`) | Built by |
|---|---|---|
| `subscribe` | `initial_snapshot` (state, `galaxy_layout`, `map_tiles`, history status, pending choice with nonce) | `projection::project_initial_snapshot_with_map` |
| `submit_choice {nonce, expected_version, option_id}` | `action_accepted` or `action_rejected {reason}` (stale version, wrong nonce, unknown option: `protocol::check_submission`), then after the engine step: `event` (0..n), `state_update` (every step), `pending_choice` / `turn_status` when asked again, `game_over` at the end | `journal::build_event`, `state_update::state_update_for`, `projection::project_turn_status`, `broadcast_game_over` |
| `undo {expected_version}` | `action_rejected {reason: undo_barrier \| no_pending_choice \| stale_version}` or, on success, a fresh `initial_snapshot` (offline: replay to the earlier position, same game id, `history.generation` + 1) | `StateUpdateInputs.may_undo`, `rng_force`, `replay` |
| `query_reach {request_id, system_id}` | `reach_into {origins, refused?}` | `state_update::answer_reach(game_id, request_id, system_id, version, Some((&state, &galaxy)))` |
| `preview_secondary {request_id, card, primary, answers}` | `secondary_preview` (for the seat the page is shown as; `busy` unless the game waits for the page) | `preview::{eligibility, answer}` |
| `run_draft` / `apply_draft` | `draft_outcome` / `draft_applied` (+ `state_update`) | `ti4_host::planning` (`script_answer`; `script::{first_answer, check_script, ScriptRuns}` for `apply_draft`) |
| `set_reaction_mode {card, mode}` | `state_update` with `reaction_modes` | `StateUpdateInputs.reaction_modes` |
| `put_staged {slot, base_revision, value?}` (with `subscribe {staged: true}`) | `seat_staged` (to the connections of the seat; a ready draft of a secondary is sent when the game asks the seat) | `staged::SeatStaged::put`, `staged::ready` |
| `set_reaction_intent`, `pass_reaction_hold` | none (bluff holds are for several humans; dropped offline) | n/a |
| `ping {sequence}` | `pong` | trivial |

Batches (`BatchRequest` and the movement/production basket as one request) are decoded by
`batch::BatchRequest` / `MovementPlan` and checked by `batch::simulate`; the batch HTTP/WS
glue is in `ti4-server/src/session/registry.rs::submit_batch`. Offline it can instead be
mapped to a `PlanStep` script for `ti4_host::planning::script_answer`, see the plan, 2.2.

### What the host crate does not do: the session core

The game logic of a host is here: the protocol, the projection, the setup and the scenarios,
the scripts and their precheck, the undo of a seat, the staged work and the ready secondary, the
rewrites of the history and the secondary preview. What stays in `ti4-server` is how a game is
hosted there:

- The session core: `session/worker.rs` (`SessionShared`: the decision log, the event log, the
  redo stack, the batches and `game_version`, coupled to `mpsc` subscribers, the file store and
  the bluff books), `registry` (the lobby, the start of a table, the gates and the publication of a
  history change), `decider` (the seat of a remote person, the reservation of a submission),
  `transport`, `session/mod.rs` (threads and channels).
- What hides a hand from remote people: `bluff`, `bluff_hold`.
- The wait and the pause of the seats that the server plays (`random_bot`; the pick is
  `ti4_rule_bot::choose`), the thin wrappers `reach.rs`, `preview.rs` (the lock, the busy check,
  the copy of the position) and `undo.rs` (`Watched`), and `table.rs` (a lobby record as a
  `TableSetup`).
- The server's side of the drafts and the staged work: `draft_basis.rs` (the copy of the game
  that a draft is played on), `script.rs` (what each seat is told of its script), `staged.rs`
  (the book of the seats, its storage, the messages to the connections of a seat) and
  `ready_secondary.rs` (the run of a ready draft on the copy, the gate of a live session).
- `storage`, `http`, `ws`, `public`, `feedback`, `fixtures`, and the HTTP glue of `dev/` (a launch
  is a lobby that starts from a scenario).

The browser has its own, small session core: `ti4_wasm::host_session::HostSession`, built out
of the pieces above (see `crates/ti4-wasm/README.md`, "The hosted game"). It produces the same
events and decisions as `SessionShared` for the same seed and answers
(`crates/ti4-wasm/tests/it/server_parity.rs` checks that against a real server session). The
sequence that `SessionShared` performs after each engine step is its specification, and it only
calls host functions for the logic:

1. For each newly recorded `DecisionRecord`: `protocol::server::verified_event_facts(record,
   selected, active_system, &state)` gives `DecisionFacts { detail, movement, seat_detail, fact }`
   (`fact`: the typed `protocol::public_fact::PublicFact`, ids only, by the same visibility rule).
2. `journal::reconcile_with_redo(...)`; unless `Duplicate`/`Reattach`, increment
   `event_counter` and `journal::build_event(&clock, game_id, counter, &decision_log, EventDraft {..})`.
   Append to the log; send `ServerMessage::Event(GameEventMsg { entry: entry.for_viewer(&viewer)? })`.
3. `journal::consume_or_fork_redo(&mut redo_decisions, &mut redo_events, &record)`; on
   `Forked`, also drop `batches` past the index and `rng_marks.truncate_to(index)`.
4. New dice: `protocol::server::public_roll(index, roll, active)` for each unseen roll of
   `game.rolls()`, then `journal::roll_is_known`/`roll_cursor`/`build_roll_event` (with the
   decision log: a roll of the action phase carries the `action_id` of its action).
   Then the facts of the state: `protocol::state_fact::observe(&mut tally, &state, &events_since)`
   compares the public state with the `PublicTally` of the last step and, with the events
   recorded since, gives what no event told (a unit the game destroyed, the end of a combat and
   its winner, a planet taken, a card played, an exploration, an agenda resolved, an objective
   revealed); `journal::build_state_event` makes each a `GameEventKind::StateChanged` event that
   carries its `PublicFact`, grouped like a roll. A replay only moves the tally on; a restarted
   session tells the facts after its last decision once.
5. `game_version += 1`, then one `state_update::state_update_for(&inputs, &inputs.path(), &viewer)`
   per viewer (offline: one). `auto_resolved` notes queued with `state_update::queue_auto_resolved`
   are drained by that update.
6. Game end: record `GameEventKind::GameFinished { winner }` and send `GameOverMsg`.

A save is `{seed, decisions: Vec<DecisionRecord>, rng_marks: RngMarks, events, redo..., engine hash}`;
`replay::replay_session_with_copy` re-executes it, and `replay::ReplayReport.hashes_match`
tells whether the engine still decides the same.
