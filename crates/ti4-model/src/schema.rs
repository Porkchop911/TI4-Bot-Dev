//! Versioned compatibility metadata for persisted artifacts.

use serde::{Deserialize, Serialize};

/// The only schema-envelope version this build can read.
pub const SCHEMA_ENVELOPE_VERSION: u32 = 1;

/// The declared relationship between an artifact and the pinned oracle contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityMode {
    /// Byte-for-byte compatible at the documented boundary.
    Exact,
    /// Equivalent behavior with a different internal representation.
    Semantic,
    /// Requires a checked translation before use.
    Translated,
}

/// Immutable facts identifying the inputs used to make an artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// Full 40-hex-character commit of the behavioral oracle.
    pub oracle_commit: String,
    /// SHA-256 digest of the content corpus.
    pub content_hash: String,
    /// Version of the random stream contract used to create the artifact.
    pub rng_version: String,
}

/// A compatibility classification and a human-readable bounded explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Compatibility {
    pub mode: CompatibilityMode,
    pub notes: String,
}

/// Common top-level envelope for versioned persisted migration artifacts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SchemaEnvelope {
    pub schema_version: u32,
    pub provenance: Provenance,
    pub compatibility: Compatibility,
}

/// A persisted artifact's envelope cannot be safely consumed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SchemaEnvelopeError {
    #[error("invalid schema envelope JSON: {0}")]
    Json(String),
    #[error("unsupported schema envelope version {found}; expected {SCHEMA_ENVELOPE_VERSION}")]
    UnsupportedVersion { found: u32 },
    #[error("invalid {field}: {value:?}")]
    InvalidField { field: &'static str, value: String },
}

impl SchemaEnvelope {
    /// Decode and validate an envelope before its artifact is consumed.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaEnvelopeError`] for malformed JSON, an unknown schema version, or invalid
    /// provenance metadata.
    pub fn parse(json: &str) -> Result<Self, SchemaEnvelopeError> {
        let envelope = serde_json::from_str::<Self>(json)
            .map_err(|error| SchemaEnvelopeError::Json(error.to_string()))?;
        envelope.validate()?;
        Ok(envelope)
    }

    /// Validate version and fixed-width provenance without accepting future contracts by guesswork.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaEnvelopeError`] for a non-current version or malformed required metadata.
    pub fn validate(&self) -> Result<(), SchemaEnvelopeError> {
        if self.schema_version != SCHEMA_ENVELOPE_VERSION {
            return Err(SchemaEnvelopeError::UnsupportedVersion {
                found: self.schema_version,
            });
        }
        validate_hex("oracle_commit", &self.provenance.oracle_commit, 40)?;
        validate_hex("content_hash", &self.provenance.content_hash, 64)?;
        if self.provenance.rng_version.trim().is_empty() {
            return Err(SchemaEnvelopeError::InvalidField {
                field: "rng_version",
                value: self.provenance.rng_version.clone(),
            });
        }
        if self.compatibility.notes.trim().is_empty() {
            return Err(SchemaEnvelopeError::InvalidField {
                field: "compatibility.notes",
                value: self.compatibility.notes.clone(),
            });
        }
        Ok(())
    }
}

fn validate_hex(
    field: &'static str,
    value: &str,
    length: usize,
) -> Result<(), SchemaEnvelopeError> {
    if value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        Ok(())
    } else {
        Err(SchemaEnvelopeError::InvalidField {
            field,
            value: value.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope() -> SchemaEnvelope {
        SchemaEnvelope {
            schema_version: SCHEMA_ENVELOPE_VERSION,
            provenance: Provenance {
                oracle_commit: "37061c511a4780d4c0719e0342533a498cd4b457".to_owned(),
                content_hash: "a".repeat(64),
                rng_version: "native-cha-cha8-v1".to_owned(),
            },
            compatibility: Compatibility {
                mode: CompatibilityMode::Translated,
                notes: "checked legacy translation".to_owned(),
            },
        }
    }

    #[test]
    fn valid_envelope_round_trips_with_stable_wire_values() {
        let value = envelope();
        let json = serde_json::to_string(&value).unwrap();
        assert!(json.contains("\"translated\""));
        assert_eq!(SchemaEnvelope::parse(&json).unwrap(), value);
    }

    #[test]
    fn unknown_schema_version_fails_clearly() {
        let mut value = envelope();
        value.schema_version = 2;
        assert_eq!(
            value.validate(),
            Err(SchemaEnvelopeError::UnsupportedVersion { found: 2 })
        );
    }

    #[test]
    fn malformed_provenance_is_refused() {
        let mut value = envelope();
        value.provenance.content_hash = "not-a-sha".to_owned();
        assert!(matches!(
            value.validate(),
            Err(SchemaEnvelopeError::InvalidField {
                field: "content_hash",
                ..
            })
        ));
    }

    #[test]
    fn unknown_wire_fields_are_not_silently_accepted() {
        let json = serde_json::to_string(&envelope())
            .unwrap()
            .replace('}', ",\"future\":true}");
        assert!(matches!(
            SchemaEnvelope::parse(&json),
            Err(SchemaEnvelopeError::Json(_))
        ));
    }
}
