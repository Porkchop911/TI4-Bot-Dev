use std::collections::BTreeMap;
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_model::id::PlayerId;
use ti4_server::protocol::client::ClientMessage;
use ti4_server::protocol::error::{ErrorKind, ProtocolError};
use ti4_server::protocol::server::{
    ActionAcceptedMsg, ActionRejectedMsg, GameOverMsg, PendingChoiceMsg, ProtocolErrorMsg,
    ServerMessage,
};
use ti4_server::protocol::status::RejectionReason;
use ti4_server::protocol::{
    PROTOCOL_VERSION, parse_client_message, parse_server_message, validate_protocol_version,
};

#[test]
fn client_subscribe_round_trips() {
    let msg = ClientMessage::Subscribe {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        player_session: Some("secret_token_123".to_owned()),
    };
    let json = serde_json::to_string_pretty(&msg).expect("serialize");
    assert!(json.contains("\"type\": \"subscribe\""));
    let deserialized: ClientMessage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(msg, deserialized);
    assert_eq!(parse_client_message(&json).expect("parse"), msg);
}

#[test]
fn client_submit_choice_round_trips() {
    let msg = ClientMessage::SubmitChoice {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        nonce: "nonce_456".to_owned(),
        expected_version: 12,
        option_id: "opt_tactical_activate".to_owned(),
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    assert!(json.contains("\"type\":\"submit_choice\""));
    let deserialized: ClientMessage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(msg, deserialized);
    assert_eq!(parse_client_message(&json).expect("parse"), msg);
}

#[test]
fn client_ping_round_trips() {
    let msg = ClientMessage::Ping {
        protocol_version: PROTOCOL_VERSION,
        sequence: 101,
    };
    let json = serde_json::to_string(&msg).expect("serialize");
    let deserialized = parse_client_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn server_action_accepted_round_trips() {
    let msg = ServerMessage::ActionAccepted(ActionAcceptedMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        game_version: 13,
        option_id: "opt_pass".to_owned(),
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    assert!(json.contains("\"type\":\"action_accepted\""));
    let deserialized = parse_server_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn server_action_rejected_round_trips() {
    let msg = ServerMessage::ActionRejected(ActionRejectedMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        game_version: 12,
        reason: RejectionReason::StaleNonce,
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    assert!(json.contains("\"type\":\"action_rejected\""));
    assert!(json.contains("\"reason\":\"stale_nonce\""));
    let deserialized = parse_server_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn server_error_round_trips() {
    let msg = ServerMessage::Error(ProtocolErrorMsg {
        protocol_version: PROTOCOL_VERSION,
        kind: ErrorKind::MalformedMessage,
        message: "Invalid payload formatting".to_owned(),
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let deserialized = parse_server_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn server_game_over_round_trips() {
    let mut scores = BTreeMap::new();
    scores.insert(PlayerId::new("player_1"), 10);
    scores.insert(PlayerId::new("player_2"), 8);

    let msg = ServerMessage::GameOver(GameOverMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        game_version: 120,
        winner: Some(PlayerId::new("player_1")),
        final_scores: scores,
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let deserialized = parse_server_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn server_pending_choice_round_trips() {
    let state = ti4_engine::setup::start_game(
        ti4_content::ContentStore::embedded(),
        &[PlayerId::new("player_1")],
        ti4_model::content_types::POK,
        None,
    )
    .expect("state");
    let msg = ServerMessage::PendingChoice(PendingChoiceMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        game_version: 14,
        nonce: "nonce_abc".to_owned(),
        choice: Choice::new(
            PlayerId::new("player_1"),
            "Select strategy card",
            vec![
                ChoiceOption::labelled("leadership", "pick_strategy_card", "1 - Leadership"),
                ChoiceOption::labelled("diplomacy", "pick_strategy_card", "2 - Diplomacy"),
            ],
        ),
        state,
        galaxy_layout: ti4_server::map::GalaxyLayout {
            version: 1,
            active_sources: vec!["base".to_owned()],
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    let deserialized = parse_server_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn server_event_round_trips() {
    let msg = ServerMessage::Event(ti4_server::protocol::server::GameEventMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        entry: ti4_server::protocol::server::GameEvent {
            id: "game_abc-1".to_owned(),
            timestamp: "12:34:56".to_owned(),
            version: Some(2),
            visibility: ti4_server::protocol::server::EventVisibility::Public,
            event: ti4_server::protocol::server::GameEventKind::DecisionResolved,
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
            seat_detail: None,
            private_detail: None,
        },
    });
    let json = serde_json::to_string(&msg).expect("serialize");
    assert!(json.contains("\"type\":\"event\""));
    assert!(json.contains("\"kind\":\"decision_resolved\""));
    let deserialized = parse_server_message(&json).expect("parse");
    assert_eq!(msg, deserialized);
}

#[test]
fn unknown_protocol_version_is_rejected() {
    let res = validate_protocol_version(99);
    assert_eq!(
        res,
        Err(ProtocolError::UnsupportedVersion {
            found: 99,
            expected: PROTOCOL_VERSION
        })
    );

    let client_json = r#"{
        "type": "ping",
        "protocol_version": 99,
        "sequence": 1
    }"#;
    assert!(matches!(
        parse_client_message(client_json),
        Err(ProtocolError::UnsupportedVersion { found: 99, .. })
    ));
}

#[test]
fn unknown_wire_fields_are_rejected() {
    let client_json_with_extra = r#"{
        "type": "ping",
        "protocol_version": 2,
        "sequence": 1,
        "extra_field": "unexpected"
    }"#;
    assert!(matches!(
        parse_client_message(client_json_with_extra),
        Err(ProtocolError::Json(_))
    ));

    let server_json_with_extra = r#"{
        "type": "pong",
        "protocol_version": 2,
        "sequence": 1,
        "unexpected": true
    }"#;
    assert!(matches!(
        parse_server_message(server_json_with_extra),
        Err(ProtocolError::Json(_))
    ));
}

#[test]
fn subscribe_accepts_only_the_v3_player_session_field() {
    let message = serde_json::json!({"type":"subscribe", "protocol_version": PROTOCOL_VERSION,
        "game_id":"game", "player_session":"private"});
    assert!(matches!(
        parse_client_message(&message.to_string()),
        Ok(ClientMessage::Subscribe {
            player_session: Some(_),
            ..
        })
    ));
    assert!(
        !format!("{:?}", parse_client_message(&message.to_string()).unwrap()).contains("private")
    );
    let mut old = message.clone();
    old.as_object_mut().unwrap().remove("player_session");
    old["seat_token"] = serde_json::json!("private");
    assert!(parse_client_message(&old.to_string()).is_err());
    old.as_object_mut().unwrap().remove("seat_token");
    old["protocol_version"] = serde_json::json!(2);
    assert!(matches!(
        parse_client_message(&old.to_string()),
        Err(ProtocolError::UnsupportedVersion { .. })
    ));
}
