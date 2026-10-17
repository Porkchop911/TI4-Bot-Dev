//! A script sent to the hosted game (`apply_draft`): checked on a copy before it is sent.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::common::{Act, GAME, Page, install, last_question, send, setup, start};
use serde_json::{Value, json};
use ti4_engine::choice::Choice;

fn turn_menu(choice: &Choice) -> bool {
    choice.player.as_str() == "a"
        && choice
            .context
            .as_ref()
            .is_some_and(|context| context.subtype == "action_menu")
}

fn apply(question: &Value, script: &Value) -> Value {
    json!({
        "type": "apply_draft", "protocol_version": 4, "game_id": GAME,
        "nonce": question["pending_choice"]["nonce"],
        "expected_version": question["game_version"],
        "script": script,
    })
}

#[test]
fn a_script_that_the_game_refuses_is_refused_before_anything_is_applied() {
    let _serial = crate::common::serial();
    let checked = Arc::new(AtomicBool::new(false));
    let checked_in = checked.clone();
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Until(Box::new(turn_menu)),
            Act::Probe(Box::new(move |page| {
                let question = last_question(&page.messages).clone();
                // The first entry is of the turn menu; the game offers no such system after it.
                let script = json!([
                    { "kind": "choose", "option_id": "tactical" },
                    { "kind": "choose", "option_id": "not-a-system" },
                ]);
                let reply = send(&apply(&question, &script));
                // Refused on the copy: the game is not resumed, so nothing of it is played.
                assert_eq!(reply["resume"], Value::Null, "{reply}");
                let messages = reply["messages"].as_array().unwrap();
                assert_eq!(messages.len(), 1, "{reply}");
                let told = &messages[0];
                assert_eq!(told["type"], "draft_applied", "{told}");
                assert_eq!(told["nonce"], question["pending_choice"]["nonce"]);
                let stopped = &told["stopped"];
                assert_eq!(stopped["applied"], 0, "{told}");
                assert_eq!(stopped["interrupted"], false, "{told}");
                assert_eq!(stopped["remaining"], script, "{told}");
                checked_in.store(true, Ordering::Relaxed);
            })),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, 3, "a");
    let end = start(&setup(3, 3), None);
    assert!(end["end"]["error"].is_null(), "{end}");
    assert!(checked.load(Ordering::Relaxed), "the script was sent");
    // The game was left at the question the script was sent at: its answer is not in the game.
    let page = page.lock().unwrap();
    let asks = &page.asks;
    let nonce = |ask: &Value| {
        crate::common::pending(ask["messages"].as_array().unwrap()).map(|(nonce, ..)| nonce)
    };
    assert_eq!(
        nonce(asks.last().unwrap()),
        crate::common::pending(&page.messages).map(|(nonce, ..)| nonce)
    );
}
