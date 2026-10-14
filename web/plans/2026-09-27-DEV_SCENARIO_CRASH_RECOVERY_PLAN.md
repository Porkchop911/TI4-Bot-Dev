# Dev scenario crash recovery

## Implementation Progress & Status (COMPLETED)

- **Recoverable bot controllers in player init records:**
  - Added optional, validated `seats: Option<BTreeMap<PlayerId, SeatController>>` (`#[serde(default, skip_serializing_if = "Option::is_none")]`) to `PlayerGameInitRecord` in [`crates/ti4-server/src/storage.rs`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-server/src/storage.rs).
  - Validated in `validate_player_init`: ensures the seat controller map exactly matches the verified player ID set without credentials, while remaining completely backwards-compatible with older records where `seats` is omitted.
  - Updated `recover_player_session`: reconstructs `GameInitRecord` using `init.seats` when present, falling back to all-`Human` for legacy saves.
- **Pre-publish durable dev scenario launch & failure semantics:**
  - In `GameRegistry::launch_dev_scenario` ([`crates/ti4-server/src/session/registry.rs`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-server/src/session/registry.rs)), when `self.store` is present, attached the store to `SessionConfig` and durably persisted:
    1. `player_sessions.json` (authoritative credentials mapping)
    2. `init.json` (immutable engine initialization, map tiles, seed, initial state, and seat controllers)
    3. `lobby.json` (running player lobby record)
  - Records are derived from the _final_ scenario configuration (including all unit/card mutations and four-view seat adjustments).
  - Directory collision protection: checks whether the target directory already exists; returns an error without deleting or modifying existing files.
  - Atomic write failure cleanup: if any write fails, deletes only artifacts created by that failed launch (`lobby.json`, `init.json`, `player_sessions.json`, and the newly created directory) and returns an error without registering or advertising an in-memory session.
  - In-memory sessions and lobbies are only started and registered once all required writes succeed.
- **Durable scripted scenario startup:**
  - Scripted scenario advancement (`advance_into_space_combat` and `advance_into_invasion` in [`crates/ti4-server/src/dev/scenarios.rs`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-server/src/dev/scenarios.rs)) durably logs decisions and events to `decisions.jsonl` and `events.jsonl` via the attached store before the launch response succeeds.
  - If scripting fails, the error is returned to the caller so no URL or token is advertised; the save on disk remains recoverable at the last committed decision.
  - Eliminated subscription race in initial choice polling by checking `client.snapshot().pending_choice` if `try_recv()` has not yet buffered the initial offer.
- **Verification & Acceptance:**
  - Added dedicated integration tests in [`crates/ti4-server/tests/dev_scenario_recovery.rs`](file:///home/zibert/github/TI4-Bot-Dev/crates/ti4-server/tests/dev_scenario_recovery.rs):
    - `dev_scenario_space_combat_crash_recovery_preserves_bots_and_play`: launches `space_combat` with store, moves fleet into combat, drops registry, restarts and recovers via `recover_all_games_report`, verifies public lobby and player session views, asserts matching decision hashes and current state, verifies bot seat controllers (`BotFirstOption`), and continues live play where human acts and bots respond automatically with new decisions logged to disk.
    - `scripted_ongoing_combat_and_four_view_presets_recover_cleanly`: verifies scripted `ongoing_combat` (replaying scripted decisions and restoring bot controllers), `ongoing_combat_four_views` (retaining all-human seats), and an ordinary player lobby compatibility case.
    - `launch_failure_cleanup_and_backwards_compatibility`: verifies directory collision safety, clean rollback on failed writes, omission of phantom lobbies on unstarted/broken sessions, and clean recovery of legacy `init.json` records omitting `seats`.
  - All test suites in `cargo test -p ti4-server` pass.
  - `cargo fmt --all --check` clean.

---

## Goal

A scenario launched from `/dev/scenarios` should remain playable at its original `/games/{game_id}` URL after the server restarts, with the same player credentials, current decision, history, and bot behavior. Preserve existing recovery behavior for ordinary games. This plan is for _future launches_; the existing `data/games/dev_combat_b7078f12de2eb695/` cannot be reconstructed from its lone `player_sessions.json`.

## Diagnosis

- `crates/ti4-server/src/dev/scenarios.rs` builds a `SessionConfig` without a store and calls `GameRegistry::launch_dev_scenario`.
- `launch_dev_scenario` in `crates/ti4-server/src/session/registry.rs` starts and registers an in-memory session, but writes neither `lobby.json` nor `init.json` and does not attach the registry's `FileGameStore` to the session. Some later credential takeovers can write `player_sessions.json` on their own.
- Startup only considers directories with `init.json` or `lobby.json` (`storage.rs::list_saved_games` / `list_saved_lobbies`). A credentials-only directory is skipped. The lobby endpoint returns 404; `web/src/App.tsx` reduces the failed request to “Unable to load lobby.”
- Existing player-session recovery (`storage.rs::recover_player_session`) reconstructs every seat as `Human`. That would silently change the bots in normal dev scenarios even if their init and lobby were saved. The four-view scenarios deliberately use all-human seats.

## Implementation

1. **Persist an entire dev launch before publishing it.** In `GameRegistry::launch_dev_scenario`, when `self.store` exists, attach that store to `SessionConfig`, then save the versioned `PlayerLobbyRecord`, `PlayerSessionsRecord`, and `PlayerGameInitRecord` needed by the existing player-lobby recovery branch. Derive all records from the _final_ scenario configuration (including the four-view mutations), not from the base setup. Use the exact launch roster, map tiles, seed, initial state, and credentials. Only start/register the session and return its URL/token once all required writes succeed. Keep the no-store registry path working for existing in-memory tests.
2. **Make bot controllers recoverable.** Add an optional, validated seat-controller map to the immutable player init record, with a backwards-compatible default of all-human for existing records; use it in `recover_player_session` when reconstructing `GameInitRecord`. Check that the map covers exactly the player IDs and does not contain credentials. Update every constructor/fixture for the init record. Confirm format/version and checksum handling remain compatible with previously valid saves.
3. **Define failure and retry semantics.** The three launch files are separate atomic writes, not one transaction. If any write fails, do not advertise a playable in-memory game; clean up only artifacts _created by that failed launch_ or ensure a subsequent startup reports the incomplete launch clearly and does not publish a phantom lobby. Never delete an existing game's records on a generated-ID collision. Ensure a retry generates a fresh game ID and cannot mistake a stale directory for an empty destination. Document the ordering and failure points alongside the launch code.
4. **Check scripted scenario startup.** `ongoing_combat` and invasion presets advance the engine through scripted decisions after launch. With the store attached, these decisions must be durably logged using the same path as player/bot decisions before the launch response succeeds. If scripting fails, return an error and leave the save recoverable at the last committed decision (or explicitly clean up a new, unadvertised save); do not return a URL to a partially initialized scenario without documenting it.

## Verification / acceptance

- Add a temp-directory integration test in `crates/ti4-server/tests/dev_scenarios.rs` (or a dedicated recovery test): launch `space_combat` using `GameRegistry::with_store`, accept at least one human decision and allow a bot response, drop the registry/session, create a new registry over the same directory, and call `recover_all_games_report`. Assert the ID is recovered, public lobby and original player session work, state/decision hashes and pending choice match, bots retain their controllers, and play continues with new decisions persisted after the restart.
- Cover a scripted `ongoing_combat` preset and an all-human four-view preset, checking replay of their initial scripted choices and correct seat controllers. Test an ordinary player lobby as a compatibility case.
- Inject a failed launch write and check no partial game becomes playable or advertised after restart; retain the records of unrelated games. Check both old player init records (missing the new controller field) and newly written records.
- Run `cargo fmt --all --check` and the relevant `ti4-server` recovery/dev-scenario tests. If the UI message changes, run the relevant web tests and `npm run build` from `web/`.

## User-facing result

Once this is implemented, restart the server with the same `TI4_DATA_DIR` (or `--data-dir`), reopen the original `/games/{game_id}` URL in the browser tab that holds its `sessionStorage` player token, and continue. A new tab without that token can view the lobby and use the supported takeover flow where available.
