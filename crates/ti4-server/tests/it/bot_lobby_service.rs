use std::sync::Arc;

use axum::http::StatusCode;
use ti4_server::create_app;
use ti4_server::session::{BotServiceConfig, GameRegistry};

#[tokio::test]
async fn bot_service_disabled_by_default_rejects_add_bot() {
    let registry = Arc::new(GameRegistry::new());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(axum::serve(listener, create_app(registry.clone())).into_future());

    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/games"))
        .json(&serde_json::json!({
            "player_count": 3,
            "seed": 42,
            "nickname": "Host"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let game_id = created["game_id"].as_str().unwrap();
    let host_token = created["player_session"].as_str().unwrap();

    // Verify bot_service_enabled is false
    assert_eq!(created["lobby"]["bot_service_enabled"], false);

    // Attempt to add a bot
    let res = client
        .post(format!("http://{addr}/api/games/{game_id}/lobby/add-bot"))
        .header("x-ti4-player-session", host_token)
        .json(&serde_json::json!({
            "password": "any_password"
        }))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    server.abort();
}

#[tokio::test]
async fn bot_service_password_and_host_validation() {
    let bot_config = BotServiceConfig {
        password: "secret_hobby_pw".to_owned(),
        advisor_url: "http://127.0.0.1:8081".to_owned(),
        bot_agent_bin: std::path::PathBuf::from("/bin/true"),
        server_port: 8080,
        max_active_bots: 4,
    };
    let registry = Arc::new(GameRegistry::new().with_bot_service(bot_config));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(axum::serve(listener, create_app(registry.clone())).into_future());

    let client = reqwest::Client::new();
    let created: serde_json::Value = client
        .post(format!("http://{addr}/api/games"))
        .json(&serde_json::json!({
            "player_count": 3,
            "seed": 42,
            "nickname": "Host"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let game_id = created["game_id"].as_str().unwrap();
    let host_token = created["player_session"].as_str().unwrap();

    // Guest joins
    let guest: serde_json::Value = client
        .post(format!("http://{addr}/api/games/{game_id}/lobby/join"))
        .json(&serde_json::json!({
            "kind": "new",
            "nickname": "Guest"
        }))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let guest_token = guest["player_session"].as_str().unwrap();

    // Verify bot_service_enabled is true
    let lobby: serde_json::Value = client
        .get(format!("http://{addr}/api/games/{game_id}/lobby"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(lobby["bot_service_enabled"], true);

    // 1. Wrong password -> 401 Unauthorized
    let res_wrong_pw = client
        .post(format!("http://{addr}/api/games/{game_id}/lobby/add-bot"))
        .header("x-ti4-player-session", host_token)
        .json(&serde_json::json!({
            "password": "wrong_password"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res_wrong_pw.status(), StatusCode::UNAUTHORIZED);

    // 2. Non-host player session -> 403 Forbidden
    let res_guest = client
        .post(format!("http://{addr}/api/games/{game_id}/lobby/add-bot"))
        .header("x-ti4-player-session", guest_token)
        .json(&serde_json::json!({
            "password": "secret_hobby_pw"
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res_guest.status(), StatusCode::FORBIDDEN);

    // 3. Remove player/bot works with host credential
    let guest_id = guest["player"]["id"].as_str().unwrap();
    let remove_res = client
        .post(format!(
            "http://{addr}/api/games/{game_id}/lobby/remove-bot"
        ))
        .header("x-ti4-player-session", host_token)
        .json(&serde_json::json!({
            "player_id": guest_id
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(remove_res.status(), StatusCode::OK);
    let updated_lobby: serde_json::Value = remove_res.json().await.unwrap();
    assert!(updated_lobby["slots"][1]["occupant"].is_null());

    server.abort();
}
