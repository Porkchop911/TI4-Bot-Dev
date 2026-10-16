# Deduplicate ti4-view / ti4-host / ti4-server and the two wasm hosts (2026-10-14)

Status: Steps 1–5 done (2026-10-14); Step 6 done (items 1–6 2026-10-14, items 7–11 2026-10-15).

## Context

Two lineages were merged into `dev` on 2026-10-14:

- **Lobby lineage (Felix, "lobby phase 5/7")** put game setup into `ti4-view` → now
  `ti4_host::setup` (`TableSetup`, `draw`, `create`, `setup_options`, `SeatOrder`). Used by the
  server lobby start and by web2's local play (wasm `ti4_setup_*`, `ti4_play_setup`).
- **Offline lineage (T3–T10)** created `ti4-host` by moving code out of `ti4-server`, with
  `ti4_host::map::create_game_with_factions` as "what a lobby start calls", plus wasm
  `host_setup::Setup`/`describe()` and `HostSession` (`ti4_host_*`, `ServerMessage` JSON) for web/'s
  offline mode.

Result: two setup paths that **give different games for the same seed**, three "setup options"
builders, two card-set modules, 2–3 layers of re-export shims (with stale docs), a separate
`ti4-view` crate that only `ti4-host`'s users use, and two wasm game hosts speaking different
protocols. Goal: one implementation per concept, one crate between engine and hosts
(`ti4-host`), items imported from where they are defined.

Decisions (owner): **old offline and web2 saves may break** (no legacy path); **include the wasm
hosts**; **fold `ti4-view` into `ti4-host`**.

## Findings

**A. Two game-setup paths**

|             | `ti4_host::setup::{draw, create}`                                 | `ti4_host::map::create_game_with_factions`                                                                                |
| ----------- | ----------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Users       | server lobby (no preset), web2 local                              | server lobby **with preset** (`ti4-server/src/session/registry.rs:3416`), web/ offline (`ti4-wasm/src/host_setup.rs:203`) |
| Seats       | drawn (`seating::lobby_seat_order`), then factions for that order | factions by join index, no seat draw                                                                                      |
| Counts      | `PLAYER_COUNTS = 2..=8`                                           | `MIN/MAX_PLAYERS = 3..=6`                                                                                                 |
| Rule bot id | `bot_kind::RULE = "smart"`                                        | `"rule"`                                                                                                                  |

**B. Three setup-option builders**: `ti4_host::setup::setup_options` (typed), its server wrapper
`session::table::setup_options` (fine), and `ti4-wasm/src/host_setup.rs::describe()`, a
hand-written `json!` copy (card-set names duplicate `setup::card_set::name`).

**C. Card sets**: `ti4_host::setup::card_set` (constants) and `ti4_host::card_set` (re-export +
server-record `LEGACY`/`resolve`).

**D. Re-export chains** view → host → server: `ti4_host::protocol::{status,view}`,
`ti4_host::projection` (`pub use ti4_view::projection::*` + 4 wrappers), `ti4_host::planning`,
`ti4_host::map`, `ti4_host::maps`; `ti4_server::{map, card_set, preset, projection, protocol,
planning, maps}`, `ti4_server::session::reaction_modes`. ~300 call sites use the server paths.
Stale doc: `ti4-server/src/planning.rs` says "defined in `ti4-view`".

**E. `create_game_with_*` explosion**: 10 variants in `ti4-view/src/map.rs` + 4 in
`ti4-host/src/map.rs`, all ending in `create_game_with_template_seated`.

**F. Two wasm hosts** on the same `play` loop / `Host` checkpoint struct in `ti4-wasm/src/lib.rs`:

- web2: bespoke C-ABI (`ti4_play*`, `ti4_draft`, `ti4_plan`, `ti4_journal`, `ti4_replay`,
  `ti4_switch_seats`, `ti4_set_stepping`, `ti4_set_rule_seats`, update buffer of view+choice);
  consumed by `web2/src/session/wasmTransport.ts`.
- web/: `ti4_host_start/command/query` → `HostSession` → `ServerMessage` JSON (same as the server).
  web2 already has `wsTransport.ts` decoding `ServerMessage` behind the same `transport.ts`
  interface, so web2 can drive the `ServerMessage` host too.

**G. `ti4-view` no longer needs to be its own crate.** Its only dependents are `ti4-host`,
`ti4-server` and `ti4-wasm`, and the last two depend on `ti4-host` anyway (`ti4-engine` only
mentions it in a comment, `timing.rs:1188`). Its dependencies are `ti4-host`'s plus `ti4-calc`, and
both are wasm-clean. It has no `tests/` dir; its only asset is
`src/maps/community_maps.json` (+ `include_str!` of `ti4-content/content/map_templates.json`,
same relative depth after the move). Half of D (the host-side shims) exists only because of the
crate split.

**H. Server code that is not about hosting.** After Step 5, `HostSession` has several local human
seats (hot seat). It then needs the per-seat logic that only the server has, and web2 drives
both hosts through one `ServerMessage` transport. Still in `ti4-server`:

- Re-export shims missed by Step 4: `session/mod.rs:20-21` (`smart_bot`,
  `smart_bot_faction_plans`), `:86-88` (`ti4_host::{batch, replay, rng_force, step_snapshot}`,
  `replay_session*`, `RngForce`, `RngMarks`). About 35 files use them.
- Undo exists twice: the server's `session/undo.rs` `UndoTrack` (barriers
  `OtherSeat`/`Random`/`Revealed`) and the wasm host's `own` + seat-switch fence + `undo_start`.
  `planning::undo::{inside, start}` is already shared. The two differ only in which events are
  barriers.
- The script application exists twice: the server's `session/script.rs` (`ScriptApplication`,
  per seat) and the wasm host's `host_session.rs` (`ScriptRun`, one person). The precheck also
  exists twice: the server's `check_script`/`Probe` and the wasm `host_api.rs` refusal of
  `draft_outcome`.
- The server keeps staged work and sends a ready secondary (`session/staged.rs`,
  `ready_secondary.rs`). The wasm host ignores `PutStaged`, and web2 has a second path
  (`localSeatStore`, the client-side ready send in `useLiveSession.ts`).
- The choice submission check (nonce, version, option) exists in both `GameSession::submit_choice` and
  `HostSession`.
- `session/splice.rs`, `session/turn_redo.rs`: pure history logic. Their DTOs are already in
  `ti4_host::protocol`. Their only server type is `storage::BatchRecord`.
- `session/preview.rs::basis`: pure, except for the "busy" clause. `HostSession` refuses
  `preview_secondary`.

**I. The planning-runner protocol has no client.** `ti4-server/src/planning/runner.rs` (1001 lines),
the runners/applications/`SecondaryRound` of `session/planning.rs`, the `ClientMessage`s
`start_planning`, `reset_planning`, `edit_planning_movement`, `apply_planning`,
`submit_planning_choice`, `secondary_planning`, and the `planning_status`/`planning_update`/`planning_result`
messages. web2 (`src/session/wire.ts:777`) and web/ (`src/protocol/client.ts:46`,
`offline/local/LocalTransport.ts:34`) ignore them, and `HostSession` refuses them. Both hosts use
drafts (`run_draft`/`apply_draft`).

**J. Two scenario systems.** `ti4-server/src/dev/scenarios.rs` (1349 lines, 14 ids on a 3-seat base
game; a thread answers through `MockClient` with sleeps until the wanted question) serves
`/api/dev/scenarios[/launch]`, the web/ e2e specs, `web/src/dev/DecisionGallery.tsx`,
`ti4-bot-agent` and 10 server tests. `ti4_host::view::scenario` (`late-game`,
`late-game-deflectors`) is a function over the state of a new game, and both hosts start it through `TableSetup`.

## Plan (one PR per step, in order)

### Step 1: fold `ti4-view` into `ti4-host` (G, host half of D)

1. Mechanical move: `git mv crates/ti4-view/src/* crates/ti4-host/src/view/`, `lib.rs` →
   `view/mod.rs` (crate doc → module doc, keep `pub mod …` list). `ti4-host/src/lib.rs` gets
   `pub mod view;`. Add `ti4-calc` to `ti4-host/Cargo.toml`; drop `ti4-view` from the workspace
   `members`/`[workspace.dependencies]`, and from `ti4-server`/`ti4-wasm` `Cargo.toml`.
   Rewrite paths: `ti4_view::` → `ti4_host::view::` (server, wasm), `ti4_view::` →
   `crate::view::` (inside host); `crate::` inside the moved files → `crate::view::` where it named
   a ti4-view module. Fix the `include_str!` paths if the depth changed (`view/maps/…` is one
   level deeper: `../../../../ti4-content/…`).
   Commit this alone, so `git log --follow` keeps the history.
2. Merge the pairs that the split had made into one module each, and delete the shims:
   - `view::status`, `view::view` are the wire DTOs: `protocol/status.rs` and `protocol/view.rs`
     become the real files (move them in, delete the `pub use` shims).
   - `view::projection` (1249 lines, views) + `projection.rs` (message wrappers, 224): one
     `projection` module (`projection/mod.rs` + `projection/messages.rs`).
   - `view::planning/` + `planning.rs` (wire DTOs): one `planning/` (`wire.rs` for the DTOs).
   - `view::map` + `map.rs`: one `map.rs` (after Step 3 it is small).
   - `view::maps::{build, template_loader}` + `maps::catalog`: one `maps/`.
   - The rest stays as `ti4_host::view::{action_cards, agendas, components, exploration,
     reaction, reaction_modes, scenario, score, seats, secondary, secret_objectives, tactical}`:
     the facts of the update the UI reads.
3. `.cargo/config.toml`: in `c-web`/`l-web`/`t-web` replace `-p ti4-view` with `-p ti4-host` (the
   package count stays the same; the host's own tests now run in `t-web`).
4. Docs: `ti4-host/README.md` (module table, "thin layer over ti4-view"), `crates/ti4-wasm/README.md`,
   `web2/AGENTS.md` (crate list back to eight; "a fact the UI needs is added in `ti4-view`" →
   "in `ti4-host/src/view/`"; paths `crates/ti4-view/src/scenario.rs`, `ti4_view::seats`,
   `ti4_view::reaction_modes`, `ti4_view::components`, `script_answer in ti4-view`), `web2/NEXT.md`,
   the engine comment. Old plans in `web2/plans/` stay as written (history).

### Step 2: one setup path (A, B, C)

- `TableSetup` gains `preset: Option<String>`. `setup::create` applies the preset logic lifted
  from `map::create_game_with_factions` (`preset::rotation`, `roster`, `seat_roster`, `apply`);
  `setup::draw` uses `SeatOrder::Join` + preset roster when a preset is set. `SpeakerOrder` moves
  from `map` to `setup`, next to `SeatOrder`.
- `registry.rs` lobby start: drop the preset branch; always `setup::draw` + `setup::create`.
  Server games are unaffected on replay (the init record stores the opening state).
- `ti4-wasm/src/host_setup.rs`: `Setup` becomes a thin request type → `TableSetup` + chosen factions
  → `setup::draw/create`. `describe()` = `serde_json::to_value(setup::setup_options("The page", 0))`
  plus the `maps`/`presets`/`content_digest`/`engine_build` extras. Player range from
  `setup::PLAYER_COUNTS`, bot id `bot_kind::RULE` (`"smart"`; accept `"rule"` via serde alias for
  the web/ form). Update `web/src/offline/local/hostApi.ts` types + form if fields change.
- `ti4_host::card_set::{LEGACY, resolve}` → `ti4-server` (storage concern; `storage.rs` or
  `session/table.rs`). `setup::card_set` is the only set module.
- Delete `map::create_game_with_{preset,options,speaker,factions}`; keep `rebuild_galaxy`.
  Migrate their test/fixture callers to `TableSetup` or `create_game_with_template_seated`.

### Step 3: collapse `create_game_with_*` (E)

Keep `create_game_with_template_seated` (the real builder) and one
`create_game_with_template(content, ids, seed, template)` for tests/fixtures; replace
`_rotated`, `_and_card_set`, `_map_*` variants at their call sites (mostly `ti4-server/tests`,
`fixtures.rs`, `dev/scenarios.rs`; grep the workspace).

### Step 4: remove the server re-export layer (server half of D)

- Delete `ti4-server/src/session/reaction_modes.rs`, the `pub use ti4_host::…` block of
  `ti4-server/src/lib.rs`, `ti4-server/src/planning.rs`'s re-exports (keep `runner`),
  `ti4-server/src/maps/` re-exports (move `variant_seed` next to its HTTP caller; keep
  `community_tests`).
- Mechanical rewrite: `crate::{protocol,projection,map,maps,card_set,preset}::` and
  `ti4_server::{…}::` → `ti4_host::…::`, then `cargo clippy --fix`. Fix stale module docs.

### Step 5: one wasm host (F)

1. Extend `HostSession` to what web2 local play needs that it lacks: several local human seats
   - seat switching (`ti4_switch_seats`/`humans` mask), rule-seat mask, stepping/pause between
     bot decisions, the wait of the bots, scenario start (already in `Setup`), draft/plan
     (`run_draft`/`apply_draft`, planning commands → `ti4_host::planning`). Expose them as
     `ClientMessage` local commands (`HostSession::local`) rather than new C exports.
2. Rewrite `web2/src/session/wasmTransport.ts` on `ti4_host_start/command/query` + the
   `ServerMessage` decoding already used by `wsTransport.ts` (share it). Keep
   `transportContract.test.ts` / `wasmTransport.test.ts` green; web2 saves switch to the
   `HostSession::save_state` format (old web2 saves break). `localTable.ts` uses the setup
   exports that remain (`ti4_setup_options/preview/draw` stay, on `ti4_host::setup`).
3. Delete the bespoke exports and their state from `ti4-wasm/src/lib.rs` (`ti4_play*`,
   `ti4_draft*`, `ti4_plan`, `ti4_journal`, `ti4_replay`, `ti4_saved_option`, the update buffer).
   The `play` loop and checkpointing stay, owned only by `HostSession`'s path.
4. Update `ti4-wasm/README.md`, `ti4-host/README.md`, `web2/AGENTS.md` ("Local play": saved game
   format, `ti4_journal`, `ti4_set_stepping`).

### Step 6: the rest of the server's game logic into the host (H, I, J)

One commit per item, in this order (planning deletion early, so `worker.rs`/`session/mod.rs` are
smaller before the moves; scripts before undo, both touch `HostSession::selected`; scenarios last).
Starts after Step 5 is committed.

1. **Shims.** (Done.) The users of the `session/mod.rs` re-exports (`smart_bot`,
   `smart_bot_faction_plans`, `batch`, `replay`, `rng_force`, `step_snapshot`, `replay_session*`,
   `RngForce`, `RngMarks`) import them from `ti4_host::…` / `ti4_rule_bot::…`; the shims are gone.
2. **Delete the planning runner (I).** (Done.)
   - `ti4-server/src/planning/` is gone, with the six planning `ClientMessage`s and their handlers
     (`ws/mod.rs`, `session/mod.rs`, `registry.rs`), the `planning_status`/`planning_update`/
     `planning_result` messages, `ti4_host::planning::wire`, `DraftApplication`,
     `SecondaryDraftStatus`, `DraftKind`, `PlanningRejection`, `SessionConfig::plans`, and the tests
     `planning_runner`, `session_planning`, `session_secondary_planning`,
     `secondary_planning_runner`, `planning_transport` (+ their cases in `protocol_roundtrip`,
     `ws_lifecycle`). `protocol_roundtrip` checks that the old message types are unknown.
   - Kept: `ti4_host::planning::{draft, audit, fork, steps, undo}` and the recorders
     (`RecordingDecider`/`ReplayDecider`, used by `replay.rs` and
     `ti4-mlp/examples/capture_offline_pilot.rs`), with their test `planning`. The helpers that
     play a strategic action to its followers and the test of a secondary drafted as a script are
     in `tests/it/secondary_draft.rs` (`ready_secondary` uses the helpers).
   - `session/draft_basis.rs::DraftBasis` replaces `session/planning.rs`: `completed_step`,
     `fork()` (the checkpoint factory), `close` and `watch: SecondaryWatch`, as `run_draft_on`,
     `apply_draft` and `ready_secondary` use them. The worker binds the watch to the game
     (`SecondaryWatch::bind`); the decider hooks (`application_option`, `application_answered`,
     `begin_secondary_application`) are gone.
   - Clients: the `planning_*` types and ignore lists are gone (`web2/src/session/wire.ts`,
     `web/src/protocol/client.ts` + test, `web/src/offline/local/LocalTransport.ts`).
     `PROTOCOL_VERSION` is 4.
3. **Submission check.** (Done.) `ti4_host::protocol::check_submission(pending: Option<(&Choice,
   &str)>, nonce, expected, current, option_id: Option<&str>) -> Result<(), RejectionReason>`
   checks, in this order, a pending choice, the nonce, the version and the option (`answer_for`).
   The server calls it in `reserve_choice` and `apply_draft` (`PendingDecision::open` is `None`
   while a submission reserves the choice); `HostSession::check_pending` calls it and then refuses
   a free-form answer, which has no place among the options. The seat and reservation checks stay
   in the server.
4. **Script application.** (Done.) `ti4_host::planning::script::ScriptRuns` (map seat → run;
   `begin`, `answer(&Choice) -> (Option<ChoiceOption>, Vec<Ended>)`, `after_step(tactical_complete,
   active) -> Vec<Ended>`; it stops another human seat's script when that seat is asked).
   `OnMismatch` says what a refused entry does: `Stop` (server: the earlier entries stay) or
   `TakeBack` (wasm: `Ended::taken_back`, the host rolls back to the choice the nonce numbers and
   reports the script as sent). `first_answer` (the first answer, or why the game refuses the
   script), `ACTION_OVER`, `refused` and `tactical_complete` move with it. Each host only sends
   `DraftApplied`.
5. **Script precheck.** (Done.) `ti4_host::planning::script::{check_script, Check}` (the
   `Probe` plays the tail of the live game on a copy, then the script, behind the actor's Never
   cards; `Refused` / `Plays` / `Unknown`). The server calls it in `apply_draft` with the copy of
   `DraftBasis`. `HostSession::apply_script` calls it through `PlayLoop::script_copy` (`lib.rs`:
   the game at its last finished step, the last checkpoint with the steps since it played again,
   and the length of the log there) and the tail of its decision log: a refused script is
   answered with `draft_applied` and the game is not resumed. The take-back of `OnMismatch` stays
   for what the copy cannot see (a decision of another seat before the mismatch).
6. **Undo.** (Done.) `ti4_host::planning::undo::UndoTrack` with `UndoRules`:
   - `Table` (server): another seat, a die, a shuffle, a revealed card cut the tail
     (`answered`, `observe` with the engine latch and `learned_identity`, `plan`, `can_undo`).
   - `Local` (wasm): only a change of the page's seats cuts (`seats_changed`), because a
     checkpoint restores the dice; `observe` does nothing. The track keeps every answer of the
     page (the history counts them), and an undo starts no earlier than the last change.
     `UndoEntry` keeps the seat, `decision`, `asked` (the checkpoint reach), subtype, kind,
     `passed` and the plan step; under `Table` also a window that may be declined, read against
     the Never cards at the undo. `start(seat, never)` is the place of the first answer one undo
     takes back (`Table`: only the seat of the tail), `steps_from`, `take_back(keep)` for a
     checkpoint. `Watched` (it holds `Arc<Mutex<SessionShared>>`) stays in the server
     (`session/undo.rs`). `HostSession` pushes an entry for each answer of the page when its
     record is published (`selected` → `after_step`/`offer`); `Own`, `HostSession::own` and the
     server's `start` wrapper are gone. Tests: the track's own (moved from the server, plus the
     local rules) and `host_local::an_undo_of_the_page_goes_over_its_other_seats_and_stops_at_a_change_of_them`.
7. **Staged work and the ready secondary.** (Done.)
   - `ti4_host::staged` (`SeatStaged`, `Put`, `SecondaryDraft`, `DraftAnswer`) and
     `ti4_host::staged::ready` (`asked`, `Ready` with its `scope`, `ready_draft` over
     `&BTreeMap<PlayerId, SeatStaged>`, `judge(&DraftOutcome, len) -> Check` (`Fits`, `Unfit`;
     `Unknown` when there is no copy), `unready_value`).
   - The server keeps `StagedBook`, `put_and_tell`, storage, `may_send`, and the run on its copy
     (`ready_secondary::check` over `run_draft_on`).
   - `HostSession` keeps the slots of each seat (in `save_state`, `Resume::staged` and every
     `Restart`). The page is one connection (`PAGE_CONNECTION`), opted in by `Start::staged` or
     `subscribe {staged}`: it is sent `SeatStaged` with `connection` after each snapshot and when
     the seat it is shown as changes; revisions count up for the whole page, so a write meant for
     another seat is stale. It handles `PutStaged` for the seat it is shown as. It sends the
     ready secondary in `ask` and after a `PutStaged` of the slot while the seat is asked (the
     reply resumes the game), through `PlayLoop::draft` (`draft_outcome_of` in `lib.rs`) and
     `ScriptRuns::begin`. `switch_seat` clears the slots of the seat that is given up. Tests:
     `tests/it/host_staged.rs`.
   - web2: `localSeatStore` and its `localStorage` keys, and the client-side ready send
     (`serverSends`, `holding` in `useLiveSession.ts`) are gone. `wasmTransport.ts` gives
     `Transport.staged` through `seatSync` (`put_staged` while the game is parked, then the save);
     `seatSync` takes the message after a snapshot as the seat's slots whatever their revision.
8. **History rewrite.** (Done.) `ti4_host::history::{splice, turn_redo}` (moved with `git mv`
   from `session/`) and `ti4_host::batch::BatchRecord` (from `storage.rs`). The server's
   registry, storage, examples and tests import them from the host. `registry/turn_redo_ops.rs`
   stays (gates, publish, storage).
9. **Secondary preview.** (Done.) `ti4_host::preview::{eligibility, answer}` (with
   `PreviewRequest`, `Position`: the published state, the decision log and the open question,
   `Basis`, `busy`). The server's `preview.rs` keeps the lock, the busy check and the copy of the
   position; `ws` keeps the seat of the connection and the rate limit. `HostSession::preview`
   answers `preview_secondary` for the seat the page is shown as, on the position it sees; it is
   busy unless the game waits for the page (a question or a step). web/ offline does not send it
   (its page keeps its own estimate). Test: `host_preview` (each reply in a game with stepping
   equals the server's `preview::answer` at the same position).
10. **One scenario system (J).** (Done.) `ti4_host::view::scenario` (`scenario/mod.rs`,
    `scenario/three_seats.rs`) is the only one:
    - `Scenario` has `category`, `humans` (seat places played by people; the rule bot plays the
      rest), `any_seed` (a scenario that finds its systems by seat plays on the seed of the
      table; `Scenario::game_seed`, `TableSetup::game_seed`), `map` (`Recommended` template or
      the seeded `Random` board) and `opening: Option<fn(&Choice, &Opening) -> OpeningAnswer>`
      (`Answer(id)` / `Stop`). `scenario::opening(scenario, state, galaxy)` gives the `Opening`:
      the first seat of `humans` and the system the scenario prepared (the border system of a
      battle, the system of an invasion). `OpeningRun` answers the questions of that seat until
      the opening stops. `start` and `scenario_preview` take the seed of the table.
    - The 15 dev scenarios are ported to `three_seats.rs` (Sol, Hacan, Letnev in round 1 on the
      seeded random board): `setup_base_3p_game` + `build_*` + the edits of `launch_scenario` →
      `apply`; the `advance_into_*` / `wait_for_decision_subtype` loops → openings with the same
      picks and stop conditions. `SCENARIOS` lists the two late games first, then these.
    - Both hosts play the opening in a new game only, for the seat of the opening when a person
      plays it: the server wraps the decider of that seat in the worker (`OpeningDecider`; only
      with no prior decisions; `restart_config` drops it); `HostSession::open` (in
      `host_api::prepare` for a request with no save and version 0) and `HostSession::scripted`
      answer before `ask`. The answers are ordinary recorded decisions.
    - `/api/dev/scenarios` lists `SCENARIOS` (as `ScenarioSummary`, the shape of web/'s
      launcher). `/launch` keeps its request and response (`scenario_id`, `seed`, `game_id`,
      `player_session`, `player_id`, `test_seats`): it makes a lobby (`dev_<id>_<hex>`) with a
      person in each seat of `humans` and the rule bot (no wait) in the others, starts it from
      the scenario through the lobby start (`setup::draw/create`, the opening set on the
      `SessionConfig`), and returns once the first person is asked
      (`GameSession::wait_until_asked`). `test_seats` holds the credential of every seat of a
      person. `dev/scenarios.rs`, `GameRegistry::launch_dev_scenario` and the `MockClient`
      polling are gone. web2's `setup_options` and `ti4_scenarios` list the ported scenarios.
    - Tests: one server test per id (`dev_scenarios::*_opens_at_*`: launch → seat and subtype as
      before on three seeds, and on seed 42 the same opening answers), the dice of the four-view
      battle on seed 42, `host_scenario` (each scenario of three seats opens at the same
      question after the same opening in a local game of the page and on the server),
      `three_seats::tests`.
11. Docs, with each item: `ti4-host/README.md` (module table: `planning::script`, `staged`,
    `history`, `preview`, `view::scenario`; the host API rows for planning; "What the host crate
    does not do"), `crates/ti4-wasm/README.md` (the list of differences, the scenarios),
    `web2/docs/agents/local-play.md` and `lobby.md` (the scenarios and their openings). (Done.)

Stays in the server: `worker`, `registry`, `decider`, `transport`, `mod.rs` (threads, channels,
lobby, reservations); `bluff`, `bluff_hold` (they hide a hand from remote people); `random_bot`
(the pick is already `ti4_rule_bot::choose`; what stays is the delay and the pause handshake);
`reach.rs` (thin wrapper); `table.rs` (lobby record → `TableSetup`); `storage`, `http`, `ws`,
`public`, `feedback`, `fixtures`, the `dev/` HTTP glue.

(Not in scope: moving server `SessionShared` onto `HostSession`; `server_parity` already guards it.)

## Verification

Builds only through the aliases of `.cargo/config.toml` (no `-p` subsets, no `--workspace`; see
`web2/AGENTS.md`, "Rust builds"); one build or suite at a time.

- While working: `cargo l-web`, `cargo t-web <filter>` (e.g. `both_hosts`, `bot_lobby_factions`,
  `map_selection`, `player_lobby_admission`, `table_in_game`, `dev_scenarios`, `server_parity`).
- Before each commit: `cargo fmt --all -- --check`, `cargo l-web`, `cargo t-web`;
  `scripts/build-wasm.sh` + `npm test` in `web2/` (host/wasm changed); `cargo b-web` +
  `npm test` (server changed, transport suite). Before the push: `cargo t-web-slow`.
- ti4-host contract: `cargo clippy -p ti4-host --target wasm32-unknown-unknown` (another target,
  so it does not disturb the alias build); CI runs it too. After Step 1 it covers the former
  ti4-view code too.
- Step 1 is a pure move: `cargo t-web` has the same test count (+ the host's own tests, now
  included), and `scripts/build-wasm.sh` gives a wasm of about the same size (size gate).
- New test (Step 2): same `TableSetup` + seed through the server lobby start and through wasm
  `Setup::build` → identical opening `GameState` (extend `crates/ti4-server/tests/it/both_hosts.rs`).
- Decision-purity gate (ti4-host README): `hash_survey` on a copy of `/var/lib/ti4-public`,
  byte-identical before/after each step (built with the alias package set).
- Step 5: `npm test` in `web2/` and `web/`; a web2 local game in the browser (start a table,
  hot-seat switch, undo, draft, reload from save); web/ offline e2e.
- Step 6: per item `cargo t-web` with the filters `undo`, `script`, `draft`, `ready_secondary`,
  `staged`, `turn_redo`, `splice`, `secondary_preview`, `server_parity`, `history`,
  `dev_scenario`, `protocol_roundtrip`; `hash_survey` byte-identical; `npm test` in `web2/` and
  `web/`. In the browser: a web2 local hot-seat game with an undo across a seat switch, a ready
  secondary sent offline, staged work that survives a reload. Scenarios: a new server test per id
  (launch → first pending choice: subtype and seat as before); the web/ e2e scenario specs
  (`space_combat_four_views` with seed 42 keeps its dice); a ported scenario in web2 local play
  opens at the same question as on the server.
