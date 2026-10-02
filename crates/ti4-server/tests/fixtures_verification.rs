use ti4_server::fixtures::{
    sample_actor_snapshot, sample_opponent_snapshot, sample_spectator_snapshot,
    sample_stale_submission_rejected, sample_terminal_game_over,
};
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::RejectionReason;
use ti4_server::protocol::{PROTOCOL_VERSION, parse_server_message};

#[test]
fn ensure_and_verify_fixtures() {
    let fixtures = [
        ("actor_snapshot.json", sample_actor_snapshot()),
        ("opponent_snapshot.json", sample_opponent_snapshot()),
        ("spectator_snapshot.json", sample_spectator_snapshot()),
        (
            "stale_submission_rejected.json",
            sample_stale_submission_rejected(),
        ),
        ("terminal_game_over.json", sample_terminal_game_over()),
    ];

    for (name, expected_msg) in &fixtures {
        if std::env::var("UPDATE_FIXTURES").is_ok() {
            let pretty = serde_json::to_string_pretty(expected_msg).expect("pretty");
            std::fs::write(
                format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR")),
                format!("{pretty}\n"),
            )
            .expect("write fixture");
        }
        let persisted =
            std::fs::read_to_string(format!("{}/fixtures/{name}", env!("CARGO_MANIFEST_DIR")))
                .expect("read checked-in fixture");
        let checked_in: serde_json::Value = serde_json::from_str(&persisted).expect("fixture JSON");
        let generated = serde_json::to_value(expected_msg).expect("generate fixture JSON");
        assert_eq!(
            checked_in, generated,
            "Fixture {name} differs from the versioned generator"
        );
        let content = serde_json::to_string(expected_msg).expect("serialize fixture");
        let parsed: ServerMessage = parse_server_message(&content).expect("parse fixture");

        assert_eq!(&parsed, expected_msg, "Fixture {name} content mismatch");
        assert_eq!(parsed.protocol_version(), PROTOCOL_VERSION);
    }
}

#[test]
fn fixture_actor_snapshot_has_choice_and_private_cards() {
    let msg = sample_actor_snapshot();

    match msg {
        ServerMessage::InitialSnapshot(snapshot) => {
            assert!(snapshot.pending_choice.is_some());
            let choice = snapshot.pending_choice.unwrap();
            assert_eq!(choice.choice.player.as_str(), "seat_a");
            assert_eq!(choice.choice.options.len(), 3);
            assert!(choice.choice.context.is_some());
            assert_eq!(choice.choice.context.unwrap().outstanding.len(), 1);

            let player_a = snapshot
                .view
                .players
                .iter()
                .find(|p| p.id.as_str() == "seat_a")
                .unwrap();
            assert_eq!(player_a.held_action_cards.len(), 2);
            assert_eq!(player_a.held_secret_objectives.len(), 1);
        }
        other => panic!("Expected InitialSnapshot, got {other:?}"),
    }
}

#[test]
fn fixture_opponent_snapshot_has_no_private_cards_or_choice() {
    let msg = sample_opponent_snapshot();

    match msg {
        ServerMessage::InitialSnapshot(snapshot) => {
            // No choice for opponent
            assert!(snapshot.pending_choice.is_none());

            // Actor seat_a has private cards redacted
            let player_a = snapshot
                .view
                .players
                .iter()
                .find(|p| p.id.as_str() == "seat_a")
                .unwrap();
            assert!(player_a.held_action_cards.is_empty());
            assert!(player_a.held_secret_objectives.is_empty());
            assert_eq!(player_a.action_cards_count, 2);
            assert_eq!(player_a.secret_objectives_count, 1);

            // Opponent seat_b sees own cards
            let player_b = snapshot
                .view
                .players
                .iter()
                .find(|p| p.id.as_str() == "seat_b")
                .unwrap();
            assert_eq!(player_b.held_action_cards.len(), 1);
            assert_eq!(player_b.held_secret_objectives.len(), 1);
        }
        other => panic!("Expected InitialSnapshot, got {other:?}"),
    }
}

#[test]
fn fixture_spectator_snapshot_redacts_all_private_cards() {
    let msg = sample_spectator_snapshot();

    match msg {
        ServerMessage::InitialSnapshot(snapshot) => {
            assert!(snapshot.pending_choice.is_none());
            for p in &snapshot.view.players {
                assert!(p.held_action_cards.is_empty());
                assert!(p.held_secret_objectives.is_empty());
            }
        }
        other => panic!("Expected InitialSnapshot, got {other:?}"),
    }
}

#[test]
fn fixture_stale_submission_rejected() {
    let msg = sample_stale_submission_rejected();

    match msg {
        ServerMessage::ActionRejected(rejected) => {
            assert_eq!(
                rejected.reason,
                RejectionReason::StaleVersion {
                    expected: 40,
                    current: 42
                }
            );
        }
        other => panic!("Expected ActionRejected, got {other:?}"),
    }
}

#[test]
fn fixture_terminal_game_over() {
    let msg = sample_terminal_game_over();

    match msg {
        ServerMessage::GameOver(game_over) => {
            assert_eq!(game_over.winner.unwrap().as_str(), "seat_a");
            assert_eq!(game_over.final_scores.len(), 3);
        }
        other => panic!("Expected GameOver, got {other:?}"),
    }
}
