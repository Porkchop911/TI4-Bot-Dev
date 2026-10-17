//! The server's half of the ready draft of a secondary (`ti4_host::staged::ready`): when the
//! game asks a seat that secondary, the server sends the seat's ready draft. Every tab and device
//! of the seat has the draft (`super::staged`), so the server sends it once, also when no tab of
//! the seat is open. The draft is played on a copy of the game ([`super::run_draft_on`]); a draft
//! that no longer fits is written back not ready, and every tab says so. A draft that is sent is
//! cleared.
//!
//! There are two moments: the game asks the seat ([`on_reach`], from the decider of the seat,
//! before the seat is told the question), and the seat marks its draft ready while the game asks
//! it already (`GameSession::send_ready_secondary`, after the write). What stays here is what is
//! the server's: the gate of a live session ([`may_send`]), the shared book and its messages.

use std::sync::Mutex;

use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_host::planning::draft::{DraftStep, ScriptAnswer, script_answer};
use ti4_host::protocol::staged::StagedSlot;
use ti4_host::staged::ready::{Check, Ready, asked, judge, ready_draft, unready_value};
use ti4_model::id::PlayerId;

use super::staged::{StagedBook, put_and_tell};
use super::worker::SessionShared;

/// The game may send a draft for `choice` now: it asks `seat` a secondary, the game is live, and
/// no change of the history is running.
pub(crate) fn may_send(shared: &SessionShared, seat: &PlayerId, choice: &Choice) -> bool {
    asked(choice, seat).is_some()
        && shared.replay_complete
        && !shared.stopped
        && !shared.finished
        && shared.error.is_none()
        && !shared.history_pause
}

/// Plays the ready draft on a copy of the game of `shared` ([`judge`]).
pub(crate) fn check(shared: &Mutex<SessionShared>, seat: &PlayerId, ready: &Ready) -> Check {
    let script = ready.draft.script();
    match super::run_draft_on(shared, seat, &ready.scope(), &script).1 {
        None => Check::Unknown,
        Some(outcome) => judge(&outcome, script.len()),
    }
}

/// The draft is not ready any more: it is written back with `ready: false`, by the server, so
/// every tab of the seat is told. A draft that changed since is left alone.
pub(crate) fn unready(
    shared: &Mutex<SessionShared>,
    book: &StagedBook,
    seat: &PlayerId,
    ready: Ready,
) {
    let _ = put_and_tell(
        shared,
        book,
        seat,
        StagedSlot::Secondary,
        ready.revision,
        Some(unready_value(ready.value)),
        None,
    );
}

/// The draft is in the game: its slot is cleared, by the server, so every tab of the seat drops
/// it. A draft that changed since is left alone.
pub(crate) fn spent(
    shared: &Mutex<SessionShared>,
    book: &StagedBook,
    seat: &PlayerId,
    revision: u64,
) {
    let _ = put_and_tell(
        shared,
        book,
        seat,
        StagedSlot::Secondary,
        revision,
        None,
        None,
    );
}

/// The first answer of a draft that is sent, and the script as the game goes on with it.
pub(crate) struct Send {
    pub option: ChoiceOption,
    pub script: Vec<DraftStep>,
    pub next: usize,
    /// The revision of the slot of the draft, to clear it once the answer is in the game.
    pub revision: u64,
}

/// The game asks `seat` the secondary with `choice` in `round`: the first answer of its ready
/// draft, when the draft fits. A draft that does not fit is made not ready, and the seat is
/// asked. Called by the decider of the seat with the choice reserved and no lock held.
pub(crate) fn on_reach(
    shared: &Mutex<SessionShared>,
    seat: &PlayerId,
    choice: &Choice,
    round: u32,
) -> Option<Send> {
    let book = shared.lock().expect("shared lock").staged.clone();
    let ready = ready_draft(&book.lock(), seat, choice, round)?;
    let script = match check(shared, seat, &ready) {
        Check::Fits => ready.draft.script(),
        Check::Unfit => {
            unready(shared, &book, seat, ready);
            return None;
        }
        Check::Unknown => return None,
    };
    match script_answer(&script, 0, choice, seat) {
        ScriptAnswer::Answer {
            option,
            next,
            script: reordered,
        } => Some(Send {
            option,
            next,
            script: reordered.unwrap_or(script),
            revision: ready.revision,
        }),
        ScriptAnswer::Done | ScriptAnswer::Interrupted { .. } | ScriptAnswer::Mismatch { .. } => {
            unready(shared, &book, seat, ready);
            None
        }
    }
}
