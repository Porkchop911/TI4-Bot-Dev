//! Host-initiated game termination: auth, lists, caps, persistence and the 429 payload.

use std::path::PathBuf;
use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use ti4_server::create_app;
use ti4_server::public::PublicConfig;
use ti4_server::session::GameRegistry;
use ti4_server::storage::FileGameStore;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ti4_term_{tag}_{:016x}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

async fn spawn(dir: &std::path::Path, public: bool, cap: Option<usize>) -> String {
    let store = Arc::new(FileGameStore::new(dir).unwrap());
    let mut registry = GameRegistry::new().with_store(store);
    if public {
        registry = registry.with_public(PublicConfig {
            public: true,
            data_dir: Some(dir.to_path_buf()),
            max_lobbies_per_ip: cap,
            ..PublicConfig::default()
        });
    }
    let registry = Arc::new(registry);
    registry.recover_all_games().unwrap();
    let app = create_app(registry);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    format!("http://127.0.0.1:{}", addr.port())
}

async fn create(base: &str) -> (String, String) {
    let r = reqwest::Client::new()
        .post(format!("{base}/api/games"))
        .json(&json!({ "player_count": 2, "seed": 1, "nickname": "Host" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let v: Value = r.json().await.unwrap();
    (
        v["game_id"].as_str().unwrap().to_owned(),
        v["player_session"].as_str().unwrap().to_owned(),
    )
}

async fn terminate(base: &str, id: &str, token: Option<&str>) -> StatusCode {
    let mut r = reqwest::Client::new().post(format!("{base}/api/games/{id}/terminate"));
    if let Some(t) = token {
        r = r.header("x-ti4-player-session", t);
    }
    r.send().await.unwrap().status()
}

async fn list_ids(base: &str) -> Vec<String> {
    let v: Value = reqwest::get(format!("{base}/api/games"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    v.as_array()
        .unwrap()
        .iter()
        .map(|g| g["game_id"].as_str().unwrap().to_owned())
        .collect()
}

async fn join_and_start(base: &str, id: &str, host: &str) -> String {
    let c = reqwest::Client::new();
    let guest = c
        .post(format!("{base}/api/games/{id}/lobby/join"))
        .json(&json!({ "kind": "new", "nickname": "Guest" }))
        .send()
        .await
        .unwrap()
        .json::<Value>()
        .await
        .unwrap()["player_session"]
        .as_str()
        .unwrap()
        .to_owned();
    for t in [host, guest.as_str()] {
        c.post(format!("{base}/api/games/{id}/lobby/ready"))
            .header("x-ti4-player-session", t)
            .json(&json!({ "ready": true }))
            .send()
            .await
            .unwrap();
    }
    let r = c
        .post(format!("{base}/api/games/{id}/lobby/start"))
        .header("x-ti4-player-session", host)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    guest
}

#[tokio::test]
async fn host_terminates_a_lobby_and_it_is_archived_not_recovered() {
    let dir = temp_dir("lobby");
    let base = spawn(&dir, false, None).await;
    let (id, host) = create(&base).await;
    assert!(list_ids(&base).await.contains(&id));
    assert_eq!(terminate(&base, &id, None).await, StatusCode::FORBIDDEN);
    assert_eq!(
        terminate(&base, &id, Some("forged")).await,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        terminate(&base, "game_nope", Some(&host)).await,
        StatusCode::NOT_FOUND
    );
    assert_eq!(terminate(&base, &id, Some(&host)).await, StatusCode::OK);
    assert!(!list_ids(&base).await.contains(&id));
    assert_eq!(
        terminate(&base, &id, Some(&host)).await,
        StatusCode::NOT_FOUND
    );
    let lobby = reqwest::get(format!("{base}/api/games/{id}/lobby"))
        .await
        .unwrap();
    assert_eq!(lobby.status(), StatusCode::NOT_FOUND);
    assert!(!dir.join(&id).exists());
    assert!(dir.join("terminated").join(&id).join("lobby.json").exists());
    // A restart over the same data dir does not bring it back.
    let again = spawn(&dir, false, None).await;
    assert!(!list_ids(&again).await.contains(&id));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn only_the_host_may_terminate_a_started_game() {
    let dir = temp_dir("started");
    let base = spawn(&dir, false, None).await;
    let (id, host) = create(&base).await;
    let guest = join_and_start(&base, &id, &host).await;
    assert_eq!(
        terminate(&base, &id, Some(&guest)).await,
        StatusCode::FORBIDDEN
    );
    assert!(list_ids(&base).await.contains(&id));
    assert_eq!(terminate(&base, &id, Some(&host)).await, StatusCode::OK);
    assert!(!list_ids(&base).await.contains(&id));
    let snap = reqwest::Client::new()
        .get(format!("{base}/api/games/{id}/snapshot"))
        .header("x-ti4-player-session", &host)
        .send()
        .await
        .unwrap();
    assert_eq!(snap.status(), StatusCode::NOT_FOUND);
    let batch = reqwest::Client::new()
        .post(format!("{base}/api/games/{id}/batches"))
        .header("x-ti4-player-session", &host)
        .json(&json!({}))
        .send()
        .await
        .unwrap();
    assert!(batch.status().is_client_error());
    assert!(dir.join("terminated").join(&id).join("init.json").exists());
    let again = spawn(&dir, false, None).await;
    assert!(!list_ids(&again).await.contains(&id));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn too_many_games_lists_the_open_games_and_terminating_frees_the_cap() {
    let dir = temp_dir("cap");
    let base = spawn(&dir, true, Some(2)).await;
    let (a, ta) = create(&base).await;
    let (b, tb) = create(&base).await;
    let _ = tb;
    let c = reqwest::Client::new();
    let refused = c
        .post(format!("{base}/api/games"))
        .json(&json!({ "player_count": 2, "seed": 1, "nickname": "Host" }))
        .send()
        .await
        .unwrap();
    assert_eq!(refused.status(), StatusCode::TOO_MANY_REQUESTS);
    let body: Value = refused.json().await.unwrap();
    assert_eq!(body["error"], "too_many_games");
    assert!(
        body["message"]
            .as_str()
            .unwrap()
            .contains("unfinished games")
    );
    let games = body["games"].as_array().unwrap();
    let ids: Vec<&str> = games
        .iter()
        .map(|g| g["game_id"].as_str().unwrap())
        .collect();
    assert!(ids.contains(&a.as_str()) && ids.contains(&b.as_str()) && ids.len() == 2);
    assert_eq!(games[0]["phase"], "lobby");
    assert_eq!(games[0]["players"], 1);
    assert!(games[0]["created_at"].as_u64().unwrap() > 1_000_000_000);

    assert_eq!(terminate(&base, &a, Some(&ta)).await, StatusCode::OK);
    let (_c3, _) = create(&base).await;
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn connected_sockets_get_a_game_terminated_close() {
    use futures_util::StreamExt;
    use tokio_tungstenite::tungstenite::Message;
    let dir = temp_dir("ws");
    let base = spawn(&dir, false, None).await;
    let (id, host) = create(&base).await;
    let guest = join_and_start(&base, &id, &host).await;
    let _ = guest;
    let url = format!("ws{}/ws/games/{id}", &base[4..]);
    let (mut ws, _) = tokio_tungstenite::connect_async(url).await.unwrap();
    assert_eq!(terminate(&base, &id, Some(&host)).await, StatusCode::OK);
    let close = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while let Some(m) = ws.next().await {
            if let Ok(Message::Close(Some(frame))) = m {
                return Some(frame);
            }
        }
        None
    })
    .await
    .expect("socket closes")
    .expect("close frame");
    assert_eq!(u16::from(close.code), 4002);
    assert_eq!(close.reason.as_str(), "game terminated");
    let _ = std::fs::remove_dir_all(dir);
}
