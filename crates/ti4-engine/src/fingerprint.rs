//! Versioned canonical hashes for events and recorded decisions.
//!
//! These hashes bind replay-visible values to a byte-stable representation. They are not a
//! replacement for validated actions: callers still use the choice boundary for legality.

use std::fmt;

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{choice::DecisionRecord, event::Event};

/// The version of the canonical hash input schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct CanonicalHashVersion(u16);

impl CanonicalHashVersion {
    /// The first version of the event and decision hash envelope.
    pub const V1: Self = Self(1);

    /// Return the encoded schema version.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

/// A SHA-256 digest tagged with the canonical-input version that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalHash {
    /// The schema version encoded in the digest input.
    pub version: CanonicalHashVersion,
    /// Lowercase hexadecimal SHA-256 digest.
    pub digest: String,
}

impl fmt::Display for CanonicalHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.digest)
    }
}

/// Hash a replay-visible event through the supplied canonical schema version.
#[must_use]
pub fn event_hash(version: CanonicalHashVersion, event: &Event) -> CanonicalHash {
    canonical_hash(version, event)
}

/// Hash one append-only replay decision through the supplied canonical schema version.
///
/// The offered-option sequence is deliberately not sorted: option order is part of a legal
/// decision's observable presentation and must remain distinct in a replay trace.
#[must_use]
pub fn decision_hash(version: CanonicalHashVersion, decision: &DecisionRecord) -> CanonicalHash {
    canonical_hash(version, decision)
}

fn canonical_hash<T: Serialize>(version: CanonicalHashVersion, value: &T) -> CanonicalHash {
    let input = HashInput {
        schema_version: version,
        value,
    };
    // `Event` and `DecisionRecord` only contain JSON-compatible values. Serializing this fixed
    // envelope cannot fail; BTreeMap-backed payloads supply canonical object-key order.
    let bytes = serde_json::to_vec(&input).expect("canonical replay values always serialize");
    let digest = Sha256::digest(bytes);
    CanonicalHash {
        version,
        digest: format!("{digest:x}"),
    }
}

#[derive(Serialize)]
struct HashInput<'a, T> {
    schema_version: CanonicalHashVersion,
    value: &'a T,
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use ti4_model::id::PlayerId;

    use super::*;

    #[test]
    fn event_hash_is_a_versioned_golden_value_and_ignores_payload_insertion_order() {
        let first = Event::new(
            7,
            "MOVE",
            [
                ("b".to_owned(), Value::from(2)),
                ("a".to_owned(), Value::from(1)),
            ]
            .into_iter()
            .collect(),
        );
        let second = Event::new(
            7,
            "MOVE",
            [
                ("a".to_owned(), Value::from(1)),
                ("b".to_owned(), Value::from(2)),
            ]
            .into_iter()
            .collect(),
        );

        let hash = event_hash(CanonicalHashVersion::V1, &first);
        assert_eq!(hash, event_hash(CanonicalHashVersion::V1, &second));
        assert_eq!(hash.version, CanonicalHashVersion::V1);
        assert_eq!(
            hash.digest,
            "f6437bfb4b4aa432844beab91244c504ee17b354086f73d918cd3cca4a5c9544"
        );
    }

    #[test]
    fn event_changes_and_schema_version_changes_produce_distinct_hashes() {
        let event = Event::new(1, "MOVE", BTreeMap::new());
        let cancelled = Event {
            cancelled: true,
            ..event.clone()
        };
        assert_ne!(
            event_hash(CanonicalHashVersion::V1, &event),
            event_hash(CanonicalHashVersion::V1, &cancelled)
        );
        assert_ne!(
            event_hash(CanonicalHashVersion::V1, &event),
            event_hash(CanonicalHashVersion(2), &event)
        );
    }

    #[test]
    fn decision_hash_covers_the_ordered_replay_record_and_has_a_golden_value() {
        let record = DecisionRecord {
            player: PlayerId::new("sol"),
            prompt: "when MOVE".to_owned(),
            chosen: "move:0".to_owned(),
            offered: vec!["move:0".to_owned(), "decline".to_owned()],
        };
        let reordered = DecisionRecord {
            offered: vec!["decline".to_owned(), "move:0".to_owned()],
            ..record.clone()
        };

        let hash = decision_hash(CanonicalHashVersion::V1, &record);
        assert_ne!(hash, decision_hash(CanonicalHashVersion::V1, &reordered));
        assert_eq!(
            hash.digest,
            "7214f5740b14f02bc279b96c6c2209b5905a199079725052f574f8073789f140"
        );
    }
}
