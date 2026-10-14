//! The live hold of a bluff: a short, interruptible wait of the engine thread (see `bluff`).

use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use ti4_engine::timing::Relation;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::projection::waiting_for_reactions_status;
use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::server::{ServerMessage, TurnStatusMsg};
use crate::session::SeatController;
use crate::session::bluff::{ActiveHold, group_for_window};
use crate::session::worker::SessionShared;

/// Reaction window `event_type`/`relation` opened for `seat`, which is not asked anything in it.
///
/// Called on the live engine thread from the table's window observer. Holds the game for a short
/// random time when `seat` declared this kind of window, and otherwise returns at once. Skipped
/// entirely while replaying, for bots, after a stop, and once a budget is used up.
///
/// `publish_settled` publishes decisions already settled in the current engine step (the same
/// thing a real question does before it blocks). The `shared` mutex is never held while waiting.
pub fn maybe_hold(
    shared: &Arc<Mutex<SessionShared>>,
    replaying: bool,
    seat: &PlayerId,
    event_type: &str,
    relation: Relation,
    state: &GameState,
    publish_settled: &mut dyn FnMut(),
) {
    if replaying {
        return;
    }
    let Some(group) = group_for_window(event_type, relation) else {
        return;
    };
    let round = state.round;
    let (wake_rx, duration) = {
        let mut lock = shared.lock().expect("shared lock");
        if lock.stopped
            || lock.finished
            || lock.error.is_some()
            || lock.replaying
            || lock.bluff.active.is_some()
            || !lock.bluff.declared(seat).contains(group)
        {
            return;
        }
        let eligible = matches!(lock.seats.get(seat), Some(SeatController::Human))
            && state
                .player(seat)
                .is_some_and(|p| !p.action_cards.is_empty())
            && lock
                .reaction_modes
                .get(seat)
                .is_none_or(|set| set.lock().expect("never set lock").is_empty());
        if !eligible {
            return;
        }
        let left = lock.bluff.budget_left(seat, round);
        let policy = lock.bluff.policy.clone();
        if left < policy.min_hold {
            return;
        }
        let span = policy.max_hold.saturating_sub(policy.min_hold);
        let span_ms = u64::try_from(span.as_millis()).unwrap_or(0);
        let jitter = if span_ms == 0 {
            0
        } else {
            rand::random::<u64>() % (span_ms + 1)
        };
        let duration = (policy.min_hold + Duration::from_millis(jitter))
            .min(policy.hard_cap)
            .min(left);
        let (tx, rx) = mpsc::channel();
        lock.bluff.active = Some(ActiveHold {
            seat: seat.clone(),
            wake: tx,
        });
        (rx, duration)
    };

    publish_settled();

    let started = Instant::now();
    {
        let mut lock = shared.lock().expect("shared lock");
        if lock.stopped {
            lock.bluff.active = None;
            return;
        }
        // The very status a real question puts up for everybody else, sent to everybody.
        let message = ServerMessage::TurnStatus(TurnStatusMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: lock.game_id.clone(),
            game_version: lock.game_version,
            status: waiting_for_reactions_status(state),
        });
        lock.subscribers
            .retain(|_, subscriber| subscriber.tx.try_send(message.clone()).is_ok());
        lock.send_reaction_intent_state(seat);
    }

    let deadline = started + duration;
    let poll = shared.lock().expect("shared lock").bluff.policy.poll;
    loop {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        match wake_rx.recv_timeout((deadline - now).min(poll)) {
            Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if shared.lock().expect("shared lock").stopped {
                    break;
                }
            }
        }
    }

    let mut lock = shared.lock().expect("shared lock");
    lock.bluff.active = None;
    if lock.stopped {
        return;
    }
    lock.bluff
        .account(seat, round, started.elapsed().min(duration));
    // Silent resume: the ordinary state update restores everyone's status. No event, no log.
    lock.broadcast_state_update();
    lock.send_reaction_intent_state(seat);
}
