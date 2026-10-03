# BF status draw atomicity — 2026-10-03

Branch `codex/bf-status-draw-atomic`, based on `c5632448`. Scope: `crates/ti4-engine/src/status.rs` and this evidence file. Permission class P1: local source, test, and evidence edits only. No external paths, network, ports, generated artifacts beyond Cargo's existing `target`, deletion, or external-state changes.

## Trigger and rule

`resolve_before_token_gain_with` reveals an objective and moves cards from the action-card deck into hands before Yssaril Scheming asks which card to discard. An invalid answer returned `IllegalChoice` after these mutations. Invalid transitions must be atomic (AGENTS.md); the printed Scheming text is in `crates/ti4-content/content/abilities.json` at `DEFAULT`. The status draw order is LRR 81.2–81.4. The implementation snapshots the state when a draw-effect table is supplied and restores it if that hook returns an illegal choice. A plain six-faction status phase follows the same successful route; no behavior bounds are changed.

## Red/green evidence

`an_illegal_scheming_discard_leaves_the_status_state_unchanged` seats Yssaril and Sol, puts three distinct cards in the deck, and gives an answer outside the discard choices. Before the fix it failed: the objective and cards had already moved. After the fix it passes and compares the complete serialized state. It also verifies the returned error is `IllegalChoice`.

- `cargo test -p ti4-engine --lib status::tests -j 4`: 8 passed.
- `cargo test -p ti4-engine -q --no-fail-fast -j 4`: 1,882 library tests passed, 1 ignored; all integration and doctests passed.
- `cargo clippy -p ti4-engine --all-targets -j 4 --message-format short`: no warning in `status.rs` after the final cleanup. Other pre-existing warnings were not modified.
- `git diff --check`: passed.
- `cargo test -p ti4-sim -q -j 4`: 50 passed, 1 ignored, 1 failed. Failure is `profile::tests::fixture_capture_is_deterministic` because the existing map-pool fixture is absent in this reused clean checkout (`pool exists in this checkout`, OS code 3). This is the previously documented missing-fixture issue; no nondeterminism was observed in the other 50 tests. No fixture was created.

Independent review: Codex Terra reviewed the final `status.rs` diff and the hook/choice/driver paths, reported no actionable findings. The user required that no subagent above Terra be created, so this is a Terra review; no frontier-model review is claimed. The package changes legality/atomicity; if the repository gate still requires a frontier pass, that gate remains pending. Historical Python was not used. No performance or Python-parity claim is made.

The separate six-faction behavior comparison through `c5632448` is in `plans/evidence/BF-CODEX-2026-10-03.md` in the active checkout. That file records proposed values only, and the retired v45 gate remains unchanged.
