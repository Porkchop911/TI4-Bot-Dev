//! Typed, deterministic timing events.
//!
//! This is the data boundary for the later timing resolver: an event has one per-trace numeric
//! identifier, a type, a mutable JSON-compatible payload, and a cancellation bit. It does not
//! decide *when* an event resolves; that belongs to M03-010.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

/// An event produced while resolving a game action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Event {
    /// One-based, per-trace occurrence identifier.
    pub id: u64,
    /// Stable event name, for example `"COMBAT_DICE_ROLLED"`.
    pub event_type: String,
    /// Facts the event carries. `BTreeMap` makes wire order independent of insertion order.
    pub payload: BTreeMap<String, Value>,
    /// A cancelled event does not resolve and has no AFTER window.
    pub cancelled: bool,
}

impl Event {
    /// Construct an uncancelled event with an explicit trace-local identifier.
    #[must_use]
    pub fn new(id: u64, event_type: impl Into<String>, payload: BTreeMap<String, Value>) -> Self {
        Self {
            id,
            event_type: event_type.into(),
            payload,
            cancelled: false,
        }
    }

    /// Mark the event cancelled. This is idempotent so competing paths do not race.
    pub fn cancel(&mut self) {
        self.cancelled = true;
    }

    /// Read a string field, returning `None` for a missing or non-string field.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.payload.get(key).and_then(Value::as_str)
    }

    /// Read an integer field, returning `None` for a missing or non-integer field.
    #[must_use]
    pub fn integer(&self, key: &str) -> Option<i64> {
        self.payload.get(key).and_then(Value::as_i64)
    }

    /// Read a boolean field, returning `None` for a missing or non-boolean field.
    #[must_use]
    pub fn boolean(&self, key: &str) -> Option<bool> {
        self.payload.get(key).and_then(Value::as_bool)
    }

    /// Deserialize a payload field into its domain type.
    ///
    /// # Errors
    /// Returns [`EventPayloadError::Missing`] for an absent field and
    /// [`EventPayloadError::Invalid`] for a field with the wrong JSON shape.
    pub fn decode<T: DeserializeOwned>(&self, key: &str) -> Result<T, EventPayloadError> {
        let value = self
            .payload
            .get(key)
            .cloned()
            .ok_or_else(|| EventPayloadError::Missing(key.to_owned()))?;
        serde_json::from_value(value).map_err(|error| EventPayloadError::Invalid {
            key: key.to_owned(),
            message: error.to_string(),
        })
    }
}

/// A deterministic per-trace event-ID allocator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventSequence {
    next_id: u64,
}

impl Default for EventSequence {
    fn default() -> Self {
        Self::new()
    }
}

impl EventSequence {
    /// Start a trace at the oracle's first event ID, one.
    #[must_use]
    pub const fn new() -> Self {
        Self { next_id: 1 }
    }

    /// Allocate the next event without relying on process-global state.
    ///
    /// # Errors
    /// Returns [`EventSequenceError::Exhausted`] rather than wrapping and duplicating an ID.
    pub fn next(
        &mut self,
        event_type: impl Into<String>,
        payload: BTreeMap<String, Value>,
    ) -> Result<Event, EventSequenceError> {
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(EventSequenceError::Exhausted)?;
        Ok(Event::new(id, event_type, payload))
    }
}

/// Failure to decode a typed event payload field.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EventPayloadError {
    /// The event did not contain the requested field.
    #[error("event payload has no {0:?} field")]
    Missing(String),
    /// The field was present but did not match the requested type.
    #[error("event payload field {key:?} is invalid: {message}")]
    Invalid {
        /// Field name.
        key: String,
        /// Serde's validation diagnostic.
        message: String,
    },
}

/// Event IDs cannot wrap: duplicate IDs make replay traces ambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EventSequenceError {
    /// The u64 identifier space has been exhausted.
    #[error("event ID space is exhausted")]
    Exhausted,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::id::PlayerId;

    fn payload(
        entries: impl IntoIterator<Item = (&'static str, Value)>,
    ) -> BTreeMap<String, Value> {
        entries
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value))
            .collect()
    }

    #[test]
    fn event_ids_are_one_based_and_trace_local() {
        let mut first = EventSequence::new();
        let mut second = EventSequence::new();
        assert_eq!(first.next("A", BTreeMap::new()).unwrap().id, 1);
        assert_eq!(first.next("B", BTreeMap::new()).unwrap().id, 2);
        assert_eq!(second.next("A", BTreeMap::new()).unwrap().id, 1);
    }

    #[test]
    fn lightweight_accessors_reject_missing_and_wrong_types() {
        let event = Event::new(
            1,
            "COMBAT_ROLL",
            payload([
                ("player", Value::String("sol".to_owned())),
                ("hits", Value::from(2)),
                ("cancelled_by_rule", Value::Bool(false)),
            ]),
        );
        assert_eq!(event.text("player"), Some("sol"));
        assert_eq!(event.integer("hits"), Some(2));
        assert_eq!(event.boolean("cancelled_by_rule"), Some(false));
        assert_eq!(event.integer("player"), None);
        assert_eq!(event.text("absent"), None);
    }

    #[test]
    fn structured_reads_distinguish_missing_from_invalid() {
        let event = Event::new(
            1,
            "E",
            payload([("player", Value::String("sol".to_owned()))]),
        );
        assert_eq!(
            event.decode::<PlayerId>("player").unwrap(),
            PlayerId::new("sol")
        );
        assert_eq!(
            event.decode::<u64>("absent"),
            Err(EventPayloadError::Missing("absent".to_owned()))
        );
        assert!(
            matches!(event.decode::<u64>("player"), Err(EventPayloadError::Invalid { key, .. }) if key == "player")
        );
    }

    #[test]
    fn cancellation_is_idempotent_and_part_of_the_wire_value() {
        let mut event = Event::new(4, "ACTION_CARD", BTreeMap::new());
        event.cancel();
        event.cancel();
        assert!(event.cancelled);
        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(serde_json::from_str::<Event>(&json).unwrap(), event);
    }

    #[test]
    fn payload_serialisation_is_independent_of_insertion_order() {
        let a = Event::new(
            1,
            "E",
            payload([("b", Value::from(2)), ("a", Value::from(1))]),
        );
        let b = Event::new(
            1,
            "E",
            payload([("a", Value::from(1)), ("b", Value::from(2))]),
        );
        assert_eq!(
            serde_json::to_vec(&a).unwrap(),
            serde_json::to_vec(&b).unwrap()
        );
    }
}
