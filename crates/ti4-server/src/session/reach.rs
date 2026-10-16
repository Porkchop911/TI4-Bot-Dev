//! The read-only "who can go here" query (see `protocol::reach`).
//!
//! Like the secondary preview it holds the session lock only to copy the newest published
//! position, then asks the engine (`ti4_engine::reach_query`) on the copy: the worker is never
//! waited on and the game, its RNG, logs and storage are never touched. Only units that stand on
//! the board (public) are read.

use std::sync::Mutex;

use ti4_host::protocol::reach::ReachIntoMsg;
use ti4_host::state_update::answer_reach;

use crate::session::worker::SessionShared;

/// Answer one reach query for `system_id`.
#[must_use]
pub fn answer(
    shared: &Mutex<SessionShared>,
    game_id: &str,
    request_id: u64,
    system_id: &str,
) -> ReachIntoMsg {
    let (state, galaxy, version) = {
        let lock = shared.lock().expect("shared lock");
        let live = !(lock.stopped
            || lock.finished
            || lock.error.is_some()
            || (!lock.replay_complete && lock.pending_decision.is_none()));
        (
            live.then(|| lock.latest_state.clone()),
            lock.preview_galaxy.clone(),
            lock.game_version,
        )
    };
    let position = match (&state, &galaxy) {
        (Some(state), Some(galaxy)) => Some((state, &**galaxy)),
        _ => None,
    };
    answer_reach(game_id, request_id, system_id, version, position)
}
