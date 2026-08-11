//! Canonical content digests.
//!
//! The upstream corpus carries no checksums — `manifest.json` records only a git commit
//! and record counts — so integrity is established here. Two digests exist for two
//! different questions:
//!
//! * `content/CHECKSUMS.sha256` hashes the **files**, and answers "are these the bytes
//!   copied from the oracle?". It is verified with `sha256sum -c`.
//! * [`digest_of`] hashes the **parsed records**, and answers "is this the same corpus?".
//!   It is stable across whitespace, key order, and end-of-line conversion, so it can be
//!   compared between a file corpus and a regenerated one, and can be embedded in a
//!   savegame to detect that content changed underneath a replay.

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest, Sha256};
use ti4_model::content_types::{ALL_CONTENT_TYPES, ContentType};

use crate::loader::ContentStore;

/// A canonical digest of a corpus: one hash per category plus an aggregate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorpusDigest {
    /// Hex SHA-256 over the whole corpus.
    pub corpus: String,
    /// Hex SHA-256 per category, keyed by category name.
    pub categories: BTreeMap<String, String>,
    /// Total records hashed.
    pub records: usize,
}

impl fmt::Display for CorpusDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({} records)", self.corpus, self.records)
    }
}

/// Canonical digest of a corpus.
///
/// Each record is hashed as its compact JSON form with keys in sorted order, length
/// prefixed so that no concatenation of records can be confused with another. Categories
/// are folded in fixed [`ALL_CONTENT_TYPES`] order, so the result does not depend on any
/// map layout inside the store.
#[must_use]
pub fn digest_of(store: &ContentStore) -> CorpusDigest {
    let mut corpus = Sha256::new();
    let mut categories = BTreeMap::new();
    let mut records = 0;

    for &category in ALL_CONTENT_TYPES {
        let mut hasher = Sha256::new();
        let entries = store.records(category);
        write_framed(&mut hasher, category.to_string().as_bytes());
        write_framed(&mut hasher, entries.len().to_string().as_bytes());
        for record in entries {
            // serde_json::Map is a BTreeMap here, so `to_string` emits keys in sorted
            // order and the encoding is whitespace-free and therefore EOL-independent.
            let canonical = serde_json::Value::Object(record.fields().clone()).to_string();
            write_framed(&mut hasher, canonical.as_bytes());
        }
        records += entries.len();

        let hex = hex::encode(hasher.finalize());
        write_framed(&mut corpus, category.to_string().as_bytes());
        write_framed(&mut corpus, hex.as_bytes());
        categories.insert(category.to_string(), hex);
    }

    CorpusDigest {
        corpus: hex::encode(corpus.finalize()),
        categories,
        records,
    }
}

/// Canonical digest of the compiled-in corpus.
#[must_use]
pub fn embedded_digest() -> CorpusDigest {
    digest_of(ContentStore::embedded())
}

/// Digest of one category, if a caller only needs that.
#[must_use]
pub fn category_digest(store: &ContentStore, category: ContentType) -> Option<String> {
    digest_of(store).categories.remove(&category.to_string())
}

/// Hash a length-prefixed field, so that `["ab","c"]` and `["a","bc"]` differ.
fn write_framed(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_is_stable_across_repeated_loads() {
        let a = digest_of(&ContentStore::parse_embedded().unwrap());
        let b = digest_of(&ContentStore::parse_embedded().unwrap());
        assert_eq!(a, b);
        assert_eq!(a.records, 1800);
    }

    #[test]
    fn a_corpus_read_from_disk_has_the_same_digest_as_the_embedded_one() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("content");
        let on_disk = ContentStore::from_dir(&dir).unwrap();
        assert_eq!(digest_of(&on_disk), embedded_digest());
    }

    #[test]
    fn every_category_contributes_a_distinct_digest() {
        let digest = embedded_digest();
        assert_eq!(digest.categories.len(), ALL_CONTENT_TYPES.len());
        let unique: std::collections::BTreeSet<&String> = digest.categories.values().collect();
        assert_eq!(unique.len(), digest.categories.len());
    }

    #[test]
    fn the_aggregate_digest_is_not_any_category_digest() {
        let digest = embedded_digest();
        assert!(!digest.categories.values().any(|c| *c == digest.corpus));
    }

    #[test]
    fn framing_prevents_boundary_collisions() {
        let mut ab_c = Sha256::new();
        write_framed(&mut ab_c, b"ab");
        write_framed(&mut ab_c, b"c");
        let mut a_bc = Sha256::new();
        write_framed(&mut a_bc, b"a");
        write_framed(&mut a_bc, b"bc");
        assert_ne!(ab_c.finalize(), a_bc.finalize());
    }

    #[test]
    fn a_category_digest_matches_the_aggregate_breakdown() {
        let store = ContentStore::embedded();
        let single = category_digest(store, ContentType::Units).unwrap();
        assert_eq!(digest_of(store).categories["units"], single);
    }
}
