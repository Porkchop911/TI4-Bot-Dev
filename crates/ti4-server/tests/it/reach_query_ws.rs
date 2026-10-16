//! The "who can go here" query over the WebSocket: open to every viewer (spectators included),
//! answered only to the asker, echoing the request id, refusing unknown systems and oversized
//! fields, and serialising compactly.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::reach::{ReachIntoMsg, ReachRefusal};
use ti4_host::protocol::server::ServerMessage;
use ti4_server::create_app;
use ti4_server::dev::launch_scenario;
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

fn query(game_id: &str, request_id: u64, system_id: &str) -> Message {
    Message::Text(
        serde_json::to_string(&ClientMessage::QueryReach {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            request_id,
            system_id: system_id.to_owned(),
        })
        .unwrap()
        .into(),
    )
}

async fn next_reach(stream: &mut Socket) -> ReachIntoMsg {
    loop {
        let reply = tokio::time::timeout(Duration::from_secs(5), stream.next())
            .await
            .expect("an answer in time")
            .expect("stream open")
            .expect("ws ok");
        match serde_json::from_str::<ServerMessage>(reply.to_text().unwrap()) {
            Ok(ServerMessage::ReachInto(message)) => return message,
            Ok(ServerMessage::Error(error)) => panic!("server error: {}", error.message),
            _ => {}
        }
    }
}

#[tokio::test]
async fn any_viewer_can_ask_and_unknown_systems_are_refused() {
    let registry = Arc::new(GameRegistry::new());
    let app = create_app(registry.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    let launch = launch_scenario(&registry, "research_tech_skips", Some(17)).expect("launch");
    let game_id = launch.game_id;

    let mut spectator = connect(&addr, &game_id, None).await;
    spectator.send(query(&game_id, 5, "18")).await.unwrap();
    let mut answer = next_reach(&mut spectator).await;
    for _ in 0..40 {
        // The worker publishes the map a moment after the game opens.
        if answer.refused != Some(ReachRefusal::Busy) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        spectator.send(query(&game_id, 5, "18")).await.unwrap();
        answer = next_reach(&mut spectator).await;
    }
    assert_eq!(answer.request_id, 5);
    assert_eq!(answer.system_id, "18");
    assert_eq!(answer.refused, None);
    for origin in &answer.origins {
        assert_ne!(origin.origin, "18", "ships already there are not listed");
        assert!(origin.ships.iter().all(|ship| ship.count > 0));
    }
    // Compact: the whole table's reach into Mecatol is a few hundred bytes.
    let size = serde_json::to_string(&ServerMessage::ReachInto(answer))
        .unwrap()
        .len();
    assert!(size < 4_000, "payload {size} bytes");

    tokio::time::sleep(Duration::from_millis(100)).await;
    spectator.send(query(&game_id, 6, "nope")).await.unwrap();
    let unknown = next_reach(&mut spectator).await;
    assert_eq!(unknown.request_id, 6);
    assert_eq!(unknown.refused, Some(ReachRefusal::UnknownSystem));
    assert!(unknown.origins.is_empty());

    // Oversized fields are rejected as malformed before any work.
    tokio::time::sleep(Duration::from_millis(100)).await;
    spectator
        .send(query(&game_id, 7, &"x".repeat(200)))
        .await
        .unwrap();
    loop {
        let reply = spectator.next().await.expect("reply").expect("ws ok");
        if let Ok(ServerMessage::Error(error)) =
            serde_json::from_str::<ServerMessage>(reply.to_text().unwrap())
        {
            assert_eq!(
                error.kind,
                ti4_host::protocol::error::ErrorKind::MalformedMessage
            );
            break;
        }
    }
}

#[test]
fn the_messages_round_trip_and_old_json_still_parses() {
    let text = r#"{"type":"query_reach","protocol_version":4,"game_id":"g","request_id":2,"system_id":"18"}"#;
    assert!(matches!(
        ti4_host::protocol::parse_client_message(text).unwrap(),
        ClientMessage::QueryReach { request_id: 2, .. }
    ));
    let message = ServerMessage::ReachInto(ReachIntoMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "g".to_owned(),
        request_id: 2,
        system_id: "18".to_owned(),
        as_of_version: 4,
        origins: vec![],
        refused: None,
    });
    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains(r#""type":"reach_into""#));
    assert!(!json.contains("refused"));
    assert_eq!(
        ti4_host::protocol::parse_server_message(&json).unwrap(),
        message
    );
}
