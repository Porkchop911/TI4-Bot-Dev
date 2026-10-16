//! `POST …/lobby/settings` and `…/lobby/nickname`: the host changes the player count, the set of
//! strategy cards and the wait of the seats that the server plays, and a player its name.

use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::{Value, json};
use ti4_server::session::GameRegistry;
use ti4_server::storage::FileGameStore;

use crate::table_http::{names, open_seats, serve};

#[tokio::test]
async fn the_host_grows_and_shrinks_the_table_and_the_map_follows_the_count() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let made = server.table(4, json!({})).await;
    let four = made.lobby["map"]["alias"].clone();
    assert!(four.is_string(), "{}", made.lobby);

    // More seats are added open, and the map is the default map of the new count.
    let six = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"player_count": 6}),
        )
        .await;
    assert_eq!(six["slots"].as_array().unwrap().len(), 6);
    assert_eq!(open_seats(&six), 5);
    assert_ne!(six["map"]["alias"], four);
    assert_eq!(six["map"]["recommended"], true);
    assert!(six["map_revision"].as_u64() > made.lobby["map_revision"].as_u64());
    let preview = server
        .get(&format!("/api/games/{}/lobby/map-preview", made.game))
        .await;
    assert_eq!(preview["seats"].as_array().unwrap().len(), 6);

    // Two rule bots and a person: fewer seats take the open seats first, then the bots.
    server
        .ok(
            &made.game,
            "bots",
            &made.host,
            json!({"kind": "smart", "count": 2}),
        )
        .await;
    server.join(&made.game, "Bo").await;
    let four_again = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"player_count": 4}),
        )
        .await;
    assert_eq!(
        names(&four_again),
        ["Ada", "Rule bot 1", "Rule bot 2", "Bo"]
    );
    assert_eq!(four_again["map"]["alias"], four);
    let three = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"player_count": 3}),
        )
        .await;
    assert_eq!(names(&three), ["Ada", "Rule bot 1", "Bo"]);
    let two = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"player_count": 2}),
        )
        .await;
    assert_eq!(names(&two), ["Ada", "Bo"]);
    // The table still starts.
    let running = server.ok(&made.game, "start", &made.host, json!({})).await;
    assert_eq!(running["phase"], "running");
}

#[tokio::test]
async fn a_person_is_never_removed_and_a_random_board_stays_random() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let made = server.table(3, json!({"map": "random"})).await;
    assert_eq!(made.lobby["map"]["kind"], "random");
    server.join(&made.game, "Bo").await;
    server.join(&made.game, "Cy").await;
    let before = server.view(&made.game).await;

    let (status, reason) = server
        .post(
            &made.game,
            "settings",
            Some(&made.host),
            json!({"player_count": 2}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert!(reason.as_str().unwrap().contains("Cy"), "{reason}");
    assert_eq!(server.view(&made.game).await, before);

    let five = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"player_count": 5}),
        )
        .await;
    assert_eq!(five["map"]["kind"], "random");
    assert_eq!(names(&five), ["Ada", "Bo", "Cy", "", ""]);
}

#[tokio::test]
async fn the_card_set_is_changed_in_the_lobby_and_the_game_plays_it() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let made = server.table(2, json!({})).await;
    assert_eq!(made.lobby["strategy_card_set"], "te");
    let view = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"strategy_card_set": "pok"}),
        )
        .await;
    assert_eq!(view["strategy_card_set"], "pok");
    let (status, _) = server
        .post(
            &made.game,
            "settings",
            Some(&made.host),
            json!({"strategy_card_set": "base_game"}),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    server.join(&made.game, "Bo").await;
    server.ok(&made.game, "start", &made.host, json!({})).await;
    let state = server
        .registry
        .get_game(&made.game)
        .unwrap()
        .current_state();
    let cards: Vec<&str> = state
        .unclaimed_strategy_cards
        .iter()
        .map(|card| card.as_str())
        .collect();
    assert!(cards.contains(&"pok4construction"), "{cards:?}");
    assert!(!cards.contains(&"te4construction"), "{cards:?}");
}

#[tokio::test]
async fn only_the_host_changes_the_settings_and_only_before_the_start() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let made = server.table(2, json!({})).await;
    let (_, guest) = server.join(&made.game, "Bo").await;
    let before = server.view(&made.game).await;
    for (token, body, expected) in [
        (
            Some(guest.as_str()),
            json!({"player_count": 3}),
            StatusCode::FORBIDDEN,
        ),
        (None, json!({"player_count": 3}), StatusCode::FORBIDDEN),
        (
            Some("forged"),
            json!({"bot_delay_ms": 0}),
            StatusCode::FORBIDDEN,
        ),
        (
            Some(made.host.as_str()),
            json!({"player_count": 9}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(made.host.as_str()),
            json!({"player_count": 1}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(made.host.as_str()),
            json!({"bot_delay_ms": 30_001}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(made.host.as_str()),
            json!({"start_from": "nope"}),
            StatusCode::BAD_REQUEST,
        ),
        (
            Some(made.host.as_str()),
            json!({"seed": 5}),
            StatusCode::UNPROCESSABLE_ENTITY,
        ),
        // One refused field refuses the call: the count is not changed either.
        (
            Some(made.host.as_str()),
            json!({"player_count": 4, "strategy_card_set": "nope"}),
            StatusCode::BAD_REQUEST,
        ),
    ] {
        let (status, reason) = server
            .post(&made.game, "settings", token, body.clone())
            .await;
        assert_eq!(status, expected, "{body}: {reason}");
    }
    assert_eq!(server.view(&made.game).await, before);

    server.ok(&made.game, "start", &made.host, json!({})).await;
    let (status, _) = server
        .post(
            &made.game,
            "settings",
            Some(&made.host),
            json!({"bot_delay_ms": 0}),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn a_player_takes_another_name_before_the_start() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let made = server.table(3, json!({})).await;
    let (_, guest) = server.join(&made.game, "Bo").await;
    let view = server
        .ok(
            &made.game,
            "nickname",
            &guest,
            json!({"nickname": "Bartholomew"}),
        )
        .await;
    assert_eq!(names(&view), ["Ada", "Bartholomew", ""]);
    // The same name again changes nothing.
    let again = server
        .ok(
            &made.game,
            "nickname",
            &guest,
            json!({"nickname": "Bartholomew"}),
        )
        .await;
    assert_eq!(again["lobby_version"], view["lobby_version"]);
    for (token, name, expected) in [
        (Some(guest.as_str()), " padded ", StatusCode::BAD_REQUEST),
        (Some(guest.as_str()), "", StatusCode::BAD_REQUEST),
        (None, "Cy", StatusCode::FORBIDDEN),
    ] {
        let (status, _) = server
            .post(&made.game, "nickname", token, json!({"nickname": name}))
            .await;
        assert_eq!(status, expected, "{name:?}");
    }
    // The credential names the player: nobody renames another one.
    let view = server
        .ok(
            &made.game,
            "nickname",
            &made.host,
            json!({"nickname": "Host"}),
        )
        .await;
    assert_eq!(names(&view), ["Host", "Bartholomew", ""]);
}

#[tokio::test]
async fn the_wait_of_a_table_is_kept_with_its_game() {
    let dir = std::env::temp_dir().join(format!("ti4_wait_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let server = serve(Arc::new(GameRegistry::new().with_store(store.clone()))).await;
    let made = server.table(3, json!({"bot_delay_ms": 0})).await;
    assert_eq!(made.lobby["bot_delay_ms"], 0);
    let view = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"bot_delay_ms": 1500}),
        )
        .await;
    assert_eq!(view["bot_delay_ms"], 1500);
    server
        .ok(
            &made.game,
            "bots",
            &made.host,
            json!({"kind": "smart", "fill": true}),
        )
        .await;
    server.ok(&made.game, "start", &made.host, json!({})).await;
    let session = server.registry.get_game(&made.game).unwrap();
    assert_eq!(session.bot_delay_ms(), 1500);
    // A value that is set while the game runs is the value of the worker that an undo starts.
    session.set_bot_delay_ms(250);
    assert_eq!(session.restart_config().random_bot_delay_ms, Some(250));
    session.stop();
    drop(session);
    let game = made.game.clone();
    drop(server);

    let recovered = GameRegistry::new().with_store(store);
    recovered.recover_all_games().expect("recover");
    let session = recovered.get_game(&game).expect("the game is back");
    assert_eq!(session.bot_delay_ms(), 1500);
    session.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn the_setup_options_name_what_a_host_can_choose() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let options: Value = server.get("/api/setup-options").await;
    let sets = options["strategy_card_sets"].as_array().unwrap();
    assert_eq!(
        sets.iter()
            .map(|set| set["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["te", "pok", "base_game_codex1"]
    );
    assert_eq!(sets[0]["default"], true);
    assert_eq!(sets[0]["name"], "Thunder's Edge");
    assert_eq!(sets[0]["cards"].as_array().unwrap().len(), 8);
    assert_eq!(sets[0]["cards"][3]["name"], "Construction");
    let groups = options["faction_groups"].as_array().unwrap();
    assert_eq!(groups[0]["name"], "Base game");
    assert_eq!(groups[1]["name"], "Prophecy of Kings");
    assert_eq!(
        groups[0]["factions"].as_array().unwrap().len()
            + groups[1]["factions"].as_array().unwrap().len(),
        24
    );
    assert_eq!(options["bot_kinds"][0]["id"], "smart");
    assert_eq!(options["scenarios"][0]["id"], "late-game");
    assert_eq!(options["max_bot_delay_ms"], 30_000);
}
