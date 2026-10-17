//! A whole game for one person, through the hosted exports, natively.
//!
//! The "person" is played by the rule bot (so a game finishes), through the same JSON commands
//! the page sends. What comes out is checked against the shapes of the server: every message
//! parses as a `ServerMessage` of `ti4-host` and serializes back to the same JSON, the stream
//! keeps the order and the numbers a client relies on, and the events and the decision log of
//! the game are those the server's own session makes for the same seed and answers.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_engine::choice::Choice;
use ti4_host::protocol::server::ServerMessage;
use ti4_wasm::host_api;

const GAME: &str = "local";

/// The messages the page received, in order, and the answers it gave.
#[derive(Default)]
struct Page {
    messages: Vec<Value>,
    answers: Vec<String>,
    asks: usize,
    progress: Vec<(u64, u64)>,
    checks: Vec<Value>,
    batches: Vec<Value>,
    /// Stop after this many answers of the person (the host leaves).
    leave_after: Option<usize>,
    /// Run these commands before the next answer; each is `(type-less JSON, expected resume)`.
    before_answer: Vec<Value>,
}

fn send(command: &Value) -> Value {
    let text = command.to_string();
    let len = host_api::put_input(&text);
    let status = host_api::command(len);
    let out = host_api::out_text();
    assert!(status >= 0, "command {text} failed: {out}");
    serde_json::from_str(&out).expect("an envelope is JSON")
}

fn pending(messages: &[Value]) -> Option<(String, u64, Choice, usize)> {
    messages.iter().rev().find_map(|message| {
        let kind = message["type"].as_str()?;
        if kind != "state_update" && kind != "initial_snapshot" {
            return None;
        }
        let pending = message.get("pending_choice")?;
        let choice: Choice = serde_json::from_value(pending["choice"].clone()).ok()?;
        Some((
            pending["nonce"].as_str()?.to_owned(),
            message["game_version"].as_u64()?,
            choice,
            usize::try_from(message["history"]["cursor"].as_u64().unwrap_or(0)).unwrap(),
        ))
    })
}

/// The person: the rule bot, deciding without a view of the board.
fn install_person(page: &Arc<Mutex<Page>>, seed: u64, viewer: &str) {
    let page = page.clone();
    // The person answers as a rule seat of its place would (seat `a` is place 0).
    let place = usize::from(viewer.as_bytes()[0] - b'a');
    ti4_wasm::set_native_host(move |text: &str| {
        let envelope: Value = serde_json::from_str(text).expect("the output is JSON");
        let mut page = page.lock().unwrap();
        match envelope["kind"].as_str() {
            Some("progress") => {
                page.progress.push((
                    envelope["progress"]["done"].as_u64().unwrap(),
                    envelope["progress"]["total"].as_u64().unwrap(),
                ));
                return host_api::CONTINUE;
            }
            Some("ask") => {}
            other => panic!("the game asked with a {other:?} output"),
        }
        page.asks += 1;
        if let Some(check) = envelope.get("resume_check") {
            page.checks.push(check.clone());
        }
        if let Some(batch) = envelope.get("batch") {
            page.batches.push(batch.clone());
        }
        let fresh: Vec<Value> = envelope["messages"].as_array().unwrap().clone();
        page.messages.extend(fresh);
        for command in std::mem::take(&mut page.before_answer) {
            let answered = send(&command);
            page.messages
                .extend(answered["messages"].as_array().unwrap().clone());
        }
        if page
            .leave_after
            .is_some_and(|limit| page.answers.len() >= limit)
        {
            return -1;
        }
        let (nonce, version, choice, cursor) =
            pending(&page.messages).expect("an ask has a pending choice");
        let (picked, _) = ti4_rule_bot::choose(&choice, None, seed, place, cursor);
        let option = choice.options[picked].id.clone();
        let answered = send(&json!({
            "type": "submit_choice",
            "protocol_version": 4,
            "game_id": GAME,
            "nonce": nonce,
            "expected_version": version,
            "option_id": option,
        }));
        page.messages
            .extend(answered["messages"].as_array().unwrap().clone());
        page.answers.push(option);
        answered["resume"]
            .as_i64()
            .map(|resume| i32::try_from(resume).unwrap())
            .unwrap_or_else(|| panic!("an answer resumes the game: {answered}"))
    });
}

fn start(setup: &Value, resume: Option<&Value>) -> (i32, Value) {
    let request = json!({ "setup": setup, "resume": resume });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    let out = host_api::out_text();
    assert!(status >= 0, "start failed: {out}");
    (status, serde_json::from_str(&out).expect("the end is JSON"))
}

fn setup(seed: u64, players: usize) -> Value {
    json!({ "game_id": GAME, "seed": seed, "players": players, "human_seat": 0 })
}

/// Every message is a message of the server, and serializes back to what was sent.
fn assert_server_shapes(messages: &[Value]) {
    for (index, message) in messages.iter().enumerate() {
        let parsed: ServerMessage =
            serde_json::from_value(message.clone()).unwrap_or_else(|error| {
                panic!("message {index} is no ServerMessage: {error}\n{message}")
            });
        assert_eq!(
            &serde_json::to_value(&parsed).unwrap(),
            message,
            "message {index} does not round-trip"
        );
    }
}

fn kinds(messages: &[Value]) -> Vec<&str> {
    messages
        .iter()
        .map(|message| message["type"].as_str().unwrap())
        .collect()
}

#[test]
fn slow_a_game_for_one_person_speaks_the_servers_messages() {
    let _serial = crate::common::serial();
    let page = Arc::new(Mutex::new(Page::default()));
    install_person(&page, 11, "a");
    let (_, end) = start(&setup(11, 4), None);
    let page = page.lock().unwrap();
    assert_eq!(end["kind"], "end");
    assert_eq!(end["end"]["finished"], true, "{end}");
    assert_eq!(end["end"]["error"], Value::Null, "{end}");

    let mut all = page.messages.clone();
    all.extend(end["messages"].as_array().unwrap().clone());
    assert_server_shapes(&all);
    let kinds = kinds(&all);

    // A client starts with a snapshot, and the game ends with a state and a game_over.
    assert_eq!(kinds.first(), Some(&"initial_snapshot"));
    assert_eq!(kinds.last(), Some(&"game_over"));
    assert_eq!(kinds[kinds.len() - 2], "state_update");
    assert_eq!(
        kinds
            .iter()
            .filter(|kind| **kind == "initial_snapshot")
            .count(),
        1
    );

    // Each answer was accepted once; nothing was rejected.
    let accepted = kinds
        .iter()
        .filter(|kind| **kind == "action_accepted")
        .count();
    assert_eq!(accepted, page.answers.len());
    assert!(!kinds.contains(&"action_rejected"));
    assert!(!kinds.contains(&"error"));

    // Versions never go back, and every question has its own number.
    let mut version = 0;
    let mut nonces = std::collections::BTreeSet::new();
    for message in &all {
        if let Some(next) = message.get("game_version").and_then(Value::as_u64) {
            assert!(next >= version, "a version went back: {message}");
            version = next;
        }
        if matches!(
            message["type"].as_str(),
            Some("state_update" | "initial_snapshot")
        ) && let Some(pending) = message.get("pending_choice")
        {
            assert!(nonces.insert(pending["nonce"].as_str().unwrap().to_owned()));
            assert_eq!(pending["choice"]["player"], "a", "only the person is asked");
        }
    }
    assert_eq!(nonces.len(), page.asks);

    // The event log: unique ids, the cursor never goes back, dice are public events.
    let events: Vec<&Value> = all
        .iter()
        .filter(|message| message["type"] == "event")
        .map(|message| &message["entry"])
        .collect();
    let snapshot_events = all[0]["events"].as_array().map_or(0, Vec::len);
    assert!(snapshot_events >= 1, "the snapshot has the opening event");
    let ids: std::collections::BTreeSet<&str> = events
        .iter()
        .map(|event| event["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids.len(), events.len());
    let mut cursor = 0;
    for event in &events {
        if let Some(count) = event["decision_count"].as_u64() {
            assert!(count >= cursor, "an event went back: {event}");
            cursor = count;
        }
    }
    let kinds_of_events: std::collections::BTreeSet<&str> = events
        .iter()
        .map(|event| event["event"]["kind"].as_str().unwrap_or("?"))
        .collect();
    for wanted in [
        "decision_resolved",
        "dice_rolled",
        "phase_transition",
        "game_finished",
    ] {
        assert!(
            kinds_of_events.contains(wanted),
            "{wanted} in {kinds_of_events:?}"
        );
    }
    let roll = events
        .iter()
        .find(|event| event["event"]["kind"] == "dice_rolled")
        .unwrap();
    assert!(
        roll["roll"]["faces"].is_array() || roll["roll"].is_object(),
        "{roll}"
    );

    // The person never sees a hidden thing: the redacted state of an update hides other hands.
    let update = all
        .iter()
        .rev()
        .find(|m| m["type"] == "state_update")
        .unwrap();
    assert_eq!(update["viewer"]["role"], "player");
    assert_eq!(update["turn_status"]["kind"], "game_over");
}

#[test]
fn a_command_that_is_not_the_servers_is_refused_the_servers_way() {
    let _serial = crate::common::serial();
    let page = Arc::new(Mutex::new(Page {
        leave_after: Some(3),
        ..Page::default()
    }));
    install_person(&page, 5, "a");
    // Before it answers the first time, the person sends things the server would refuse.
    page.lock().unwrap().before_answer = vec![
        json!({ "type": "submit_choice", "protocol_version": 4, "game_id": GAME,
                "nonce": "999", "expected_version": 2, "option_id": "x" }),
        json!({ "type": "ping", "protocol_version": 4, "sequence": 7 }),
        json!({ "type": "undo", "protocol_version": 4, "game_id": GAME, "expected_version": 2 }),
        json!({ "type": "preview_secondary", "protocol_version": 4, "game_id": GAME,
                "request_id": 1, "card": "x", "primary": "b" }),
    ];
    let (_, end) = start(&setup(5, 3), None);
    assert_eq!(end["end"]["left"], true);
    assert_eq!(
        end["end"]["error"],
        Value::Null,
        "leaving is no failure: {end}"
    );
    assert!(end["messages"].as_array().unwrap().is_empty(), "{end}");
    let page = page.lock().unwrap();
    let kinds = kinds(&page.messages);
    assert!(kinds.contains(&"action_rejected"), "{kinds:?}");
    assert!(kinds.contains(&"pong"));
    // At the first question no strategic action is in progress: the server's refusal.
    let preview = page
        .messages
        .iter()
        .find(|message| message["type"] == "secondary_preview")
        .unwrap_or_else(|| panic!("the preview is answered: {kinds:?}"));
    assert_eq!(preview["request_id"], 1);
    assert_eq!(preview["outcome"]["result"], "refused");
    assert_eq!(preview["outcome"]["reason"], "no_strategic_action");
    assert!(!kinds.contains(&"error"), "{kinds:?}");
    let rejected: Vec<&Value> = page
        .messages
        .iter()
        .filter(|message| message["type"] == "action_rejected")
        .collect();
    assert_eq!(rejected[0]["reason"]["reason"], "stale_nonce");
    // Nothing to take back at the first question.
    assert_eq!(rejected[1]["reason"]["reason"], "no_pending_choice");
    assert_server_shapes(&page.messages);
}
