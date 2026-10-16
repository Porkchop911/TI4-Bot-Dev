use std::collections::BTreeMap;
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::error::{ErrorKind, ProtocolError};
use ti4_host::protocol::server::{
    ActionAcceptedMsg, ActionRejectedMsg, GameOverMsg, PendingChoiceMsg, ProtocolErrorMsg,
    ServerMessage,
};
use ti4_host::protocol::status::RejectionReason;
use ti4_host::protocol::{
    PROTOCOL_VERSION, parse_client_message, parse_server_message, validate_protocol_version,
};
use ti4_model::id::PlayerId;

#[test]
fn client_subscribe_round_trips() {
    let msg = ClientMessage::Subscribe {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        player_session: Some("secret_token_123".to_owned()),
        staged: false,
    };
    let json = serde_json::to_string_pretty(&msg).expect("serialize");
    assert!(json.contains("\"type\": \"subscribe\""));
    let deserialized: ClientMessage = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(msg, deserialized);
    assert_eq!(parse_client_message(&json).expect("parse"), msg);
}

#[test]
fn staged_work_is_asked_for_and_written_in_the_form_that_web2_sends() {
    // A client that keeps its staged work itself sends no `staged`, and gets none.
    let old = serde_json::json!({"type":"subscribe", "protocol_version": PROTOCOL_VERSION,
        "game_id":"game", "player_session":"private"});
    assert!(matches!(
        parse_client_message(&old.to_string()),
        Ok(ClientMessage::Subscribe { staged: false, .. })
    ));
    let mut wanted = old.clone();
    wanted["staged"] = serde_json::json!(true);
    let parsed = parse_client_message(&wanted.to_string()).expect("parse");
    assert!(matches!(
        parsed,
        ClientMessage::Subscribe { staged: true, .. }
    ));
    assert_eq!(serde_json::to_value(&parsed).unwrap(), wanted);

    let put = serde_json::json!({"type":"put_staged", "protocol_version": PROTOCOL_VERSION,
        "game_id":"game", "slot":"secondary", "base_revision": 4,
        "value": {"round": 2, "primary": "b", "card": "technology",
            "answers": [{"id": "yes", "label": "Follow"}], "ready": true}});
    let parsed = parse_client_message(&put.to_string()).expect("parse");
    // The server reads the draft of a secondary in the form that web2 writes it.
    let draft = ti4_host::staged::SecondaryDraft::of(&put["value"]).expect("a draft");
    assert!(draft.ready);
    assert_eq!(draft.answers[0].id, "yes");
    assert!(matches!(
        &parsed,
        ClientMessage::PutStaged {
            slot: ti4_host::protocol::staged::StagedSlot::Secondary,
            base_revision: 4,
            value: Some(_),
            ..
        }
    ));
    assert_eq!(parsed.validate_bounds(), Ok(()));
    let mut clear = put.clone();
    clear.as_object_mut().unwrap().remove("value");
    assert!(matches!(
        parse_client_message(&clear.to_string()),
        Ok(ClientMessage::PutStaged { value: None, .. })
    ));
    let mut unknown = put;
    unknown["slot"] = serde_json::json!("hand");
    assert!(parse_client_message(&unknown.to_string()).is_err());
}

#[test]
fn the_planning_runner_messages_are_unknown() {
    // A draft is a script (`run_draft`, `apply_draft`); the server runs no plan of its own.
    for kind in [
        "start_planning",
        "reset_planning",
        "edit_planning_movement",
        "apply_planning",
        "submit_planning_choice",
        "secondary_planning",
    ] {
        let message = serde_json::json!({
            "type": kind, "protocol_version": PROTOCOL_VERSION, "game_id": "game_abc",
        });
        assert!(
            parse_client_message(&message.to_string()).is_err(),
            "{kind}"
        );
    }
    for kind in ["planning_status", "planning_update", "planning_result"] {
        let message = serde_json::json!({
            "type": kind, "protocol_version": PROTOCOL_VERSION, "game_id": "game_abc",
        });
        assert!(
            parse_server_message(&message.to_string()).is_err(),
            "{kind}"
        );
    }
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
        galaxy_layout: ti4_host::map::GalaxyLayout {
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
    let msg = ServerMessage::Event(ti4_host::protocol::server::GameEventMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        entry: ti4_host::protocol::server::GameEvent {
            id: "game_abc-1".to_owned(),
            timestamp: "12:34:56".to_owned(),
            version: Some(2),
            visibility: ti4_host::protocol::server::EventVisibility::Public,
            event: ti4_host::protocol::server::GameEventKind::DecisionResolved,
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
            seat_detail: None,
            private_detail: None,
            roll: None,
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

#[test]
fn the_script_messages_round_trip_in_the_form_that_a_local_game_takes() {
    use ti4_host::planning::draft::DraftStep;
    use ti4_host::planning::draft::EntryRefusal;
    use ti4_host::planning::steps::PlanStep;
    use ti4_host::planning::steps::RefusalCode;
    use ti4_host::protocol::client::MAX_SCRIPT_ENTRIES;
    use ti4_host::protocol::server::{DraftAppliedMsg, ScriptStopped};

    // The script of the page: an answer by its id, then the steps as `submitPlan` has them.
    let json = serde_json::json!({
        "type": "apply_draft",
        "protocol_version": PROTOCOL_VERSION,
        "game_id": "game_abc",
        "nonce": "n1",
        "expected_version": 7,
        "script": [
            { "kind": "choose", "option_id": "tactical" },
            { "kind": "done_moving" },
        ],
    })
    .to_string();
    let script = vec![
        DraftStep::choose("tactical"),
        DraftStep::Step(PlanStep::DoneMoving),
    ];
    let apply = ClientMessage::ApplyDraft {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        nonce: "n1".to_owned(),
        expected_version: 7,
        script: script.clone(),
    };
    assert_eq!(parse_client_message(&json).expect("parse"), apply);
    assert!(apply.validate_bounds().is_ok());

    let run = |script: Vec<DraftStep>| ClientMessage::RunDraft {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        request_id: "r1".to_owned(),
        script,
        scope: ti4_host::planning::audit::PlanScope::Tactical,
    };
    let json = serde_json::to_string(&run(script.clone())).unwrap();
    assert!(json.contains("\"type\":\"run_draft\""));
    assert_eq!(parse_client_message(&json).unwrap(), run(script.clone()));
    // A draft with nothing chosen is a script too; one that is too long is not taken.
    assert!(run(Vec::new()).validate_bounds().is_ok());
    let long = vec![DraftStep::choose("x"); MAX_SCRIPT_ENTRIES + 1];
    assert_eq!(run(long).validate_bounds(), Err("script"));
    assert!(!format!("{:?}", run(script.clone())).contains("tactical"));

    for stopped in [
        None,
        Some(ScriptStopped {
            applied: 1,
            remaining: script[1..].to_vec(),
            interrupted: true,
            reason: "The game asks for another decision first.".to_owned(),
            refusal: None,
        }),
        // A refusal names the entry that the game refused, and why.
        Some(ScriptStopped {
            applied: 0,
            remaining: script.clone(),
            interrupted: false,
            reason: "The game does not offer \"x\" here.".to_owned(),
            refusal: Some(EntryRefusal::of(&script, 0, RefusalCode::NotOffered)),
        }),
    ] {
        let message = ServerMessage::DraftApplied(DraftAppliedMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: "game_abc".to_owned(),
            nonce: "n1".to_owned(),
            stopped,
        });
        let json = serde_json::to_string(&message).unwrap();
        assert!(json.contains("\"type\":\"draft_applied\""));
        assert_eq!(parse_server_message(&json).unwrap(), message);
        assert_eq!(message.game_version(), None);
    }
}

#[test]
fn the_undo_of_a_seat_round_trips_and_is_absent_from_a_message_without_it() {
    use ti4_host::protocol::server::{ActionRejectedMsg, HistoryStatus};
    use ti4_host::protocol::status::UndoBarrier;

    let json = serde_json::json!({
        "type": "undo",
        "protocol_version": PROTOCOL_VERSION,
        "game_id": "game_abc",
        "expected_version": 7,
    })
    .to_string();
    let undo = ClientMessage::Undo {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_abc".to_owned(),
        expected_version: 7,
    };
    assert_eq!(parse_client_message(&json).unwrap(), undo);
    assert!(undo.validate_bounds().is_ok());

    // A refusal names the barrier in the words of the plan.
    for (barrier, word) in [
        (UndoBarrier::Revealed, "revealed"),
        (UndoBarrier::Random, "random"),
        (UndoBarrier::OtherSeat, "other_seat"),
    ] {
        let message = ServerMessage::ActionRejected(ActionRejectedMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: "game_abc".to_owned(),
            game_version: 7,
            reason: RejectionReason::UndoBarrier { barrier },
        });
        let json = serde_json::to_value(&message).unwrap();
        assert_eq!(json["reason"]["reason"], "undo_barrier");
        assert_eq!(json["reason"]["barrier"], word);
        assert_eq!(parse_server_message(&json.to_string()).unwrap(), message);
    }

    // `can_undo` is a field of the history status only when it is true: a message of a game
    // without it is as before.
    let status = |can_undo| HistoryStatus {
        cursor: 3,
        redo_count: 0,
        generation: 0,
        can_undo,
    };
    let without = serde_json::to_value(status(false)).unwrap();
    assert!(without.get("can_undo").is_none());
    assert_eq!(
        serde_json::from_value::<HistoryStatus>(without).unwrap(),
        status(false)
    );
    let with = serde_json::to_value(status(true)).unwrap();
    assert_eq!(with["can_undo"], true);
    assert_eq!(
        serde_json::from_value::<HistoryStatus>(with).unwrap(),
        status(true)
    );
}
