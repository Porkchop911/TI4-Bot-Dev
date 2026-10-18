//! A connection whose reader falls behind loses its session subscription (the bounded queue
//! filled). The socket must then close, so the page reconnects and reads a fresh snapshot;
//! an open socket that still answers pings but never carries another update leaves the page
//! frozen at an old version for good (nightly run s1-05-2251: "seat 1 UI never reached v1689").

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::server::ServerMessage;
use ti4_server::create_app;
use ti4_server::session::GameRegistry;

use crate::random_bots::bot_config;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_subscriber_dropped_for_falling_behind_has_its_socket_closed() {
    let registry = Arc::new(GameRegistry::new());
    let app = create_app(registry.clone());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });

    let session = registry
        .create_game(bot_config("slow_reader", 6, 7, "te"))
        .expect("create game");

    let (mut stream, _) =
        tokio_tungstenite::connect_async(format!("ws://{addr}/ws/games/slow_reader"))
            .await
            .expect("connect");
    let subscribe = ClientMessage::Subscribe {
        protocol_version: PROTOCOL_VERSION,
        game_id: "slow_reader".to_owned(),
        player_session: None,
        staged: false,
    };
    stream
        .send(Message::Text(
            serde_json::to_string(&subscribe).unwrap().into(),
        ))
        .await
        .expect("subscribe");

    // Read nothing while the bots play far more versions than the queues and socket buffers hold.
    let start = session.decision_log().len();
    let deadline = Instant::now() + Duration::from_secs(240);
    while session.decision_log().len() < start + 600 && !session.is_finished() {
        assert!(Instant::now() < deadline, "the bot game did not advance");
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    assert!(
        !session.is_finished(),
        "the game ended after {} decisions, before the reader fell behind",
        session.decision_log().len()
    );

    // Drain what was buffered: the socket must then end instead of staying open and silent.
    let mut buffered = 0usize;
    let drain_until = Instant::now() + Duration::from_secs(120);
    let closed = loop {
        if Instant::now() > drain_until {
            break false;
        }
        match tokio::time::timeout(Duration::from_secs(5), stream.next()).await {
            Err(_) => break false,
            Ok(None | Some(Err(_)) | Some(Ok(Message::Close(_)))) => break true,
            Ok(Some(Ok(message))) => {
                if let Ok(text) = message.to_text()
                    && let Ok(ServerMessage::StateUpdate(_)) = serde_json::from_str(text)
                {
                    buffered += 1;
                }
            }
        }
    };
    session.stop();
    assert!(
        closed,
        "the socket stayed open without updates after {buffered} buffered state updates"
    );
}
