//! Wire protocol data transfer objects and message definitions.

use ti4_engine::choice::Choice;

pub mod board_delta;
pub mod client;
pub mod error;
pub mod public_fact;
pub mod reach;
pub mod server;
pub mod splice;
pub mod staged;
pub mod state_fact;
pub mod status;
pub mod turn_redo;
pub mod view;

pub use client::ClientMessage;
pub use error::{ErrorKind, ProtocolError};
pub use server::{
    ActionAcceptedMsg, ActionRejectedMsg, DraftAppliedMsg, DraftOutcomeMsg, EventVisibility,
    GameEvent, GameEventKind, GameEventMsg, GameOverMsg, InitialSnapshotMsg, PendingChoiceEnvelope,
    PendingChoiceMsg, PongMsg, ProtocolErrorMsg, ScriptStopped, ServerMessage, StateUpdateMsg,
    TurnStatusMsg,
};
pub use status::{PublicTurnStatus, RejectionReason, ViewerRole};
pub use view::{
    BoardView, GameView, PlacedUnitView, PlanetView, PlayerView, SystemView, TableView,
};

/// Current supported wire protocol schema version.
pub const PROTOCOL_VERSION: u16 = 4;

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

/// Checks a submission against the choice that the game waits in: the request names that
/// choice (`nonce`), the game is still at the version the request expects, and `option_id`, when
/// the request answers with one, is an answer of the choice (a listed option or a free-form one).
/// `pending` is the waiting choice with its nonce, `None` when no choice can take an answer.
/// `current` is the version of the game. Both hosts call it; the seat check and the reservation
/// of the choice stay with the host that has them.
///
/// # Errors
///
/// [`RejectionReason::NoPendingChoice`], [`RejectionReason::StaleNonce`],
/// [`RejectionReason::StaleVersion`] or [`RejectionReason::UnknownOption`], checked in this order.
pub fn check_submission(
    pending: Option<(&Choice, &str)>,
    nonce: &str,
    expected: u64,
    current: u64,
    option_id: Option<&str>,
) -> Result<(), RejectionReason> {
    let Some((choice, pending_nonce)) = pending else {
        return Err(RejectionReason::NoPendingChoice);
    };
    if pending_nonce != nonce {
        return Err(RejectionReason::StaleNonce);
    }
    if expected != current {
        return Err(RejectionReason::StaleVersion { expected, current });
    }
    match option_id {
        Some(option_id) if choice.answer_for(option_id).is_none() => {
            Err(RejectionReason::UnknownOption {
                option_id: option_id.to_owned(),
            })
        }
        _ => Ok(()),
    }
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

#[cfg(test)]
mod tests {
    use super::{RejectionReason, check_submission};
    use ti4_engine::choice::{Choice, ChoiceOption};

    fn asked() -> Choice {
        Choice::new(
            ti4_model::id::PlayerId::new("p1"),
            "pick",
            vec![ChoiceOption::labelled("a", "pick", "A")],
        )
        .accepting_free_form("deal:")
    }

    #[test]
    fn submission_checks_in_order() {
        let choice = asked();
        let pending = Some((&choice, "n1"));
        assert_eq!(check_submission(pending, "n1", 3, 3, Some("a")), Ok(()));
        assert_eq!(
            check_submission(pending, "n1", 3, 3, Some("deal:x")),
            Ok(())
        );
        assert_eq!(check_submission(pending, "n1", 3, 3, None), Ok(()));
        assert_eq!(
            check_submission(None, "n1", 3, 3, Some("a")),
            Err(RejectionReason::NoPendingChoice)
        );
        assert_eq!(
            check_submission(pending, "n0", 2, 3, Some("b")),
            Err(RejectionReason::StaleNonce)
        );
        assert_eq!(
            check_submission(pending, "n1", 2, 3, Some("b")),
            Err(RejectionReason::StaleVersion {
                expected: 2,
                current: 3
            })
        );
        assert_eq!(
            check_submission(pending, "n1", 3, 3, Some("b")),
            Err(RejectionReason::UnknownOption {
                option_id: "b".to_owned()
            })
        );
    }
}
