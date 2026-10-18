use ti4_host::projection::project_combat_view;
use ti4_host::projection::{project_initial_snapshot, project_state_update};
use ti4_model::id::PlayerId;
use ti4_model::id::{SystemId, UnitTypeId};
use ti4_model::units::Unit;
use ti4_model::view::{HIDDEN, view_for};
use ti4_server::fixtures::{create_sample_game, create_sample_pending_choice};

#[test]
fn board_projection_preserves_normal_and_galvanized_cargo() {
    let mut game = create_sample_game();
    let system = SystemId::new("18");
    let owner = PlayerId::new("seat_a");
    let normal = Unit::new(UnitTypeId::new("infantry"), owner.clone());
    let mut galvanized = normal.clone();
    galvanized.galvanized = true;
    game.system_mut(&system).planet_units.insert(
        ti4_model::id::PlanetId::new("jord"),
        vec![normal, galvanized],
    );

    let snapshot = project_initial_snapshot("cargo", 1, &game, &ViewerRole::Player(owner), None);
    let json = serde_json::to_value(&snapshot).expect("snapshot");
    let units = json["view"]["board"]["systems"]["18"]["units"]
        .as_array()
        .expect("board units");
    let cargo: Vec<_> = units.iter().filter(|u| u["planet"] == "jord").collect();
    assert_eq!(cargo.len(), 2);
    assert_eq!(cargo[0]["galvanized"], false);
    assert_eq!(cargo[1]["galvanized"], true);
    let decoded: ti4_host::protocol::server::InitialSnapshotMsg =
        serde_json::from_value(json).expect("round trip");
    assert_eq!(decoded.view, snapshot.view);
}

#[test]
fn invasion_boundary_is_public_even_during_an_unrelated_nested_offer() {
    let mut game = create_sample_game();
    let system = SystemId::new("18");
    let planet = ti4_model::id::PlanetId::new("jord");

    // Add defending units on the planet so invasion has defenders (visible to all)
    let defender = PlayerId::new("seat_b");
    game.system_mut(&system)
        .planet_units
        .entry(planet.clone())
        .or_insert_with(Vec::new)
        .push(Unit::new(UnitTypeId::new("infantry"), defender.clone()));
    // Set planet control to show defender is on planet
    game.system_mut(&system)
        .planet_control
        .insert(planet.clone(), defender.clone());

    game.active_invasion = Some(ti4_model::state::ActiveInvasion {
        system: system.clone(),
        invader: PlayerId::new("seat_a"),
        seq: 7,
        phase: "ground_battle".to_owned(),
        planet: Some(planet.clone()),
        defender: Some(defender.clone()),
        ground_round: 2,
        last_step: Some(ti4_model::state::InvasionStep {
            planet: planet.clone(),
            kind: "ground_round".to_owned(),
            round: 2,
            before: vec![Unit::new(
                UnitTypeId::new("infantry"),
                PlayerId::new("seat_a"),
            )],
            after: vec![],
            dice: vec![ti4_model::state::InvasionDie {
                planet: planet.clone(),
                player: PlayerId::new("seat_a"),
                group: "combat value 8".to_owned(),
                face: 9,
                target: 8,
                hit: true,
            }],
            hits: [(PlayerId::new("seat_a"), 1)].into_iter().collect(),
            harrow_hits: 0,
        }),
    });
    let offer = create_sample_pending_choice();
    for viewer in [
        ViewerRole::Player(PlayerId::new("seat_a")),
        ViewerRole::Player(PlayerId::new("seat_b")),
        ViewerRole::Spectator,
    ] {
        let snapshot =
            project_initial_snapshot("invasion", 1, &game, &viewer, Some((&offer, "nested")));
        let json = serde_json::to_value(snapshot).expect("snapshot");
        assert_eq!(json["view"]["board"]["invasion"]["invasion_seq"], 7);
        assert_eq!(json["view"]["board"]["invasion"]["current_planet"], "jord");
        let step = &json["view"]["board"]["invasion"]["last_step"];
        assert_eq!(step["planet"], "jord");
        assert_eq!(step["before"][0]["unit_type"], "infantry");
        assert_eq!(step["dice"][0]["face"], 9);
        assert_eq!(step["hits"]["seat_a"], 1);
    }
}

#[test]
fn combat_projection_follows_the_driver_not_contested_ships_or_the_last_survivor() {
    let mut game = create_sample_game();
    let system = SystemId::new("18");
    let attacker = PlayerId::new("seat_a");
    let defender = PlayerId::new("seat_b");
    game.active_system = Some(system.clone());
    game.system_mut(&system)
        .units
        .push(Unit::new(UnitTypeId::new("cruiser"), defender.clone()));
    assert!(
        project_combat_view(&game, None, &[]).is_none(),
        "contested is not combat"
    );
    game.active_space_combat = Some((system.clone(), attacker.clone(), defender.clone()));
    assert_eq!(
        project_combat_view(&game, None, &[]).unwrap().defender,
        defender
    );
    game.system_mut(&system)
        .units
        .retain(|unit| unit.owner == attacker);
    assert!(
        project_combat_view(&game, None, &[]).is_some(),
        "victory reaction is still in combat"
    );
    game.active_space_combat = None;
    assert!(
        project_combat_view(&game, None, &[]).is_none(),
        "invasion is outside combat"
    );
}
use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::ViewerRole;
use ti4_host::protocol::{EventVisibility, GameEvent, GameEventKind};

#[test]
fn actor_payload_includes_own_private_information_and_pending_choice() {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let seat_a = PlayerId::new("seat_a");
    let viewer_a = ViewerRole::Player(seat_a.clone());

    let snapshot = project_initial_snapshot(
        "test_game",
        1,
        &game,
        &viewer_a,
        Some((&choice, "nonce_123")),
    );
    let json = serde_json::to_string(&ServerMessage::InitialSnapshot(snapshot)).expect("serialize");

    // Actor sees own cards
    assert!(
        json.contains("direct_hit"),
        "Actor must see own action card"
    );
    assert!(
        json.contains("destroy_their_greatest_ship"),
        "Actor must see own secret objective"
    );

    // Actor sees pending choice and options
    assert!(
        json.contains("opt_carrier"),
        "Actor must see offered choice option"
    );
    assert!(
        json.contains("opt_infantry"),
        "Actor must see offered choice option"
    );
    assert!(
        json.contains("produce_units"),
        "Actor must see decision context subtype"
    );

    // Actor sees outstanding constraints
    assert!(
        json.contains("Resources"),
        "Actor must see outstanding resource constraint"
    );

    // Actor DOES NOT see opponent's private cards
    assert!(
        !json.contains("flank_speed"),
        "Actor must not see opponent's action card"
    );
    assert!(
        !json.contains("brave_the_void"),
        "Actor must not see opponent's secret objective"
    );
}

#[test]
fn opponent_payload_strictly_redacts_actor_private_cards_options_and_constraints() {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let seat_b = PlayerId::new("seat_b");
    let viewer_b = ViewerRole::Player(seat_b.clone());

    let snapshot = project_initial_snapshot(
        "test_game",
        1,
        &game,
        &viewer_b,
        Some((&choice, "nonce_123")),
    );
    let json = serde_json::to_string(&ServerMessage::InitialSnapshot(snapshot)).expect("serialize");

    // 1. Proves opponent payload contains NO action-card identity of actor
    assert!(
        !json.contains("direct_hit"),
        "Opponent payload leaked actor's direct_hit card!"
    );
    assert!(
        !json.contains("morale_boost"),
        "Opponent payload leaked actor's morale_boost card!"
    );

    // 2. Proves opponent payload contains NO secret-objective identity of actor
    assert!(
        !json.contains("destroy_their_greatest_ship"),
        "Opponent payload leaked actor's secret objective!"
    );

    // 3. Proves opponent payload contains NO legal options belonging to the actor
    assert!(
        !json.contains("opt_carrier"),
        "Opponent payload leaked actor's choice option id!"
    );
    assert!(
        !json.contains("opt_infantry"),
        "Opponent payload leaked actor's choice option id!"
    );
    assert!(
        !json.contains("Produce Carrier"),
        "Opponent payload leaked actor's choice label!"
    );

    // 4. Proves opponent payload contains NO pending_choice object
    assert!(
        !json.contains("\"pending_choice\""),
        "Opponent payload must not include pending_choice!"
    );

    // 5. Proves opponent payload contains NO actor-only outstanding constraints
    assert!(
        !json.contains("\"outstanding\""),
        "Opponent payload must not include outstanding constraints!"
    );

    // 6. Proves opponent sees accurate public counts
    assert!(
        json.contains("\"action_cards_count\":2"),
        "Public action card count must be visible"
    );
    assert!(
        json.contains("\"secret_objectives_count\":1"),
        "Public secret objective count must be visible"
    );

    // 7. Proves opponent sees public waiting turn status without leaking details
    assert!(
        json.contains("\"kind\":\"waiting_for_decision\""),
        "Opponent must see turn status as waiting_for_decision"
    );
    assert!(
        json.contains("\"seat\":\"seat_a\""),
        "Waiting seat must be seat_a"
    );
    assert!(
        json.contains("\"stage\":\"Waiting for player\""),
        "Public waiting status must not disclose an actor-only decision context"
    );
    assert!(
        !json.contains("Reaction Window"),
        "Public waiting status must not disclose a private reaction window"
    );
}

#[test]
fn spectator_payload_redacts_all_private_cards_and_pending_choices() {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let viewer_spec = ViewerRole::Spectator;

    let snapshot = project_initial_snapshot(
        "test_game",
        1,
        &game,
        &viewer_spec,
        Some((&choice, "nonce_123")),
    );
    let json = serde_json::to_string(&ServerMessage::InitialSnapshot(snapshot)).expect("serialize");

    // Spectator sees NO private cards from ANY player
    assert!(!json.contains("direct_hit"));
    assert!(!json.contains("morale_boost"));
    assert!(!json.contains("destroy_their_greatest_ship"));
    assert!(!json.contains("flank_speed"));
    assert!(!json.contains("brave_the_void"));
    assert!(!json.contains("shields_holding"));
    assert!(!json.contains("unveil_flagship"));

    // Spectator sees NO pending choice or legal options
    assert!(!json.contains("\"pending_choice\""));
    assert!(!json.contains("opt_carrier"));

    // Spectator sees public counts
    assert!(json.contains("\"action_cards_count\":2"));
    assert!(json.contains("\"action_cards_count\":1"));
}

#[test]
fn state_update_preserves_identical_redaction_guarantees() {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let viewer_b = ViewerRole::Player(PlayerId::new("seat_b"));

    let update = project_state_update(
        "test_game",
        2,
        &game,
        &viewer_b,
        Some((&choice, "nonce_123")),
    );
    let json = serde_json::to_string(&ServerMessage::StateUpdate(update)).expect("serialize");

    assert!(!json.contains("direct_hit"));
    assert!(!json.contains("destroy_their_greatest_ship"));
    assert!(!json.contains("opt_carrier"));
    assert!(!json.contains("\"pending_choice\""));
}

/// The wasm build hands `SessionUpdate` to web2 and the server sends `StateUpdateMsg`. One client
/// reads both, so the fields they share must serialize the same.
#[test]
fn a_session_update_is_the_shared_part_of_a_state_update() {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let mut viewers = vec![ViewerRole::Spectator];
    viewers.extend(
        game.players
            .iter()
            .map(|p| ViewerRole::Player(p.id.clone())),
    );
    let mut asked_seen = false;
    for viewer in &viewers {
        let pending = Some((&choice, "nonce_123"));
        let message = serde_json::to_value(project_state_update("g", 2, &game, viewer, pending))
            .expect("state update");
        let update = serde_json::to_value(ti4_host::projection::project_session_update(
            &game,
            viewer,
            pending,
            &[],
            &ti4_host::projection::facts_context(None),
        ))
        .expect("session update");
        let fields = update.as_object().expect("an object");
        asked_seen |= fields.contains_key("pending_choice");
        for (name, value) in fields {
            assert_eq!(&message[name], value, "{name} for {viewer:?}");
        }
        for name in [
            "viewer",
            "view",
            "turn_status",
            "action_cards",
            "secret_objectives",
            "score",
            "agendas",
        ] {
            assert!(fields.contains_key(name), "{name} for {viewer:?}");
        }
        assert_eq!(
            fields.contains_key("pending_choice"),
            message.get("pending_choice").is_some()
        );
    }
    assert!(asked_seen, "the asked seat is among the viewers");
}

#[test]
fn player_view_uses_the_model_redaction_result_including_search_warrant() {
    let mut game = create_sample_game();
    game.laws.insert("warrant".to_owned(), "seat_a".to_owned());
    let viewer = PlayerId::new("seat_b");
    let expected = view_for(&game, &viewer);

    let snapshot =
        project_initial_snapshot("test_game", 1, &game, &ViewerRole::Player(viewer), None);
    let expected_a = expected.player(&PlayerId::new("seat_a")).unwrap();
    let projected_a = snapshot
        .view
        .players
        .iter()
        .find(|player| player.id == PlayerId::new("seat_a"))
        .unwrap();

    assert!(
        expected_a
            .action_cards
            .iter()
            .all(|card| card.as_str() == HIDDEN)
    );
    assert!(projected_a.held_action_cards.is_empty());
    assert_eq!(
        projected_a.held_secret_objectives, expected_a.secret_objectives,
        "Search Warrant's engine-owned exception must reach the server projection"
    );
}

#[test]
fn spectator_excludes_hidden_markers_from_held_card_lists() {
    let game = create_sample_game();
    let snapshot = project_initial_snapshot("test_game", 1, &game, &ViewerRole::Spectator, None);

    for player in snapshot.view.players {
        assert!(player.held_action_cards.is_empty(), "{}", player.id);
        assert!(player.held_secret_objectives.is_empty(), "{}", player.id);
        assert!(player.action_cards_count > 0 || player.secret_objectives_count > 0);
    }
}

#[test]
fn event_history_is_projected_to_its_explicit_audience() {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let events = vec![
        GameEvent {
            id: "test-1".to_owned(),
            timestamp: "00:00:00".to_owned(),
            version: Some(1),
            visibility: EventVisibility::Public,
            event: GameEventKind::DecisionResolved,
            decision_count: Some(1),
            batch_id: None,
            batch_start_cursor: None,
            batch_end_cursor: None,
            action_id: None,
            action_start_cursor: None,
            actor: None,
            round: None,
            phase: None,
            action_type: None,
            action_actor: None,
            stage: None,
            detail: None,
            movement: None,
            fact: None,
            seat_detail: Some(ti4_host::protocol::server::SeatDecisionDetail {
                seat: PlayerId::new("seat_a"),
                detail: "Private objective: hidden_test".into(),
            }),
            private_detail: None,
            roll: None,
            board: None,
        },
        GameEvent {
            id: "test-2".to_owned(),
            timestamp: "00:00:01".to_owned(),
            version: Some(1),
            visibility: EventVisibility::Seat(PlayerId::new("seat_a")),
            event: GameEventKind::DecisionResolved,
            decision_count: Some(2),
            batch_id: None,
            batch_start_cursor: None,
            batch_end_cursor: None,
            action_id: None,
            action_start_cursor: None,
            actor: None,
            round: None,
            phase: None,
            action_type: None,
            action_actor: None,
            stage: None,
            detail: None,
            movement: None,
            fact: None,
            seat_detail: None,
            private_detail: None,
            roll: None,
            board: None,
        },
    ];

    let actor = ti4_host::projection::project_initial_snapshot_with_map(
        "test_game",
        1,
        &game,
        &ViewerRole::Player(PlayerId::new("seat_a")),
        Some((&choice, "nonce_123")),
        &[],
        &ti4_host::map::GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        None,
        None,
        &events,
    );
    let opponent = ti4_host::projection::project_initial_snapshot_with_map(
        "test_game",
        1,
        &game,
        &ViewerRole::Player(PlayerId::new("seat_b")),
        Some((&choice, "nonce_123")),
        &[],
        &ti4_host::map::GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        None,
        None,
        &events,
    );

    assert_eq!(actor.events.len(), 2);
    assert_eq!(
        actor.events[0].private_detail.as_deref(),
        Some("Private objective: hidden_test")
    );
    assert!(actor.events.iter().all(|event| event.seat_detail.is_none()));
    assert_eq!(opponent.events.len(), 1);
    assert_eq!(opponent.events[0].id, "test-1");
    assert!(opponent.events[0].private_detail.is_none());
    let spectator = ti4_host::projection::project_initial_snapshot(
        "test_game",
        1,
        &game,
        &ViewerRole::Spectator,
        None,
    );
    assert!(spectator.events.is_empty());
    let spectator = ti4_host::projection::project_initial_snapshot_with_map(
        "test_game",
        1,
        &game,
        &ViewerRole::Spectator,
        None,
        &[],
        &ti4_host::map::GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        None,
        None,
        &events,
    );
    assert!(spectator.events[0].private_detail.is_none());
    assert!(
        !serde_json::to_string(&spectator.events)
            .unwrap()
            .contains("hidden_test")
    );
}

/// A game of eight on its map in the late game of the scenarios, where every seat holds action
/// cards and secret objectives, with what the facts are made from.
fn late_game() -> (
    ti4_model::state::GameState,
    ti4_content::galaxy::Galaxy,
    Vec<ti4_host::protocol::view::BoardTileView>,
) {
    use ti4_model::content_types::POK;
    let content = ti4_content::ContentStore::embedded();
    let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f", "g", "h"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();
    let loader = ti4_host::maps::TemplateLoader::load().expect("the templates load");
    let template = ti4_host::maps::default_template_for(content, &loader, 8, POK)
        .expect("a template for eight");
    let (mut state, mut galaxy) =
        ti4_host::map::create_game_with_template(content, &ids, 42, Some(&template))
            .expect("a game");
    ti4_host::view::scenario::apply(
        ti4_host::view::scenario::LATE_GAME,
        &mut state,
        &mut galaxy,
        content,
        POK,
    )
    .expect("the scenario applies");
    // Every seat has something to hide, also where the scenario gives it nothing.
    for index in 0..state.players.len() {
        if state.players[index].action_cards.is_empty() {
            let card = state.action_card_deck.remove(0);
            state.players[index].action_cards.push(card);
        }
        if state.players[index].secret_objectives.is_empty() {
            let secret = state.secret_deck.remove(0);
            state.players[index].secret_objectives.push(secret);
        }
    }
    let tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    (state, galaxy, tiles)
}

/// The facts of a message and the hands of its player views, as they are sent.
fn facts_and_hands(message: &serde_json::Value) -> String {
    let mut sent = serde_json::Map::new();
    for name in [
        "tactical",
        "exploration",
        "action_cards",
        "secret_objectives",
        "score",
        "agendas",
    ] {
        if let Some(fact) = message.get(name) {
            sent.insert(name.to_owned(), fact.clone());
        }
    }
    let hands: Vec<serde_json::Value> = message["view"]["players"]
        .as_array()
        .expect("the players")
        .iter()
        .map(|player| {
            serde_json::json!([
                player["held_action_cards"],
                player["held_secret_objectives"],
                player["scored_secret_objectives"],
            ])
        })
        .collect();
    sent.insert("hands".to_owned(), hands.into());
    serde_json::Value::Object(sent).to_string()
}

#[test]
fn the_facts_show_no_hand_of_another_seat_and_no_order_of_a_deck() {
    let (state, galaxy, tiles) = late_game();
    let layout = ti4_host::map::GalaxyLayout::from_galaxy(&galaxy, ti4_model::content_types::POK);
    let quoted = |id: &str| format!("\"{id}\"");
    let mut viewers = vec![ViewerRole::Spectator];
    viewers.extend(
        state
            .players
            .iter()
            .map(|p| ViewerRole::Player(p.id.clone())),
    );
    for viewer in &viewers {
        let snapshot = ti4_host::projection::project_initial_snapshot_with_map(
            "g",
            1,
            &state,
            viewer,
            None,
            &tiles,
            &layout,
            Some(&galaxy),
            None,
            &[],
        );
        let update = ti4_host::projection::project_state_update_with_map(
            "g",
            1,
            &state,
            viewer,
            None,
            &tiles,
            &layout,
            Some(&galaxy),
            None,
        );
        // A snapshot and an update have the same facts.
        let snapshot = serde_json::to_value(&snapshot).expect("the snapshot");
        let update = serde_json::to_value(&update).expect("the update");
        let sent = facts_and_hands(&update);
        assert_eq!(facts_and_hands(&snapshot), sent, "{viewer:?}");
        for name in [
            "exploration",
            "action_cards",
            "secret_objectives",
            "score",
            "agendas",
        ] {
            assert!(update.get(name).is_some(), "{name} for {viewer:?}");
        }
        assert!(update.get("tactical").is_none(), "nobody is asked");

        // No card of a hand of another seat, and no objective that it has not scored.
        for player in &state.players {
            let own = viewer.is_actor(&player.id);
            assert!(!player.action_cards.is_empty() && !player.secret_objectives.is_empty());
            for card in &player.action_cards {
                assert_eq!(
                    sent.contains(&quoted(card.as_str())),
                    own,
                    "{card} of {} for {viewer:?}",
                    player.id
                );
            }
            for secret in &player.secret_objectives {
                assert_eq!(
                    sent.contains(&quoted(secret.as_str())),
                    own,
                    "{secret} of {} for {viewer:?}",
                    player.id
                );
            }
        }
        // No card of a deck whose cards are hidden: these facts have the count only.
        assert!(!state.action_card_deck.is_empty() && !state.secret_deck.is_empty());
        for card in &state.action_card_deck {
            assert!(
                !sent.contains(&quoted(card.as_str())),
                "{card} for {viewer:?}"
            );
        }
        for secret in &state.secret_deck {
            assert!(
                !sent.contains(&quoted(secret.as_str())),
                "{secret} for {viewer:?}"
            );
        }
        for agenda in &state.agenda_deck {
            assert!(
                !update["agendas"]
                    .to_string()
                    .contains(&quoted(agenda.as_str())),
                "{agenda} for {viewer:?}"
            );
        }
        assert_eq!(update["action_cards"]["deck"], state.action_card_deck.len());
        assert_eq!(update["secret_objectives"]["deck"], state.secret_deck.len());
        // An exploration deck is sent as what it holds, sorted: its order is not.
        let mut shuffled = false;
        for (deck, cards) in &state.exploration_decks {
            let sent: Vec<String> =
                serde_json::from_value(update["exploration"]["decks"][deck.as_str()].clone())
                    .expect("the cards of the deck");
            let mut sorted = cards.clone();
            sorted.sort();
            assert_eq!(sent, sorted, "{deck} for {viewer:?}");
            shuffled |= *cards != sorted;
        }
        assert!(
            shuffled,
            "a deck of the game is in another order than it is sent"
        );
        // The limit of a hand is of the viewer: a spectator has none.
        assert_eq!(
            update["action_cards"].get("hand_limit").is_some(),
            matches!(viewer, ViewerRole::Player(_)),
            "{viewer:?}"
        );
    }
}

#[test]
fn secret_objective_progress_reaches_only_the_holding_seat() {
    use ti4_model::id::SecretObjectiveId;

    let mut game = create_sample_game();
    // seat_a holds a countable secret and a feat secret; seat_b holds a countable one whose
    // identity Search Warrant turns face up for everybody.
    game.player_mut(&PlayerId::new("seat_a"))
        .unwrap()
        .secret_objectives = vec![
        SecretObjectiveId::new("ctr"),
        SecretObjectiveId::new("dtgs"),
    ];
    game.player_mut(&PlayerId::new("seat_b"))
        .unwrap()
        .secret_objectives = vec![SecretObjectiveId::new("eap")];
    game.laws.insert("warrant".to_owned(), "seat_b".to_owned());

    let progress_of = |viewer: ViewerRole, seat: &str| {
        let snapshot = project_initial_snapshot("test_game", 1, &game, &viewer, None);
        snapshot
            .view
            .players
            .iter()
            .find(|player| player.id == PlayerId::new(seat))
            .unwrap()
            .secret_objective_progress
            .clone()
    };

    let own_a = progress_of(ViewerRole::Player(PlayerId::new("seat_a")), "seat_a");
    let ctr = own_a
        .get(&SecretObjectiveId::new("ctr"))
        .expect("own countable secret has progress");
    assert_eq!(ctr.threshold, 6);
    assert_eq!(ctr.satisfied, ctr.have >= 6);
    assert!(
        !own_a.contains_key(&SecretObjectiveId::new("dtgs")),
        "event-conditioned secrets have no running count"
    );

    let own_b = progress_of(ViewerRole::Player(PlayerId::new("seat_b")), "seat_b");
    assert_eq!(own_b[&SecretObjectiveId::new("eap")].threshold, 4);

    // Nobody else learns either seat's progress, not even for a law-revealed secret.
    for (viewer, seat) in [
        (ViewerRole::Player(PlayerId::new("seat_b")), "seat_a"),
        (ViewerRole::Player(PlayerId::new("seat_c")), "seat_a"),
        (ViewerRole::Spectator, "seat_a"),
        (ViewerRole::Player(PlayerId::new("seat_a")), "seat_b"),
        (ViewerRole::Player(PlayerId::new("seat_c")), "seat_b"),
        (ViewerRole::Spectator, "seat_b"),
    ] {
        assert!(
            progress_of(viewer.clone(), seat).is_empty(),
            "{viewer:?} must not see {seat}'s secret progress"
        );
    }
    for viewer in [
        ViewerRole::Player(PlayerId::new("seat_b")),
        ViewerRole::Player(PlayerId::new("seat_c")),
        ViewerRole::Spectator,
    ] {
        let snapshot = project_initial_snapshot("test_game", 1, &game, &viewer, None);
        let json = serde_json::to_string(&snapshot.view).expect("serialize");
        assert!(
            !json.contains("\"ctr\""),
            "{viewer:?} learned seat_a's secret"
        );
    }
}

/// How large an update of a game of eight is with its facts. web2 reads the view and the facts;
/// `state` is for `web/`.
#[test]
fn an_update_of_eight_seats_with_its_facts_stays_small() {
    let (state, galaxy, tiles) = late_game();
    let layout = ti4_host::map::GalaxyLayout::from_galaxy(&galaxy, ti4_model::content_types::POK);
    let update = ti4_host::projection::project_state_update_with_map(
        "g",
        1,
        &state,
        &ViewerRole::Player(PlayerId::new("a")),
        None,
        &tiles,
        &layout,
        Some(&galaxy),
        None,
    );
    let whole = serde_json::to_value(ServerMessage::StateUpdate(update)).expect("the update");
    let size = |value: &serde_json::Value| value.to_string().len();
    let facts: usize = [
        "exploration",
        "action_cards",
        "secret_objectives",
        "score",
        "agendas",
    ]
    .iter()
    .map(|name| size(&whole[*name]))
    .sum();
    println!(
        "update of eight seats: {} bytes; facts {facts}, view {}, state {}, galaxy_layout {}",
        size(&whole),
        size(&whole["view"]),
        size(&whole["state"]),
        size(&whole["galaxy_layout"]),
    );
    assert!(facts < 32 * 1024, "the facts are {facts} bytes");
    assert!(
        size(&whole) < 256 * 1024,
        "the update is {} bytes",
        size(&whole)
    );
}

#[test]
fn objective_progress_details_follow_the_redaction_split() {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{ObjectiveId, PlanetId, SecretObjectiveId};

    let content = ContentStore::embedded();
    let seats: Vec<PlayerId> = ["seat_a", "seat_b", "seat_c"]
        .into_iter()
        .map(PlayerId::new)
        .collect();
    let (mut game, galaxy) =
        ti4_host::map::create_game_with_template(content, &seats, 7, None).expect("map game");
    // The generated map has no anomaly; swap one blue tile for a gravity rift so Learn the
    // Secrets has something to count. The projection rebuilds adjacency from these tiles.
    let mut tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    let rift = tiles
        .iter_mut()
        .find(|tile| tile.system_id == "19")
        .expect("tile 19 is on the map");
    rift.system_id = "41".to_owned();
    // The real board's tiles include the Fracture and the parked Wormhole Nexus; the projection
    // must still rebuild a galaxy from them, or every map-shaped count goes missing.
    let galaxy = ti4_host::projection::placed_galaxy(&tiles).expect("tiles rebuild a galaxy");
    assert!(galaxy.adjacent("18").len() >= 6, "Mecatol keeps its ring");
    let layout = ti4_host::map::GalaxyLayout::from_galaxy(&galaxy, POK);

    // Everyone shares Mecatol Rex, so all three are neighbours; seat_a also holds it.
    let mecatol = SystemId::new("18");
    for seat in &seats {
        game.system_mut(&mecatol)
            .units
            .push(Unit::new(UnitTypeId::new("cruiser"), seat.clone()));
    }
    game.system_mut(&mecatol)
        .planet_control
        .insert(PlanetId::new("mr"), seats[0].clone());
    game.revealed_objectives
        .push(ObjectiveId::new("push_boundaries"));

    // seat_a holds Learn the Secrets of the Cosmos and parks a ship beside the rift.
    let beside_anomaly = galaxy
        .adjacent("41")
        .into_iter()
        .find(|id| *id != "18")
        .map(SystemId::new)
        .expect("the rift has a neighbour");
    game.system_mut(&beside_anomaly)
        .units
        .push(Unit::new(UnitTypeId::new("destroyer"), seats[0].clone()));
    game.player_mut(&seats[0]).unwrap().secret_objectives = vec![SecretObjectiveId::new("lsc")];

    let view_of = |viewer: &ViewerRole| {
        ti4_host::projection::project_initial_snapshot_with_map(
            "test_game",
            1,
            &game,
            viewer,
            None,
            &tiles,
            &layout,
            None,
            None,
            &[],
        )
        .view
    };

    // Push Boundaries: every seat's per-neighbour comparison reaches every viewer (planet control
    // is public), and the rows agree with the count beside them.
    for viewer in [
        ViewerRole::Player(seats[0].clone()),
        ViewerRole::Player(seats[1].clone()),
        ViewerRole::Spectator,
    ] {
        let view = view_of(&viewer);
        for seat in &seats {
            let progress =
                &view.table.objective_progress[seat][&ObjectiveId::new("push_boundaries")];
            assert_eq!(progress.neighbours.len(), 2, "{seat} has two neighbours");
            for row in &progress.neighbours {
                assert_ne!(&row.player, seat);
                assert_eq!(row.ahead, row.mine > row.theirs);
            }
            let ahead = progress.neighbours.iter().filter(|row| row.ahead).count();
            assert_eq!(u32::try_from(ahead).unwrap(), progress.have);
        }
        let a = &view.table.objective_progress[&seats[0]][&ObjectiveId::new("push_boundaries")];
        let b = &view.table.objective_progress[&seats[1]][&ObjectiveId::new("push_boundaries")];
        let a_vs_b = a
            .neighbours
            .iter()
            .find(|row| row.player == seats[1])
            .unwrap();
        let b_vs_a = b
            .neighbours
            .iter()
            .find(|row| row.player == seats[0])
            .unwrap();
        assert_eq!((a_vs_b.mine, a_vs_b.theirs), (b_vs_a.theirs, b_vs_a.mine));
    }

    // Learn the Secrets: the holder sees the qualifying systems; nobody else sees anything.
    let own = view_of(&ViewerRole::Player(seats[0].clone()));
    let lsc = &own
        .players
        .iter()
        .find(|player| player.id == seats[0])
        .unwrap()
        .secret_objective_progress[&SecretObjectiveId::new("lsc")];
    assert_eq!(lsc.threshold, 3);
    assert!(lsc.systems.contains(&beside_anomaly));
    assert_eq!(u32::try_from(lsc.systems.len()).unwrap(), lsc.have);
    assert!(lsc.neighbours.is_empty());
    for viewer in [ViewerRole::Player(seats[1].clone()), ViewerRole::Spectator] {
        let view = view_of(&viewer);
        let held = view
            .players
            .iter()
            .find(|player| player.id == seats[0])
            .unwrap();
        assert!(held.secret_objective_progress.is_empty());
        assert!(
            !serde_json::to_string(&view.players)
                .unwrap()
                .contains("\"lsc\"")
        );
    }
}
