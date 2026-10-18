//! WebSocket connection lifecycle and routing for live multiplayer sessions.

pub mod table;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::mpsc;
use tracing::{debug, warn};

use ti4_host::preview::PreviewRequest;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::error::ErrorKind;
use ti4_host::protocol::server::{
    ActionRejectedMsg, DraftAppliedMsg, DraftOutcomeMsg, PongMsg, PreviewRefusal, PreviewResult,
    ProtocolErrorMsg, SecondaryPreviewMsg, ServerMessage,
};
use ti4_host::protocol::status::{RejectionReason, ViewerRole};

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
/// Maximum complete client WebSocket message, including JSON framing. A script of a draft is
/// the largest (`RunDraft`, `ApplyDraft`); the local game gives its requests the same room.
pub const MAX_CLIENT_MESSAGE_BYTES: usize = 64 * 1024;

/// The newest draft request of a connection: its `request_id` and its script.
type DraftRequest = Option<(
    String,
    ti4_host::planning::audit::PlanScope,
    Vec<ti4_host::planning::draft::DraftStep>,
)>;
/// Least time between two reaction declarations on one connection.
const MIN_INTENT_INTERVAL: Duration = Duration::from_millis(500);
/// Most staged writes of one connection in [`STAGED_WINDOW`]. A page debounces its writes;
/// this only bounds a page that does not.
const MAX_STAGED_PUTS: usize = 20;
const STAGED_WINDOW: Duration = Duration::from_secs(1);
/// Least time between two secondary previews on one connection (and only one runs at a time).
const MIN_PREVIEW_INTERVAL: Duration = Duration::from_millis(200);
/// Least time between two "who can go here" queries on one connection.
const MIN_REACH_INTERVAL: Duration = Duration::from_millis(50);

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
    let pump_registry = registry.clone();
    let pump_game = game_id.clone();
    let outbound_credential = Arc::new(std::sync::Mutex::new(None::<String>));
    let credential_for_pump = outbound_credential.clone();

    // Outbound pump task: forwards ServerMessage as JSON text to WebSocket sink
    let outbound_task = tokio::spawn(async move {
        loop {
            let msg = tokio::select! {
                msg = outbound_rx.recv() => match msg { Some(msg) => msg, None => break },
                result = &mut replacement_rx => {
                    if result.is_ok() {
                        let (code, reason) = if pump_registry.is_terminated(&pump_game) {
                            (4002, "game terminated")
                        } else {
                            (4001, "session replaced")
                        };
                        let _ = ws_sender.send(Message::Close(Some(CloseFrame {
                            code,
                            reason: reason.into(),
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
    let mut last_intent: Option<std::time::Instant> = None;
    let mut last_preview: Option<std::time::Instant> = None;
    let mut last_reach: Option<std::time::Instant> = None;
    // The staged writes of this connection in the last window, and whether it asked for the
    // staged work of its seat.
    let mut staged_puts: std::collections::VecDeque<std::time::Instant> =
        std::collections::VecDeque::new();
    let mut staged_wanted = false;
    // The nonces that this connection answered, newest last: an answer that is refused is told
    // apart from one that another connection of the seat gave (`answered_elsewhere`).
    let mut own_answers: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    let preview_running = Arc::new(AtomicBool::new(false));
    let mut auth_check = tokio::time::interval(Duration::from_millis(250));
    // The draft requests of this connection. One is run at a time and only the newest waits,
    // so a page that sends a script for each change cannot queue work.
    let mut drafts: Option<tokio::sync::watch::Sender<DraftRequest>> = None;
    let idle_timeout = registry.public().config.ws_idle_timeout;
    let mut last_inbound = std::time::Instant::now();
    // Set when the session dropped this connection's subscription (its queue filled because the
    // page read too slowly). The socket must close then, so the page reconnects for a fresh
    // snapshot; left open it would answer pings and never carry another update.
    let subscription_lost = Arc::new(tokio::sync::Notify::new());

    // Inbound processing loop
    loop {
        let msg_result = tokio::select! {
            msg = ws_receiver.next() => match msg { Some(msg) => msg, None => break },
            () = subscription_lost.notified() => break,
            _ = auth_check.tick() => {
                if session.error().is_some() {
                    break;
                }
                if idle_timeout.is_some_and(|limit| last_inbound.elapsed() > limit) {
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
        last_inbound = std::time::Instant::now();
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
            ClientMessage::RunDraft {
                game_id: message_game_id,
                request_id,
                script,
                scope,
                ..
            } => {
                let (Some(ViewerRole::Player(player)), Some(token), true) =
                    (&current_role, &current_token, message_game_id == game_id)
                else {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind: ErrorKind::Unauthorized,
                            message: "only a seated player of this game can run a draft".to_owned(),
                        }))
                        .await;
                    continue;
                };
                let requests = drafts.get_or_insert_with(|| {
                    let (requests, mut newest) = tokio::sync::watch::channel::<DraftRequest>(None);
                    let registry = registry.clone();
                    let session = session.clone();
                    let game_id = game_id.clone();
                    let token = token.clone();
                    let player = player.clone();
                    let outbound_tx = outbound_tx.clone();
                    tokio::spawn(async move {
                        while newest.changed().await.is_ok() {
                            let Some((request_id, scope, script)) =
                                newest.borrow_and_update().clone()
                            else {
                                continue;
                            };
                            let run = {
                                let registry = registry.clone();
                                let session = session.clone();
                                let game_id = game_id.clone();
                                let token = token.clone();
                                let player = player.clone();
                                tokio::task::spawn_blocking(move || {
                                    registry.run_player_draft(
                                        &game_id, &token, &player, &session, &scope, &script,
                                    )
                                })
                            };
                            let Ok(Some((base_version, outcome))) = run.await else {
                                return;
                            };
                            let message = ServerMessage::DraftOutcome(DraftOutcomeMsg {
                                protocol_version: PROTOCOL_VERSION,
                                game_id: game_id.clone(),
                                request_id,
                                base_version,
                                outcome: Box::new(outcome),
                            });
                            if outbound_tx.send(message).await.is_err() {
                                return;
                            }
                        }
                    });
                    requests
                });
                let _ = requests.send(Some((request_id, scope, script)));
            }
            ClientMessage::ApplyDraft {
                game_id: message_game_id,
                nonce,
                expected_version,
                script,
                ..
            } => {
                let (Some(ViewerRole::Player(player)), Some(token), true) =
                    (&current_role, &current_token, message_game_id == game_id)
                else {
                    let _ = outbound_tx
                        .send(ServerMessage::ActionRejected(ActionRejectedMsg {
                            protocol_version: PROTOCOL_VERSION,
                            game_id: game_id.clone(),
                            game_version: expected_version,
                            reason: RejectionReason::UnauthorizedSeat { seat: None },
                        }))
                        .await;
                    continue;
                };
                let elsewhere = answered_elsewhere(&mut own_answers, &nonce, staged_wanted);
                let registry = registry.clone();
                let session = session.clone();
                let game_id = game_id.clone();
                let token = token.clone();
                let player = player.clone();
                let outbound_tx = outbound_tx.clone();
                tokio::spawn(async move {
                    let applied = {
                        let player = player.clone();
                        let session = session.clone();
                        let game_id = game_id.clone();
                        let nonce = nonce.clone();
                        tokio::task::spawn_blocking(move || {
                            registry.apply_player_draft(
                                &game_id,
                                &token,
                                &player,
                                &session,
                                &nonce,
                                expected_version,
                                &script,
                            )
                        })
                    };
                    let message = match applied.await {
                        // The script is in the game: the seat is told where it ends.
                        Ok(Ok(None)) => return,
                        Ok(Ok(Some(stopped))) => ServerMessage::DraftApplied(DraftAppliedMsg {
                            protocol_version: PROTOCOL_VERSION,
                            game_id,
                            nonce,
                            stopped: Some(stopped),
                        }),
                        Ok(Err(reason)) => ServerMessage::ActionRejected(ActionRejectedMsg {
                            protocol_version: PROTOCOL_VERSION,
                            game_id,
                            game_version: expected_version,
                            reason: elsewhere(&session, &player, &nonce).unwrap_or(reason),
                        }),
                        Err(error) => {
                            warn!(%game_id, %nonce, %error, "apply_draft task failed");
                            return;
                        }
                    };
                    let _ = outbound_tx.send(message).await;
                });
            }
            ClientMessage::Undo {
                game_id: message_game_id,
                expected_version,
                ..
            } => {
                let rejected = |reason| {
                    ServerMessage::ActionRejected(ActionRejectedMsg {
                        protocol_version: PROTOCOL_VERSION,
                        game_id: game_id.clone(),
                        game_version: expected_version,
                        reason,
                    })
                };
                let (Some(ViewerRole::Player(player)), Some(token), true) =
                    (&current_role, &current_token, message_game_id == game_id)
                else {
                    let _ = outbound_tx
                        .send(rejected(RejectionReason::UnauthorizedSeat { seat: None }))
                        .await;
                    continue;
                };
                // The game is played again without the decision: this connection waits for it.
                let undone = {
                    let registry = registry.clone();
                    let session = session.clone();
                    let game_id = game_id.clone();
                    let token = token.clone();
                    let player = player.clone();
                    tokio::task::spawn_blocking(move || {
                        registry.undo_player(&game_id, &token, &player, &session, expected_version)
                    })
                    .await
                };
                match undone {
                    Ok(Ok(())) => {
                        // The game is another session now. The page subscribes to it again,
                        // and its snapshot has what was taken back.
                        if let Some(tx) = replacement_tx.take() {
                            let _ = tx.send(());
                        }
                        break;
                    }
                    Ok(Err(reason)) => {
                        let _ = outbound_tx.send(rejected(reason)).await;
                    }
                    Err(error) => warn!(%game_id, %error, "undo task failed"),
                }
            }
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
                staged,
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
                let mut snapshot = session.get_snapshot(&role);
                if let ViewerRole::Player(seat) = &role {
                    snapshot.undone = session.undone_for(seat);
                }
                let _ = outbound_tx
                    .send(ServerMessage::InitialSnapshot(snapshot))
                    .await;
                if let (ViewerRole::Player(seat), true) = (&role, staged) {
                    staged_wanted = true;
                    let mut message = session.seat_staged(seat).message(&game_id);
                    message.connection = connection_id;
                    let _ = outbound_tx.send(ServerMessage::SeatStaged(message)).await;
                }

                // Bridge session broadcast updates to tokio outbound queue
                let tx_clone = outbound_tx.clone();
                let registry_for_updates = registry.clone();
                let game_for_updates = game_id.clone();
                let token_for_updates = current_token.clone();
                let staged_for_updates = staged_wanted;
                let lost = subscription_lost.clone();
                tokio::spawn(async move {
                    let mut check = tokio::time::interval(Duration::from_millis(50));
                    loop {
                        tokio::select! {
                            _ = check.tick() => {},
                            _ = tx_clone.closed() => return,
                        }
                        loop {
                            match subscription.try_recv() {
                                Ok(ServerMessage::SeatStaged(_)) if !staged_for_updates => {}
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
                                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                                    lost.notify_one();
                                    return;
                                }
                            }
                        }
                    }
                });
            }
            ClientMessage::SetReactionMode {
                game_id: message_game_id,
                card,
                mode,
                ..
            } => {
                let refusal = if message_game_id != game_id {
                    Some((
                        ErrorKind::MalformedMessage,
                        "set_reaction_mode game_id does not match the WebSocket path".to_owned(),
                    ))
                } else {
                    match &current_role {
                        // The seat is the connection's own, never taken from the message.
                        Some(ViewerRole::Player(seat)) => session
                            .set_reaction_mode(seat, &card, mode)
                            .err()
                            .map(|message| (ErrorKind::MalformedMessage, message)),
                        Some(ViewerRole::Spectator) | None => Some((
                            ErrorKind::Unauthorized,
                            "only a seated player can change reaction modes".to_owned(),
                        )),
                    }
                };
                if let Some((kind, message)) = refusal {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind,
                            message,
                        }))
                        .await;
                }
            }
            ClientMessage::PutStaged {
                game_id: message_game_id,
                slot,
                base_revision,
                value,
                ..
            } => {
                let now = std::time::Instant::now();
                while staged_puts
                    .front()
                    .is_some_and(|at| now.duration_since(*at) >= STAGED_WINDOW)
                {
                    staged_puts.pop_front();
                }
                let refusal = if message_game_id != game_id {
                    Some((
                        ErrorKind::MalformedMessage,
                        "put_staged game_id does not match the WebSocket path".to_owned(),
                    ))
                } else {
                    match (&current_role, connection_id) {
                        // The seat is the connection's own, never taken from the message.
                        (Some(ViewerRole::Player(seat)), Some(connection)) => {
                            if staged_puts.len() >= MAX_STAGED_PUTS {
                                Some((
                                    ErrorKind::MalformedMessage,
                                    "too many staged writes; wait a moment".to_owned(),
                                ))
                            } else {
                                staged_puts.push_back(now);
                                match session.put_staged(
                                    seat,
                                    slot,
                                    base_revision,
                                    value,
                                    connection,
                                ) {
                                    Ok(ti4_host::staged::Put::Stale) => {
                                        // Nothing is written: the connection takes what is.
                                        let _ = outbound_tx
                                            .send(ServerMessage::SeatStaged(
                                                session.seat_staged(seat).message(&game_id),
                                            ))
                                            .await;
                                        None
                                    }
                                    Ok(ti4_host::staged::Put::Written)
                                        if slot
                                            == ti4_host::protocol::staged::StagedSlot::Secondary =>
                                    {
                                        // A draft that is marked ready while the game asks the
                                        // seat is sent now.
                                        if let Some(token) = current_token.clone() {
                                            let registry = registry.clone();
                                            let session = session.clone();
                                            let game_id = game_id.clone();
                                            let seat = seat.clone();
                                            tokio::task::spawn_blocking(move || {
                                                registry.send_ready_secondary(
                                                    &game_id, &token, &seat, &session,
                                                );
                                            });
                                        }
                                        None
                                    }
                                    Ok(_) => None,
                                    Err(message) => Some((ErrorKind::EngineError, message)),
                                }
                            }
                        }
                        _ => Some((
                            ErrorKind::Unauthorized,
                            "only a seated player can stage work".to_owned(),
                        )),
                    }
                };
                if let Some((kind, message)) = refusal {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind,
                            message,
                        }))
                        .await;
                    // What was not written is not there: the connection takes what is.
                    if let Some(ViewerRole::Player(seat)) = &current_role {
                        let _ = outbound_tx
                            .send(ServerMessage::SeatStaged(
                                session.seat_staged(seat).message(&game_id),
                            ))
                            .await;
                    }
                }
            }
            ClientMessage::SetReactionIntent {
                game_id: message_game_id,
                triggers,
                ..
            } => {
                let refusal = if message_game_id != game_id {
                    Some((
                        ErrorKind::MalformedMessage,
                        "set_reaction_intent game_id does not match the WebSocket path".to_owned(),
                    ))
                } else if last_intent
                    .is_some_and(|at: std::time::Instant| at.elapsed() < MIN_INTENT_INTERVAL)
                {
                    Some((
                        ErrorKind::MalformedMessage,
                        "too many reaction declarations; wait a moment".to_owned(),
                    ))
                } else {
                    last_intent = Some(std::time::Instant::now());
                    match &current_role {
                        // The seat is the connection's own, never taken from the message.
                        Some(ViewerRole::Player(seat)) => session
                            .set_reaction_intent(seat, &triggers)
                            .err()
                            .map(|message| (ErrorKind::MalformedMessage, message)),
                        Some(ViewerRole::Spectator) | None => Some((
                            ErrorKind::Unauthorized,
                            "only a seated player can declare reactions".to_owned(),
                        )),
                    }
                };
                if let Some((kind, message)) = refusal {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind,
                            message,
                        }))
                        .await;
                    // Re-sync the seat's own view of its settings after a refusal.
                    if let Some(ViewerRole::Player(seat)) = &current_role {
                        let _ = outbound_tx
                            .send(ServerMessage::ReactionIntentState(
                                session.reaction_intent_state(seat),
                            ))
                            .await;
                    }
                }
            }
            ClientMessage::PreviewSecondary {
                game_id: message_game_id,
                request_id,
                card,
                primary,
                answers,
                ..
            } => {
                if message_game_id != game_id {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind: ErrorKind::MalformedMessage,
                            message: "preview_secondary game_id does not match the WebSocket path"
                                .to_owned(),
                        }))
                        .await;
                    continue;
                }
                let refused = |reason: PreviewRefusal, detail: String| {
                    ServerMessage::SecondaryPreview(SecondaryPreviewMsg {
                        protocol_version: PROTOCOL_VERSION,
                        game_id: game_id.clone(),
                        request_id,
                        as_of_version: session.game_version(),
                        as_of_decisions: 0,
                        card: card.clone(),
                        outcome: PreviewResult::Refused { reason, detail },
                    })
                };
                // The seat is the connection's own, never taken from the message.
                let Some(ViewerRole::Player(seat)) = current_role.clone() else {
                    let _ = outbound_tx
                        .send(refused(
                            PreviewRefusal::NotSeated,
                            "only a seated player can preview a secondary".to_owned(),
                        ))
                        .await;
                    continue;
                };
                // Bounded request rate: one at a time, and not more often than the interval.
                let too_soon = last_preview
                    .is_some_and(|at: std::time::Instant| at.elapsed() < MIN_PREVIEW_INTERVAL);
                if too_soon || preview_running.swap(true, Ordering::AcqRel) {
                    // (A request that found one running did not take the flag; one that was
                    // merely too soon never touched it.)
                    let _ = outbound_tx
                        .send(refused(
                            PreviewRefusal::RateLimited,
                            format!(
                                "too many previews; retry in {} ms",
                                MIN_PREVIEW_INTERVAL.as_millis()
                            ),
                        ))
                        .await;
                    continue;
                }
                last_preview = Some(std::time::Instant::now());
                let request = PreviewRequest {
                    request_id,
                    card,
                    primary,
                    answers,
                };
                let session = session.clone();
                let outbound_tx = outbound_tx.clone();
                let running = preview_running.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        session.preview_secondary(&seat, &request)
                    })
                    .await;
                    running.store(false, Ordering::Release);
                    if let Ok(message) = result {
                        let _ = outbound_tx
                            .send(ServerMessage::SecondaryPreview(message))
                            .await;
                    }
                });
            }
            ClientMessage::QueryReach {
                game_id: message_game_id,
                request_id,
                system_id,
                ..
            } => {
                if message_game_id != game_id {
                    let _ = outbound_tx
                        .send(ServerMessage::Error(ProtocolErrorMsg {
                            protocol_version: PROTOCOL_VERSION,
                            kind: ErrorKind::MalformedMessage,
                            message: "query_reach game_id does not match the WebSocket path"
                                .to_owned(),
                        }))
                        .await;
                    continue;
                }
                // Bounded request rate; the answer is cheap but a client could ask in a loop.
                if last_reach
                    .is_some_and(|at: std::time::Instant| at.elapsed() < MIN_REACH_INTERVAL)
                {
                    let _ = outbound_tx
                        .send(ServerMessage::ReachInto(
                            ti4_host::protocol::reach::ReachIntoMsg {
                                protocol_version: PROTOCOL_VERSION,
                                game_id: game_id.clone(),
                                request_id,
                                system_id,
                                as_of_version: session.game_version(),
                                origins: Vec::new(),
                                refused: Some(ti4_host::protocol::reach::ReachRefusal::RateLimited),
                            },
                        ))
                        .await;
                    continue;
                }
                last_reach = Some(std::time::Instant::now());
                let session = session.clone();
                let outbound_tx = outbound_tx.clone();
                tokio::spawn(async move {
                    let result = tokio::task::spawn_blocking(move || {
                        session.reach_into(request_id, &system_id)
                    })
                    .await;
                    if let Ok(message) = result {
                        let _ = outbound_tx.send(ServerMessage::ReachInto(message)).await;
                    }
                });
            }
            ClientMessage::PassReactionHold {
                game_id: message_game_id,
                ..
            } => {
                // Silent: a Pass from anyone but the held seat, or with nothing held, does nothing.
                if message_game_id == game_id {
                    if let Some(ViewerRole::Player(seat)) = &current_role {
                        session.pass_reaction_hold(seat);
                    }
                }
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
                        let elsewhere = answered_elsewhere(&mut own_answers, &nonce, staged_wanted);
                        let registry = registry.clone();
                        let session = session.clone();
                        let game_id = game_id.clone();
                        let token = token.to_owned();
                        let acting_seat = acting_seat.clone();
                        let outbound_tx = outbound_tx.clone();
                        tokio::spawn(async move {
                            debug!(%game_id, %nonce, expected_version, %option_id, "submit_choice received");
                            let log_game_id = game_id.clone();
                            let log_nonce = nonce.clone();
                            let seat = acting_seat.clone();
                            let asked = session.clone();
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
                                Ok(Ok(accepted)) => {
                                    debug!(game_id = %log_game_id, nonce = %log_nonce, "submit_choice accepted");
                                    ServerMessage::ActionAccepted(accepted)
                                }
                                Ok(Err(reason)) => {
                                    debug!(game_id = %log_game_id, nonce = %log_nonce, ?reason, "submit_choice rejected");
                                    ServerMessage::ActionRejected(ActionRejectedMsg {
                                        protocol_version: PROTOCOL_VERSION,
                                        game_id: message_game_id.clone(),
                                        game_version: expected_version,
                                        reason: elsewhere(&asked, &seat, &log_nonce)
                                            .unwrap_or(reason),
                                    })
                                }
                                Err(error) => {
                                    // The client gets no reply at all in this case.
                                    warn!(game_id = %log_game_id, nonce = %log_nonce, %error, "submit_choice task failed");
                                    return;
                                }
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

/// How many nonces a connection keeps of its own answers.
const OWN_ANSWERS_KEPT: usize = 16;

/// What an answer to `nonce` that the game refuses is told, when the seat answered it already
/// from another connection (or the server sent a ready draft for it): `answered_elsewhere` to a
/// connection that asked for the staged work of its seat (web2 shows no toast for it), and
/// `stale_nonce` to every other one (web shows a text for each reason it knows). `None` for a
/// refusal of anything else. Notes `nonce` as an answer of this connection.
fn answered_elsewhere(
    own_answers: &mut std::collections::VecDeque<String>,
    nonce: &str,
    staged_wanted: bool,
) -> impl FnOnce(&GameSession, &ti4_model::id::PlayerId, &str) -> Option<RejectionReason> + use<> {
    let again = own_answers.iter().any(|own| own == nonce);
    if !again {
        if own_answers.len() >= OWN_ANSWERS_KEPT {
            own_answers.pop_front();
        }
        own_answers.push_back(nonce.to_owned());
    }
    move |session, seat, nonce| {
        (!again && session.was_answered(seat, nonce)).then_some(if staged_wanted {
            RejectionReason::AnsweredElsewhere
        } else {
            RejectionReason::StaleNonce
        })
    }
}
