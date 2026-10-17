//! The secondary preview on the server (the eligibility and the answer are
//! `ti4_host::preview`). What stays here is the lock and the busy check:
//!
//! * It takes the session lock only to check that the game is live and the request eligible, and
//!   to copy the newest published position (`latest_state`: the state at the top of the worker
//!   loop or at the last nested offer, whichever is fresher while a step is running), then
//!   releases it. The computation runs on the caller's thread on the copy; the worker is never
//!   waited on, and the game, its RNG, the decision log, the event log and storage are never
//!   touched.
//! * The answer goes only to the connection that asked, for that connection's own seat.

use std::sync::{Arc, Mutex};

use ti4_content::galaxy::Galaxy;
use ti4_host::preview::{Basis, Position, PreviewRequest, Refusal, busy, eligibility};
use ti4_host::protocol::server::SecondaryPreviewMsg;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::GameState;

use crate::session::worker::SessionShared;

/// The position a preview is computed from, copied out under the lock.
struct Copied {
    state: GameState,
    galaxy: Option<Arc<Galaxy>>,
    decisions: usize,
}

/// Why the game is not live or the request means nothing, or the copy of the position.
fn basis(
    shared: &SessionShared,
    seat: &PlayerId,
    card: &StrategyCardId,
    primary: &PlayerId,
) -> Result<Copied, Refusal> {
    if shared.stopped
        || shared.finished
        || shared.error.is_some()
        // (`history_active` is not asked: a session made by a batch commit or a history change
        // carries it for its whole life, so it says nothing about being live.)
        // A session started mid-step (a batch commit replaces it) replays the earlier decisions
        // inside its first step, so `replay_complete` only becomes true at the next loop top. A
        // live decision pending for a seat proves the replay has caught up.
        || (!shared.replay_complete && shared.pending_decision.is_none())
    {
        return Err(busy());
    }
    let position = Position {
        state: &shared.latest_state,
        decision_log: &shared.decision_log,
        asked: shared
            .pending_decision
            .as_ref()
            .map(|pending| (&pending.seat, &pending.choice)),
    };
    eligibility(&position, seat, card, primary)?;
    Ok(Copied {
        state: shared.latest_state.clone(),
        galaxy: shared.preview_galaxy.clone(),
        decisions: shared.decision_log.len(),
    })
}

/// Answer one preview request for `seat`. Never blocks the worker beyond the short lock to copy
/// the position, and changes nothing.
#[must_use]
pub fn answer(
    shared: &Mutex<SessionShared>,
    game_id: &str,
    seat: &PlayerId,
    request: &PreviewRequest,
) -> SecondaryPreviewMsg {
    let card = StrategyCardId::new(request.card.clone());
    let primary = PlayerId::new(request.primary.clone());
    let (copy, version) = {
        let lock = shared.lock().expect("shared lock");
        (basis(&lock, seat, &card, &primary), lock.game_version)
    };
    let basis = copy.as_ref().map_err(Clone::clone).map(|copy| Basis {
        state: &copy.state,
        galaxy: copy.galaxy.as_deref(),
        decisions: copy.decisions,
    });
    ti4_host::preview::answer(game_id, seat, request, version, basis)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::worker::{PendingDecision, PendingSubmissionState};
    use ti4_content::ContentStore;
    use ti4_engine::choice::Choice;
    use ti4_host::protocol::server::PreviewRefusal;

    fn shared() -> SessionShared {
        let ids = [PlayerId::new("a"), PlayerId::new("b")];
        let state = ti4_engine::setup::start_game(
            ContentStore::embedded(),
            &ids,
            ti4_model::content_types::POK,
            None,
        )
        .expect("game");
        SessionShared::new("g".to_owned(), state)
    }

    fn refusal(shared: &SessionShared) -> Option<PreviewRefusal> {
        basis(
            shared,
            &PlayerId::new("b"),
            &StrategyCardId::new("pok7technology"),
            &PlayerId::new("a"),
        )
        .err()
        .map(|(reason, _)| reason)
    }

    #[test]
    fn a_session_still_replaying_is_busy_until_a_live_decision_is_pending() {
        let mut shared = shared();
        shared.replay_complete = false;
        assert_eq!(refusal(&shared), Some(PreviewRefusal::Busy));
        // A session started mid-step by a batch commit only sets replay_complete at its next
        // loop top; a live decision pending for a seat shows the replay has caught up.
        shared.pending_decision = Some(PendingDecision {
            seat: PlayerId::new("a"),
            nonce: "n".to_owned(),
            game_version: 1,
            choice: Choice::new(PlayerId::new("a"), "x", vec![]),
            submission_state: PendingSubmissionState::AwaitingSubmission,
            submitted_option_id: None,
            reply_tx: None,
        });
        assert_eq!(refusal(&shared), Some(PreviewRefusal::NoStrategicAction));
        shared.stopped = true;
        assert_eq!(refusal(&shared), Some(PreviewRefusal::Busy));
    }
}
