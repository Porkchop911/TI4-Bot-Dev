//! The staged work of the seats of the hosted game (`ti4_host::staged`): `put_staged` and
//! `seat_staged` as a connection of the server speaks them, the save, and a ready draft of a
//! secondary that the session sends when the game asks the seat.

use std::sync::{Arc, Mutex};

use crate::common::{Act, GAME, Page, install, last_question, send, setup};
use serde_json::{Value, json};
use ti4_engine::choice::Choice;
use ti4_wasm::host_api;

/// Starts a game whose page takes part in the staged work; returns the `end` envelope.
fn start_staged(setup: &Value, resume: Option<&Value>) -> Value {
    let request = json!({ "setup": setup, "resume": resume, "staged": true });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    let out = host_api::out_text();
    assert!(status >= 0, "start failed: {out}");
    serde_json::from_str(&out).expect("the end is JSON")
}

fn put(slot: &str, base: u64, value: &Value) -> Value {
    json!({
        "type": "put_staged", "protocol_version": 4, "game_id": GAME,
        "slot": slot, "base_revision": base, "value": value,
    })
}

fn told(messages: &[Value]) -> Vec<Value> {
    messages
        .iter()
        .filter(|message| message["type"] == "seat_staged")
        .cloned()
        .collect()
}

fn reply_messages(reply: &Value) -> Vec<Value> {
    reply["messages"].as_array().unwrap().clone()
}

fn secondary_of_a(choice: &Choice) -> bool {
    choice.player.as_str() == "a"
        && choice.details.get("kind").and_then(Value::as_str) == Some("strategy_secondary")
}

#[test]
fn a_write_of_the_page_is_told_back_and_kept_by_the_save() {
    let _serial = crate::common::serial();
    let save = Arc::new(Mutex::new(Value::Null));
    let save_in = save.clone();
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Probe(Box::new(move |page| {
                // The page is told the slots of its seat after the snapshot, as a connection.
                let first = told(&page.messages);
                assert_eq!(first.len(), 1, "{first:?}");
                assert_eq!(first[0]["connection"], 1);
                assert_eq!(first[0]["revision"], 0);
                let movement = json!({ "nonce": "7", "steps": [] });
                let written = reply_messages(&send(&put("movement", 0, &movement)));
                assert_eq!(written.len(), 1, "{written:?}");
                assert_eq!(written[0]["writer"], 1, "{written:?}");
                assert_eq!(written[0]["revision"], 1);
                assert_eq!(written[0]["slots"]["movement"]["value"], movement);
                // A write on an old revision is stale: the page is told what is kept.
                let stale = reply_messages(&send(&put("movement", 0, &json!({ "x": 1 }))));
                assert_eq!(stale.len(), 1, "{stale:?}");
                assert!(stale[0].get("writer").is_none(), "{stale:?}");
                assert_eq!(stale[0]["slots"]["movement"]["value"], movement);
                // The same value again changes nothing and tells nothing.
                assert!(reply_messages(&send(&put("movement", 1, &movement))).is_empty());
                let saved = send(&json!({ "type": "save_state" }))["save"].clone();
                assert_eq!(saved["staged"]["a"]["slots"]["movement"]["value"], movement);
                *save_in.lock().unwrap() = saved;
            })),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, 3, "a");
    let end = start_staged(&setup(3, 3), None);
    assert!(end["end"]["error"].is_null(), "{end}");

    // The game is played again from its save: the slots of the seat come back with it.
    let saved = save.lock().unwrap().clone();
    let again = Arc::new(Mutex::new(Page {
        script: vec![Act::Leave].into(),
        ..Page::default()
    }));
    install(&again, 3, "a");
    let end = start_staged(&setup(3, 3), Some(&saved));
    assert!(end["end"]["error"].is_null(), "{end}");
    let again = again.lock().unwrap();
    let first = told(&again.messages);
    assert_eq!(first.len(), 1, "{first:?}");
    assert_eq!(first[0]["connection"], 1);
    assert_eq!(first[0]["revision"], 1);
    assert_eq!(
        first[0]["slots"]["movement"]["value"],
        json!({ "nonce": "7", "steps": [] })
    );
}

/// The id of the option that declines the secondary that `question` asks.
fn decline(question: &Value) -> Value {
    let choice = &question["pending_choice"]["choice"];
    choice["options"]
        .as_array()
        .unwrap()
        .iter()
        .find(|option| option["id"] == "decline" || option["label"] == "decline")
        .unwrap_or_else(|| panic!("a secondary may be declined: {choice}"))["id"]
        .clone()
}

/// The draft of a secondary that declines the secondary that `question` asks.
fn declining_draft(question: &Value, ready: bool) -> Value {
    let choice = &question["pending_choice"]["choice"];
    json!({
        "answers": [{ "id": decline(question), "label": "Decline" }],
        "card": choice["details"]["card"],
        "primary": choice["details"]["played_by"],
        "ready": ready,
        "round": question["state"]["round"],
    })
}

#[test]
fn a_secondary_marked_ready_while_the_seat_is_asked_is_sent_by_the_session() {
    let _serial = crate::common::serial();
    let sent = Arc::new(Mutex::new(false));
    let sent_in = sent.clone();
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Until(Box::new(secondary_of_a)),
            Act::Probe(Box::new(move |page| {
                let question = last_question(&page.messages).clone();
                let nonce = question["pending_choice"]["nonce"].clone();
                // Not ready: it is kept, and the seat is still asked.
                let reply = send(&put("secondary", 0, &declining_draft(&question, false)));
                assert!(reply["resume"].is_null(), "{reply}");
                let revision = told(&reply_messages(&reply))[0]["revision"]
                    .as_u64()
                    .unwrap();
                // Ready: the session sends it, and clears the slot. Its answer resumes the game.
                let ready = put("secondary", revision, &declining_draft(&question, true));
                let sent_in = sent_in.clone();
                page.script.push_front(Act::Leave);
                page.script.push_front(Act::Probe(Box::new(move |page| {
                    let reply = page.replies.last().unwrap().clone();
                    assert!(reply["resume"].is_i64(), "the draft answers: {reply}");
                    let messages = reply_messages(&reply);
                    let slots = told(&messages);
                    assert_eq!(slots.len(), 2, "{slots:?}");
                    assert_eq!(slots[0]["writer"], 1);
                    assert!(slots[1].get("writer").is_none(), "a write of the session");
                    assert!(
                        slots[1]["slots"]["secondary"]["value"].is_null(),
                        "{slots:?}"
                    );
                    assert!(
                        messages
                            .iter()
                            .any(|m| m["type"] == "draft_applied" && m["nonce"] == nonce),
                        "{messages:?}"
                    );
                    *sent_in.lock().unwrap() = true;
                })));
                page.script.push_front(Act::Cmd(ready));
            })),
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, 3, "a");
    let end = start_staged(&setup(3, 3), None);
    assert!(end["end"]["error"].is_null(), "{end}");
    assert!(*sent.lock().unwrap(), "the seat was asked its secondary");
}

#[test]
fn a_ready_secondary_is_sent_when_the_game_asks_the_seat() {
    let _serial = crate::common::serial();
    let save = Arc::new(Mutex::new((Value::Null, Value::Null)));
    let save_in = save.clone();
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Until(Box::new(secondary_of_a)),
            Act::Probe(Box::new(move |page| {
                let question = last_question(&page.messages).clone();
                let mut saved = send(&json!({ "type": "save_state" }))["save"].clone();
                // The draft was marked ready before the game asked the seat: it is in the save.
                saved["staged"] = json!({ "a": { "revision": 1, "slots": { "secondary": {
                    "revision": 1, "value": declining_draft(&question, true),
                } } } });
                *save_in.lock().unwrap() = (saved, decline(&question));
            })),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, 3, "a");
    let end = start_staged(&setup(3, 3), None);
    assert!(end["end"]["error"].is_null(), "{end}");

    let (saved, declined) = save.lock().unwrap().clone();
    let decided = saved["decisions"].as_array().unwrap().len();
    let after = Arc::new(Mutex::new(Value::Null));
    let after_in = after.clone();
    let again = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Probe(Box::new(move |_| {
                *after_in.lock().unwrap() = send(&json!({ "type": "save_state" }))["save"].clone();
            })),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&again, 3, "a");
    let end = start_staged(&setup(3, 3), Some(&saved));
    assert!(end["end"]["error"].is_null(), "{end}");
    // The game asked the seat its secondary again, and the draft answered it: the page was
    // asked the next question, with the slot cleared by the session.
    let after = after.lock().unwrap().clone();
    assert_eq!(
        after["decisions"][decided], declined,
        "{}",
        after["decisions"]
    );
    assert!(
        after["staged"]["a"]["slots"]["secondary"]["value"].is_null(),
        "{after}"
    );
    let again = again.lock().unwrap();
    let first = told(&again.messages);
    assert_eq!(first[0]["connection"], 1, "{first:?}");
    assert!(
        first[0]["slots"]["secondary"]["value"].is_null(),
        "{first:?}"
    );
}
