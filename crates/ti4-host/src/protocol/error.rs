//! Protocol error types and error kinds.

use serde::{Deserialize, Serialize};

/// Error encountered when parsing, validating, or processing protocol messages.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtocolError {
    #[error("JSON serialization or deserialization error: {0}")]
    Json(String),
    #[error("unsupported protocol version {found}; expected {expected}")]
    UnsupportedVersion { found: u16, expected: u16 },
    #[error("invalid message field '{field}': {detail}")]
    InvalidField { field: &'static str, detail: String },
    #[error("unknown or unexpected message variant")]
    UnknownVariant,
    #[error("malformed message: {0}")]
    Malformed(String),
}

/// Category of protocol or transport error communicated to the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    MalformedMessage,
    UnsupportedVersion,
    Unauthorized,
    Disconnected,
    GameNotFound,
    EngineError,
}
