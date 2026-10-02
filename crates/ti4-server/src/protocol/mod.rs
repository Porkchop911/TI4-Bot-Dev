//! Wire protocol data transfer objects and message definitions.

pub mod client;
pub mod error;
pub mod server;
pub mod status;
pub mod view;

pub use client::ClientMessage;
pub use error::{ErrorKind, ProtocolError};
pub use server::{
    ActionAcceptedMsg, ActionRejectedMsg, EventVisibility, GameEvent, GameEventKind, GameEventMsg,
    GameOverMsg, InitialSnapshotMsg, PendingChoiceEnvelope, PendingChoiceMsg, PongMsg,
    ProtocolErrorMsg, ServerMessage, StateUpdateMsg, TurnStatusMsg,
};
pub use status::{PublicTurnStatus, RejectionReason, ViewerRole};
pub use view::{
    BoardView, GameView, PlacedUnitView, PlanetView, PlayerView, SystemView, TableView,
};

/// Current supported wire protocol schema version.
pub const PROTOCOL_VERSION: u16 = 3;

/// Validates that the provided protocol version matches the current build.
///
/// # Errors
///
/// Returns [`ProtocolError::UnsupportedVersion`] if versions do not match.
pub fn validate_protocol_version(version: u16) -> Result<(), ProtocolError> {
    if version != PROTOCOL_VERSION {
        return Err(ProtocolError::UnsupportedVersion {
            found: version,
            expected: PROTOCOL_VERSION,
        });
    }
    Ok(())
}

/// Parses and validates a client-to-server message from raw JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Json`] for malformed JSON or unknown fields, and
/// [`ProtocolError::UnsupportedVersion`] if the version is not supported.
pub fn parse_client_message(json: &str) -> Result<ClientMessage, ProtocolError> {
    let msg = serde_json::from_str::<ClientMessage>(json)
        .map_err(|e| ProtocolError::Json(e.to_string()))?;
    validate_protocol_version(msg.protocol_version())?;
    Ok(msg)
}

/// Parses and validates a server-to-client message from raw JSON.
///
/// # Errors
///
/// Returns [`ProtocolError::Json`] for malformed JSON or unknown fields, and
/// [`ProtocolError::UnsupportedVersion`] if the version is not supported.
pub fn parse_server_message(json: &str) -> Result<ServerMessage, ProtocolError> {
    let msg = serde_json::from_str::<ServerMessage>(json)
        .map_err(|e| ProtocolError::Json(e.to_string()))?;
    validate_protocol_version(msg.protocol_version())?;
    Ok(msg)
}
