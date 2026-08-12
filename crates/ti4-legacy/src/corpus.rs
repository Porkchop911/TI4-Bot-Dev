//! Validation for the retained bounded legacy-trace corpus.

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path},
};

use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::converter::{BoundedTraceError, PINNED_ORACLE_COMMIT, parse_bounded_trace};

/// Version of the retained bounded trace corpus manifest.
pub const BOUNDED_CORPUS_SCHEMA_VERSION: &str = "m03-007b-v1";
/// Maximum retained corpus size, excluding the small JSON manifest.
pub const MAX_BOUNDED_CORPUS_BYTES: u64 = 20 * 1024 * 1024;
/// Maximum size for a single bounded trace.
pub const MAX_BOUNDED_TRACE_BYTES: u64 = 512 * 1024;
/// The required number of trace fixtures.
pub const BOUNDED_CORPUS_TRACE_COUNT: usize = 100;

/// Verified aggregate facts about a bounded trace corpus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoundedCorpusSummary {
    /// Number of validated trace files.
    pub traces: usize,
    /// Total NDJSON byte count.
    pub bytes: u64,
}

/// Corpus manifest or fixture validation failure.
#[derive(Debug, thiserror::Error)]
pub enum CorpusError {
    /// The manifest or a referenced trace file could not be read.
    #[error("cannot read {path}: {source}")]
    Read {
        /// Location that could not be read.
        path: String,
        /// Underlying operating-system error.
        source: std::io::Error,
    },
    /// The manifest is not valid JSON.
    #[error("manifest is not valid JSON: {0}")]
    ManifestJson(serde_json::Error),
    /// The manifest or a trace entry violates its declared format.
    #[error("invalid corpus: {0}")]
    Invalid(String),
    /// A trace does not pass the bounded-oracle parser.
    #[error("trace {path} is invalid: {source}")]
    Trace {
        /// Relative trace filename.
        path: String,
        /// Underlying trace-parser failure.
        source: BoundedTraceError,
    },
}

#[derive(Debug, Deserialize)]
struct Manifest {
    schema_version: String,
    oracle_commit: String,
    traces: Vec<ManifestTrace>,
}

#[derive(Debug, Deserialize)]
struct ManifestTrace {
    id: String,
    scenario: String,
    seed: i64,
    rounds: u64,
    path: String,
    bytes: u64,
    sha256: String,
}

/// Validate every retained bounded trace against its manifest and pinned provenance.
///
/// # Errors
/// Returns [`CorpusError`] before accepting any summary when a file is missing, a checksum or
/// size disagrees, a manifest path escapes the corpus directory, or a trace's own header differs
/// from its manifest entry.
pub fn validate_bounded_fixture_corpus(
    root: impl AsRef<Path>,
) -> Result<BoundedCorpusSummary, CorpusError> {
    let root = root.as_ref();
    let canonical_root = fs::canonicalize(root).map_err(|source| CorpusError::Read {
        path: root.display().to_string(),
        source,
    })?;
    let manifest = read_manifest(root)?;
    validate_manifest(&manifest)?;

    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut total_bytes = 0_u64;
    for entry in &manifest.traces {
        if entry.id.is_empty() || !ids.insert(&entry.id) {
            return Err(CorpusError::Invalid(format!(
                "duplicate or empty trace id {:?}",
                entry.id
            )));
        }
        let relative = Path::new(&entry.path);
        if !is_safe_relative_path(relative) || !paths.insert(&entry.path) {
            return Err(CorpusError::Invalid(format!(
                "unsafe or duplicate trace path {:?}",
                entry.path
            )));
        }
        if entry.bytes > MAX_BOUNDED_TRACE_BYTES || entry.rounds == 0 || entry.scenario.is_empty() {
            return Err(CorpusError::Invalid(format!(
                "invalid manifest entry {:?}",
                entry.id
            )));
        }
        let trace_path = root.join(relative);
        let canonical_trace_path =
            fs::canonicalize(&trace_path).map_err(|source| CorpusError::Read {
                path: trace_path.display().to_string(),
                source,
            })?;
        if !canonical_trace_path.starts_with(&canonical_root) {
            return Err(CorpusError::Invalid(format!(
                "trace path {:?} resolves outside the corpus root",
                entry.path
            )));
        }
        let bytes = fs::read(&canonical_trace_path).map_err(|source| CorpusError::Read {
            path: trace_path.display().to_string(),
            source,
        })?;
        let actual_bytes = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if actual_bytes != entry.bytes {
            return Err(CorpusError::Invalid(format!(
                "trace {:?} has {actual_bytes} bytes, manifest says {}",
                entry.path, entry.bytes
            )));
        }
        let actual_hash = format!("{:x}", Sha256::digest(&bytes));
        if actual_hash != entry.sha256 {
            return Err(CorpusError::Invalid(format!(
                "trace {:?} checksum mismatch",
                entry.path
            )));
        }
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| CorpusError::Invalid(format!("trace {:?} is not UTF-8", entry.path)))?;
        let trace = parse_bounded_trace(text).map_err(|source| CorpusError::Trace {
            path: entry.path.clone(),
            source,
        })?;
        if trace.scenario != entry.scenario
            || trace.seed != entry.seed
            || trace.rounds != entry.rounds
        {
            return Err(CorpusError::Invalid(format!(
                "trace {:?} disagrees with its manifest",
                entry.path
            )));
        }
        total_bytes = total_bytes
            .checked_add(actual_bytes)
            .ok_or_else(|| CorpusError::Invalid("corpus size overflow".to_owned()))?;
    }
    if total_bytes > MAX_BOUNDED_CORPUS_BYTES {
        return Err(CorpusError::Invalid(format!(
            "corpus has {total_bytes} bytes, limit is {MAX_BOUNDED_CORPUS_BYTES}"
        )));
    }
    Ok(BoundedCorpusSummary {
        traces: manifest.traces.len(),
        bytes: total_bytes,
    })
}

fn read_manifest(root: &Path) -> Result<Manifest, CorpusError> {
    let manifest_path = root.join("manifest.json");
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|source| CorpusError::Read {
        path: manifest_path.display().to_string(),
        source,
    })?;
    serde_json::from_str::<Manifest>(&manifest_text).map_err(CorpusError::ManifestJson)
}

fn validate_manifest(manifest: &Manifest) -> Result<(), CorpusError> {
    if manifest.schema_version != BOUNDED_CORPUS_SCHEMA_VERSION {
        return Err(CorpusError::Invalid(format!(
            "unsupported manifest schema {:?}",
            manifest.schema_version
        )));
    }
    if manifest.oracle_commit != PINNED_ORACLE_COMMIT {
        return Err(CorpusError::Invalid(format!(
            "unexpected oracle commit {:?}",
            manifest.oracle_commit
        )));
    }
    if manifest.traces.len() != BOUNDED_CORPUS_TRACE_COUNT {
        return Err(CorpusError::Invalid(format!(
            "manifest has {} traces, expected {BOUNDED_CORPUS_TRACE_COUNT}",
            manifest.traces.len()
        )));
    }
    Ok(())
}

fn is_safe_relative_path(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    #[test]
    fn retained_bounded_corpus_has_verified_provenance_and_inputs() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/legacy_entropy/bounded-v1");

        let summary = validate_bounded_fixture_corpus(root).unwrap();

        assert_eq!(summary.traces, BOUNDED_CORPUS_TRACE_COUNT);
        assert!(summary.bytes > 0);
        assert!(summary.bytes <= MAX_BOUNDED_CORPUS_BYTES);
    }

    #[test]
    fn trace_paths_are_normal_relative_components_only() {
        assert!(is_safe_relative_path(Path::new("traces/trace-001.ndjson")));
        assert!(!is_safe_relative_path(Path::new("")));
        assert!(!is_safe_relative_path(Path::new("./trace-001.ndjson")));
        assert!(!is_safe_relative_path(Path::new("../trace-001.ndjson")));
        assert!(!is_safe_relative_path(Path::new("C:/trace-001.ndjson")));
    }
}
