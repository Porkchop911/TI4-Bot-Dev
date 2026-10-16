//! A draft of a secondary that its seat marked ready is sent by the host when the game asks the
//! seat that secondary, so the seat is not asked. The server sends it once for every tab and
//! device of the seat, also when no tab of the seat is open (`ti4_server::session::ready_secondary`);
//! the browser host sends it for the seats of its page (`ti4_wasm::host_session`).
//!
//! The draft is played on a copy of the game (`run_draft` with the scope of the secondary), and
//! it is sent only when the game plays every answer and the secondary is over with it, or ends
//! where the game shows something hidden ([`judge`]; `isOver` of
//! `web2/src/session/secondaryDraft.ts`). A draft that no longer fits is not ready any more: the
//! host writes it back with `ready: false` ([`unready_value`]), and the page says so. A draft
//! that is sent is cleared.
//!
//! There are two moments: the game asks the seat, and the seat marks its draft ready while the
//! game asks it already.

use std::collections::BTreeMap;

use ti4_engine::choice::Choice;
use ti4_model::id::PlayerId;

use super::{SeatStaged, SecondaryDraft};
use crate::planning::audit::PlanScope;
use crate::planning::draft::{DraftOutcome, DraftStop};
use crate::protocol::staged::StagedSlot;

/// The game asks `seat` its secondary of a strategic action with `choice`: the primary and the
/// card, as the question names them.
#[must_use]
pub fn asked<'a>(choice: &'a Choice, seat: &PlayerId) -> Option<(&'a str, &'a str)> {
    let detail = |key: &str| choice.details.get(key).and_then(serde_json::Value::as_str);
    (&choice.player == seat && detail("kind") == Some("strategy_secondary"))
        .then(|| detail("played_by").zip(detail("card")))
        .flatten()
}

/// The ready draft of a seat, with the revision of its slot and the value as it is kept.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub revision: u64,
    pub draft: SecondaryDraft,
    pub value: serde_json::Value,
}

impl Ready {
    /// The scope that the draft is played in: the secondary of its strategic action.
    #[must_use]
    pub fn scope(&self) -> PlanScope {
        PlanScope::Secondary {
            primary: self.draft.primary.clone(),
            card: self.draft.card.clone(),
        }
    }
}

/// The draft of `seat` that is ready for the secondary that `choice` asks in `round`.
#[must_use]
pub fn ready_draft(
    seats: &BTreeMap<PlayerId, SeatStaged>,
    seat: &PlayerId,
    choice: &Choice,
    round: u32,
) -> Option<Ready> {
    let (primary, card) = asked(choice, seat)?;
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Check {
    /// The game plays it whole: send it.
    Fits,
    /// The game no longer plays it whole.
    Unfit,
    /// The game cannot be copied now: nothing is decided.
    Unknown,
}

/// What the run of a draft of `len` answers on a copy of the game says: it fits when the game
/// took every answer and the secondary is over with them, or stops where something hidden is
/// shown.
#[must_use]
pub fn judge(outcome: &DraftOutcome, len: usize) -> Check {
    if outcome.consumed == len
        && matches!(
            outcome.stop,
            DraftStop::Complete | DraftStop::Stopped { .. }
        )
    {
        Check::Fits
    } else {
        Check::Unfit
    }
}

/// The value of a draft that is not ready any more: written back by the host, so the page is
/// told.
#[must_use]
pub fn unready_value(mut value: serde_json::Value) -> serde_json::Value {
    value["ready"] = serde_json::Value::Bool(false);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::staged::StagedValue;

    fn choice(seat: &str, kind: &str) -> Choice {
        serde_json::from_value(serde_json::json!({
            "player": seat,
            "prompt": "Follow?",
            "options": [],
            "details": { "kind": kind, "played_by": "b", "card": "leadership" },
        }))
        .expect("a choice")
    }

    #[test]
    fn a_ready_draft_is_of_the_secondary_that_is_asked() {
        let draft = |ready: bool, round: u32| {
            serde_json::json!({
                "answers": [{ "id": "follow", "label": "Follow" }],
                "card": "leadership",
                "primary": "b",
                "ready": ready,
                "round": round,
            })
        };
        let seats = |value: serde_json::Value| {
            BTreeMap::from([(
                PlayerId::new("a"),
                SeatStaged {
                    revision: 4,
                    slots: BTreeMap::from([(
                        StagedSlot::Secondary,
                        StagedValue {
                            revision: 4,
                            value: Some(value),
                        },
                    )]),
                },
            )])
        };
        let a = PlayerId::new("a");
        let asked = choice("a", "strategy_secondary");
        let ready = ready_draft(&seats(draft(true, 2)), &a, &asked, 2).expect("ready");
        assert_eq!(ready.revision, 4);
        assert_eq!(
            ready.scope(),
            PlanScope::Secondary {
                primary: PlayerId::new("b"),
                card: ready.draft.card.clone(),
            }
        );
        // Not ready, another round, another seat, another question: nothing to send.
        assert_eq!(ready_draft(&seats(draft(false, 2)), &a, &asked, 2), None);
        assert_eq!(ready_draft(&seats(draft(true, 1)), &a, &asked, 2), None);
        assert_eq!(
            ready_draft(&seats(draft(true, 2)), &PlayerId::new("c"), &asked, 2),
            None
        );
        assert_eq!(
            ready_draft(&seats(draft(true, 2)), &a, &choice("a", "agenda_vote"), 2),
            None
        );
        assert_eq!(
            unready_value(draft(true, 2))["ready"],
            serde_json::Value::Bool(false)
        );
    }
}
