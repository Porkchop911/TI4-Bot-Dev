use std::thread;
use std::time::Duration;
use ti4_model::id::PlayerId;
use ti4_server::fixtures::create_sample_game;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

fn drive_session(game_id: &str, steps_to_take: usize) -> Vec<String> {
    let state = create_sample_game();
    let seat_a = PlayerId::new("seat_a");
    let seat_b = PlayerId::new("seat_b");

    let config = SessionConfig::new(game_id, state)
        .with_seat(seat_a.clone(), SeatController::Human)
        .with_seat(seat_b.clone(), SeatController::BotFirstOption);

    let session = std::sync::Arc::new(GameSession::start(config));
    let client = MockClient::connect(session.clone(), ViewerRole::Player(seat_a));

    for _ in 0..steps_to_take {
        let mut choice_opt = None;
        for _ in 0..50 {
            if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
                choice_opt = Some(msg.choice);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        if let Some(choice) = choice_opt {
            let option_id = choice.options[0].id.clone();
            let (_, nonce, version) = session.current_pending_decision().expect("pending");
            let res = client.submit(&nonce, version, &option_id);
            assert!(res.is_ok(), "submit failed: {res:?}");
        } else {
            break;
        }
    }

    // Wait briefly for last step to settle
    thread::sleep(Duration::from_millis(50));

    let decision_hashes = session
        .decision_hashes()
        .into_iter()
        .map(|h| h.digest)
        .collect();
    session.stop();
    decision_hashes
}

#[test]
fn deterministic_replay_produces_identical_decision_logs() {
    let decisions_1 = drive_session("game_determinism_1", 3);
    let decisions_2 = drive_session("game_determinism_2", 3);

    assert!(
        !decisions_1.is_empty(),
        "At least one decision should be recorded"
    );
    assert_eq!(
        decisions_1, decisions_2,
        "Decision hashes must be byte-for-byte identical across runs"
    );
}
