//! The corpus manifest: what was extracted, from where, and how much of it there is.
//!
//! `manifest.json` is written by the oracle's `tools/extract_asyncti4.py` and is the only
//! provenance the corpus carries — there are no per-file checksums upstream. This crate
//! adds those separately in [`crate::provenance`].

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::ContentError;

/// Provenance and record counts for one extraction of the corpus.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Schema of the manifest itself, e.g. `"1.1.0"`.
    pub schema_version: String,
    /// When the extraction ran (RFC 3339).
    pub extracted_at: String,
    /// Where the records came from.
    pub upstream: Upstream,
    /// The source tags treated as official and therefore kept.
    pub official_sources: Vec<String>,
    /// Rendering-only fields stripped during extraction.
    pub dropped_fields: Vec<String>,
    /// Corpus-wide counts.
    pub totals: Totals,
    /// Per-category counts, keyed by category name.
    pub categories: BTreeMap<String, CategoryCounts>,
    /// Homebrew source tags filtered out, with the record count each would have added.
    pub excluded_sources: BTreeMap<String, u64>,
}

/// The upstream project a corpus was extracted from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Upstream {
    pub project: String,
    pub licence: String,
    pub commit: String,
    pub committed_at: String,
}

/// Corpus-wide record counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Totals {
    pub records: u64,
    pub categories: u64,
    /// Records with no `source` field — always the `colors`, `combat_modifiers`, and
    /// `map_templates` categories in full.
    pub untagged: u64,
}

/// Record counts for one category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryCounts {
    pub records: u64,
    pub untagged: u64,
    pub by_source: BTreeMap<String, u64>,
}

impl Manifest {
    /// Parse `manifest.json`.
    ///
    /// # Errors
    /// Returns [`ContentError::Manifest`] if the JSON is malformed or missing a field.
    pub fn parse(json: &str) -> Result<Self, ContentError> {
        serde_json::from_str(json).map_err(|source| ContentError::Manifest { source })
    }

    /// Counts for one category by name.
    #[must_use]
    pub fn category(&self, name: &str) -> Option<&CategoryCounts> {
        self.categories.get(name)
    }
}
