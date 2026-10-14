//! The socket of a table (`/ws/tables/{game_id}`): a connection is told every change of the
//! lobby, a visitor gets the view with no credential in it, and a player is connected while a
//! socket of that player is open.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

use ti4_server::create_app;
use ti4_server::protocol::PROTOCOL_VERSION;
use ti4_server::public::PublicConfig;
use ti4_server::session::GameRegistry;

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Server {
    base: String,
    client: reqwest::Client,
}

async fn serve(registry: GameRegistry) -> Server {
    let app = create_app(Arc::new(registry));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("127.0.0.1:{}", listener.local_addr().unwrap().port());
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap();
    });
    Server {
        base,
        client: reqwest::Client::new(),
    }
}

impl Server {
    /// Makes a table of four; returns (game id, host session, host player id).
    async fn table(&self) -> (String, String, String) {
        let created: Value = self
            .client
            .post(format!("http://{}/api/games", self.base))
            .json(&json!({"player_count": 4, "seed": 7, "nickname": "Ada"}))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        (
            created["game_id"].as_str().unwrap().to_owned(),
            created["player_session"].as_str().unwrap().to_owned(),
            created["player"]["id"].as_str().unwrap().to_owned(),
        )
    }

    async fn post(&self, game: &str, route: &str, token: Option<&str>, body: Value) -> Value {
        let mut request = self
            .client
            .post(format!(
                "http://{}/api/games/{game}/lobby/{route}",
                self.base
            ))
            .json(&body);
        if let Some(token) = token {
            request = request.header("x-ti4-player-session", token);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        assert_eq!(status, StatusCode::OK, "{route}: {text}");
        serde_json::from_str(&text).unwrap()
    }

    async fn open(&self, game: &str) -> Socket {
        let url = format!("ws://{}/ws/tables/{game}", self.base);
        tokio_tungstenite::connect_async(url).await.unwrap().0
    }

    /// A socket that is subscribed, and the table that the server answered with.
    async fn subscribed(&self, game: &str, session: Option<&str>) -> (Socket, Value) {
        let mut socket = self.open(game).await;
        subscribe(&mut socket, session).await;
        let table = next_table(&mut socket).await;
        (socket, table)
    }
}

async fn subscribe(socket: &mut Socket, session: Option<&str>) {
    let mut message = json!({"type": "subscribe", "protocol_version": PROTOCOL_VERSION});
    if let Some(session) = session {
        message["player_session"] = json!(session);
    }
    socket
        .send(Message::Text(message.to_string().into()))
        .await
        .unwrap();
}

async fn next_message(socket: &mut Socket) -> Value {
    loop {
        let message = tokio::time::timeout(Duration::from_secs(5), socket.next())
            .await
            .expect("a message of the table socket")
            .expect("the socket is open")
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(&text).unwrap();
        }
    }
}

async fn next_table(socket: &mut Socket) -> Value {
    let message = next_message(socket).await;
    assert_eq!(message["type"], "table", "{message}");
    assert_eq!(message["protocol_version"], PROTOCOL_VERSION);
    message
}

/// The next table that `found` accepts. Presence and the change itself can be two messages.
async fn table_where(socket: &mut Socket, found: impl Fn(&Value) -> bool) -> Value {
    loop {
        let table = next_table(socket).await;
        if found(&table) {
            return table;
        }
    }
}

fn nicknames(table: &Value) -> Vec<String> {
    table["lobby"]["slots"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|slot| slot["nickname"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn a_second_connection_is_told_every_change_of_the_lobby() {
    let server = serve(GameRegistry::new()).await;
    let (game, host, host_id) = server.table().await;
    let (mut watcher, first) = server.subscribed(&game, Some(&host)).await;
    assert_eq!(first["viewer"], host_id);
    assert_eq!(first["lobby"]["phase"], "lobby");
    assert_eq!(nicknames(&first), ["Ada"]);
    assert_eq!(first["lobby"]["slots"][0]["connected"], true);

    // A player joins.
    let joined = server
        .post(
            &game,
            "join",
            None,
            json!({"kind": "new", "nickname": "Blair"}),
        )
        .await;
    let guest = joined["player_session"].as_str().unwrap().to_owned();
    let guest_id = joined["player"]["id"].as_str().unwrap().to_owned();
    let table = table_where(&mut watcher, |t| nicknames(t).len() == 2).await;
    assert_eq!(nicknames(&table), ["Ada", "Blair"]);
    assert_eq!(
        table["lobby"]["lobby_version"],
        joined["lobby"]["lobby_version"]
    );

    // A faction is chosen.
    server
        .post(
            &game,
            "seat-faction",
            Some(&guest),
            json!({"player_id": guest_id, "faction": "hacan"}),
        )
        .await;
    let table = table_where(&mut watcher, |t| {
        !t["lobby"]["slots"][1]["faction"].is_null()
    })
    .await;
    assert_eq!(table["lobby"]["slots"][1]["faction"], "hacan");

    // The map changes.
    let before = table["lobby"]["map_revision"].as_u64().unwrap();
    server
        .post(
            &game,
            "map",
            Some(&host),
            json!({"map": {"kind": "random"}}),
        )
        .await;
    let table = table_where(&mut watcher, |t| {
        t["lobby"]["map_revision"].as_u64().unwrap() > before
    })
    .await;
    assert_eq!(table["lobby"]["map"]["kind"], "random");

    // The player leaves.
    server.post(&game, "leave", Some(&guest), json!({})).await;
    let table = table_where(&mut watcher, |t| nicknames(t).len() == 1).await;
    assert_eq!(nicknames(&table), ["Ada"]);

    // The game starts: the phase is the last thing that the lobby tells.
    server
        .post(
            &game,
            "bots",
            Some(&host),
            json!({"kind": "random", "fill": true}),
        )
        .await;
    server.post(&game, "start", Some(&host), json!({})).await;
    let table = table_where(&mut watcher, |t| t["lobby"]["phase"] == "running").await;
    assert_eq!(nicknames(&table).len(), 4);
}

#[tokio::test]
async fn a_visitor_gets_the_view_with_no_credential_in_it() {
    let server = serve(GameRegistry::new()).await;
    let (game, host, host_id) = server.table().await;
    let (mut visitor, table) = server.subscribed(&game, None).await;
    assert!(table.get("viewer").is_none(), "{table}");
    assert_eq!(table["lobby"]["host_player_id"], host_id);
    assert!(!table.to_string().contains(&host), "{table}");
    assert!(!table.to_string().contains("session"), "{table}");
    assert!(table["lobby"].get("seed").is_none());
    // A visitor is nobody's presence.
    assert_eq!(table["lobby"]["slots"][0]["connected"], false);

    let joined = server
        .post(
            &game,
            "join",
            None,
            json!({"kind": "new", "nickname": "Blair"}),
        )
        .await;
    let guest = joined["player_session"].as_str().unwrap();
    let table = table_where(&mut visitor, |t| nicknames(t).len() == 2).await;
    assert!(!table.to_string().contains(guest), "{table}");
    assert!(!table.to_string().contains(&host), "{table}");
}

#[tokio::test]
async fn a_closed_socket_is_not_connected_after_the_grace_time() {
    let server = serve(GameRegistry::new().with_presence_grace(Duration::from_millis(300))).await;
    let (game, host, _) = server.table().await;
    let (mut visitor, table) = server.subscribed(&game, None).await;
    assert_eq!(table["lobby"]["slots"][0]["connected"], false);

    let (mut player, _) = server.subscribed(&game, Some(&host)).await;
    let table = table_where(&mut visitor, |t| {
        t["lobby"]["slots"][0]["connected"] == true
    })
    .await;
    assert_eq!(table["lobby"]["slots"][0]["can_take_over"], false);

    // A socket that pings stays connected past the grace time.
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let ping = json!({"type": "ping", "protocol_version": PROTOCOL_VERSION});
        player
            .send(Message::Text(ping.to_string().into()))
            .await
            .unwrap();
        assert_eq!(next_message(&mut player).await["type"], "pong");
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(50), visitor.next())
            .await
            .is_err(),
        "nothing changed, so nothing is sent"
    );

    // The socket closes: nobody tells the end of the grace time, and the visitor still hears it.
    player.close(None).await.unwrap();
    let table = table_where(&mut visitor, |t| {
        t["lobby"]["slots"][0]["connected"] == false
    })
    .await;
    assert_eq!(table["lobby"]["slots"][0]["can_take_over"], true);
}

#[tokio::test]
async fn a_subscription_is_refused_with_a_reason() {
    let server = serve(GameRegistry::new()).await;
    let (game, host, host_id) = server.table().await;

    // A table that the server does not have.
    let url = format!("ws://{}/ws/tables/nope", server.base);
    let err = tokio_tungstenite::connect_async(url).await.unwrap_err();
    assert!(
        matches!(err, tokio_tungstenite::tungstenite::Error::Http(ref r) if r.status() == 404),
        "{err:?}"
    );

    let mut socket = server.open(&game).await;
    // Another protocol.
    let old = json!({"type": "subscribe", "protocol_version": PROTOCOL_VERSION + 1});
    socket
        .send(Message::Text(old.to_string().into()))
        .await
        .unwrap();
    let answer = next_message(&mut socket).await;
    assert_eq!(answer["type"], "error");
    assert_eq!(answer["kind"], "unsupported_version");
    // A player session that the table does not know.
    subscribe(&mut socket, Some("not-a-session")).await;
    let answer = next_message(&mut socket).await;
    assert_eq!(answer["kind"], "unauthorized", "{answer}");
    // The same socket can still subscribe.
    subscribe(&mut socket, Some(&host)).await;
    assert_eq!(next_table(&mut socket).await["viewer"], host_id);
}

#[tokio::test]
async fn a_player_who_is_removed_goes_on_as_a_visitor() {
    let server = serve(GameRegistry::new()).await;
    let (game, _host, _) = server.table().await;
    let joined = server
        .post(
            &game,
            "join",
            None,
            json!({"kind": "new", "nickname": "Blair"}),
        )
        .await;
    let guest = joined["player_session"].as_str().unwrap().to_owned();
    let (mut socket, table) = server.subscribed(&game, Some(&guest)).await;
    assert_eq!(table["viewer"], joined["player"]["id"]);
    server.post(&game, "leave", Some(&guest), json!({})).await;
    let table = table_where(&mut socket, |t| nicknames(t).len() == 1).await;
    assert!(table.get("viewer").is_none(), "{table}");
}

#[tokio::test]
async fn the_public_server_asks_for_the_password() {
    let dir = std::env::temp_dir().join(format!("ti4_table_socket_{:016x}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    let server = serve(GameRegistry::new().with_public(PublicConfig {
        public: true,
        data_dir: Some(dir.clone()),
        access_password: Some("hunter2".into()),
        cookie_secret: Some("test-secret".into()),
        ..PublicConfig::default()
    }))
    .await;
    let granted = server
        .client
        .post(format!("http://{}/api/access", server.base))
        .json(&json!({"password": "hunter2"}))
        .send()
        .await
        .unwrap();
    assert_eq!(granted.status(), StatusCode::OK);
    let cookie = granted.headers()["set-cookie"].to_str().unwrap();
    let cookie = cookie.split(';').next().unwrap().to_owned();
    let created: Value = server
        .client
        .post(format!("http://{}/api/games", server.base))
        .header("cookie", &cookie)
        .json(&json!({"player_count": 2, "seed": 1, "nickname": "Host"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let game = created["game_id"].as_str().unwrap();

    let url = format!("ws://{}/ws/tables/{game}", server.base);
    let err = tokio_tungstenite::connect_async(&url).await.unwrap_err();
    assert!(
        matches!(err, tokio_tungstenite::tungstenite::Error::Http(ref r) if r.status() == 401),
        "{err:?}"
    );
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (mut socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert_eq!(response.status(), 101);
    subscribe(&mut socket, None).await;
    assert_eq!(next_table(&mut socket).await["lobby"]["game_id"], game);
    let _ = std::fs::remove_dir_all(dir);
}
