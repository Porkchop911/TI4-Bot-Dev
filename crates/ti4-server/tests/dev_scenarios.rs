use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use ti4_server::dev::{available_scenarios, execute_launch_scenario};
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::registry::HistoryAction;
use ti4_server::session::{GameRegistry, MockClient};

#[test]
fn test_available_scenarios_listing() {
    let scenarios = available_scenarios();
    assert_eq!(
        scenarios
            .iter()
            .map(|scenario| scenario.id.as_str())
            .collect::<Vec<_>>(),
        [
            "research_tech_skips",
            "tactical_action",
            "production_batch",
            "production_payment_batch",
            "production_payment_autospend",
            "space_combat",
            "ongoing_combat",
            "ongoing_combat_four_views",
            "ongoing_invasion_four_views",
            "ongoing_invasion_coexistence",
            "ongoing_invasion_parley",
            "endgame",
            "score_objective_status",
            "score_objective_imperial",
            "full_game",
        ]
    );
}

#[test]
fn invasion_scenarios_launch_with_public_boundary_and_dev_only_seat_tokens() {
    for scenario in [
        "ongoing_invasion_four_views",
        "ongoing_invasion_coexistence",
        "ongoing_invasion_parley",
    ] {
        let registry = Arc::new(GameRegistry::new());
        let launch =
            execute_launch_scenario(&registry, scenario, Some(42)).expect("launch invasion");
        assert_eq!(launch.test_seats.as_ref().map(BTreeMap::len), Some(3));
        let session = registry.get_game(&launch.game_id).expect("session");
        let view = session.get_snapshot(&ViewerRole::Spectator);
        let invasion = view.view.board.invasion.expect("active invasion");
        assert_eq!(invasion.invader.as_str(), launch.player_id);
        assert_eq!(
            invasion.planets.len(),
            if scenario.ends_with("coexistence") {
                2
            } else {
                1
            }
        );
        if scenario.ends_with("coexistence") {
            let first = &invasion.planets[0];
            let defender = view
                .view
                .players
                .iter()
                .find(|seat| seat.faction.as_str() == "letnev")
                .unwrap();
            assert_eq!(
                invasion.odds_context[first].opponent.as_ref(),
                Some(&defender.id),
                "the controller fights first even with another ground-force owner present"
            );
            assert!(invasion.odds_context.values().any(|context| {
                context
                    .ground_force_types
                    .iter()
                    .filter(|id| *id == "infantry")
                    .count()
                    >= 3
            }));
            let system_units = &view.view.board.systems[&invasion.system_id].units;
            let space_infantry = system_units
                .iter()
                .filter(|u| {
                    u.owner.as_str() == launch.player_id
                        && u.planet.is_none()
                        && u.unit_type.as_str() == "infantry"
                })
                .count();
            assert_eq!(space_infantry, 6);
        }
        assert!(view.pending_choice.is_none(), "spectator has no offer");
    }
}

#[test]
fn test_launch_tactical_scenario_and_flow() {
    let registry = Arc::new(GameRegistry::new());
    let response = execute_launch_scenario(&registry, "tactical_action", Some(42))
        .expect("launch tactical scenario");

    assert!(response.game_id.starts_with("dev_tactical_"));
    let session = registry
        .get_game(&response.game_id)
        .expect("session in registry");

    let p1 = ti4_model::id::PlayerId::new(&response.player_id);
    let client = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));

    // Wait for the initial action phase choice
    let wait_for_choice = |client: &MockClient| {
        for _ in 0..100 {
            if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
                return Some(msg.choice);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    };

    let choice = wait_for_choice(&client).expect("choice for tactical action");
    assert_eq!(choice.player, p1);
    assert_eq!(choice.prompt, "action phase");

    // Should offer taking a tactical action
    let tactical_opt = choice
        .options
        .iter()
        .find(|o| o.id == "tactical")
        .expect("tactical action option");

    let (_, nonce_1, ver_1) = session
        .current_pending_decision()
        .expect("pending decision");
    let res = client.submit(&nonce_1, ver_1, &tactical_opt.id);
    assert!(res.is_ok(), "submit tactical action: {res:?}");

    // Next decision should be activating a system
    let activate_choice = wait_for_choice(&client).expect("choice for system activation");
    assert_eq!(activate_choice.options[0].kind, "activate");
    assert!(!activate_choice.options.is_empty());
}

#[test]
fn dev_movement_history_rewinds_to_the_action_choice() {
    let registry = Arc::new(GameRegistry::new());
    let launched = execute_launch_scenario(&registry, "tactical_action", Some(42)).unwrap();
    let session = registry.get_game(&launched.game_id).unwrap();
    let seat = ti4_model::id::PlayerId::new(&launched.player_id);
    let mut previous_nonce = None;
    for option_id in ["tactical", "22"] {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
            if let Some(pending) = snapshot
                .pending_choice
                .filter(|p| previous_nonce.as_ref() != Some(&p.nonce))
                && let Some(chosen) = pending.choice.options.iter().find(|o| o.id == option_id)
            {
                let result =
                    session.submit_choice(&seat, &pending.nonce, snapshot.game_version, &chosen.id);
                if matches!(
                    result,
                    Err(ti4_server::protocol::status::RejectionReason::NoPendingChoice)
                ) {
                    continue;
                }
                assert!(result.is_ok(), "{option_id}: {result:?}");
                previous_nonce = Some(pending.nonce);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "waiting for {option_id}: {:?}",
                session.error()
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while !session.history_ready() {
        assert!(
            std::time::Instant::now() < deadline,
            "{:?}",
            session.error()
        );
        thread::sleep(Duration::from_millis(5));
    }
    let result = registry.change_history(
        &launched.game_id,
        &launched.player_session,
        session.game_version(),
        HistoryAction::UndoPipeline,
    );
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(result.unwrap().history.cursor, 0);
}

#[test]
fn test_launch_space_combat_scenario_has_hostile_units() {
    let registry = Arc::new(GameRegistry::new());
    let response = execute_launch_scenario(&registry, "space_combat", Some(12345))
        .expect("launch space combat scenario");

    assert!(response.game_id.starts_with("dev_combat_"));
    let session = registry
        .get_game(&response.game_id)
        .expect("session in registry");

    let p1 = ti4_model::id::PlayerId::new(&response.player_id);
    let client = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));

    let wait_for_choice = |client: &MockClient| {
        for _ in 0..100 {
            if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
                return Some(msg.choice);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    };

    // 1. Initial action choice -> select "tactical"
    let choice = wait_for_choice(&client).expect("initial choice");
    assert_eq!(choice.player, p1);
    let (_, nonce_1, ver_1) = session.current_pending_decision().unwrap();
    client.submit(&nonce_1, ver_1, "tactical").unwrap();

    // 2. Activate choice -> activate border system where Letnev ships are
    let initial_snapshot = session.get_snapshot(&ViewerRole::Player(p1.clone()));
    assert!(
        initial_snapshot.view.players.iter().any(|player| {
            player.id == p1
                && player
                    .held_action_cards
                    .iter()
                    .any(|card| card.as_str() == "dh1")
        }),
        "Player 1 must start the space_combat scenario holding Direct Hit"
    );
    let letnev_id = initial_snapshot
        .view
        .players
        .iter()
        .find(|p| p.faction.as_str().to_lowercase().contains("letnev"))
        .map(|p| p.id.clone())
        .expect("letnev player");
    let border_sys_id = initial_snapshot
        .view
        .board
        .systems
        .values()
        .find(|s| {
            s.system_id.as_str() != "10"
                && s.system_id.as_str() != "01"
                && s.units.iter().any(|u| u.owner == letnev_id)
        })
        .map(|s| s.system_id.clone())
        .expect("border system with Letnev units");

    let choice = wait_for_choice(&client).expect("activate choice");
    assert_eq!(choice.player, p1);
    let activate_opt = choice
        .options
        .iter()
        .find(|o| o.id == border_sys_id.as_str())
        .expect("activate border system option");
    let (_, nonce_2, ver_2) = session.current_pending_decision().unwrap();
    client.submit(&nonce_2, ver_2, &activate_opt.id).unwrap();

    // 3. Movement choice -> move ships then done_moving
    let mut choice = wait_for_choice(&client).expect("movement choice");
    // Advance movement and loading until space combat begins
    while !choice
        .options
        .iter()
        .any(|o| o.kind == "retreat" || o.kind == "sustain" || o.kind == "casualty")
    {
        let (_, nonce, ver) = session.current_pending_decision().unwrap();
        if let Some(load_opt) = choice.options.iter().find(|o| o.id.starts_with("load|0")) {
            client.submit(&nonce, ver, &load_opt.id).unwrap();
        } else if let Some(done_load) = choice.options.iter().find(|o| o.id == "done_loading") {
            client.submit(&nonce, ver, &done_load.id).unwrap();
        } else if let Some(move_opt) = choice.options.iter().find(|o| o.id.starts_with("move|")) {
            client.submit(&nonce, ver, &move_opt.id).unwrap();
        } else if let Some(done_move) = choice.options.iter().find(|o| o.id == "done_moving") {
            client.submit(&nonce, ver, &done_move.id).unwrap();
        } else {
            panic!("unhandled choice in movement loop: {:?}", choice);
        }
        choice = wait_for_choice(&client).expect("next choice in combat setup");
    }

    let combat_choice = choice;
    assert_eq!(combat_choice.player, p1);
    let subtype = combat_choice
        .context
        .as_ref()
        .map(|c| c.subtype.as_str())
        .unwrap_or("");
    assert!(
        matches!(
            subtype,
            "announce_retreat" | "sustain_damage" | "assign_casualty"
        ),
        "unexpected combat choice subtype: {subtype}"
    );

    // Board projection should contain combat view
    let snapshot = session.get_snapshot(&ViewerRole::Player(p1.clone()));
    assert!(
        snapshot.view.board.combat.is_some(),
        "board combat must be projected"
    );
    let combat = snapshot.view.board.combat.unwrap();
    assert_eq!(combat.attacker, p1);
    assert_eq!(combat.defender, letnev_id);
}

#[tokio::test]
async fn test_dev_scenarios_http_api() {
    let registry = Arc::new(GameRegistry::new());
    let app = ti4_server::create_app(registry);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind listener");
    let addr = listener.local_addr().expect("local addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve app") });

    let client = reqwest::Client::new();
    let res = client
        .get(format!("http://{addr}/api/dev/scenarios"))
        .send()
        .await
        .expect("get dev scenarios");
    assert_eq!(res.status(), reqwest::StatusCode::OK);
    let list: Vec<ti4_server::dev::ScenarioSummary> = res.json().await.expect("json scenario list");
    assert_eq!(list.len(), available_scenarios().len());

    let launch_res = client
        .post(format!("http://{addr}/api/dev/scenarios/launch"))
        .json(&serde_json::json!({
            "scenario_id": "tactical_action",
            "seed": 999
        }))
        .send()
        .await
        .expect("post launch scenario");
    assert_eq!(launch_res.status(), reqwest::StatusCode::OK);
    let launched: ti4_server::dev::LaunchScenarioResponse =
        launch_res.json().await.expect("json launch response");
    assert!(launched.game_id.starts_with("dev_tactical_"));

    // Verify snapshot endpoint works with token
    let snapshot_res = client
        .get(format!(
            "http://{addr}/api/games/{}/snapshot",
            launched.game_id
        ))
        .header("x-ti4-player-session", &launched.player_session)
        .send()
        .await
        .expect("get snapshot");
    assert_eq!(snapshot_res.status(), reqwest::StatusCode::OK);

    // Verify lobby endpoint works with token
    let lobby_res = client
        .get(format!(
            "http://{addr}/api/games/{}/lobby",
            launched.game_id
        ))
        .header("x-ti4-player-session", &launched.player_session)
        .send()
        .await
        .expect("get lobby");
    assert_eq!(lobby_res.status(), reqwest::StatusCode::OK);
    let lobby_json: serde_json::Value = lobby_res.json().await.expect("lobby json");
    assert_eq!(lobby_json["phase"], "running");
}

#[test]
fn test_launch_ongoing_combat_scenario_starts_in_combat() {
    let registry = Arc::new(GameRegistry::new());
    let response = execute_launch_scenario(&registry, "ongoing_combat", Some(42))
        .expect("launch ongoing combat scenario");

    assert!(response.game_id.starts_with("dev_combat_"));
    let session = registry
        .get_game(&response.game_id)
        .expect("session in registry");

    let p1 = ti4_model::id::PlayerId::new(&response.player_id);
    let snapshot = session.get_snapshot(&ViewerRole::Player(p1.clone()));

    // Combat is active immediately on launch!
    assert!(
        snapshot.view.board.combat.is_some(),
        "board combat must be active on scenario launch"
    );
    let combat = snapshot.view.board.combat.unwrap();
    assert_eq!(combat.attacker, p1);

    // Sol holds cards for several stages of space combat.
    let p1_player = snapshot
        .view
        .players
        .iter()
        .find(|p| p.id == p1)
        .expect("p1 player in snapshot");
    for card in ["dh1", "sh1", "courageous", "salvage"] {
        assert!(
            p1_player
                .held_action_cards
                .iter()
                .any(|c| c.as_str() == card),
            "Player 1 (Sol) must hold {card}"
        );
    }

    // Both sides fielded a Dreadnought in the battle system
    let sys = snapshot
        .view
        .board
        .systems
        .get(&combat.system_id)
        .expect("combat system");
    assert!(
        sys.units
            .iter()
            .any(|u| u.owner == p1 && u.unit_type.as_str() == "dreadnought"),
        "Attacker (Sol) must have dreadnought in combat"
    );
    assert!(
        sys.units
            .iter()
            .any(|u| u.owner == combat.defender && u.unit_type.as_str() == "dreadnought"),
        "Defender must have dreadnought in combat"
    );

    // The player enters combat before the dice and Direct Hit reaction.
    assert!(snapshot.pending_choice.is_some());
    let pending = snapshot.pending_choice.unwrap();
    assert_eq!(pending.choice.player, p1);
    let subtype = pending
        .choice
        .context
        .as_ref()
        .map(|c| c.subtype.as_str())
        .unwrap_or("");
    assert!(
        matches!(subtype, "announce_retreat" | "retreat_to"),
        "expected a pre-roll retreat choice, got: {subtype}"
    );

    assert!(
        combat.dice_rolls.is_empty(),
        "combat should start before the first roll"
    );
}

#[test]
fn ongoing_combat_can_undo_twice() {
    let registry = Arc::new(GameRegistry::new());
    let launched = execute_launch_scenario(&registry, "ongoing_combat", Some(42)).unwrap();
    let seat = ti4_model::id::PlayerId::new(&launched.player_id);
    let session = registry.get_game(&launched.game_id).unwrap();
    let first = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let retreat = first.pending_choice.as_ref().unwrap();
    let stay = retreat
        .choice
        .options
        .iter()
        .find(|o| o.id == "decline" || o.id == "stay")
        .unwrap();
    session
        .submit_choice(&seat, &retreat.nonce, first.game_version, &stay.id)
        .unwrap();

    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
        if let Some(reaction) = snapshot.pending_choice.as_ref()
            && let Some(play) = reaction
                .choice
                .options
                .iter()
                .find(|o| o.id.ends_with(":SUSTAIN_DAMAGE_USED:after"))
        {
            let accepted = session
                .submit_choice(&seat, &reaction.nonce, snapshot.game_version, &play.id)
                .unwrap();
            assert_eq!(accepted.option_id, play.id);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "waiting for Direct Hit reaction"
        );
        thread::sleep(Duration::from_millis(5));
    }

    for attempt in 0..2 {
        let session = registry.get_game(&launched.game_id).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !session.history_ready() {
            assert!(
                std::time::Instant::now() < deadline,
                "waiting for history: {:?}",
                session.error()
            );
            thread::sleep(Duration::from_millis(5));
        }
        let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
        let result = registry.change_history(
            &launched.game_id,
            &launched.player_session,
            snapshot.game_version,
            HistoryAction::Undo,
        );
        assert!(result.is_ok(), "undo {attempt} failed: {result:?}");
    }
}

#[test]
fn test_launch_research_tech_skips_scenario() {
    let registry = Arc::new(GameRegistry::new());
    let launched = execute_launch_scenario(&registry, "research_tech_skips", Some(42))
        .expect("launch scenario succeeds");

    let session = registry.get_game(&launched.game_id).expect("game exists");
    let seat = ti4_model::id::PlayerId::new(&launched.player_id);
    let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let pending = snapshot.pending_choice.expect("pending choice exists");
    assert_eq!(
        pending.choice.context.as_ref().map(|c| c.subtype.as_str()),
        Some("research_technology")
    );
}

#[test]
fn test_launch_score_objective_status_scenario() {
    let registry = Arc::new(GameRegistry::new());
    let launched = execute_launch_scenario(&registry, "score_objective_status", Some(42))
        .expect("launch scenario succeeds");

    let session = registry.get_game(&launched.game_id).expect("game exists");
    let seat = ti4_model::id::PlayerId::new(&launched.player_id);
    let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let pending = snapshot.pending_choice.expect("pending choice exists");
    assert_eq!(
        pending.choice.context.as_ref().map(|c| c.subtype.as_str()),
        Some("score_objective")
    );
    let option_ids: Vec<&str> = pending
        .choice
        .options
        .iter()
        .map(|o| o.id.as_str())
        .collect();
    assert!(option_ids.contains(&"lead"), "should offer lead");
    assert!(
        option_ids.contains(&"trade_routes"),
        "should offer trade_routes"
    );
    assert!(option_ids.contains(&"decline"), "should offer decline");

    let nonce = pending.nonce;
    let version = snapshot.game_version;
    session
        .submit_choice(&seat, &nonce, version, "trade_routes")
        .expect("submit trade_routes scoring");

    let mut updated = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while updated
        .view
        .players
        .iter()
        .find(|p| p.id == seat)
        .unwrap()
        .victory_points
        < 3
    {
        assert!(
            std::time::Instant::now() < deadline,
            "waiting for VP update"
        );
        thread::sleep(Duration::from_millis(10));
        updated = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    }

    let scored = updated.view.table.scored_objectives.get(&seat);
    assert!(
        scored.is_some_and(|objs| objs.contains(&ti4_model::id::ObjectiveId::new("trade_routes")))
    );
}

#[test]
fn test_launch_score_objective_imperial_scenario() {
    let registry = Arc::new(GameRegistry::new());
    let launched = execute_launch_scenario(&registry, "score_objective_imperial", Some(42))
        .expect("launch scenario succeeds");

    let session = registry.get_game(&launched.game_id).expect("game exists");
    let seat = ti4_model::id::PlayerId::new(&launched.player_id);
    let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let pending = snapshot.pending_choice.expect("pending choice exists");
    assert_eq!(
        pending.choice.context.as_ref().map(|c| c.subtype.as_str()),
        Some("imperial_score_objective")
    );
    let option_ids: Vec<&str> = pending
        .choice
        .options
        .iter()
        .map(|o| o.id.as_str())
        .collect();
    assert!(option_ids.contains(&"lead"), "should offer lead");
    assert!(
        option_ids.contains(&"trade_routes"),
        "should offer trade_routes"
    );

    let nonce = pending.nonce;
    let version = snapshot.game_version;
    session
        .submit_choice(&seat, &nonce, version, "lead")
        .expect("submit lead scoring");

    let mut updated = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while updated
        .view
        .players
        .iter()
        .find(|p| p.id == seat)
        .unwrap()
        .victory_points
        < 3
    {
        assert!(
            std::time::Instant::now() < deadline,
            "waiting for VP update"
        );
        thread::sleep(Duration::from_millis(10));
        updated = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    }

    let scored = updated.view.table.scored_objectives.get(&seat);
    assert!(scored.is_some_and(|objs| objs.contains(&ti4_model::id::ObjectiveId::new("lead"))));
}
