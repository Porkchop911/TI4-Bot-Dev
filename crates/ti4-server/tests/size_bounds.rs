use ti4_server::fixtures::{
    sample_actor_snapshot, sample_opponent_snapshot, sample_spectator_snapshot,
    sample_stale_submission_rejected, sample_terminal_game_over,
};
use ti4_server::protocol::PROTOCOL_VERSION;
use ti4_server::protocol::client::ClientMessage;
use ti4_server::protocol::server::ServerMessage;

const SNAPSHOT_BOUND_BYTES: usize = 256 * 1024; // 256 KiB
const CHOICE_BOUND_BYTES: usize = 64 * 1024; // 64 KiB
const CLIENT_MSG_BOUND_BYTES: usize = 4 * 1024; // 4 KiB
const STATUS_BOUND_BYTES: usize = 4 * 1024; // 4 KiB

#[test]
fn snapshot_sizes_remain_within_bounds() {
    let actor_json = serde_json::to_string(&sample_actor_snapshot()).expect("serialize");
    assert!(
        actor_json.len() <= SNAPSHOT_BOUND_BYTES,
        "Actor snapshot exceeded bound: {} bytes > {}",
        actor_json.len(),
        SNAPSHOT_BOUND_BYTES
    );

    let opponent_json = serde_json::to_string(&sample_opponent_snapshot()).expect("serialize");
    assert!(
        opponent_json.len() <= SNAPSHOT_BOUND_BYTES,
        "Opponent snapshot exceeded bound: {} bytes > {}",
        opponent_json.len(),
        SNAPSHOT_BOUND_BYTES
    );

    let spectator_json = serde_json::to_string(&sample_spectator_snapshot()).expect("serialize");
    assert!(
        spectator_json.len() <= SNAPSHOT_BOUND_BYTES,
        "Spectator snapshot exceeded bound: {} bytes > {}",
        spectator_json.len(),
        SNAPSHOT_BOUND_BYTES
    );

    // Opponent snapshot must be strictly smaller than actor snapshot because private holdings and choices are omitted
    assert!(
        opponent_json.len() < actor_json.len(),
        "Opponent snapshot should be smaller than actor snapshot due to redaction"
    );
}

#[test]
fn pending_choice_size_remains_within_bounds() {
    let msg = sample_actor_snapshot();
    if let ServerMessage::InitialSnapshot(snapshot) = msg {
        let choice = snapshot.pending_choice.expect("actor choice present");
        let choice_json = serde_json::to_string(&choice).expect("serialize");
        assert!(
            choice_json.len() <= CHOICE_BOUND_BYTES,
            "PendingChoice exceeded bound: {} bytes",
            choice_json.len()
        );
    } else {
        panic!("Expected InitialSnapshot");
    }
}

#[test]
fn client_submission_messages_remain_within_bounds() {
    let submit = ClientMessage::SubmitChoice {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_long_identifier_uuid_v4_style".to_owned(),
        nonce: "opaque_nonce_token_string_here".to_owned(),
        expected_version: 1_000_000,
        option_id: "produce_unit_carrier_xyz".to_owned(),
    };
    let submit_json = serde_json::to_string(&submit).expect("serialize");
    assert!(
        submit_json.len() <= CLIENT_MSG_BOUND_BYTES,
        "SubmitChoice exceeded bound: {} bytes",
        submit_json.len()
    );

    let subscribe = ClientMessage::Subscribe {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_long_identifier_uuid_v4_style".to_owned(),
        player_session: Some("secret_player_token_sample".to_owned()),
    };
    let subscribe_json = serde_json::to_string(&subscribe).expect("serialize");
    assert!(
        subscribe_json.len() <= CLIENT_MSG_BOUND_BYTES,
        "Subscribe exceeded bound: {} bytes",
        subscribe_json.len()
    );

    let ping = ClientMessage::Ping {
        protocol_version: PROTOCOL_VERSION,
        sequence: 999_999,
    };
    let ping_json = serde_json::to_string(&ping).expect("serialize");
    assert!(
        ping_json.len() <= CLIENT_MSG_BOUND_BYTES,
        "Ping exceeded bound: {} bytes",
        ping_json.len()
    );
}

#[test]
fn status_and_error_responses_remain_within_bounds() {
    let rejected_json =
        serde_json::to_string(&sample_stale_submission_rejected()).expect("serialize");
    assert!(
        rejected_json.len() <= STATUS_BOUND_BYTES,
        "ActionRejected exceeded bound: {} bytes",
        rejected_json.len()
    );

    let game_over_json = serde_json::to_string(&sample_terminal_game_over()).expect("serialize");
    assert!(
        game_over_json.len() <= STATUS_BOUND_BYTES,
        "GameOver exceeded bound: {} bytes",
        game_over_json.len()
    );
}
