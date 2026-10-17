//! What a seat has staged and not sent: the staged movement, landings and production, the
//! private draft of a tactical action, and the draft of a secondary. Each host keeps it for the
//! seat: the server for every tab and device of the seat (`ti4_server::session::staged`: the
//! shared book, its storage and the messages to the connections), the browser host in its
//! session and its save. The values are the client's; a host stores and forwards them. It reads
//! one of them: a draft of a secondary that is ready is sent by the host when the game asks the
//! seat ([`SecondaryDraft`], [`ready`]). The wire types are in [`crate::protocol::staged`].
//!
//! A slot is written by compare-and-set on its revision ([`SeatStaged::put`]). Revisions count
//! up for the whole seat, so the newest message of a seat is the one with the highest
//! `revision`.

pub mod ready;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use ti4_model::id::{PlayerId, StrategyCardId};

use crate::planning::draft::DraftStep;
use crate::protocol::staged::{SeatStagedMsg, StagedSlot, StagedValue};

/// Every slot of one seat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatStaged {
    /// The revision of the newest write of any slot (0: never written).
    pub revision: u64,
    #[serde(default)]
    pub slots: BTreeMap<StagedSlot, StagedValue>,
}

impl SeatStaged {
    /// The message that carries these slots to the connections of the seat.
    #[must_use]
    pub fn message(self, game_id: &str) -> SeatStagedMsg {
        SeatStagedMsg::new(game_id, self.revision, self.slots)
    }

    /// The revision of `slot`: 0 for a slot that was never written.
    #[must_use]
    pub fn slot_revision(&self, slot: StagedSlot) -> u64 {
        self.slots.get(&slot).map_or(0, |staged| staged.revision)
    }

    /// Write `value` into `slot` when `base_revision` is the revision of the slot. Returns
    /// whether it was written. A put of the value that the slot holds is taken and changes
    /// nothing.
    pub fn put(
        &mut self,
        slot: StagedSlot,
        base_revision: u64,
        value: Option<serde_json::Value>,
    ) -> Put {
        if base_revision != self.slot_revision(slot) {
            return Put::Stale;
        }
        let current = self
            .slots
            .get(&slot)
            .and_then(|staged| staged.value.as_ref());
        if current == value.as_ref() {
            return Put::Unchanged;
        }
        self.revision += 1;
        self.slots.insert(
            slot,
            StagedValue {
                revision: self.revision,
                value,
            },
        );
        Put::Written
    }
}

/// What became of a [`SeatStaged::put`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Put {
    Written,
    /// The slot holds that value already.
    Unchanged,
    /// The base revision is not the slot's: nothing was written.
    Stale,
}

/// The draft of a secondary as web2 keeps it in the slot `secondary` (`SecondaryDraft` of
/// `web2/src/session/secondaryDraft.ts`). Only what a host needs to send it is read; the label
/// of an answer and every other field are the page's.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SecondaryDraft {
    /// The strategic action that the draft is of: its round, who played the card, and the card.
    pub round: u32,
    pub primary: PlayerId,
    pub card: StrategyCardId,
    /// The answers in their order.
    pub answers: Vec<DraftAnswer>,
    /// The player marked the draft ready: it is sent when the game asks the seat.
    pub ready: bool,
}

/// One answer of a [`SecondaryDraft`]: the id of the option.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DraftAnswer {
    pub id: String,
}

impl SecondaryDraft {
    /// The draft in a slot, when the value is one.
    #[must_use]
    pub fn of(value: &serde_json::Value) -> Option<Self> {
        serde_json::from_value(value.clone()).ok()
    }

    /// What the game is asked to play: the answers by their ids.
    #[must_use]
    pub fn script(&self) -> Vec<DraftStep> {
        self.answers
            .iter()
            .map(|answer| DraftStep::choose(&answer.id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_put_is_a_compare_and_set_on_the_revision_of_its_slot() {
        let mut seat = SeatStaged::default();
        let value = |n: u64| Some(serde_json::json!({ "n": n }));
        assert_eq!(seat.put(StagedSlot::Plan, 0, value(1)), Put::Written);
        assert_eq!(seat.slot_revision(StagedSlot::Plan), 1);
        // Another slot has its own base, and the revisions count for the seat.
        assert_eq!(seat.put(StagedSlot::Movement, 0, value(2)), Put::Written);
        assert_eq!(seat.slot_revision(StagedSlot::Movement), 2);
        assert_eq!(seat.put(StagedSlot::Plan, 0, value(3)), Put::Stale);
        assert_eq!(seat.put(StagedSlot::Plan, 1, value(1)), Put::Unchanged);
        assert_eq!(seat.revision, 2);
        // A clear keeps the revision, so a late put on the old value is still stale.
        assert_eq!(seat.put(StagedSlot::Plan, 1, None), Put::Written);
        assert_eq!(seat.slot_revision(StagedSlot::Plan), 3);
        assert_eq!(seat.put(StagedSlot::Plan, 1, value(4)), Put::Stale);
        let text = serde_json::to_string(&seat).unwrap();
        assert!(text.contains("\"plan\""), "{text}");
        assert_eq!(serde_json::from_str::<SeatStaged>(&text).unwrap(), seat);
    }

    #[test]
    fn a_draft_of_a_secondary_is_read_as_web2_writes_it() {
        // `SecondaryDraft` of `web2/src/session/secondaryDraft.ts`, with the keys sorted as a
        // host keeps them.
        let value = serde_json::json!({
            "answers": [{ "id": "follow", "label": "Follow" }, { "id": "tech:gravity_drive", "label": "Gravity Drive" }],
            "card": "leadership",
            "primary": "b",
            "ready": true,
            "round": 2,
        });
        let draft = SecondaryDraft::of(&value).expect("a draft");
        assert_eq!(draft.round, 2);
        assert_eq!(draft.primary, PlayerId::new("b"));
        assert_eq!(draft.card.as_str(), "leadership");
        assert!(draft.ready);
        assert_eq!(
            draft.script(),
            vec![
                DraftStep::choose("follow"),
                DraftStep::choose("tech:gravity_drive"),
            ]
        );
        // The movement of a draft is no draft of a secondary.
        assert_eq!(
            SecondaryDraft::of(&serde_json::json!({ "nonce": "x" })),
            None
        );
    }
}
