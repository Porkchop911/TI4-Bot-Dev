//! The one error type of the crate.

use std::fmt;

/// Anything that stopped a bundle being read, a model file being written or read, or a forward
/// pass.
#[derive(Debug)]
pub enum InferError {
    /// The file system refused.
    Io {
        /// What was being attempted.
        context: String,
        /// The underlying failure.
        source: std::io::Error,
    },
    /// A training bundle that this crate does not accept.
    Bundle(String),
    /// The file does not start with `TI4M`.
    Magic,
    /// A version of the model file other than [`crate::format::VERSION`].
    Version(u32),
    /// The file is shorter or longer than its header says.
    Truncated {
        /// The length the header implies.
        expected: usize,
        /// The length of the bytes.
        found: usize,
    },
    /// The SHA-256 trailer does not match the bytes before it.
    Digest {
        /// The trailer.
        expected: String,
        /// The digest of the bytes.
        found: String,
    },
    /// A trunk width that is zero, not a multiple of 8, or narrower than the identity embedding.
    Width(usize),
    /// A model file whose header or sections are wrong in another way.
    Format(String),
    /// A model that the writer cannot pack: shapes that disagree, a weight that is not finite.
    Parts(String),
    /// A faction row outside the roster.
    Faction(usize),
    /// A head index outside the model's heads.
    Head(usize),
    /// A decision with no option.
    EmptyLegalSet,
    /// An option whose columns and values differ in length.
    Ragged {
        /// The number of columns.
        columns: usize,
        /// The number of values.
        values: usize,
    },
    /// A column outside the input table.
    OutOfRange {
        /// The column.
        column: i64,
        /// The rows of the input table.
        capacity: usize,
    },
    /// A feature value that is not finite.
    NotFinite(f32),
    /// A backend that this target or this CPU does not have.
    Backend(crate::Backend),
}

impl fmt::Display for InferError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io { context, source } => write!(f, "{context}: {source}"),
            Self::Bundle(detail) => write!(f, "invalid bundle: {detail}"),
            Self::Magic => write!(f, "not a .ti4m model file"),
            Self::Version(version) => write!(
                f,
                "model file version {version} is not {}",
                crate::format::VERSION
            ),
            Self::Truncated { expected, found } => write!(
                f,
                "the model file has {found} bytes, its header says {expected}"
            ),
            Self::Digest { expected, found } => write!(
                f,
                "the model file hashes {found}, its trailer says {expected}"
            ),
            Self::Width(width) => write!(
                f,
                "width {width} is not a positive multiple of 8 of at least {}",
                crate::EMBED_DIM
            ),
            Self::Format(detail) => write!(f, "invalid model file: {detail}"),
            Self::Parts(detail) => write!(f, "cannot pack the model: {detail}"),
            Self::Faction(row) => write!(f, "faction row {row} is outside the roster"),
            Self::Head(head) => write!(f, "head {head} is not one of the model's heads"),
            Self::EmptyLegalSet => write!(f, "the legal set is empty"),
            Self::Ragged { columns, values } => {
                write!(f, "an option has {columns} columns and {values} values")
            }
            Self::OutOfRange { column, capacity } => {
                write!(
                    f,
                    "column {column} is outside the input table of {capacity} rows"
                )
            }
            Self::NotFinite(value) => write!(f, "feature value {value} is not finite"),
            Self::Backend(backend) => write!(
                f,
                "the backend {} is not available on this target or CPU",
                backend.name()
            ),
        }
    }
}

impl std::error::Error for InferError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io { source, .. } => Some(source),
            _ => None,
        }
    }
}
