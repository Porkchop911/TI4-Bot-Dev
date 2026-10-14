//! The secondary preview over the WebSocket: only a seated connection gets an answer, only that
//! connection gets it, requests are rate limited, and a malformed one is refused as malformed.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_server::create_app;
use ti4_server::dev::execute_launch_scenario;
use ti4_server::protocol::PROTOCOL_VERSION;
use ti4_server::protocol::client::ClientMessage;
use ti4_server::protocol::server::{
    PreviewRefusal, PreviewResult, SecondaryPreviewMsg, ServerMessage,
};
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::GameRegistry;

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn connect(addr: &str, game_id: &str, token: Option<&str>) -> Socket {
    let (mut stream, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/games/{game_id}"))
            .await
            .expect("connect");
    let subscribe = ClientMessage::Subscribe {
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        player_session: token.map(str::to_owned),
        staged: false,
    };
    stream
        .send(Message::Text(
            serde_json::to_string(&subscribe).unwrap().into(),
        ))
        .await
        .expect("subscribe");
    let reply = stream.next().await.expect("snapshot").expect("ws ok");
    assert!(matches!(
        serde_json::from_str::<ServerMessage>(reply.to_text().unwrap()).unwrap(),
        ServerMessage::InitialSnapshot(_)
    ));
    stream
}

fn request(game_id: &str, primary: &str, request_id: u64, answers: &[&str]) -> Message {
    Message::Text(
        serde_json::to_string(&ClientMessage::PreviewSecondary {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            request_id,
            card: "pok7technology".to_owned(),
            primary: primary.to_owned(),
            answers: answers.iter().map(ToString::to_string).collect(),
        })
        .unwrap()
        .into(),
    )
}

async fn next_preview(stream: &mut Socket) -> SecondaryPreviewMsg {
    loop {
        let reply = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("an answer in time")
            .expect("stream open")
            .expect("ws ok");
        match serde_json::from_str::<ServerMessage>(reply.to_text().unwrap()) {
            Ok(ServerMessage::SecondaryPreview(message)) => return message,
            Ok(ServerMessage::Error(error)) => panic!("server error: {}", error.message),
            _ => {}
        }
    }
}

fn refusal(message: &SecondaryPreviewMsg) -> PreviewRefusal {
    match &message.outcome {
        PreviewResult::Refused { reason, .. } => *reason,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn previews_are_seated_only_private_and_rate_limited() {
    let registry = Arc::new(GameRegistry::new());
    let app = create_app(registry.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    let launch =
        execute_launch_scenario(&registry, "research_tech_skips", Some(17)).expect("launch");
    let game_id = launch.game_id;
    let session = registry.get_game(&game_id).expect("game");
    let Some(ViewerRole::Player(seat)) = session.viewer_for_seat_token(&launch.player_session)
    else {
        panic!("the launch token names a seat");
    };
    let primary = seat.to_string();

    let mut owner = connect(&addr, &game_id, Some(&launch.player_session)).await;
    let mut spectator = connect(&addr, &game_id, None).await;

    // A spectator is refused, with the id it sent echoed back.
    spectator
        .send(request(&game_id, &primary, 11, &[]))
        .await
        .unwrap();
    let answer = next_preview(&mut spectator).await;
    assert_eq!(answer.request_id, 11);
    assert_eq!(refusal(&answer), PreviewRefusal::NotSeated);

    // The seat that played the card is not a follower of it.
    owner
        .send(request(&game_id, &primary, 21, &[]))
        .await
        .unwrap();
    let answer = next_preview(&mut owner).await;
    assert_eq!(answer.request_id, 21);
    assert_eq!(refusal(&answer), PreviewRefusal::IsPrimary);

    // An immediate second request on the same connection is rate limited, not evaluated.
    owner
        .send(request(&game_id, &primary, 22, &[]))
        .await
        .unwrap();
    let answer = next_preview(&mut owner).await;
    assert_eq!(answer.request_id, 22);
    assert_eq!(refusal(&answer), PreviewRefusal::RateLimited);
    tokio::time::sleep(Duration::from_millis(300)).await;
    owner
        .send(request(&game_id, &primary, 23, &[]))
        .await
        .unwrap();
    assert_eq!(
        refusal(&next_preview(&mut owner).await),
        PreviewRefusal::IsPrimary
    );

    // Nothing of the owner's answers reached the spectator's connection.
    let leaked = tokio::time::timeout(Duration::from_millis(300), async {
        loop {
            let Some(Ok(message)) = spectator.next().await else {
                return false;
            };
            if let Ok(text) = message.to_text()
                && let Ok(ServerMessage::SecondaryPreview(preview)) =
                    serde_json::from_str::<ServerMessage>(text)
                && preview.request_id != 11
            {
                return true;
            }
        }
    })
    .await;
    assert!(
        !matches!(leaked, Ok(true)),
        "the answer is private to the asking connection"
    );

    // Oversized fields are rejected before any work.
    let long = Message::Text(
        serde_json::to_string(&ClientMessage::PreviewSecondary {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            request_id: 1,
            card: "x".repeat(500),
            primary: "p1".to_owned(),
            answers: vec![],
        })
        .unwrap()
        .into(),
    );
    tokio::time::sleep(Duration::from_millis(300)).await;
    owner.send(long).await.unwrap();
    loop {
        let reply = owner.next().await.expect("reply").expect("ws ok");
        if let Ok(ServerMessage::Error(error)) =
            serde_json::from_str::<ServerMessage>(reply.to_text().unwrap())
        {
            assert_eq!(
                error.kind,
                ti4_server::protocol::error::ErrorKind::MalformedMessage
            );
            break;
        }
    }
}

#[test]
fn the_message_round_trips_through_the_server_message_envelope() {
    let refused = SecondaryPreviewMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "g".to_owned(),
        request_id: 3,
        as_of_version: 9,
        as_of_decisions: 4,
        card: "pok7technology".to_owned(),
        outcome: PreviewResult::Refused {
            reason: PreviewRefusal::AlreadyAsked,
            detail: "d".to_owned(),
        },
    };
    let message = ServerMessage::SecondaryPreview(refused.clone());
    let text = serde_json::to_string(&message).unwrap();
    assert_eq!(
        serde_json::from_str::<ServerMessage>(&text).unwrap(),
        message
    );
    let ok = ServerMessage::SecondaryPreview(SecondaryPreviewMsg {
        outcome: PreviewResult::Preview {
            preview: ti4_engine::secondary_preview::SecondaryPreview::Complete {
                skipped: vec![],
                unused_answers: 0,
            },
        },
        ..refused
    });
    let text = serde_json::to_string(&ok).unwrap();
    assert_eq!(serde_json::from_str::<ServerMessage>(&text).unwrap(), ok);
}
