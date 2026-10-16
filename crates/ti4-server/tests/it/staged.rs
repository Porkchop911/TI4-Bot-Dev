//! What a seat has staged (`ClientMessage::PutStaged`, `ServerMessage::SeatStaged`): kept by the
//! server for the seat, the same in every connection of the seat, and nobody else's.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_model::id::PlayerId;
use ti4_server::protocol::PROTOCOL_VERSION;
use ti4_server::protocol::client::{ClientMessage, MAX_STAGED_VALUE_BYTES};
use ti4_server::protocol::error::ErrorKind;
use ti4_server::protocol::server::{SeatStagedMsg, ServerMessage};
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::staged::{Put, StagedSlot};
use ti4_server::session::{GameRegistry, MockClient};
use ti4_server::storage::FileGameStore;

use crate::draft_scripts::{asked, movement_script, next_message, passive, to_the_turn_menu};

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

fn store() -> Arc<FileGameStore> {
    let dir = std::env::temp_dir().join(format!("ti4_staged_{:032x}", rand::random::<u128>()));
    Arc::new(FileGameStore::new(&dir).expect("a store"))
}

/// A game of six on the server: the first seat is played here, the others at random at once.
fn game(registry: &GameRegistry, game_id: &str) -> (PlayerId, String) {
    let (_, seat, credential) = registry
        .create_player_game_with_random_seats(
            game_id.to_owned(),
            6,
            3,
            "Host",
            None,
            None,
            ti4_server::card_set::DEFAULT,
            ti4_server::map::SpeakerOrder::Seat,
            ti4_server::session::table::SeatOrder::Join,
            Some(0),
        )
        .expect("a started game");
    (seat, credential.as_str().to_owned())
}

fn text(message: &ClientMessage) -> Message {
    Message::Text(serde_json::to_string(message).unwrap().into())
}

async fn connect(url: &str, game_id: &str, credential: Option<&str>, staged: bool) -> Socket {
    let (mut socket, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    socket
        .send(text(&ClientMessage::Subscribe {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            player_session: credential.map(str::to_owned),
            staged,
        }))
        .await
        .unwrap();
    next_message(&mut socket, |message| {
        matches!(message, ServerMessage::InitialSnapshot(_))
    })
    .await;
    socket
}

async fn staged(socket: &mut Socket) -> SeatStagedMsg {
    let ServerMessage::SeatStaged(message) = next_message(socket, |message| {
        matches!(message, ServerMessage::SeatStaged(_))
    })
    .await
    else {
        unreachable!()
    };
    message
}

async fn put(
    socket: &mut Socket,
    game_id: &str,
    slot: StagedSlot,
    base_revision: u64,
    value: Option<serde_json::Value>,
) {
    socket
        .send(text(&ClientMessage::PutStaged {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            slot,
            base_revision,
            value,
        }))
        .await
        .unwrap();
}

/// The messages of `socket` until the answer to a ping, after the bridges of the server had the
/// time to forward what they have.
async fn drain(socket: &mut Socket) -> Vec<ServerMessage> {
    tokio::time::sleep(Duration::from_millis(300)).await;
    socket
        .send(text(&ClientMessage::Ping {
            protocol_version: PROTOCOL_VERSION,
            sequence: 7,
        }))
        .await
        .unwrap();
    let mut seen = Vec::new();
    let found = async {
        while let Some(Ok(message)) = socket.next().await {
            if let Ok(text) = message.to_text()
                && let Ok(parsed) = serde_json::from_str::<ServerMessage>(text)
            {
                if matches!(parsed, ServerMessage::Pong(_)) {
                    return;
                }
                seen.push(parsed);
            }
        }
    };
    tokio::time::timeout(Duration::from_secs(10), found)
        .await
        .expect("the pong arrives");
    seen
}

fn no_staged(messages: &[ServerMessage]) -> bool {
    !messages
        .iter()
        .any(|message| matches!(message, ServerMessage::SeatStaged(_)))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_tabs_of_a_seat_share_what_is_staged_and_nobody_else_sees_it() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    let store = store();
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    let game_id = "staged_tabs";
    let (seat, credential) = game(&registry, game_id);
    let app = ti4_server::create_app(registry.clone());
    let server =
        tokio::spawn(async move { axum::serve(listener, app).await.expect("the server runs") });
    let url = format!("ws://{address}/ws/games/{game_id}");

    // Each tab is told what is staged, and its own id.
    let mut a = connect(&url, game_id, Some(&credential), true).await;
    let first = staged(&mut a).await;
    assert_eq!(first.revision, 0);
    assert!(first.slots.is_empty());
    let a_id = first.connection.expect("the id of the connection");
    let mut b = connect(&url, game_id, Some(&credential), true).await;
    let b_id = staged(&mut b).await.connection.expect("an id");
    assert_ne!(a_id, b_id);
    // A tab that did not ask, a spectator, and another seat get nothing.
    let mut old_page = connect(&url, game_id, Some(&credential), false).await;
    let mut spectator = connect(&url, game_id, None, true).await;
    let session = registry.get_game(game_id).expect("the game runs");
    let other = session
        .get_snapshot(&ViewerRole::Spectator)
        .view
        .players
        .iter()
        .map(|player| player.id.clone())
        .find(|player| *player != seat)
        .expect("another seat");
    let other_client = MockClient::connect(session.clone(), ViewerRole::Player(other));

    // A writes the draft: B is told, with A as the writer, and A gets its echo.
    let draft = serde_json::json!({ "draft": { "script": ["tactical"], "undo": [] } });
    put(&mut a, game_id, StagedSlot::Plan, 0, Some(draft.clone())).await;
    let seen = staged(&mut b).await;
    assert_eq!(seen.writer, Some(a_id));
    assert_eq!(seen.connection, None);
    assert_eq!(seen.slots[&StagedSlot::Plan].value, Some(draft.clone()));
    let revision = seen.slots[&StagedSlot::Plan].revision;
    assert_eq!(seen.revision, revision);
    assert_eq!(staged(&mut a).await, seen);

    // B writes on the old revision: nothing is written, and B is told what is there.
    put(
        &mut b,
        game_id,
        StagedSlot::Plan,
        0,
        Some(serde_json::json!({ "draft": "older" })),
    )
    .await;
    let refused = staged(&mut b).await;
    assert_eq!(refused.writer, None);
    assert_eq!(refused.slots[&StagedSlot::Plan].value, Some(draft.clone()));
    assert_eq!(refused.revision, revision);
    // On the revision that it was told, B's write is taken.
    let secondary = serde_json::json!({ "answers": ["follow"], "ready": false });
    put(
        &mut b,
        game_id,
        StagedSlot::Secondary,
        0,
        Some(secondary.clone()),
    )
    .await;
    let seen = staged(&mut a).await;
    assert_eq!(seen.writer, Some(b_id));
    assert_eq!(seen.slots[&StagedSlot::Plan].value, Some(draft.clone()));
    assert_eq!(
        seen.slots[&StagedSlot::Secondary].value,
        Some(secondary.clone())
    );
    assert!(seen.revision > revision);

    // A value over the bound is refused before it reaches the seat.
    let oversized = serde_json::json!({ "pad": "x".repeat(MAX_STAGED_VALUE_BYTES) });
    put(&mut a, game_id, StagedSlot::Movement, 0, Some(oversized)).await;
    let ServerMessage::Error(error) =
        next_message(&mut a, |message| matches!(message, ServerMessage::Error(_))).await
    else {
        unreachable!()
    };
    assert_eq!(error.kind, ErrorKind::MalformedMessage);
    assert!(error.message.contains("value"), "{}", error.message);
    assert!(
        !session
            .seat_staged(&seat)
            .slots
            .contains_key(&StagedSlot::Movement)
    );
    // A spectator cannot write.
    put(&mut spectator, game_id, StagedSlot::Plan, 0, None).await;
    let ServerMessage::Error(error) = next_message(&mut spectator, |message| {
        matches!(message, ServerMessage::Error(_))
    })
    .await
    else {
        unreachable!()
    };
    assert_eq!(error.kind, ErrorKind::Unauthorized);

    assert!(no_staged(&drain(&mut old_page).await));
    assert!(no_staged(&drain(&mut spectator).await));
    let mut of_other = Vec::new();
    while let Ok(message) = other_client.try_recv() {
        of_other.push(message);
    }
    assert!(no_staged(&of_other));

    // A tab that comes later is told everything.
    let mut c = connect(&url, game_id, Some(&credential), true).await;
    let late = staged(&mut c).await;
    assert_eq!(late.slots, seen.slots);

    // The server is restarted: the recovered game has what was staged.
    let kept = session.seat_staged(&seat);
    drop((a, b, c, old_page, spectator, other_client));
    server.abort();
    session.stop();
    drop((session, registry));
    let restarted = Arc::new(GameRegistry::new().with_store(store));
    let report = restarted.recover_all_games_report().expect("recovered");
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let recovered = restarted.get_game(game_id).expect("the game is back");
    assert_eq!(recovered.seat_staged(&seat), kept);
    assert_eq!(
        recovered.seat_staged(&seat).slots[&StagedSlot::Secondary].value,
        Some(secondary)
    );
    recovered.stop();
}

#[test]
fn what_is_staged_and_what_was_undone_outlive_an_undo() {
    let registry = GameRegistry::new().with_store(store());
    let game_id = "staged_undo";
    let (seat, credential) = game(&registry, game_id);
    let session = registry.get_game(game_id).expect("the game runs");
    let (_, nonce, version) = to_the_turn_menu(&session, &seat);
    let (script, _, _) = movement_script(&session, &seat);
    let movement = serde_json::json!({ "nonce": "n1", "moves": [1, 2] });
    assert_eq!(
        session.put_staged(&seat, StagedSlot::Movement, 0, Some(movement.clone()), 1),
        Ok(Put::Written)
    );
    assert_eq!(
        session.apply_draft(&seat, &nonce, version, &script),
        Ok(None)
    );
    let (_, _, version) = asked(&session, &seat);

    registry
        .undo_player(game_id, &credential, &seat, &session, version)
        .expect("the undo is taken");
    let replaced = registry.get_game(game_id).expect("the game runs");
    assert!(!Arc::ptr_eq(&replaced, &session));
    assert_eq!(
        replaced.seat_staged(&seat).slots[&StagedSlot::Movement].value,
        Some(movement.clone())
    );
    // A write to the session that was replaced is not lost: the two share what is staged.
    assert_eq!(
        session.put_staged(&seat, StagedSlot::Plan, 0, Some(serde_json::json!(1)), 1),
        Ok(Put::Written)
    );
    assert!(
        replaced
            .seat_staged(&seat)
            .slots
            .contains_key(&StagedSlot::Plan)
    );

    // Every connection of the seat is told the steps, until the seat decides again.
    let (choice, nonce, version) = asked(&replaced, &seat);
    let undone = replaced.undone_for(&seat);
    assert!(!undone.is_empty());
    assert_eq!(replaced.undone_for(&seat), undone);
    let decisions = replaced.decision_log().len();
    replaced
        .submit_choice(&seat, &nonce, version, &passive(&choice))
        .expect("the answer is taken");
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while replaced.decision_log().len() == decisions {
        assert!(
            std::time::Instant::now() < deadline,
            "the answer is recorded"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(replaced.undone_for(&seat).is_empty());
    replaced.stop();
}
