//! Client to server messages.

use serde::{Deserialize, Serialize};
pub use ti4_model::state::ReactionMode;

/// Largest externally supplied protocol fields accepted by the server.
pub const MAX_GAME_ID_BYTES: usize = 64;
pub const MAX_PLAYER_SESSION_BYTES: usize = 128;
pub const MAX_NONCE_BYTES: usize = 128;
pub const MAX_OPTION_ID_BYTES: usize = 1024;
pub const MAX_CARD_NAME_BYTES: usize = 128;
/// The most entries a script may have. A whole tactical action is some tens of entries.
pub const MAX_SCRIPT_ENTRIES: usize = 512;
/// Most trigger ids one `SetReactionIntent` may carry (the server then allows fewer, see
/// `session::bluff::MAX_DECLARED_TRIGGERS`) and the longest id.
pub const MAX_INTENT_TRIGGERS: usize = 16;
pub const MAX_TRIGGER_ID_BYTES: usize = 32;
pub use super::staged::MAX_STAGED_VALUE_BYTES;
/// Most answers one `PreviewSecondary` may carry (a Leadership purchase scripts its whole payment).
pub const MAX_PREVIEW_ANSWERS: usize = ti4_engine::secondary_preview::MAX_ANSWERS;

/// Messages submitted from a client to the authoritative server.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum ClientMessage {
    /// Subscribe to live updates for a game session.
    Subscribe {
        protocol_version: u16,
        game_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        player_session: Option<String>,
        /// The connection takes part in the staged work of its seat: it is sent
        /// `ServerMessage::SeatStaged` after the snapshot and after every change, from any
        /// connection of the seat. Left out by a client that keeps its staged work itself.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        staged: bool,
    },
    /// Submit an engine-offered option ID for an outstanding decision.
    SubmitChoice {
        protocol_version: u16,
        game_id: String,
        nonce: String,
        expected_version: u64,
        option_id: String,
    },
    /// Set how this connection's own seat wants one action card handled in reaction windows.
    ///
    /// The seat is the connection's, never named by the message, so a player can only change
    /// their own modes and a spectator cannot change any. `card` is the printed card name: every
    /// copy of it is covered, for the rest of the game.
    SetReactionMode {
        protocol_version: u16,
        game_id: String,
        card: String,
        mode: ReactionMode,
    },
    /// Play `script` as the next turn of this connection's seat on a copy of the game, and
    /// tell where the run ended (`ServerMessage::DraftOutcome` with this `request_id`). The game
    /// is not changed and the server keeps nothing of the script. Of the requests that wait on
    /// one connection only the newest is run: an older one gets no answer.
    ///
    /// With `scope` the script is of the secondary of this seat, for the strategic action that
    /// is resolved now (`secondary` of the update names it, and says whether the seat may draft).
    RunDraft {
        protocol_version: u16,
        game_id: String,
        request_id: String,
        script: Vec<crate::planning::draft::DraftStep>,
        /// Left out: the next tactical action.
        #[serde(default)]
        #[cfg_attr(feature = "ts", ts(optional = nullable))]
        scope: crate::planning::audit::PlanScope,
    },
    /// Answer the pending choice of this connection's seat, and the choices after it, with
    /// `script`: a draft or a plan that is sent to the live game. A script that the game would
    /// refuse changes nothing; where a script stopped, or that it was played whole, is told
    /// with `ServerMessage::DraftApplied`.
    ApplyDraft {
        protocol_version: u16,
        game_id: String,
        nonce: String,
        expected_version: u64,
        script: Vec<crate::planning::draft::DraftStep>,
    },
    /// Take back the last decision of this connection's seat, when the last update said that
    /// it may (`HistoryStatus::can_undo`): a movement, the landings and a purchase go back
    /// whole. The game is then another session: the connection is closed as replaced, and the
    /// next snapshot of the seat has the steps that were taken back. A refusal is
    /// `ServerMessage::ActionRejected`, with the barrier when one is the cause.
    Undo {
        protocol_version: u16,
        game_id: String,
        expected_version: u64,
    },
    /// Declare which kinds of reaction window this connection's seat wants to bluff about.
    ///
    /// The whole declaration replaces the previous one; re-sending the current one is a no-op.
    /// Ephemeral and private to the seat: the server never persists or logs it, and other
    /// viewers never see it. Older servers answer an unknown message type with a malformed
    /// message error (the protocol version is not bumped, as for `SetReactionMode`), so clients
    /// send it only when the player has something declared.
    SetReactionIntent {
        protocol_version: u16,
        game_id: String,
        triggers: Vec<String>,
    },
    /// End the seat's running bluff hold early.
    PassReactionHold {
        protocol_version: u16,
        game_id: String,
    },
    /// Ask what this connection's own seat would be asked for the strategy-card secondary in
    /// progress if its window opened right now ("as of now"; read-only and ephemeral: nothing is
    /// persisted, logged or changed, and only the asking seat gets the answer).
    ///
    /// `answers` are the answers to the questions already previewed: the first answers the
    /// window question (`yes`), the next the follow-up questions in order. The seat is the
    /// connection's, never named by the message. Older servers answer an unknown message type
    /// with a malformed message error (the protocol version is not bumped, as for the bluff
    /// messages), so a client treats that as "no preview available" and falls back to its
    /// estimate; it sends this message only while it is preparing a secondary.
    PreviewSecondary {
        protocol_version: u16,
        game_id: String,
        /// Echoed in the answer so a client can match replies to requests.
        request_id: u64,
        /// The strategy card in progress (as the client read it from the public log).
        card: String,
        /// The seat that played it.
        primary: String,
        #[serde(default)]
        answers: Vec<String>,
    },
    /// Ask which ships could move into a system if it were activated now ("who can go here";
    /// read-only and ephemeral, see `protocol::reach`). Open to every viewer, spectators
    /// included: it reads only what stands on the public board.
    QueryReach {
        protocol_version: u16,
        game_id: String,
        /// Echoed in the answer so a client can match replies to requests.
        request_id: u64,
        system_id: String,
    },
    /// Replace what this connection's seat has staged in one slot (`None` clears it). The
    /// server keeps it for the seat, not for the connection, so every tab and device of the
    /// seat has it. The value is the client's; the server reads only a ready draft of a
    /// secondary (slot `secondary`), which it sends when the game asks the seat. The put is a
    /// compare-and-set: `base_revision` is the revision of the slot that the client changed
    /// (0 for a slot that was never written). A stale one writes nothing, and the connection is
    /// sent the slots as they are.
    PutStaged {
        protocol_version: u16,
        game_id: String,
        slot: super::staged::StagedSlot,
        base_revision: u64,
        #[serde(default)]
        #[cfg_attr(feature = "ts", ts(optional, type = "unknown"))]
        value: Option<serde_json::Value>,
    },
    /// Keep-alive ping message.
    Ping {
        protocol_version: u16,
        sequence: u64,
    },
}

impl std::fmt::Debug for ClientMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subscribe {
                protocol_version,
                game_id,
                player_session,
                staged,
            } => f
                .debug_struct("Subscribe")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .field(
                    "player_session",
                    &player_session.as_ref().map(|_| "[redacted]"),
                )
                .field("staged", staged)
                .finish(),
            Self::PutStaged {
                game_id,
                slot,
                base_revision,
                value,
                ..
            } => f
                .debug_struct("PutStaged")
                .field("game_id", game_id)
                .field("slot", slot)
                .field("base_revision", base_revision)
                .field("cleared", &value.is_none())
                .finish(),
            Self::SubmitChoice {
                protocol_version,
                game_id,
                nonce,
                expected_version,
                option_id,
            } => f
                .debug_struct("SubmitChoice")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .field("nonce", nonce)
                .field("expected_version", expected_version)
                .field("option_id", option_id)
                .finish(),
            Self::SetReactionMode {
                protocol_version,
                game_id,
                card,
                mode,
            } => f
                .debug_struct("SetReactionMode")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .field("card", card)
                .field("mode", mode)
                .finish(),
            Self::RunDraft {
                game_id,
                request_id,
                script,
                ..
            } => f
                .debug_struct("RunDraft")
                .field("game_id", game_id)
                .field("request_id", request_id)
                .field("entries", &script.len())
                .finish(),
            Self::ApplyDraft {
                game_id,
                nonce,
                expected_version,
                script,
                ..
            } => f
                .debug_struct("ApplyDraft")
                .field("game_id", game_id)
                .field("nonce", nonce)
                .field("expected_version", expected_version)
                .field("entries", &script.len())
                .finish(),
            Self::Undo {
                game_id,
                expected_version,
                ..
            } => f
                .debug_struct("Undo")
                .field("game_id", game_id)
                .field("expected_version", expected_version)
                .finish(),
            Self::SetReactionIntent {
                protocol_version,
                game_id,
                triggers,
            } => f
                .debug_struct("SetReactionIntent")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .field("triggers", &format_args!("[{} declared]", triggers.len()))
                .finish(),
            Self::PassReactionHold {
                protocol_version,
                game_id,
            } => f
                .debug_struct("PassReactionHold")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .finish(),
            Self::PreviewSecondary {
                protocol_version,
                game_id,
                request_id,
                card,
                primary,
                answers,
            } => f
                .debug_struct("PreviewSecondary")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .field("request_id", request_id)
                .field("card", card)
                .field("primary", primary)
                .field("answers", answers)
                .finish(),
            Self::QueryReach {
                protocol_version,
                game_id,
                request_id,
                system_id,
            } => f
                .debug_struct("QueryReach")
                .field("protocol_version", protocol_version)
                .field("game_id", game_id)
                .field("request_id", request_id)
                .field("system_id", system_id)
                .finish(),
            Self::Ping {
                protocol_version,
                sequence,
            } => f
                .debug_struct("Ping")
                .field("protocol_version", protocol_version)
                .field("sequence", sequence)
                .finish(),
        }
    }
}

impl ClientMessage {
    /// Returns the protocol version advertised by this message.
    #[must_use]
    pub fn protocol_version(&self) -> u16 {
        match self {
            Self::Subscribe {
                protocol_version, ..
            }
            | Self::SubmitChoice {
                protocol_version, ..
            }
            | Self::SetReactionMode {
                protocol_version, ..
            }
            | Self::SetReactionIntent {
                protocol_version, ..
            }
            | Self::PassReactionHold {
                protocol_version, ..
            }
            | Self::PreviewSecondary {
                protocol_version, ..
            }
            | Self::QueryReach {
                protocol_version, ..
            }
            | Self::Ping {
                protocol_version, ..
            }
            | Self::PutStaged {
                protocol_version, ..
            }
            | Self::RunDraft {
                protocol_version, ..
            }
            | Self::ApplyDraft {
                protocol_version, ..
            }
            | Self::Undo {
                protocol_version, ..
            } => *protocol_version,
        }
    }

    /// Rejects variable-length client fields before they reach session state.
    pub fn validate_bounds(&self) -> Result<(), &'static str> {
        match self {
            Self::Subscribe {
                game_id,
                player_session,
                ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                if let Some(token) = player_session {
                    bounded(token, MAX_PLAYER_SESSION_BYTES, "player_session")?;
                }
            }
            Self::SubmitChoice {
                game_id,
                nonce,
                option_id,
                ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                bounded(nonce, MAX_NONCE_BYTES, "nonce")?;
                bounded(option_id, MAX_OPTION_ID_BYTES, "option_id")?;
            }
            Self::SetReactionMode { game_id, card, .. } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                bounded(card, MAX_CARD_NAME_BYTES, "card")?;
            }
            Self::RunDraft {
                game_id,
                request_id,
                script,
                ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                bounded(request_id, MAX_NONCE_BYTES, "request_id")?;
                bounded_script(script)?;
            }
            Self::ApplyDraft {
                game_id,
                nonce,
                script,
                ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                bounded(nonce, MAX_NONCE_BYTES, "nonce")?;
                bounded_script(script)?;
            }
            Self::Undo { game_id, .. } => bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?,
            Self::SetReactionIntent {
                game_id, triggers, ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                if triggers.len() > MAX_INTENT_TRIGGERS {
                    return Err("triggers");
                }
                for trigger in triggers {
                    bounded(trigger, MAX_TRIGGER_ID_BYTES, "triggers")?;
                }
            }
            Self::PassReactionHold { game_id, .. } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
            }
            Self::PreviewSecondary {
                game_id,
                card,
                primary,
                answers,
                ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                bounded(card, MAX_CARD_NAME_BYTES, "card")?;
                bounded(primary, MAX_PLAYER_SESSION_BYTES, "primary")?;
                if answers.len() > MAX_PREVIEW_ANSWERS {
                    return Err("answers");
                }
                for answer in answers {
                    bounded(answer, MAX_OPTION_ID_BYTES, "answers")?;
                }
            }
            Self::QueryReach {
                game_id, system_id, ..
            } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                bounded(system_id, super::reach::MAX_SYSTEM_ID_BYTES, "system_id")?;
            }
            Self::PutStaged { game_id, value, .. } => {
                bounded(game_id, MAX_GAME_ID_BYTES, "game_id")?;
                if value.as_ref().is_some_and(|value| {
                    !serde_json::to_string(value)
                        .is_ok_and(|text| text.len() <= MAX_STAGED_VALUE_BYTES)
                }) {
                    return Err("value");
                }
            }
            Self::Ping { .. } => {}
        }
        Ok(())
    }
}

/// A script may be empty: a draft with nothing chosen opens the turn menu. Its texts are
/// bounded by the size of a message.
fn bounded_script(script: &[crate::planning::draft::DraftStep]) -> Result<(), &'static str> {
    if script.len() > MAX_SCRIPT_ENTRIES {
        Err("script")
    } else {
        Ok(())
    }
}

fn bounded(value: &str, limit: usize, field: &'static str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > limit {
        Err(field)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_oversized_client_fields() {
        let message = ClientMessage::SubmitChoice {
            protocol_version: 1,
            game_id: "game".to_owned(),
            nonce: "n".repeat(MAX_NONCE_BYTES + 1),
            expected_version: 1,
            option_id: "option".to_owned(),
        };

        assert_eq!(message.validate_bounds(), Err("nonce"));
    }

    #[test]
    fn reaction_intent_messages_round_trip_and_are_bounded() {
        let text = r#"{"type":"set_reaction_intent","protocol_version":4,"game_id":"g","triggers":["agenda","movement"]}"#;
        let message: ClientMessage = serde_json::from_str(text).expect("decodes");
        assert_eq!(
            message,
            ClientMessage::SetReactionIntent {
                protocol_version: 4,
                game_id: "g".to_owned(),
                triggers: vec!["agenda".to_owned(), "movement".to_owned()],
            }
        );
        assert_eq!(serde_json::to_string(&message).expect("encodes"), text);
        assert_eq!(message.validate_bounds(), Ok(()));
        // The seat is the connection's, never named by the message.
        assert!(serde_json::from_str::<ClientMessage>(
            r#"{"type":"set_reaction_intent","protocol_version":4,"game_id":"g","triggers":[],"seat":"p2"}"#
        ).is_err());
        let too_many = ClientMessage::SetReactionIntent {
            protocol_version: 4,
            game_id: "g".to_owned(),
            triggers: vec!["agenda".to_owned(); MAX_INTENT_TRIGGERS + 1],
        };
        assert_eq!(too_many.validate_bounds(), Err("triggers"));
        let long = ClientMessage::SetReactionIntent {
            protocol_version: 4,
            game_id: "g".to_owned(),
            triggers: vec!["x".repeat(MAX_TRIGGER_ID_BYTES + 1)],
        };
        assert_eq!(long.validate_bounds(), Err("triggers"));
        let empty_id = ClientMessage::SetReactionIntent {
            protocol_version: 4,
            game_id: "g".to_owned(),
            triggers: vec![String::new()],
        };
        assert_eq!(empty_id.validate_bounds(), Err("triggers"));
        // Clearing is a valid, empty declaration.
        let clear = ClientMessage::SetReactionIntent {
            protocol_version: 4,
            game_id: "g".to_owned(),
            triggers: vec![],
        };
        assert_eq!(clear.validate_bounds(), Ok(()));
        let pass = r#"{"type":"pass_reaction_hold","protocol_version":4,"game_id":"g"}"#;
        let message: ClientMessage = serde_json::from_str(pass).expect("decodes");
        assert_eq!(serde_json::to_string(&message).expect("encodes"), pass);
        assert_eq!(message.validate_bounds(), Ok(()));
        // The declaration never shows up in debug output either.
        let shown = format!(
            "{:?}",
            ClientMessage::SetReactionIntent {
                protocol_version: 4,
                game_id: "g".to_owned(),
                triggers: vec!["agenda".to_owned()],
            }
        );
        assert!(!shown.contains("agenda"));
    }

    #[test]
    fn preview_secondary_round_trips_and_is_bounded() {
        let text = r#"{"type":"preview_secondary","protocol_version":4,"game_id":"g","request_id":7,"card":"pok7technology","primary":"p1","answers":["yes"]}"#;
        let message: ClientMessage = serde_json::from_str(text).expect("decodes");
        assert_eq!(
            message,
            ClientMessage::PreviewSecondary {
                protocol_version: 4,
                game_id: "g".to_owned(),
                request_id: 7,
                card: "pok7technology".to_owned(),
                primary: "p1".to_owned(),
                answers: vec!["yes".to_owned()],
            }
        );
        assert_eq!(serde_json::to_string(&message).expect("encodes"), text);
        assert_eq!(message.validate_bounds(), Ok(()));
        // No answers at all is the window question.
        let bare: ClientMessage = serde_json::from_str(
            r#"{"type":"preview_secondary","protocol_version":4,"game_id":"g","request_id":1,"card":"c","primary":"p1"}"#,
        )
        .expect("answers default to none");
        assert_eq!(bare.validate_bounds(), Ok(()));
        // The asking seat is the connection's, never named by the message.
        assert!(serde_json::from_str::<ClientMessage>(
            r#"{"type":"preview_secondary","protocol_version":4,"game_id":"g","request_id":1,"card":"c","primary":"p1","seat":"p2"}"#
        ).is_err());
        let many = ClientMessage::PreviewSecondary {
            protocol_version: 4,
            game_id: "g".to_owned(),
            request_id: 1,
            card: "c".to_owned(),
            primary: "p1".to_owned(),
            answers: vec!["yes".to_owned(); MAX_PREVIEW_ANSWERS + 1],
        };
        assert_eq!(many.validate_bounds(), Err("answers"));
        let long = ClientMessage::PreviewSecondary {
            protocol_version: 4,
            game_id: "g".to_owned(),
            request_id: 1,
            card: "x".repeat(MAX_CARD_NAME_BYTES + 1),
            primary: "p1".to_owned(),
            answers: vec![],
        };
        assert_eq!(long.validate_bounds(), Err("card"));
    }

    #[test]
    fn set_reaction_mode_round_trips_and_is_bounded() {
        let text = r#"{"type":"set_reaction_mode","protocol_version":1,"game_id":"g","card":"Sabotage","mode":"never"}"#;
        let message: ClientMessage = serde_json::from_str(text).expect("decodes");
        assert_eq!(
            message,
            ClientMessage::SetReactionMode {
                protocol_version: 1,
                game_id: "g".to_owned(),
                card: "Sabotage".to_owned(),
                mode: ReactionMode::Never,
            }
        );
        assert_eq!(serde_json::to_string(&message).expect("encodes"), text);
        assert_eq!(message.validate_bounds(), Ok(()));
        let long = ClientMessage::SetReactionMode {
            protocol_version: 1,
            game_id: "g".to_owned(),
            card: "x".repeat(MAX_CARD_NAME_BYTES + 1),
            mode: ReactionMode::Always,
        };
        assert_eq!(long.validate_bounds(), Err("card"));
        // An unknown mode or an extra seat field is not accepted.
        assert!(serde_json::from_str::<ClientMessage>(
            r#"{"type":"set_reaction_mode","protocol_version":1,"game_id":"g","card":"Sabotage","mode":"sometimes"}"#
        ).is_err());
        assert!(serde_json::from_str::<ClientMessage>(
            r#"{"type":"set_reaction_mode","protocol_version":1,"game_id":"g","card":"Sabotage","mode":"never","seat":"p2"}"#
        ).is_err());
    }
}
