//! The socket of a table: `GET /ws/tables/{game_id}`.
//!
//! A table has it from the moment it is made, also while it is a lobby (the socket of a game
//! answers 404 then). The page subscribes with its player session, or with none as a visitor.
//! The server answers with the lobby view and sends it again after every change: a player who
//! came or left, a faction, the map, the phase, and who is connected. The message is the view,
//! whole. The calls that change a table stay HTTP (`http/games.rs`).
//!
//! A player is connected while a socket of that player is open and pings, as on the socket of
//! a game (`GameRegistry::connect_player`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use ti4_model::id::PlayerId;

use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::error::ErrorKind;

use crate::session::GameRegistry;
use crate::session::registry::{LobbyError, PlayerLobbyView};

/// The largest message of a page: a subscription with a player session.
const MAX_TABLE_MESSAGE_BYTES: usize = 4 * 1024;
/// How often a socket looks for a change that nothing tells: a player whose grace time ran out.
const PRESENCE_CHECK: Duration = Duration::from_millis(500);

/// What a page sends on the socket of a table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum TableClientMessage {
    /// Asks for the table, now and after every change. Without `player_session`: as a visitor.
    Subscribe {
        protocol_version: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        player_session: Option<String>,
    },
    /// Keeps the player of the subscription connected, and a silent socket open.
    Ping { protocol_version: u16 },
}

/// What the server sends on the socket of a table.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TableServerMessage {
    /// The table as it is. `viewer` is the player of the subscription; a visitor has none.
    Table {
        protocol_version: u16,
        #[serde(skip_serializing_if = "Option::is_none")]
        viewer: Option<PlayerId>,
        lobby: Box<PlayerLobbyView>,
    },
    Pong {
        protocol_version: u16,
    },
    Error {
        protocol_version: u16,
        kind: ErrorKind,
        message: String,
    },
}

/// WebSocket upgrade handler for `GET /ws/tables/{game_id}`.
pub async fn table_ws_handler(
    Path(game_id): Path<String>,
    ws: WebSocketUpgrade,
    State(registry): State<Arc<GameRegistry>>,
) -> Response {
    if registry.player_lobby_status(&game_id, None).is_err() {
        return (StatusCode::NOT_FOUND, format!("Game '{game_id}' not found")).into_response();
    }
    ws.max_frame_size(MAX_TABLE_MESSAGE_BYTES)
        .max_message_size(MAX_TABLE_MESSAGE_BYTES)
        .on_upgrade(move |socket| async move {
            let changes = registry.watch_table(&game_id);
            let mut connection = Connection {
                registry: registry.clone(),
                game_id: game_id.clone(),
                subscribed: false,
                credential: None,
                presence: None,
                sent: None,
            };
            connection.run(socket, changes).await;
            connection.disconnect();
            drop(connection);
            registry.unwatch_table(&game_id);
        })
}

struct Connection {
    registry: Arc<GameRegistry>,
    game_id: String,
    subscribed: bool,
    /// The player session of the subscription. Dropped when the table no longer knows it.
    credential: Option<String>,
    /// The player and the connection that count as its presence.
    presence: Option<(PlayerId, u64)>,
    /// The table message that was sent last: the same view is not sent twice.
    sent: Option<String>,
}

impl Connection {
    async fn run(&mut self, socket: WebSocket, mut changes: tokio::sync::watch::Receiver<u64>) {
        let (mut sender, mut receiver) = socket.split();
        let idle_timeout = self.registry.public().config.ws_idle_timeout;
        let mut last_inbound = Instant::now();
        let mut check = tokio::time::interval(PRESENCE_CHECK);
        loop {
            let reply = tokio::select! {
                message = receiver.next() => {
                    let Some(Ok(message)) = message else { break };
                    last_inbound = Instant::now();
                    let text = match message {
                        Message::Text(text) => text.to_string(),
                        Message::Binary(_) => {
                            let reply = error(ErrorKind::MalformedMessage, "Text messages only");
                            if sender.send(Message::Text(reply.into())).await.is_err() {
                                break;
                            }
                            continue;
                        }
                        Message::Ping(_) | Message::Pong(_) => continue,
                        Message::Close(_) => break,
                    };
                    self.receive(&text)
                }
                changed = changes.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    None
                }
                _ = check.tick() => {
                    if idle_timeout.is_some_and(|limit| last_inbound.elapsed() > limit) {
                        break;
                    }
                    None
                }
            };
            if let Some(reply) = reply
                && sender.send(Message::Text(reply.into())).await.is_err()
            {
                break;
            }
            if !self.subscribed {
                continue;
            }
            match self.table() {
                Ok(Some(table)) => {
                    if sender.send(Message::Text(table.into())).await.is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                // The server no longer has the table.
                Err(()) => break,
            }
        }
    }

    /// Answers a message of the page. The table itself is sent by the loop.
    fn receive(&mut self, text: &str) -> Option<String> {
        let message = match serde_json::from_str::<TableClientMessage>(text) {
            Ok(message) => message,
            Err(err) => return Some(error(ErrorKind::MalformedMessage, &err.to_string())),
        };
        let (TableClientMessage::Subscribe {
            protocol_version, ..
        }
        | TableClientMessage::Ping { protocol_version }) = &message;
        if *protocol_version != PROTOCOL_VERSION {
            return Some(error(
                ErrorKind::UnsupportedVersion,
                &format!(
                    "Unsupported protocol version {protocol_version}. Expected {PROTOCOL_VERSION}"
                ),
            ));
        }
        match message {
            TableClientMessage::Subscribe { player_session, .. } => {
                self.disconnect();
                self.sent = None;
                self.subscribed = false;
                if let Some(credential) = player_session {
                    match self.registry.connect_player(&self.game_id, &credential) {
                        Ok(presence) => {
                            self.presence = Some(presence);
                            self.credential = Some(credential);
                        }
                        Err(LobbyError::NotFound) => {
                            return Some(error(ErrorKind::GameNotFound, "Game not found"));
                        }
                        Err(_) => {
                            return Some(error(
                                ErrorKind::Unauthorized,
                                "The table does not know this player session",
                            ));
                        }
                    }
                }
                self.subscribed = true;
                None
            }
            TableClientMessage::Ping { .. } => {
                if let (Some(credential), Some((_, connection))) =
                    (&self.credential, &self.presence)
                {
                    // A player that the table no longer has is a visitor from the next view on.
                    let _ = self
                        .registry
                        .ping_player(&self.game_id, credential, *connection);
                }
                serde_json::to_string(&TableServerMessage::Pong {
                    protocol_version: PROTOCOL_VERSION,
                })
                .ok()
            }
        }
    }

    /// The table for this viewer, when it is not what was sent last. `Err`: the table is gone.
    fn table(&mut self) -> Result<Option<String>, ()> {
        let status = self
            .registry
            .player_lobby_status(&self.game_id, self.credential.as_deref());
        let (lobby, viewer) = match status {
            Ok(status) => status,
            // The player left or was removed: the socket goes on as a visitor.
            Err(LobbyError::InvalidCapability) if self.credential.is_some() => {
                self.disconnect();
                self.registry
                    .player_lobby_status(&self.game_id, None)
                    .map_err(|_| ())?
            }
            Err(_) => return Err(()),
        };
        let message = serde_json::to_string(&TableServerMessage::Table {
            protocol_version: PROTOCOL_VERSION,
            viewer,
            lobby: Box::new(lobby),
        })
        .map_err(|_| ())?;
        if self.sent.as_deref() == Some(message.as_str()) {
            return Ok(None);
        }
        self.sent = Some(message.clone());
        Ok(Some(message))
    }

    /// Ends the presence of the subscription: the player is a visitor, or the socket is closed.
    fn disconnect(&mut self) {
        self.credential = None;
        if let Some((player, connection)) = self.presence.take() {
            self.registry
                .disconnect_player(&self.game_id, &player, connection);
        }
    }
}

fn error(kind: ErrorKind, message: &str) -> String {
    serde_json::to_string(&TableServerMessage::Error {
        protocol_version: PROTOCOL_VERSION,
        kind,
        message: message.to_owned(),
    })
    .unwrap_or_default()
}
