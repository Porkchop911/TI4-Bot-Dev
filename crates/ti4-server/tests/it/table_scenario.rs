//! "Start from" of a table: the game starts from a scenario of `ti4_view::scenario`, as the Dev
//! sheet of a local game starts it, so that several people can test a screen of the late game.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::json;
use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::public::PublicConfig;
use ti4_server::session::registry::LobbyError;
use ti4_server::session::table::TableSettings;
use ti4_server::session::{GameRegistry, GameSession};
use ti4_server::storage::FileGameStore;
use ti4_view::scenario::LATE_GAME;

use crate::table_http::{open_seats, serve};

/// Waits until the game asks `seat`, and answers with the first option.
fn answer_first(session: &GameSession, seat: &PlayerId) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some((asked, nonce, version)) = session.current_pending_decision() {
            assert_eq!(&asked, seat, "the scenario asks seat A first");
            let option = session
                .get_snapshot(&ViewerRole::Player(asked.clone()))
                .pending_choice
                .expect("a pending choice")
                .choice
                .options[0]
                .id
                .clone();
            if session
                .submit_choice(&asked, &nonce, version, &option)
                .is_ok()
            {
                return;
            }
        }
        assert!(Instant::now() < deadline, "{:?}", session.error());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
async fn a_table_of_two_people_and_bots_starts_the_late_game_and_recovers_it() {
    let dir = std::env::temp_dir().join(format!("ti4_scenario_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let server = serve(Arc::new(GameRegistry::new().with_store(store.clone()))).await;
    let made = server.table(4, json!({"bot_delay_ms": 0})).await;
    let (guest_id, guest) = server.join(&made.game, "Bo").await;

    // The table takes the seats of the scenario, and its rows show the values of the scenario.
    let view = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"start_from": LATE_GAME}),
        )
        .await;
    assert_eq!(view["start_from"], LATE_GAME);
    assert_eq!(view["slots"].as_array().unwrap().len(), 8);
    assert_eq!(open_seats(&view), 6);
    assert_eq!(view["seat_order"], "join");
    assert_eq!(view["strategy_card_set"], "pok");
    assert_eq!(view["slots"][0]["faction"], "sol");
    assert_eq!(view["slots"][1]["faction"], "hacan");
    assert!(view["slots"][2].get("faction").is_none());

    // The map, the factions and the count are not asked.
    for (route, token, body) in [
        ("map", &made.host, json!({"map": {"kind": "random"}})),
        (
            "seat-faction",
            &guest,
            json!({"player_id": guest_id, "faction": "naalu"}),
        ),
        ("settings", &made.host, json!({"player_count": 6})),
    ] {
        let (status, reason) = server.post(&made.game, route, Some(token), body).await;
        assert_eq!(status, StatusCode::CONFLICT, "{route}: {reason}");
    }
    // The board of the preview is the board of the scenario: it has the asteroid field that
    // the scenario puts in the place of system 31.
    let preview = server
        .get(&format!("/api/games/{}/lobby/map-preview", made.game))
        .await;
    assert_eq!(preview["seats"].as_array().unwrap().len(), 8);
    let systems: Vec<&str> = preview["tiles"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tile| tile["system_id"].as_str().unwrap())
        .collect();
    assert!(
        systems.contains(&"44") && !systems.contains(&"31"),
        "{systems:?}"
    );

    // Open seats are filled with seats that the server plays, as in every table.
    let (status, _) = server
        .post(&made.game, "start", Some(&made.host), json!({}))
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    server
        .ok(
            &made.game,
            "bots",
            &made.host,
            json!({"kind": "random", "fill": true}),
        )
        .await;
    let running = server.ok(&made.game, "start", &made.host, json!({})).await;
    assert_eq!(running["phase"], "running");
    assert_eq!(running["start_from"], LATE_GAME);

    // Each person has the seat of the join: the host is seat A, the guest seat B.
    let session = server.registry.get_game(&made.game).unwrap();
    let seats = session.lobby_details().0;
    assert_eq!(seats[0].as_str(), made.host_id);
    assert_eq!(seats[1].as_str(), guest_id);
    // The game is the game that a local game starts from the same scenario.
    let (expected, galaxy) =
        ti4_view::scenario::start(ContentStore::embedded(), LATE_GAME, &seats).unwrap();
    let init = store.load_player_init(&made.game).unwrap();
    assert_eq!(
        serde_json::to_value(&init.initial_state).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(init.start_from.as_deref(), Some(LATE_GAME));
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    assert_eq!(session.map_tiles(), tiles);
    let state = session.current_state();
    assert_eq!(state.round, 4);
    assert_eq!(state.player(&seats[0]).unwrap().faction.as_str(), "sol");

    // Seat A has the turn. Its answer is in the history that a restart plays again.
    answer_first(&session, &seats[0]);
    let deadline = Instant::now() + Duration::from_secs(20);
    while session.decision_log().is_empty() {
        assert!(Instant::now() < deadline, "{:?}", session.error());
        std::thread::sleep(Duration::from_millis(5));
    }
    session.stop();
    let recorded = session.decision_log();
    drop(session);
    let game = made.game.clone();
    drop(server);

    let recovered = GameRegistry::new().with_store(store);
    recovered
        .recover_all_games()
        .expect("the scenario game recovers");
    let session = recovered.get_game(&game).expect("the game is back");
    assert_eq!(session.map_tiles(), tiles);
    assert_eq!(session.lobby_details().0, seats);
    let replayed = session.decision_log();
    assert!(replayed.len() >= recorded.len());
    assert_eq!(replayed[..recorded.len()], recorded[..]);
    assert!(session.error().is_none(), "{:?}", session.error());
    session.stop();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn the_public_server_starts_no_scenario() {
    let registry = GameRegistry::new().with_public(PublicConfig {
        public: true,
        ..PublicConfig::default()
    });
    let (_, _, token) = registry
        .create_player_lobby("scenario_public".to_owned(), 4, 1, "Ada")
        .unwrap();
    let late = TableSettings {
        start_from: Some(Some(LATE_GAME.to_owned())),
        ..TableSettings::default()
    };
    assert!(matches!(
        registry.change_player_lobby_settings("scenario_public", token.as_str(), &late),
        Err(LobbyError::InvalidSetting(_))
    ));
    assert!(
        ti4_server::session::table::setup_options(true)
            .scenarios
            .is_empty()
    );
}

#[tokio::test]
async fn round_1_gives_the_table_its_setup_back() {
    let server = serve(Arc::new(GameRegistry::new())).await;
    let made = server.table(4, json!({})).await;
    server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"start_from": LATE_GAME}),
        )
        .await;
    let view = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"start_from": null}),
        )
        .await;
    assert!(view.get("start_from").is_none(), "{view}");
    assert_eq!(view["seat_order"], "random");
    assert_eq!(view["strategy_card_set"], "te");
    // The count stays, and the host sets it again.
    assert_eq!(view["slots"].as_array().unwrap().len(), 8);
    let four = server
        .ok(
            &made.game,
            "settings",
            &made.host,
            json!({"player_count": 4}),
        )
        .await;
    assert_eq!(four["slots"].as_array().unwrap().len(), 4);
}
