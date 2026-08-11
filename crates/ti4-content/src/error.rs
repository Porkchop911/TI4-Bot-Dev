//! Typed failures for corpus loading and validation.

use std::path::PathBuf;
use ti4_model::content_types::ContentType;

/// Anything that can go wrong turning corpus files into a [`crate::ContentStore`].
///
/// Every variant names the category and the record index, because a corpus problem is
/// diagnosed by looking at one record in one file and nothing else.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("cannot read content file {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("content file {file} is not valid JSON: {source}")]
    Json {
        file: &'static str,
        #[source]
        source: serde_json::Error,
    },

    #[error("content file {file} must be a JSON array at the top level, found {found}")]
    NotAnArray {
        file: &'static str,
        found: &'static str,
    },

    #[error("{category} record {index} must be a JSON object, found {found}")]
    NotAnObject {
        category: ContentType,
        index: usize,
        found: &'static str,
    },

    #[error(
        "{category} record {index} carries unknown source tag {tag:?}; \
         the corpus is expected to hold only the seven official expansions"
    )]
    UnknownSource {
        category: ContentType,
        index: usize,
        tag: String,
    },

    #[error("{category} record {index} is missing its identity field {field:?}")]
    MissingIdentity {
        category: ContentType,
        index: usize,
        field: &'static str,
    },

    #[error("{category} record {index} has a non-string identity field {field:?}")]
    NonStringIdentity {
        category: ContentType,
        index: usize,
        field: &'static str,
    },

    #[error("{category} has duplicate identity {id:?} at records {first} and {second}")]
    DuplicateIdentity {
        category: ContentType,
        id: String,
        first: usize,
        second: usize,
    },

    #[error("manifest.json is not valid JSON: {source}")]
    Manifest {
        #[source]
        source: serde_json::Error,
    },

    #[error(
        "corpus disagrees with its manifest: {detail}; \
         the content directory and manifest.json must come from the same extraction"
    )]
    ManifestMismatch { detail: String },
}

/// Failures found by [`crate::validate`] — broken references between categories.
///
/// Distinct from [`ContentError`]: the corpus parsed fine, but a record points at
/// something that does not exist.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error(
    "{category} record {record_id:?} field {field:?} references unknown {target} {reference:?}"
)]
pub struct ReferenceError {
    pub category: ContentType,
    pub record_id: String,
    pub field: &'static str,
    pub target: ContentType,
    pub reference: String,
}
