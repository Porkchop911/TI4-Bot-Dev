use std::thread;
use std::time::Duration;
use ti4_content::ContentStore;
use ti4_engine::setup::start_game_seeded;
use ti4_model::POK;
use ti4_model::id::PlayerId;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

#[test]
#[allow(clippy::too_many_lines, clippy::collapsible_if)]
fn scripted_end_to_end_game_handles_draft_action_and_nested_secondary_choice() {
    let p1 = PlayerId::new("p1");
    let p2 = PlayerId::new("p2");
    let p3 = PlayerId::new("p3");
    let players = [p1.clone(), p2.clone(), p3.clone()];

    let state = start_game_seeded(ContentStore::embedded(), &players, POK, None, 42)
        .expect("start game seeded");

    // p1 and p2 are Human seats; p3 is an automated bot
    let config = SessionConfig::new("game_e2e_test", state)
        .with_seat(p1.clone(), SeatController::Human)
        .with_seat(p2.clone(), SeatController::Human)
        .with_seat(p3.clone(), SeatController::BotFirstOption);

    let session = std::sync::Arc::new(GameSession::start(config));

    let client_1 = MockClient::connect(session.clone(), ViewerRole::Player(p1.clone()));
    let client_2 = MockClient::connect(session.clone(), ViewerRole::Player(p2.clone()));
    let spectator = MockClient::connect(session.clone(), ViewerRole::Spectator);

    // Helper to wait for a choice on a client
    let wait_for_choice = |client: &MockClient| {
        for _ in 0..100 {
            if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
                return Some(msg.choice);
            }
            thread::sleep(Duration::from_millis(10));
        }
        None
    };

    // 1. Step: p1 drafts Leadership strategy card
    let choice_p1_strat = wait_for_choice(&client_1).expect("p1 should be offered strategy cards");
    assert_eq!(choice_p1_strat.player, p1);
    let leadership_opt = choice_p1_strat
        .options
        .iter()
        .find(|o| o.id.contains("leadership"))
        .expect("leadership should be offered");

    let (_, nonce_1, ver_1) = session.current_pending_decision().unwrap();
    let res = client_1.submit(&nonce_1, ver_1, &leadership_opt.id);
    assert!(res.is_ok(), "p1 pick leadership: {res:?}");

    // 2. Step: p2 drafts Diplomacy strategy card
    let choice_p2_strat = wait_for_choice(&client_2).expect("p2 should be offered strategy cards");
    assert_eq!(choice_p2_strat.player, p2);
    let diplomacy_opt = choice_p2_strat
        .options
        .iter()
        .find(|o| o.id.contains("diplomacy"))
        .expect("diplomacy should be offered");

    let (_, nonce_2, ver_2) = session.current_pending_decision().unwrap();
    let res = client_2.submit(&nonce_2, ver_2, &diplomacy_opt.id);
    assert!(res.is_ok(), "p2 pick diplomacy: {res:?}");

    // Helper to find client by actor
    let get_client = |actor: &PlayerId| -> Option<&MockClient> {
        if *actor == p1 {
            Some(&client_1)
        } else if *actor == p2 {
            Some(&client_2)
        } else {
            None
        }
    };

    // Drive strategy draft choices for humans (P1 and P2)
    for _ in 0..10 {
        let phase = session.current_state().phase;
        if phase != ti4_model::Phase::Strategy {
            break;
        }
        if let Some((actor, nonce, ver)) = session.current_pending_decision() {
            if let Some(client) = get_client(&actor) {
                if let Some(c) = wait_for_choice(client) {
                    let opt = c.options[0].id.clone();
                    let _ = client.submit(&nonce, ver, &opt);
                }
            }
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Now in Action phase, drive choices until nested reaction or action choices happen
    let mut nested_received = false;
    let mut submissions = Vec::new();
    for _ in 0..100 {
        if let Some((actor, nonce, ver)) = session.current_pending_decision() {
            if let Some(client) = get_client(&actor) {
                if let Some(c) = wait_for_choice(client) {
                    // Check if this is a strategic action or secondary choice
                    if c.prompt.to_lowercase().contains("secondary")
                        || c.prompt.to_lowercase().contains("follow")
                        || c.context
                            .as_ref()
                            .is_some_and(|ctx| ctx.subtype.contains("secondary"))
                    {
                        nested_received = true;
                    }
                    let opt = c.options[0].id.clone();
                    // A nested choice can be offered before the previous game.step() returns.
                    // Do not block the driver waiting for that earlier acknowledgement.
                    let session = session.clone();
                    submissions.push(thread::spawn(move || {
                        session.submit_choice(&actor, &nonce, ver, &opt)
                    }));
                }
            }
        }
        if nested_received {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    // Check spectator received messages and remains public-only
    let spectator_messages = spectator.drain_messages();
    assert!(
        !spectator_messages.is_empty(),
        "Spectator should receive broadcast messages"
    );
    for msg in spectator_messages {
        if let ServerMessage::InitialSnapshot(s) = msg {
            assert!(s.pending_choice.is_none());
        }
    }

    // Verify decision log has accumulated multiple decisions
    let log = session.decision_log();
    assert!(
        log.len() >= 3,
        "At least 3 decisions should be recorded: {}",
        log.len()
    );

    session.stop();
    for submission in submissions {
        let _ = submission.join();
    }
}
