#![allow(clippy::too_many_lines)]

use std::thread;
use std::time::Duration;
use ti4_model::id::PlayerId;
use ti4_server::fixtures::create_sample_game;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::{RejectionReason, ViewerRole};
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

#[test]
fn wrong_seat_stale_nonce_and_unknown_option_preserve_state_and_log() {
    let state = create_sample_game();
    let seat_a = PlayerId::new("seat_a");
    let seat_b = PlayerId::new("seat_b");
    let seat_c = PlayerId::new("seat_c");

    let config = SessionConfig::new("test_rejections", state)
        .with_seat(seat_a.clone(), SeatController::Human)
        .with_seat(seat_b.clone(), SeatController::Human)
        .with_seat(seat_c.clone(), SeatController::BotFirstOption);

    let session = std::sync::Arc::new(GameSession::start(config));
    let client_a = MockClient::connect(session.clone(), ViewerRole::Player(seat_a.clone()));
    let client_b = MockClient::connect(session.clone(), ViewerRole::Player(seat_b.clone()));

    // Wait for the first choice for seat_a to arrive
    let mut pending_opt = None;
    for _ in 0..50 {
        if let Ok(ServerMessage::PendingChoice(msg)) = client_a.try_recv() {
            pending_opt = Some((msg.nonce, msg.choice));
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let (nonce, choice) = pending_opt.expect("seat_a should receive pending choice");
    let (pending_seat, pending_nonce, version) = session
        .current_pending_decision()
        .expect("active pending decision");

    assert_eq!(pending_seat, seat_a);
    assert_eq!(pending_nonce, nonce);

    let initial_log_len = session.decision_log().len();
    let valid_option_id = choice.options[0].id.clone();

    // 1. Wrong seat: seat_b attempts to submit choice for seat_a's turn
    let wrong_seat_res = client_b.submit(&nonce, version, &valid_option_id);
    assert_eq!(
        wrong_seat_res,
        Err(RejectionReason::UnauthorizedSeat {
            seat: Some(seat_b.clone())
        })
    );
    assert_eq!(
        session.decision_log().len(),
        initial_log_len,
        "Decision log length must not change on wrong seat"
    );
    assert_eq!(
        session.current_pending_decision(),
        Some((seat_a.clone(), nonce.clone(), version)),
        "Pending decision must remain unchanged"
    );

    // 2. Stale nonce: seat_a submits with bad nonce
    let bad_nonce_res = client_a.submit("bad_nonce_xyz", version, &valid_option_id);
    assert_eq!(bad_nonce_res, Err(RejectionReason::StaleNonce));
    assert_eq!(
        session.decision_log().len(),
        initial_log_len,
        "Decision log length must not change on stale nonce"
    );

    // 3. Stale version: seat_a submits with bad expected_version
    let bad_ver_res = client_a.submit(&nonce, version + 99, &valid_option_id);
    assert_eq!(
        bad_ver_res,
        Err(RejectionReason::StaleVersion {
            expected: version + 99,
            current: version,
        })
    );
    assert_eq!(
        session.decision_log().len(),
        initial_log_len,
        "Decision log length must not change on stale version"
    );

    // 4. Unknown option: seat_a submits an option ID not offered by the engine
    let bad_opt_res = client_a.submit(&nonce, version, "unoffered_illegal_option");
    assert_eq!(
        bad_opt_res,
        Err(RejectionReason::UnknownOption {
            option_id: "unoffered_illegal_option".to_owned(),
        })
    );
    assert_eq!(
        session.decision_log().len(),
        initial_log_len,
        "Decision log length must not change on unknown option"
    );

    // 5. Valid submission succeeds
    let valid_res = client_a.submit(&nonce, version, &valid_option_id);
    assert!(valid_res.is_ok(), "Valid submission should succeed");
    let events = session.event_log();
    assert!(
        events.iter().all(|event| matches!(
            event.event,
            ti4_server::protocol::GameEventKind::GameInitialized { .. }
                | ti4_server::protocol::GameEventKind::DecisionResolved
                | ti4_server::protocol::GameEventKind::PhaseTransition { .. }
                | ti4_server::protocol::GameEventKind::GameFinished { .. }
        )),
        "public event history must use the fixed non-private event vocabulary"
    );

    // 6. Duplicate submission: submitting again for the same nonce fails
    let dup_res = client_a.submit(&nonce, version, &valid_option_id);
    assert!(
        matches!(
            dup_res,
            Err(RejectionReason::StaleNonce
                | RejectionReason::StaleVersion { .. }
                | RejectionReason::NoPendingChoice
                | RejectionReason::UnauthorizedSeat { .. })
        ),
        "Duplicate submission must be rejected: got {dup_res:?}"
    );

    session.stop();
}
