//! A staged plan (`BatchPlan` of `web/`) sent as one request to the hosted game.

use std::sync::{Arc, Mutex};

use crate::common::{Act, GAME, Page, install, last_question, send, setup, start};
use serde_json::{Value, json};
use ti4_engine::choice::Choice;

fn movement_question(choice: &Choice) -> bool {
    choice.player.as_str() == "a"
        && choice
            .context
            .as_ref()
            .is_some_and(|context| context.subtype == "movement_step")
        && choice.options.iter().any(|option| option.kind == "move")
}

/// A movement plan from the question the page is asked: the first ship that can move, then done.
fn plan_from(question: &Value, steps: usize) -> Value {
    let choice: Choice =
        serde_json::from_value(question["pending_choice"]["choice"].clone()).unwrap();
    let moves: Vec<Value> = choice
        .options
        .iter()
        .filter(|option| option.kind == "move")
        .take(steps)
        .map(|option| {
            json!({
                "kind": "move",
                "origin": option.payload["origin"],
                "unit": option.payload["unit"],
                "damaged": option.payload.get("damaged").cloned().unwrap_or(json!(false)),
            })
        })
        .collect();
    let destination = question["state"]["active_system"].clone();
    let mut all = moves;
    all.push(json!({ "kind": "done_moving" }));
    json!({ "kind": "tactical_movement", "destination": destination, "steps": all })
}

fn submit(question: &Value, plan: Value) -> Value {
    json!({
        "type": "submit_batch",
        "nonce": question["pending_choice"]["nonce"],
        "expected_version": question["game_version"],
        "plan": plan,
    })
}

#[test]
fn slow_a_movement_plan_is_applied_whole_or_not_at_all() {
    let _serial = crate::common::serial();
    let seed = 6;
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Until(Box::new(movement_question)),
            // 1. The plan names a ship that is not there: refused at once, nothing changes.
            Act::Probe(Box::new(|page| {
                let question = last_question(&page.messages).clone();
                let mut plan = plan_from(&question, 1);
                plan["steps"][0]["unit"] = json!("not-a-unit");
                let reply = send(&submit(&question, plan));
                assert_eq!(reply["resume"], Value::Null, "{reply}");
                assert_eq!(reply["batch"]["status"], "refused", "{reply}");
                assert!(reply["batch"]["message"].as_str().unwrap().len() > 5);
            })),
            // 2. The first step is fine and the second is not: the game goes back to this
            //    question, and the next one is this one again.
            Act::Probe(Box::new(|page| {
                let question = last_question(&page.messages).clone();
                let mut plan = plan_from(&question, 1);
                let good = plan["steps"][0].clone();
                let mut bad = good.clone();
                bad["unit"] = json!("not-a-unit");
                plan["steps"] = json!([good, bad, { "kind": "done_moving" }]);
                page.script.push_front(Act::Probe(Box::new(|page| {
                    // The next question after the rollback: the same nonce, the same decisions.
                    let before = page.asks.len();
                    let _ = before;
                })));
                page.script.push_front(Act::Cmd(submit(&question, plan)));
            })),
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, seed, "a");
    // Run until the rollback came back around, then take the whole game on to the end.
    let page_after = page.clone();
    let checked = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let checked_in = checked.clone();
    page.lock()
        .unwrap()
        .script
        .push_back(Act::Probe(Box::new(move |page| {
            checked_in.store(true, std::sync::atomic::Ordering::Relaxed);
            let asks = &page.asks;
            let (this, previous) = (&asks[asks.len() - 1], &asks[asks.len() - 2]);
            // After a rolled-back batch the same question is asked again (same nonce).
            let nonce = |ask: &Value| {
                crate::common::pending(ask["messages"].as_array().unwrap()).map(|(nonce, ..)| nonce)
            };
            assert_eq!(nonce(this), nonce(previous), "the same question again");
            assert_eq!(this["batch"]["status"], "refused", "{}", this["batch"]);
            let _ = &page_after;
        })));
    let end = start(&setup(seed, 4), None);
    assert!(end["end"]["error"].is_null(), "{end}");
    assert!(
        checked.load(std::sync::atomic::Ordering::Relaxed),
        "the rollback was seen"
    );

    // 3. A good plan is applied as the engine asks, and is told once it ended.
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Until(Box::new(movement_question)),
            Act::Probe(Box::new(|page| {
                let question = last_question(&page.messages).clone();
                let cursor = question["history"]["cursor"].as_u64().unwrap();
                let plan = plan_from(&question, 1);
                page.script.push_front(Act::Probe(Box::new(move |page| {
                    let ask = page.asks.last().unwrap();
                    assert_eq!(ask["batch"]["status"], "applied", "{}", ask["batch"]);
                    let now = last_question(&page.messages)["history"]["cursor"]
                        .as_u64()
                        .unwrap();
                    assert!(now >= cursor + 2, "the move and done_moving were decided");
                })));
                page.script.push_front(Act::Cmd(submit(&question, plan)));
            })),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, seed, "a");
    let _ = GAME;
    let end = start(&setup(seed, 4), None);
    assert_eq!(end["end"]["left"], true, "{end}");
}

/// The offline host sends the engine's routes and loads (`tactical`) with the movement question,
/// as the server does: the page stages pickups from them instead of guessing the route.
#[test]
fn slow_the_movement_question_carries_the_engine_route_facts() {
    let _serial = crate::common::serial();
    let seed = 6;
    let seen = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let seen_in = seen.clone();
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Until(Box::new(movement_question)),
            Act::Probe(Box::new(move |page| {
                let question = last_question(&page.messages).clone();
                let facts = &question["tactical"];
                assert_eq!(facts["kind"], "movement", "{facts}");
                assert_eq!(
                    facts["active"], question["state"]["active_system"],
                    "{facts}"
                );
                let ships = facts["ships"].as_array().expect("ships");
                let mut moving = 0;
                for ship in ships {
                    let Some(route) = ship.get("move") else {
                        continue;
                    };
                    moving += 1;
                    assert_eq!(
                        route["path"].as_array().unwrap().last(),
                        Some(&facts["active"])
                    );
                }
                assert!(moving > 0, "no ship with a route: {facts}");
                seen_in.store(true, std::sync::atomic::Ordering::Relaxed);
            })),
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, seed, "a");
    let end = start(&setup(seed, 4), None);
    assert!(end["end"]["error"].is_null(), "{end}");
    assert!(
        seen.load(std::sync::atomic::Ordering::Relaxed),
        "the question was seen"
    );
}
