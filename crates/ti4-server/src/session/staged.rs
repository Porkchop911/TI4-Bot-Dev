//! The staged work of the seats of one game on the server: the book that the sessions of the
//! game share, its persistence, and the messages to the connections of a seat. The slots of a
//! seat, the compare-and-set of a write and the draft of a secondary are `ti4_host::staged`; the
//! server reads a draft of a secondary that is ready to send it when the game asks the seat
//! (`super::ready_secondary`).

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ti4_host::staged::{Put, SeatStaged};
use ti4_model::id::PlayerId;

use ti4_host::protocol::staged::StagedSlot;

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
    let message = ti4_host::protocol::server::ServerMessage::SeatStaged(message);
    // Sent under the lock of the book, so the connections get the writes in their order.
    let viewer = ti4_host::protocol::status::ViewerRole::Player(seat.clone());
    shared
        .lock()
        .expect("shared lock")
        .subscribers
        .retain(|_, subscriber| {
            subscriber.viewer != viewer || subscriber.tx.try_send(message.clone()).is_ok()
        });
    Ok(put)
}
