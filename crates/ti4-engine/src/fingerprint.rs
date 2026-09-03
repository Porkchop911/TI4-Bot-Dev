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

    /// Adds the typed decision context to the decision envelope (OBS-003b).
    ///
    /// V1 remains readable and byte-identical: it hashes the record with any context stripped, so
    /// an old replay keeps its digest and a new record can still be fingerprinted the old way for
    /// comparison against one.
    pub const V2: Self = Self(2);

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
///
/// V1 hashes the record with any typed context stripped. That is the whole backward contract: a
/// replay written before contexts existed keeps its digest, and a record that now carries one can
/// still be fingerprinted the old way to compare against an old trace. V2 binds the context.
#[must_use]
pub fn decision_hash(version: CanonicalHashVersion, decision: &DecisionRecord) -> CanonicalHash {
    if version == CanonicalHashVersion::V1 {
        return canonical_hash(version, &decision.without_context());
    }
    canonical_hash(version, decision)
}

/// The context fields bound into a V2 decision fingerprint, pinned rather than implied.
///
/// Every field participates, including `outstanding`. That is deliberate: the continuation state is
/// what distinguishes "pay three influence" asked with one influence of credit from the same
/// sentence asked with none, and a replay that could not tell those apart would not be a replay.
/// `actor` participates too even though `DecisionRecord::player` already carries it, because the
/// fingerprint should be checkable from the context alone rather than by knowing they agree.
pub const V2_CONTEXT_FIELDS: [&str; 8] = [
    "version", "actor", "source", "subtype", "phase", "round", "optional", "target",
];

/// The one context field carrying values rather than identity, bound alongside the fields above.
pub const V2_CONTEXT_QUANTITIES: &str = "outstanding";

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
            context: None,
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

    /// An old replay, as JSON written before contexts existed.
    const OLD_REPLAY_FIXTURE: &str = r#"{
        "player": "sol",
        "prompt": "when MOVE",
        "chosen": "move:0",
        "offered": ["move:0", "decline"]
    }"#;

    fn a_context() -> crate::decision_context::DecisionContext {
        crate::decision_context::DecisionContext::new(
            PlayerId::new("sol"),
            crate::decision_context::DecisionSource::Rule("52.3".to_owned()),
            "buy_command_token",
            ti4_model::state::Phase::Action,
            3,
        )
    }

    #[test]
    fn an_old_replay_reads_back_with_no_context_and_keeps_its_digest() {
        // The backward contract, in the form it actually has to survive: JSON on disk that predates
        // the field. It must deserialise, it must default to no context, and it must still hash to
        // the value an old trace recorded.
        let record: DecisionRecord =
            serde_json::from_str(OLD_REPLAY_FIXTURE).expect("an old record still reads");
        assert!(
            record.context.is_none(),
            "absent means absent, not defaulted to something"
        );
        assert_eq!(
            decision_hash(CanonicalHashVersion::V1, &record).digest,
            "7214f5740b14f02bc279b96c6c2209b5905a199079725052f574f8073789f140"
        );
    }

    #[test]
    fn v1_ignores_a_context_and_v2_binds_it() {
        let plain = DecisionRecord {
            player: PlayerId::new("sol"),
            prompt: "when MOVE".to_owned(),
            chosen: "move:0".to_owned(),
            offered: vec!["move:0".to_owned(), "decline".to_owned()],
            context: None,
        };
        let carried = DecisionRecord {
            context: Some(a_context()),
            ..plain.clone()
        };

        // V1 is the old contract and must not notice the new field, or every stored digest moves.
        assert_eq!(
            decision_hash(CanonicalHashVersion::V1, &plain),
            decision_hash(CanonicalHashVersion::V1, &carried),
            "V1 hashes the record with the context stripped"
        );
        // V2 exists precisely to notice it.
        assert_ne!(
            decision_hash(CanonicalHashVersion::V2, &plain),
            decision_hash(CanonicalHashVersion::V2, &carried),
            "V2 binds the context"
        );
        assert_ne!(
            decision_hash(CanonicalHashVersion::V1, &carried),
            decision_hash(CanonicalHashVersion::V2, &carried),
            "the envelope version is part of the input"
        );
    }

    #[test]
    fn v2_separates_decisions_that_differ_only_in_context() {
        // The case the whole package is for: the same sentence, the same options, asked with
        // different continuation state. A replay that could not tell these apart is not a replay.
        let base = DecisionRecord {
            player: PlayerId::new("sol"),
            prompt: "spend 3 more influence for a command token".to_owned(),
            chosen: "strategic_tokens".to_owned(),
            offered: vec!["strategic_tokens".to_owned(), "decline".to_owned()],
            context: Some(a_context()),
        };
        let with_credit = DecisionRecord {
            context: Some(
                a_context().owing(crate::decision_context::OutstandingConstraint::new(
                    crate::decision_context::ConstraintKind::Influence,
                    3,
                    1,
                )),
            ),
            ..base.clone()
        };
        let different_subtype = DecisionRecord {
            context: Some(crate::decision_context::DecisionContext::new(
                PlayerId::new("sol"),
                crate::decision_context::DecisionSource::Rule("52.3".to_owned()),
                "redistribute_command_tokens",
                ti4_model::state::Phase::Action,
                3,
            )),
            ..base.clone()
        };

        let v2 = |record: &DecisionRecord| decision_hash(CanonicalHashVersion::V2, record).digest;
        assert_ne!(
            v2(&base),
            v2(&with_credit),
            "outstanding credit participates"
        );
        assert_ne!(v2(&base), v2(&different_subtype), "subtype participates");
        // And V1 still cannot tell any of them apart, which is why V2 was needed.
        let v1 = |record: &DecisionRecord| decision_hash(CanonicalHashVersion::V1, record).digest;
        assert_eq!(v1(&base), v1(&with_credit));
        assert_eq!(v1(&base), v1(&different_subtype));
    }

    #[test]
    fn the_participating_context_fields_are_pinned() {
        // Pinned as data so that adding a context field is a deliberate fingerprint decision rather
        // than an accident of `derive(Serialize)`.
        assert_eq!(V2_CONTEXT_FIELDS.len(), 8);
        assert_eq!(V2_CONTEXT_QUANTITIES, "outstanding");
        let named: std::collections::BTreeSet<&str> = V2_CONTEXT_FIELDS
            .iter()
            .copied()
            .chain(std::iter::once(V2_CONTEXT_QUANTITIES))
            .collect();
        let declared: std::collections::BTreeSet<&str> =
            crate::decision_context::DecisionContext::visibility()
                .keys()
                .copied()
                .collect();
        assert_eq!(
            named, declared,
            "every context field is accounted for in the fingerprint contract, or the contract is stale"
        );
    }
}
