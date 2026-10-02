#![allow(clippy::too_many_lines)]

use std::sync::Arc;
use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::replay::replay_session;
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

#[test]
fn recovery_replay_produces_identical_canonical_hashes_and_event_logs() {
    let p1 = PlayerId::new("p1");
    let p2 = PlayerId::new("p2");
    let p3 = PlayerId::new("p3");
    let players = vec![p1.clone(), p2.clone(), p3.clone()];

    let content = ContentStore::embedded();
    let seed = 424_242;
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(content, &players, seed).expect("create map");
    let map_tiles = ti4_server::map::build_board_tiles(content, &galaxy);

    let config = SessionConfig::new("game_recovery_test", state.clone())
        .with_galaxy(galaxy.clone(), map_tiles)
        .with_seat(p1.clone(), SeatController::Human)
        .with_seat(p2.clone(), SeatController::Human)
        .with_seat(p3.clone(), SeatController::BotFirstOption);

    let session = Arc::new(GameSession::start(config));

    // Connect continuous client before moves
    let client1 = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));
    let client_spec = MockClient::connect(session.clone(), ViewerRole::Spectator);

    let mut continuous_events = Vec::new();

    // The initial snapshot has the game initialized event
    let snap1 = client1.snapshot();
    continuous_events.extend(snap1.events);

    // P1 receives choice for strategy card
    let pending_1 = loop {
        match client1.recv().expect("recv") {
            ServerMessage::Event(e) => continuous_events.push(e.entry),
            ServerMessage::PendingChoice(choice_msg) => break choice_msg,
            _ => {}
        }
    };

    assert_eq!(pending_1.choice.player, p1);
    let chosen_1 = &pending_1.choice.options[0].id;

    // P1 submits choice
    let accepted_1 = session
        .submit_choice(&p1, &pending_1.nonce, 2, chosen_1)
        .expect("submit p1");
    assert_eq!(&accepted_1.option_id, chosen_1);

    // Read continuous events following P1 choice
    let _pending_2 = loop {
        match client1.recv().expect("recv") {
            ServerMessage::Event(e) => continuous_events.push(e.entry),
            ServerMessage::PendingChoice(choice_msg) => {
                if choice_msg.choice.player == p1 {
                    break Some(choice_msg.choice);
                }
            }
            ServerMessage::TurnStatus(_) => {
                // p2 choice is pending
                break None;
            }
            _ => {}
        }
    };

    // P2 submits choice
    let (p2_seat, p2_nonce, p2_version) = session
        .current_pending_decision()
        .expect("pending decision for p2");
    assert_eq!(p2_seat, p2);

    let client2 = MockClient::connect(session.clone(), ViewerRole::Player(p2.clone()));
    let snap2 = client2.snapshot();
    let p2_choice = snap2.pending_choice.expect("p2 pending choice in snapshot");
    let chosen_2 = &p2_choice.choice.options[0].id;

    let accepted_2 = session
        .submit_choice(&p2, &p2_nonce, p2_version, chosen_2)
        .expect("submit p2");
    assert_eq!(&accepted_2.option_id, chosen_2);

    // Await next pending choice so bot P3 moves settle completely
    loop {
        match client1.recv().expect("recv") {
            ServerMessage::Event(e) => continuous_events.push(e.entry),
            ServerMessage::PendingChoice(_) => break,
            ServerMessage::TurnStatus(m)
                if matches!(
                    m.status,
                    ti4_server::protocol::PublicTurnStatus::WaitingForDecision { .. }
                ) =>
            {
                break;
            }
            _ => {}
        }
    }

    // Drain any remaining continuous events
    for msg in client1.drain_messages() {
        if let ServerMessage::Event(e) = msg {
            continuous_events.push(e.entry);
        }
    }
    // A subscribe/snapshot race can deliver the same initial event twice.
    let mut seen_ids = std::collections::BTreeSet::new();
    continuous_events.retain(|event| seen_ids.insert(event.id.clone()));
    while seen_ids.len() < session.event_log().len() {
        if let Ok(ServerMessage::Event(e)) = client1.recv() {
            if seen_ids.insert(e.entry.id.clone()) {
                continuous_events.push(e.entry);
            }
        }
    }
    for msg in client_spec.drain_messages() {
        let _ = msg;
    }

    // Now test reconnection: a reconnecting client connects and receives InitialSnapshot
    let reconnecting_client = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));
    let reconnect_snap = reconnecting_client.snapshot();

    // 1. Verify snapshot event log matches continuous event log
    // Subscription and the first snapshot are separate reads. An event emitted between
    // them may arrive both in the snapshot and in the queue; clients deduplicate by ID.
    assert_eq!(
        reconnect_snap.events, continuous_events,
        "Reconnecting client event log must be identical to continuous client event log"
    );

    // 2. Verify uninterrupted run canonical hashes
    let original_hashes = session.decision_hashes();
    assert!(
        !original_hashes.is_empty(),
        "Expected accepted decisions in decision log"
    );

    // 3. Replay fixture state and accepted decisions directly.
    let direct_report = replay_session(&state, Some(&galaxy), &session.decision_log())
        .expect("direct replay_session");
    assert!(direct_report.hashes_match);
    assert_eq!(direct_report.replayed_hashes, original_hashes);

    session.stop();
}

#[tokio::test]
async fn http_snapshot_fetches_current_state_and_events_for_reconnecting_client() {
    let registry = Arc::new(ti4_server::session::GameRegistry::new());
    let app = ti4_server::create_app(registry.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr = listener.local_addr().expect("local addr");

    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve axum app");
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    // 1. Create a game via HTTP POST
    let create_res = client
        .post(format!("{base_url}/api/games"))
        .json(&serde_json::json!({
            "player_count": 3, "nickname": "Host",
            "seed": 12345
        }))
        .send()
        .await
        .expect("create game request");
    assert_eq!(create_res.status(), reqwest::StatusCode::OK);

    let created: serde_json::Value = create_res.json().await.expect("parse created lobby");
    let game_id = created["game_id"].as_str().expect("generated game ID");
    let p1_id = PlayerId::new(created["player"]["id"].as_str().unwrap());
    let p1_token = created["player_session"]
        .as_str()
        .expect("p1 capability")
        .to_owned();
    let mut credentials = vec![p1_token.clone()];
    for _ in 0..2 {
        let claimed = client
            .post(format!("{base_url}/api/games/{game_id}/lobby/join"))
            .json(&serde_json::json!({ "kind": "new", "nickname": "Guest" }))
            .send()
            .await
            .expect("claim player");
        assert_eq!(claimed.status(), reqwest::StatusCode::OK);
        credentials.push(
            claimed
                .json::<serde_json::Value>()
                .await
                .expect("claim response")["player_session"]
                .as_str()
                .expect("claimed credential")
                .to_owned(),
        );
    }
    for token in &credentials {
        let ready = client
            .post(format!("{base_url}/api/games/{game_id}/lobby/ready"))
            .header("x-ti4-player-session", token)
            .json(&serde_json::json!({ "ready": true }))
            .send()
            .await
            .expect("ready player");
        assert_eq!(ready.status(), reqwest::StatusCode::OK);
    }
    let started = client
        .post(format!("{base_url}/api/games/{game_id}/lobby/start"))
        .header("x-ti4-player-session", &p1_token)
        .send()
        .await
        .expect("start lobby");
    assert_eq!(started.status(), reqwest::StatusCode::OK);

    let session = registry.get_game(game_id).expect("session exists");

    // 2. Wait for initial pending decision to be raised by worker thread
    let mut decision = session.current_pending_decision();
    for _ in 0..50 {
        if decision.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        decision = session.current_pending_decision();
    }
    let (p1_seat, p1_nonce, p1_ver) = decision.expect("p1 pending choice");
    assert_eq!(p1_seat, p1_id);

    let snap_res = client
        .get(format!("{base_url}/api/games/{game_id}/snapshot"))
        .header("x-ti4-player-session", &p1_token)
        .send()
        .await
        .expect("fetch snapshot");
    assert_eq!(snap_res.status(), reqwest::StatusCode::OK);

    let snapshot = match snap_res
        .json::<ti4_server::protocol::server::ServerMessage>()
        .await
        .expect("deserialize snapshot")
    {
        ti4_server::protocol::server::ServerMessage::InitialSnapshot(snapshot) => snapshot,
        message => panic!("expected initial snapshot, got {message:?}"),
    };
    assert_eq!(snapshot.game_id, game_id);
    assert!(
        !snapshot.events.is_empty(),
        "Expected initial events in snapshot"
    );
    assert!(matches!(
        snapshot.events[0].event,
        ti4_server::protocol::GameEventKind::GameInitialized { .. }
    ));

    // 3. Submit an action and verify snapshot updates event log
    let p1_opt = snapshot
        .pending_choice
        .expect("p1 pending choice")
        .choice
        .options[0]
        .id
        .clone();
    session
        .submit_choice(&p1_seat, &p1_nonce, p1_ver, &p1_opt)
        .expect("submit choice");

    // Wait for the next decision to be raised so the transition settles
    for _ in 0..50 {
        if let Some((seat, _, _)) = session.current_pending_decision()
            && seat != p1_seat
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }

    // 4. Reconnecting client fetches snapshot via HTTP
    let reconnect_res = client
        .get(format!("{base_url}/api/games/{game_id}/snapshot"))
        .header("x-ti4-player-session", &p1_token)
        .send()
        .await
        .expect("fetch reconnect snapshot");
    assert_eq!(reconnect_res.status(), reqwest::StatusCode::OK);

    let reconnect_snap = match reconnect_res
        .json::<ti4_server::protocol::server::ServerMessage>()
        .await
        .expect("deserialize reconnect snapshot")
    {
        ti4_server::protocol::server::ServerMessage::InitialSnapshot(snapshot) => snapshot,
        message => panic!("expected initial snapshot, got {message:?}"),
    };

    assert!(reconnect_snap.events.iter().any(|e| matches!(
        e.event,
        ti4_server::protocol::GameEventKind::DecisionResolved
    )));
    assert_eq!(reconnect_snap.events.len(), session.event_log().len());

    session.stop();
}
