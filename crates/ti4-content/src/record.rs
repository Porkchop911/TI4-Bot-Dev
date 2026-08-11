//! A single content record.
//!
//! Records come from `AsyncTI4` verbatim, so this type reads their field names as-is rather
//! than reshaping them — the same decision the oracle's `engine/content.py` documents.
//! Interpretation belongs to the rules code that consumes a category; what lives here is
//! only the typed access needed to read a JSON value without repeating `as_str().unwrap()`
//! at every call site.

use serde_json::{Map, Value};
use ti4_model::content_types::{ContentType, IdentityKey, Source, SourceSet};

use crate::error::ContentError;

/// One record from one content category, with its identity and source tag resolved once.
///
/// The identity and source are extracted at load time rather than looked up per access:
/// source filtering ran 1.27 million times per four-round game in the oracle, which is why
/// `from_sources` is cached there. Here the answer is simply already computed.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    category: ContentType,
    index: usize,
    identity: Option<String>,
    source: Option<Source>,
    fields: Map<String, Value>,
}

impl Record {
    /// Build a record from a raw JSON value, resolving its identity and source tag.
    pub(crate) fn new(
        category: ContentType,
        index: usize,
        value: Value,
    ) -> Result<Self, ContentError> {
        let Value::Object(fields) = value else {
            return Err(ContentError::NotAnObject {
                category,
                index,
                found: type_name(&value),
            });
        };

        let identity = match category.identity_key() {
            IdentityKey::Composite => None,
            key => {
                let field = key.field().expect("non-composite keys name a field");
                match fields.get(field) {
                    Some(Value::String(s)) => Some(s.clone()),
                    Some(_) => {
                        return Err(ContentError::NonStringIdentity {
                            category,
                            index,
                            field,
                        });
                    }
                    None => {
                        return Err(ContentError::MissingIdentity {
                            category,
                            index,
                            field,
                        });
                    }
                }
            }
        };

        // An untagged category has no `source` field at all; a tagged one must parse. An
        // unrecognised tag means the corpus grew a source we do not model, which is a
        // change to make loudly rather than to filter away.
        let source = match fields.get("source") {
            None | Some(Value::Null) => None,
            Some(Value::String(tag)) => {
                Some(
                    tag.parse::<Source>()
                        .map_err(|_| ContentError::UnknownSource {
                            category,
                            index,
                            tag: tag.clone(),
                        })?,
                )
            }
            Some(other) => {
                return Err(ContentError::UnknownSource {
                    category,
                    index,
                    tag: other.to_string(),
                });
            }
        };

        Ok(Self {
            category,
            index,
            identity,
            source,
            fields,
        })
    }

    /// The category this record came from.
    #[must_use]
    pub const fn category(&self) -> ContentType {
        self.category
    }

    /// Position in the category file. File order is load-bearing: decks are built from it
    /// and then shuffled with a seeded RNG, so any reordering changes every seeded game.
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }

    /// The record's identity within its category, or `None` for `franken_errata`, whose
    /// identity is the pair `itemCategory` + `itemId`.
    #[must_use]
    pub fn id(&self) -> Option<&str> {
        self.identity.as_deref()
    }

    /// The expansion this record belongs to, or `None` for the three untagged categories.
    #[must_use]
    pub const fn source(&self) -> Option<Source> {
        self.source
    }

    /// Whether this record is in scope for a source set.
    ///
    /// An untagged record is never in scope for any set, matching the oracle's
    /// `r.get("source") in sources` — the three untagged categories are read unfiltered.
    #[must_use]
    pub fn in_sources(&self, sources: SourceSet) -> bool {
        self.source.is_some_and(|s| sources.contains(s))
    }

    /// The raw JSON value of a field.
    #[must_use]
    pub fn raw(&self, key: &str) -> Option<&Value> {
        self.fields.get(key)
    }

    /// All fields, in sorted key order.
    ///
    /// `serde_json::Map` is a `BTreeMap` unless its `preserve_order` feature is enabled,
    /// which nothing in this workspace does. Iteration is therefore deterministic, which
    /// is what the canonical digest in [`crate::provenance`] relies on.
    #[must_use]
    pub const fn fields(&self) -> &Map<String, Value> {
        &self.fields
    }

    /// A string field, absent if unset or not a string.
    #[must_use]
    pub fn text(&self, key: &str) -> Option<&str> {
        self.fields.get(key)?.as_str()
    }

    /// An integer field. Accepts a JSON number or a numeric string, because the corpus
    /// writes some counts as strings (`productionValue: "5"`).
    ///
    /// A leading `+` is *not* accepted even though Rust would parse it, because in this
    /// corpus it is an operator: `productionValue: "+2"` means "planet resources plus two",
    /// and reading it as the number 2 would silently halve a space dock's output.
    #[must_use]
    pub fn int(&self, key: &str) -> Option<i64> {
        match self.fields.get(key)? {
            Value::Number(n) => n.as_i64(),
            Value::String(s) => {
                let s = s.trim();
                if s.starts_with('+') {
                    return None;
                }
                s.parse().ok()
            }
            _ => None,
        }
    }

    /// A fractional field. Fighters and infantry cost 0.5 each (two per build).
    #[must_use]
    pub fn float(&self, key: &str) -> Option<f64> {
        match self.fields.get(key)? {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => s.trim().parse().ok(),
            _ => None,
        }
    }

    /// A boolean field, defaulting to false when absent — the corpus omits false flags.
    #[must_use]
    pub fn flag(&self, key: &str) -> bool {
        self.fields
            .get(key)
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// A string-array field, empty when absent. Non-string elements are skipped.
    #[must_use]
    pub fn strings(&self, key: &str) -> Vec<&str> {
        self.fields
            .get(key)
            .and_then(Value::as_array)
            .map(|items| items.iter().filter_map(Value::as_str).collect())
            .unwrap_or_default()
    }
}

/// A JSON type name for error messages.
pub(crate) const fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use ti4_model::content_types::{BASE, FULL};

    fn record(value: Value) -> Record {
        Record::new(ContentType::Units, 0, value).unwrap()
    }

    #[test]
    fn identity_and_source_are_resolved_at_construction() {
        let r = record(json!({"id": "carrier1", "source": "base"}));
        assert_eq!(r.id(), Some("carrier1"));
        assert_eq!(r.source(), Some(Source::Base));
        assert!(r.in_sources(BASE));
    }

    #[test]
    fn an_untagged_record_is_in_no_source_set() {
        let r = record(json!({"id": "swatch"}));
        assert_eq!(r.source(), None);
        assert!(!r.in_sources(FULL));
    }

    #[test]
    fn a_missing_identity_field_is_an_error() {
        let err = Record::new(ContentType::Units, 3, json!({"name": "Carrier"})).unwrap_err();
        assert!(matches!(
            err,
            ContentError::MissingIdentity {
                index: 3,
                field: "id",
                ..
            }
        ));
    }

    #[test]
    fn an_unknown_source_tag_is_an_error_not_a_silent_drop() {
        let err =
            Record::new(ContentType::Units, 1, json!({"id": "x", "source": "ds"})).unwrap_err();
        assert!(matches!(err, ContentError::UnknownSource { tag, .. } if tag == "ds"));
    }

    #[test]
    fn franken_errata_has_no_single_field_identity() {
        let r = Record::new(
            ContentType::FrankenErrata,
            0,
            json!({"itemCategory": "units", "itemId": "carrier", "source": "pok"}),
        )
        .unwrap();
        assert_eq!(r.id(), None);
    }

    #[test]
    fn numeric_fields_accept_both_json_numbers_and_numeric_strings() {
        let r = record(json!({"id": "x", "productionValue": "5", "cost": 0.5, "moveValue": 2}));
        assert_eq!(r.int("productionValue"), Some(5));
        assert_eq!(r.int("moveValue"), Some(2));
        assert!((r.float("cost").unwrap() - 0.5).abs() < f64::EPSILON);
        assert_eq!(r.int("missing"), None);
    }

    #[test]
    fn a_plus_prefixed_production_value_is_not_a_plain_integer() {
        // "+2" means "planet resources plus two" and must not read as 2.
        let r = record(json!({"id": "x", "productionValue": "+2"}));
        assert_eq!(r.int("productionValue"), None);
        assert_eq!(r.text("productionValue"), Some("+2"));
    }

    #[test]
    fn absent_flags_are_false_and_absent_lists_are_empty() {
        let r = record(json!({"id": "x"}));
        assert!(!r.flag("isShip"));
        assert!(r.strings("planets").is_empty());
    }
}
