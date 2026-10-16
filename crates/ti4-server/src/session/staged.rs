//! What a seat has staged and not sent: the staged movement, landings and production, the
//! private draft of a tactical action, and the draft of a secondary. The server keeps it for the
//! seat, so every tab and every device of the seat has the same staged work. The values are the
//! client's; the server stores and forwards them. It reads one of them: a draft of a secondary
//! that is ready is sent by the server when the game asks the seat ([`SecondaryDraft`],
//! `super::ready_secondary`). The wire types are in `protocol::staged` (`ti4-host`); this is the
//! state and its persistence.
//!
//! A slot is written by compare-and-set on its revision ([`SeatStaged::put`]). Revisions count
//! up for the whole seat, so the newest message of a seat is the one with the highest
//! `revision`.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use ti4_model::id::{PlayerId, StrategyCardId};

use crate::protocol::staged::SeatStagedMsg;
pub use crate::protocol::staged::{StagedSlot, StagedValue};

/// Every slot of one seat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

/// The draft of a secondary as web2 keeps it in the slot `secondary` (`SecondaryDraft` of
/// `web2/src/session/secondaryDraft.ts`). Only what the server needs to send it is read; the
/// label of an answer and every other field are the page's.
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
    pub fn script(&self) -> Vec<ti4_view::planning::draft::DraftStep> {
        self.answers
            .iter()
            .map(|answer| ti4_view::planning::draft::DraftStep::choose(&answer.id))
            .collect()
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

/// The staged work of every seat of one game. A clone shares it: the sessions that replace each
/// other (undo, redo, a batch) have one book, so a write that reaches the old session while the
/// new one starts is not lost.
#[derive(Debug, Clone, Default)]
pub struct StagedBook(Arc<Mutex<BTreeMap<PlayerId, SeatStaged>>>);

impl StagedBook {
    #[must_use]
    pub fn new(seats: BTreeMap<PlayerId, SeatStaged>) -> Self {
        Self(Arc::new(Mutex::new(seats)))
    }

    /// The lock of the book. Taken before the lock of a session, never after it.
    pub fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<PlayerId, SeatStaged>> {
        self.0.lock().expect("staged lock")
    }
}

/// Write one slot of what `seat` has staged ([`SeatStaged::put`]), save it, and send it to every
/// connection of the seat with `writer` as the writer: a connection, or none for a write of the
/// server. A stale or unchanged put writes and sends nothing.
///
/// # Errors
/// A message for the client when the write could not be saved (it is then not applied).
pub(crate) fn put_and_tell(
    shared: &Mutex<super::worker::SessionShared>,
    book: &StagedBook,
    seat: &PlayerId,
    slot: StagedSlot,
    base_revision: u64,
    value: Option<serde_json::Value>,
    writer: Option<u64>,
) -> Result<Put, String> {
    let mut seats = book.lock();
    let before = seats.get(seat).cloned();
    let entry = seats.entry(seat.clone()).or_default();
    let put = entry.put(slot, base_revision, value);
    let current = entry.clone();
    if put != Put::Written {
        if before.is_none() {
            seats.remove(seat);
        }
        return Ok(put);
    }
    let (game_id, store) = {
        let lock = shared.lock().expect("shared lock");
        (lock.game_id.clone(), lock.store.clone())
    };
    if let Some(store) = store {
        let record = crate::storage::StagedRecord {
            seats: seats.clone(),
        };
        if let Err(error) = store.save_staged(&game_id, &record) {
            match before {
                Some(before) => seats.insert(seat.clone(), before),
                None => seats.remove(seat),
            };
            return Err(format!("could not save the staged work: {error}"));
        }
    }
    let mut message = current.message(&game_id);
    message.writer = writer;
    let message = crate::protocol::server::ServerMessage::SeatStaged(message);
    // Sent under the lock of the book, so the connections get the writes in their order.
    let viewer = crate::protocol::status::ViewerRole::Player(seat.clone());
    shared
        .lock()
        .expect("shared lock")
        .subscribers
        .retain(|_, subscriber| {
            subscriber.viewer != viewer || subscriber.tx.try_send(message.clone()).is_ok()
        });
    Ok(put)
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
        // `SecondaryDraft` of `web2/src/session/secondaryDraft.ts`, with the keys sorted as the
        // server keeps them.
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
                ti4_view::planning::draft::DraftStep::choose("follow"),
                ti4_view::planning::draft::DraftStep::choose("tech:gravity_drive"),
            ]
        );
        // The movement of a draft is no draft of a secondary.
        assert_eq!(
            SecondaryDraft::of(&serde_json::json!({ "nonce": "x" })),
            None
        );
    }
}
