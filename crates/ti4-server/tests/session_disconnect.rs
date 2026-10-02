use std::thread;
use std::time::Duration;
use ti4_model::id::PlayerId;
use ti4_server::fixtures::create_sample_game;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

#[test]
fn disconnected_client_leaves_decision_pending_without_defaulting() {
    let state = create_sample_game();
    let seat_a = PlayerId::new("seat_a");
    let seat_b = PlayerId::new("seat_b");

    let config = SessionConfig::new("test_disconnect", state)
        .with_seat(seat_a.clone(), SeatController::Human)
        .with_seat(seat_b.clone(), SeatController::BotFirstOption);

    let session = std::sync::Arc::new(GameSession::start(config));

    let (nonce, version, option_id) = {
        // Connect client
        let client = MockClient::connect(session.clone(), ViewerRole::Player(seat_a.clone()));

        // Receive pending choice
        let mut pending = None;
        for _ in 0..50 {
            if let Ok(ServerMessage::PendingChoice(msg)) = client.try_recv() {
                pending = Some((msg.nonce, msg.choice));
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }

        let (n, choice) = pending.expect("choice should be received");
        let opt = choice.options[0].id.clone();
        let v = session
            .current_pending_decision()
            .map(|(_, _, ver)| ver)
            .unwrap();

        (n, v, opt)
        // client is dropped here (disconnected)
    };

    let log_len_before = session.decision_log().len();

    // Sleep to simulate elapsed time without client response
    thread::sleep(Duration::from_millis(100));

    // Assert the session has NOT picked a default option and is still awaiting seat_a
    assert!(
        !session.is_finished(),
        "Session must not complete on disconnect"
    );
    assert_eq!(
        session.decision_log().len(),
        log_len_before,
        "Decision log must not grow while disconnected"
    );
    assert_eq!(
        session.current_pending_decision(),
        Some((seat_a.clone(), nonce.clone(), version)),
        "Decision must remain pending on the exact same nonce and version"
    );

    // Reconnecting client: snapshot reflects the pending choice
    let reconnect_client = MockClient::connect(session.clone(), ViewerRole::Player(seat_a.clone()));
    let snapshot = reconnect_client.snapshot();
    assert!(snapshot.pending_choice.is_some());
    assert_eq!(snapshot.pending_choice.as_ref().unwrap().nonce, nonce);

    // Reconnected client submits the choice successfully
    let res = reconnect_client.submit(&nonce, version, &option_id);
    assert!(
        res.is_ok(),
        "Submission after reconnect should succeed: {res:?}"
    );

    session.stop();
}
