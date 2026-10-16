//! The read-only secondary preview a seated follower can ask for while a strategic action is in
//! progress (see `ti4_engine::secondary_preview` for how the answer is computed).
//!
//! What this module guarantees:
//!
//! * **Read only.** It takes the session lock only to check eligibility and to copy the newest
//!   published position (`latest_state`: the state at the top of the worker loop or at the last
//!   nested offer, whichever is fresher while a step is running), then releases it. The
//!   computation runs on the caller's thread on the copy; the worker is never waited on, and the
//!   game, its RNG, the decision log, the event log and storage are never touched.
//! * **Private.** The answer goes only to the connection that asked, for that connection's own
//!   seat. It is computed from that seat's own holdings and the public board.
//! * **Only when it means something.** A seated follower, while the action phase's latest action
//!   is a strategic action by another seat whose card is still unexhausted, and before that seat
//!   has been asked anything of the action. Anything else is refused with a reason.
//!
//! The freshest position available while the primary is mid-step is the one published at its
//! last nested offer; before the primary's first question it is the position at the top of the
//! step, i.e. BEFORE the primary. Either way the answer is "as of now": the primary's remaining
//! effects, followers asked earlier in clockwise order and action cards can still change what the
//! real question offers, so a client re-checks its plan against the real question.

use std::sync::{Arc, Mutex};

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::secondary_preview::{SecondaryPreview, preview_secondary};
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::{GameState, Phase};

use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::server::{PreviewRefusal, PreviewResult, SecondaryPreviewMsg};
use crate::session::worker::SessionShared;

/// One preview request as the connection layer hands it over (the seat is the connection's).
#[derive(Debug, Clone)]
pub struct PreviewRequest {
    pub request_id: u64,
    pub card: String,
    pub primary: String,
    pub answers: Vec<String>,
}

/// The position a preview is computed from, copied out under the lock.
struct Basis {
    state: GameState,
    galaxy: Option<Arc<Galaxy>>,
    version: u64,
    decisions: usize,
}

/// Why no strategic action is in progress for this follower, or the basis for the preview.
fn basis(
    shared: &SessionShared,
    seat: &PlayerId,
    card: &StrategyCardId,
    primary: &PlayerId,
) -> Result<Basis, (PreviewRefusal, String)> {
    let refuse = |reason, detail: &str| Err((reason, detail.to_owned()));
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
        return refuse(PreviewRefusal::Busy, "the game is not live right now");
    }
    let state = &shared.latest_state;
    if state.phase != Phase::Action || state.player(seat).is_none() {
        return refuse(
            PreviewRefusal::NoStrategicAction,
            "no strategic action is in progress",
        );
    }
    if seat == primary {
        return refuse(PreviewRefusal::IsPrimary, "you played this card");
    }
    // The action in progress is the newest "action phase" decision, taken by the primary, and
    // the primary is still the active seat (Coup d'Etat passes the turn without exhausting).
    let log = &shared.decision_log;
    let Some(start) = log
        .iter()
        .rposition(|record| record.prompt == "action phase")
    else {
        return refuse(
            PreviewRefusal::NoStrategicAction,
            "no strategic action is in progress",
        );
    };
    let action = &log[start];
    let named = action.chosen.strip_prefix("strategic|");
    let strategic = action.chosen == ti4_engine::STRATEGIC_ACTION_ID || named.is_some();
    let holds_unexhausted = state.player(primary).is_some_and(|holder| {
        holder.strategy_cards.contains(card) && !holder.exhausted_strategy_cards.contains(card)
    });
    if !strategic
        || &action.player != primary
        || named.is_some_and(|id| id != card.as_str())
        || state.active.as_ref() != Some(primary)
        || !holds_unexhausted
    {
        return refuse(
            PreviewRefusal::NoStrategicAction,
            "that strategic action is not in progress",
        );
    }
    // Every decision the seat answered after the action began is in the log; once there is one
    // the seat has been asked. A question open for the seat right now counts as asked too.
    let asked_now = shared.pending_decision.as_ref().is_some_and(|pending| {
        &pending.seat == seat
            && pending
                .choice
                .details
                .get("kind")
                .and_then(serde_json::Value::as_str)
                == Some("strategy_secondary")
    });
    if asked_now || log[start + 1..].iter().any(|record| &record.player == seat) {
        return refuse(PreviewRefusal::AlreadyAsked, "you have already been asked");
    }
    Ok(Basis {
        state: state.clone(),
        galaxy: shared.preview_galaxy.clone(),
        version: shared.game_version,
        decisions: log.len(),
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
    let basis = {
        let lock = shared.lock().expect("shared lock");
        basis(&lock, seat, &card, &primary).map_err(|refusal| (refusal, lock.game_version))
    };
    let (as_of_version, as_of_decisions, outcome) = match basis {
        Err(((reason, detail), version)) => (version, 0, PreviewResult::Refused { reason, detail }),
        Ok(basis) => {
            let preview: SecondaryPreview = preview_secondary(
                &basis.state,
                ContentStore::embedded(),
                POK,
                basis.galaxy.as_deref(),
                &card,
                &primary,
                seat,
                &request.answers,
            );
            (
                basis.version,
                basis.decisions,
                PreviewResult::Preview { preview },
            )
        }
    };
    SecondaryPreviewMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        request_id: request.request_id,
        as_of_version,
        as_of_decisions,
        card: request.card.clone(),
        outcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::worker::{PendingDecision, PendingSubmissionState};
    use ti4_engine::choice::Choice;

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
