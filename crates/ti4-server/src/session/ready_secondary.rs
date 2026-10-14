//! A draft of a secondary that its seat marked ready is sent by the server, when the game asks
//! the seat that secondary. Every tab and device of the seat has the draft (`super::staged`), so
//! the server sends it once, also when no tab of the seat is open.
//!
//! This is the check that a page made before: the draft is played on a copy of the game
//! ([`super::run_draft_on`], as `run_draft` with the scope of the secondary), and it is sent
//! only when the game plays every answer and the secondary is over with it, or ends where the
//! game shows something hidden (`isOver` of `web2/src/session/secondaryDraft.ts`). A draft that
//! no longer fits is not ready any more: the server writes it back with `ready: false`, and
//! every tab says so. A draft that is sent is cleared.
//!
//! There are two moments: the game asks the seat ([`on_reach`], from the decider of the seat,
//! before the seat is told the question), and the seat marks its draft ready while the game asks
//! it already (`GameSession::send_ready_secondary`, after the write).

use std::sync::Mutex;

use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_model::id::PlayerId;
use ti4_view::planning::audit::PlanScope;
use ti4_view::planning::draft::{DraftStep, DraftStop, ScriptAnswer, script_answer};

use super::staged::{SecondaryDraft, StagedBook, StagedSlot, put_and_tell};
use super::worker::SessionShared;

/// The ready draft of a seat, with the revision of its slot and the value as it is kept.
pub(crate) struct Ready {
    pub revision: u64,
    pub draft: SecondaryDraft,
    pub value: serde_json::Value,
}

/// The game asks `seat` its secondary of a strategic action with `choice`: the primary and the
/// card, as the question names them.
fn asked<'a>(choice: &'a Choice, seat: &PlayerId) -> Option<(&'a str, &'a str)> {
    let detail = |key: &str| choice.details.get(key).and_then(serde_json::Value::as_str);
    (&choice.player == seat && detail("kind") == Some("strategy_secondary"))
        .then(|| detail("played_by").zip(detail("card")))
        .flatten()
}

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

/// The draft of `seat` that is ready for the secondary that `choice` asks in `round`.
pub(crate) fn ready_draft(
    book: &StagedBook,
    seat: &PlayerId,
    choice: &Choice,
    round: u32,
) -> Option<Ready> {
    let (primary, card) = asked(choice, seat)?;
    let seats = book.lock();
    let slot = seats.get(seat)?.slots.get(&StagedSlot::Secondary)?;
    let value = slot.value.clone()?;
    let draft = SecondaryDraft::of(&value)?;
    (draft.ready
        && !draft.answers.is_empty()
        && draft.round == round
        && draft.primary.as_str() == primary
        && draft.card.as_str() == card)
        .then_some(Ready {
            revision: slot.revision,
            draft,
            value,
        })
}

/// What the game makes of a ready draft.
pub(crate) enum Check {
    /// The game plays it whole: send this script.
    Fits(Vec<DraftStep>),
    /// The game no longer plays it whole.
    Unfit,
    /// The game cannot be copied now: nothing is decided.
    Unknown,
}

/// Plays the draft on a copy of the game of `shared`.
pub(crate) fn check(
    shared: &Mutex<SessionShared>,
    seat: &PlayerId,
    draft: &SecondaryDraft,
) -> Check {
    let scope = PlanScope::Secondary {
        primary: draft.primary.clone(),
        card: draft.card.clone(),
    };
    let script = draft.script();
    match super::run_draft_on(shared, seat, &scope, &script).1 {
        None => Check::Unknown,
        Some(outcome)
            if outcome.consumed == script.len()
                && matches!(
                    outcome.stop,
                    DraftStop::Complete | DraftStop::Stopped { .. }
                ) =>
        {
            Check::Fits(script)
        }
        Some(_) => Check::Unfit,
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
    let mut value = ready.value;
    value["ready"] = serde_json::Value::Bool(false);
    let _ = put_and_tell(
        shared,
        book,
        seat,
        StagedSlot::Secondary,
        ready.revision,
        Some(value),
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
    let ready = ready_draft(&book, seat, choice, round)?;
    let script = match check(shared, seat, &ready.draft) {
        Check::Fits(script) => script,
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
