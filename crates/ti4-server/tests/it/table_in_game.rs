//! The table of a running game (phase 7 of the lobby plan): the game says who sits in each seat,
//! the lobby view says the faction of every seat, and the host changes the wait of the seats
//! that the server plays with `POST /api/games/{game_id}/settings`.

use std::sync::Arc;
use std::time::Duration;

use reqwest::StatusCode;
use serde_json::{Value, json};
use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_server::session::GameRegistry;
use ti4_server::storage::FileGameStore;

use crate::table_http::{Made, Server, serve};

/// A table of four: Ada (the host), Bo, a rule bot and a random seat. It is started.
async fn started(server: &Server) -> (Made, String, String) {
    let made = server.table(4, json!({"bot_delay_ms": 0})).await;
    let (bo, bo_token) = server.join(&made.game, "Bo").await;
    for kind in ["smart", "random"] {
        server
            .ok(&made.game, "bots", &made.host, json!({"kind": kind}))
            .await;
    }
    server.ok(&made.game, "start", &made.host, json!({})).await;
    (made, bo, bo_token)
}

async fn settings(
    server: &Server,
    game: &str,
    token: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut request = server
        .client
        .post(server.url(&format!("/api/games/{game}/settings")))
        .json(&body);
    if let Some(token) = token {
        request = request.header("x-ti4-player-session", token);
    }
    let response = request.send().await.unwrap();
    let status = response.status();
    let text = response.text().await.unwrap();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

#[tokio::test]
async fn a_game_says_who_sits_in_each_seat_and_holds_no_credential() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let (made, bo, bo_token) = started(&server).await;
    let session = server.registry.get_game(&made.game).unwrap();

    // Every viewer is told the same: the fact is public.
    let host = PlayerId::new(made.host_id.as_str());
    for viewer in [ViewerRole::Spectator, ViewerRole::Player(host.clone())] {
        let snapshot = serde_json::to_value(session.get_snapshot(&viewer)).unwrap();
        let seats = snapshot["seats"]
            .as_object()
            .expect("the seats of the table");
        assert_eq!(seats.len(), 4);
        assert_eq!(
            seats[&made.host_id],
            json!({"nickname": "Ada", "kind": "human"})
        );
        assert_eq!(seats[&bo], json!({"nickname": "Bo", "kind": "human"}));
        let mut played: Vec<(&str, &str)> = seats
            .values()
            .filter(|seat| seat["kind"] != "human")
            .map(|seat| {
                (
                    seat["kind"].as_str().unwrap(),
                    seat["nickname"].as_str().unwrap(),
                )
            })
            .collect();
        played.sort_unstable();
        assert_eq!(
            played,
            [("random", "Random bot 1"), ("smart", "Rule bot 1")]
        );
        // A seat of the fact is a player of the game.
        for player in snapshot["view"]["players"].as_array().unwrap() {
            assert!(seats.contains_key(player["id"].as_str().unwrap()));
        }
        let text = snapshot["seats"].to_string();
        assert!(!text.contains(&made.host) && !text.contains(&bo_token));
    }

    // The lobby view of the running table has the faction that the start gave each seat.
    let view = server.view(&made.game).await;
    assert_eq!(view["phase"], "running");
    let snapshot = serde_json::to_value(session.get_snapshot(&ViewerRole::Spectator)).unwrap();
    for (slot, seat) in view["slots"]
        .as_array()
        .unwrap()
        .iter()
        .zip(snapshot["view"]["seating_order"].as_array().unwrap())
    {
        assert_eq!(&slot["occupant"], seat, "the slots are the seats");
        let player = snapshot["view"]["players"]
            .as_array()
            .unwrap()
            .iter()
            .find(|player| &player["id"] == seat)
            .unwrap();
        assert_eq!(slot["faction"], player["faction"]);
    }
    session.stop();
}

#[tokio::test]
async fn the_host_changes_the_wait_of_a_running_game_and_it_is_kept() {
    let dir = std::env::temp_dir().join(format!("ti4_table_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let server = serve(Arc::new(GameRegistry::new().with_store(store.clone()))).await;

    // Before the start the call is refused: the lobby has its own.
    let lobby = server.table(2, json!({})).await;
    let (status, _) = settings(
        &server,
        &lobby.game,
        Some(&lobby.host),
        json!({"bot_delay_ms": 500}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (made, _, bo_token) = started(&server).await;
    let session = server.registry.get_game(&made.game).unwrap();
    assert_eq!(session.bot_delay_ms(), 0);
    let before = server.view(&made.game).await["lobby_version"]
        .as_u64()
        .unwrap();

    // Only the host, and only a wait that a table can have.
    let body = json!({"bot_delay_ms": 1500});
    assert_eq!(
        settings(&server, &made.game, Some(&bo_token), body.clone())
            .await
            .0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        settings(&server, &made.game, None, body.clone()).await.0,
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        settings(
            &server,
            &made.game,
            Some(&made.host),
            json!({"bot_delay_ms": 30_001})
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(session.bot_delay_ms(), 0);

    let (status, view) = settings(&server, &made.game, Some(&made.host), body).await;
    assert_eq!(status, StatusCode::OK, "{view}");
    assert_eq!(view["bot_delay_ms"], 1500);
    assert!(view["lobby_version"].as_u64().unwrap() > before);
    assert_eq!(session.bot_delay_ms(), 1500);
    // The worker that an undo starts has it too.
    assert_eq!(session.restart_config().random_bot_delay_ms, Some(1500));

    session.stop();
    drop(session);
    let game = made.game.clone();
    drop(server);
    let recovered = GameRegistry::new().with_store(store);
    recovered.recover_all_games().expect("recover");
    let session = recovered.get_game(&game).expect("the game is back");
    assert_eq!(session.bot_delay_ms(), 1500);
    // A recovered game still says who sits where.
    let snapshot = serde_json::to_value(session.get_snapshot(&ViewerRole::Spectator)).unwrap();
    assert_eq!(snapshot["seats"].as_object().unwrap().len(), 4);
    session.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn a_seat_that_is_taken_over_has_the_name_of_who_took_it() {
    let registry = GameRegistry::new().with_presence_grace(Duration::from_millis(10));
    let server = serve(Arc::new(registry)).await;
    let (made, bo, _) = started(&server).await;
    tokio::time::sleep(Duration::from_millis(30)).await;

    // A visitor sees which seat can be taken over, and takes it.
    let view = server.view(&made.game).await;
    let slot = view["slots"]
        .as_array()
        .unwrap()
        .iter()
        .find(|slot| slot["occupant"] == bo.as_str())
        .unwrap();
    assert_eq!(slot["can_take_over"], true, "{view}");
    let (status, taken) = server
        .post(
            &made.game,
            "join",
            None,
            json!({"kind": "takeover", "player_id": bo, "nickname": "Cy"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{taken}");
    assert!(taken["player_session"].is_string());

    let session = server.registry.get_game(&made.game).unwrap();
    let snapshot = serde_json::to_value(session.get_snapshot(&ViewerRole::Spectator)).unwrap();
    assert_eq!(snapshot["seats"][&bo]["nickname"], "Cy");
    assert_eq!(
        session.restart_config().nicknames[&PlayerId::new(bo.as_str())],
        "Cy"
    );
    session.stop();
}
