//! WebSocket connection lifecycle and routing for live multiplayer sessions.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::client::ClientMessage;
use crate::protocol::error::ErrorKind;
use crate::protocol::server::{ActionRejectedMsg, PongMsg, ProtocolErrorMsg, ServerMessage};
use crate::protocol::status::{RejectionReason, ViewerRole};
use crate::session::{GameRegistry, GameSession};

/// WebSocket upgrade handler for `GET /ws/games/{game_id}`.
pub async fn ws_handler(
    Path(game_id): Path<String>,
    ws: WebSocketUpgrade,
    State(registry): State<Arc<GameRegistry>>,
) -> Response {
    if let Some(session) = registry.get_game(&game_id) {
        ws.max_frame_size(MAX_CLIENT_MESSAGE_BYTES)
            .max_message_size(MAX_CLIENT_MESSAGE_BYTES)
            .on_upgrade(move |socket| handle_socket(socket, game_id, session, registry))
            .into_response()
    } else {
        (StatusCode::NOT_FOUND, format!("Game '{game_id}' not found")).into_response()
    }
}

/// Outbound queue bounded capacity to prevent slow consumers from buffering indefinitely.
const OUTBOUND_QUEUE_CAPACITY: usize = 128;
/// Maximum complete client WebSocket message, including JSON framing.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 4 * 1024;

#[allow(clippy::too_many_lines)]
async fn handle_socket(
    socket: WebSocket,
    game_id: String,
    session: Arc<GameSession>,
    registry: Arc<GameRegistry>,
) {
    let (mut ws_sender, mut ws_receiver) = socket.split();

    // Bounded outbound channel for messages destined for this client
    let (outbound_tx, mut outbound_rx) = mpsc::channel::<ServerMessage>(OUTBOUND_QUEUE_CAPACITY);
    let (replacement_tx, mut replacement_rx) = tokio::sync::oneshot::channel::<()>();
    let mut replacement_tx = Some(replacement_tx);
    let outbound_registry = registry.clone();
    let outbound_game = game_id.clone();
    let outbound_credential = Arc::new(std::sync::Mutex::new(None::<String>));
    let credential_for_pump = outbound_credential.clone();

    // Outbound pump task: forwards ServerMessage as JSON text to WebSocket sink
    let outbound_task = tokio::spawn(async move {
        loop {
            let msg = tokio::select! {
                msg = outbound_rx.recv() => match msg { Some(msg) => msg, None => break },
                result = &mut replacement_rx => {
                    if result.is_ok() {
                        let _ = ws_sender.send(Message::Close(Some(CloseFrame {
                            code: 4001,
                            reason: "session replaced".into(),
                        }))).await;
                    }
                    break;
                }
            };
            let credential = credential_for_pump.lock().expect("credential lock").clone();
            if credential.as_deref().is_some_and(|token| {
                outbound_registry
                    .authenticate_player_session(&outbound_game, token)
                    .is_err()
            }) {
                break;
            }
            match serde_json::to_string(&msg) {
                Ok(text) => {
                    if ws_sender.send(Message::Text(text.into())).await.is_err() {
                        break;
                    }
                }
                Err(err) => {
                    warn!("Failed to serialize ServerMessage: {err}");
                }
            }
        }
    });

    let mut current_role: Option<ViewerRole> = None;
    let mut current_token: Option<String> = None;
    let mut connection_id: Option<u64> = None;
    let mut auth_check = tokio::time::interval(Duration::from_millis(250));

    // Inbound processing loop
    loop {
        let msg_result = tokio::select! {
            msg = ws_receiver.next() => match msg { Some(msg) => msg, None => break },
            _ = auth_check.tick() => {
                if session.error().is_some() {
                    break;
                }
                if registry.get_game(&game_id).is_none_or(|current| !Arc::ptr_eq(&current, &session)) {
                    if let Some(tx) = replacement_tx.take() {
                        let _ = tx.send(());
                    }
                    break;
                }
                if current_token.as_deref().is_some_and(|token| registry.authenticate_player_session(&game_id, token).is_err()) {
                    break;
                }
                continue;
            }
        };
        let ws_msg = match msg_result {
            Ok(msg) => msg,
            Err(err) => {
                debug!("WebSocket read error: {err}");
                break;
            }
        };

        let text = match ws_msg {
            Message::Text(t) => t.to_string(),
            Message::Binary(b) => {
                let Ok(s) = String::from_utf8(b.to_vec()) else {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind: ErrorKind::MalformedMessage,
                            message: "Invalid UTF-8 in binary payload".to_owned(),
                        }))
                        .await;
                    continue;
                };
                s
            }
            Message::Ping(_) | Message::Pong(_) => continue,
            Message::Close(_) => break,
        };

        let client_msg: ClientMessage = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(err) => {
                let _ = outbound_tx
                    .send(ServerMessage::Error(ProtocolErrorMsg {
                        protocol_version: PROTOCOL_VERSION,
                        kind: ErrorKind::MalformedMessage,
                        message: err.to_string(),
                    }))
                    .await;
                continue;
            }
        };

        if let Err(field) = client_msg.validate_bounds() {
            let _ = outbound_tx
                .send(ServerMessage::Error(ProtocolErrorMsg {
                    protocol_version: PROTOCOL_VERSION,
                    kind: ErrorKind::MalformedMessage,
                    message: format!("Invalid or oversized {field}"),
                }))
                .await;
            continue;
        }

        // Validate protocol version
        if client_msg.protocol_version() != PROTOCOL_VERSION {
            let _ = outbound_tx
                .send(ServerMessage::Error(ProtocolErrorMsg {
                    protocol_version: PROTOCOL_VERSION,
                    kind: ErrorKind::UnsupportedVersion,
                    message: format!(
                        "Unsupported protocol version {}. Expected {}",
                        client_msg.protocol_version(),
                        PROTOCOL_VERSION
                    ),
                }))
                .await;
            continue;
        }

        match client_msg {
            ClientMessage::Ping { sequence, .. } => {
                if let (Some(token), Some(connection)) = (&current_token, connection_id) {
                    if registry.ping_player(&game_id, token, connection).is_err() {
                        break;
                    }
                }
                let _ = outbound_tx
                    .send(ServerMessage::Pong(PongMsg {
                        protocol_version: PROTOCOL_VERSION,
                        sequence,
                    }))
                    .await;
            }
            ClientMessage::Subscribe {
                game_id: message_game_id,
                player_session,
                ..
            } => {
                if message_game_id != game_id {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind: ErrorKind::MalformedMessage,
                            message: "Subscribe game_id does not match the WebSocket path"
                                .to_owned(),
                        }))
                        .await;
                    continue;
                }
                if current_role.is_some() {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind: ErrorKind::MalformedMessage,
                            message: "A connection may subscribe to only one viewer role"
                                .to_owned(),
                        }))
                        .await;
                    continue;
                }
                let role = match player_session {
                    Some(token) => {
                        if let Ok((seat, connection)) = registry.connect_player(&game_id, &token) {
                            connection_id = Some(connection);
                            *outbound_credential.lock().expect("credential lock") =
                                Some(token.clone());
                            current_token = Some(token);
                            ViewerRole::Player(seat)
                        } else {
                            let _ = outbound_tx
                                .send(ServerMessage::Error(ProtocolErrorMsg {
                                    protocol_version: PROTOCOL_VERSION,
                                    kind: ErrorKind::Unauthorized,
                                    message: "Invalid player session".to_owned(),
                                }))
                                .await;
                            continue;
                        }
                    }
                    _ => ViewerRole::Spectator,
                };
                current_role = Some(role.clone());

                // Subscribe to session updates
                let subscription = session.subscribe(role.clone());

                // Send immediate snapshot upon subscription
                let snapshot = session.get_snapshot(&role);
                let _ = outbound_tx
                    .send(ServerMessage::InitialSnapshot(snapshot))
                    .await;

                // Bridge session broadcast updates to tokio outbound queue
                let tx_clone = outbound_tx.clone();
                let registry_for_updates = registry.clone();
                let game_for_updates = game_id.clone();
                let token_for_updates = current_token.clone();
                tokio::spawn(async move {
                    let mut check = tokio::time::interval(Duration::from_millis(50));
                    loop {
                        tokio::select! {
                            _ = check.tick() => {},
                            _ = tx_clone.closed() => return,
                        }
                        loop {
                            match subscription.try_recv() {
                                Ok(broadcast_msg) => {
                                    if token_for_updates.as_deref().is_some_and(|token| {
                                        registry_for_updates
                                            .authenticate_player_session(&game_for_updates, token)
                                            .is_err()
                                    }) || tx_clone.send(broadcast_msg).await.is_err()
                                    {
                                        return;
                                    }
                                }
                                Err(std::sync::mpsc::TryRecvError::Empty) => break,
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => return,
                            }
                        }
                    }
                });
            }
            ClientMessage::SubmitChoice {
                game_id: message_game_id,
                expected_version,
                nonce,
                option_id,
                ..
            } => {
                if message_game_id != game_id {
                    let _ = outbound_tx
                        .send(ServerMessage::ActionRejected(ActionRejectedMsg {
                            protocol_version: PROTOCOL_VERSION,
                            game_id: game_id.clone(),
                            game_version: expected_version,
                            reason: RejectionReason::NoPendingChoice,
                        }))
                        .await;
                    continue;
                }
                match &current_role {
                    Some(ViewerRole::Player(acting_seat)) => {
                        let Some(token) = current_token.as_deref() else {
                            let _ = outbound_tx
                                .send(ServerMessage::ActionRejected(ActionRejectedMsg {
                                    protocol_version: PROTOCOL_VERSION,
                                    game_id: game_id.clone(),
                                    game_version: expected_version,
                                    reason: RejectionReason::UnauthorizedSeat {
                                        seat: Some(acting_seat.clone()),
                                    },
                                }))
                                .await;
                            continue;
                        };
                        let registry = registry.clone();
                        let session = session.clone();
                        let game_id = game_id.clone();
                        let token = token.to_owned();
                        let acting_seat = acting_seat.clone();
                        let outbound_tx = outbound_tx.clone();
                        tokio::spawn(async move {
                            let result = tokio::task::spawn_blocking(move || {
                                registry.submit_player_choice(
                                    &game_id,
                                    &token,
                                    &acting_seat,
                                    &session,
                                    &nonce,
                                    expected_version,
                                    &option_id,
                                )
                            })
                            .await;
                            let message = match result {
                                Ok(Ok(accepted)) => ServerMessage::ActionAccepted(accepted),
                                Ok(Err(reason)) => {
                                    ServerMessage::ActionRejected(ActionRejectedMsg {
                                        protocol_version: PROTOCOL_VERSION,
                                        game_id: message_game_id.clone(),
                                        game_version: expected_version,
                                        reason,
                                    })
                                }
                                Err(_) => return,
                            };
                            let _ = outbound_tx.send(message).await;
                        });
                    }
                    Some(ViewerRole::Spectator) | None => {
                        let _ = outbound_tx
                            .send(ServerMessage::ActionRejected(ActionRejectedMsg {
                                protocol_version: PROTOCOL_VERSION,
                                game_id: game_id.clone(),
                                game_version: expected_version,
                                reason: RejectionReason::UnauthorizedSeat { seat: None },
                            }))
                            .await;
                    }
                }
            }
        }
    }

    if let (Some(ViewerRole::Player(player)), Some(connection)) = (&current_role, connection_id) {
        registry.disconnect_player(&game_id, player, connection);
    }
    if replacement_tx.is_none() {
        let _ = outbound_task.await;
    } else {
        outbound_task.abort();
    }
}
