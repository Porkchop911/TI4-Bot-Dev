//! History, save and resume, batches and queries of the hosted game, through the JSON commands.

use std::sync::{Arc, Mutex};

use crate::common::{Act, GAME, Page, install, last_question, pending, send, setup, start};
use serde_json::{Value, json};
use ti4_wasm::host_api;

fn page(script: Vec<Act>) -> Arc<Mutex<Page>> {
    Arc::new(Mutex::new(Page {
        script: script.into(),
        ..Page::default()
    }))
}

fn decisions_hash() -> String {
    let save = send(&json!({ "type": "save_state" }));
    save["save"]["decisions_hash"].as_str().unwrap().to_owned()
}

/// The events a page knows: the ones of its snapshot and the ones it was sent since.
fn events_of(messages: &[Value]) -> Vec<Value> {
    let mut events = Vec::new();
    for message in messages {
        match message["type"].as_str() {
            Some("initial_snapshot") => {
                events = message["events"].as_array().cloned().unwrap_or_default();
            }
            Some("event") => events.push(message["entry"].clone()),
            _ => {}
        }
    }
    events
}

#[test]
fn slow_an_undo_goes_back_one_answer_and_the_game_goes_on_the_same() {
    let _serial = crate::common::serial();
    let seed = 13;
    // The game as it is played without an undo.
    let plain = page(vec![]);
    install(&plain, seed, "a");
    start(&setup(seed, 4), None);
    let reference = decisions_hash();
    let answers = plain.lock().unwrap().answers.len();

    // The same game, with an undo after the 25th answer.
    let undone = page(vec![
        Act::Answer(25),
        Act::Probe(Box::new(|page| {
            let question = last_question(&page.messages);
            assert_eq!(
                question["history"]["can_undo"], true,
                "{}",
                question["history"]
            );
            let version = question["game_version"].clone();
            page.script.push_front(Act::Cmd(json!({
                "type": "undo", "protocol_version": 4, "game_id": GAME,
                "expected_version": version,
            })));
        })),
    ]);
    install(&undone, seed, "a");
    let end = start(&setup(seed, 4), None);
    assert_eq!(end["end"]["finished"], true);
    assert_eq!(
        decisions_hash(),
        reference,
        "an undo must not change the game"
    );
    let page = undone.lock().unwrap();
    // The undo was a request for the engine to take an answer back; the snapshot after it says so.
    let snapshots: Vec<&Value> = page
        .messages
        .iter()
        .filter(|m| m["type"] == "initial_snapshot")
        .collect();
    assert_eq!(
        snapshots.len(),
        2,
        "a snapshot at the start and one after the undo"
    );
    assert_eq!(snapshots[1]["history"]["generation"], 1);
    assert_eq!(snapshots[1]["history"]["redo_count"], 1);
    // The page answered the question that was taken back a second time.
    assert_eq!(page.answers.len(), answers + 1);
}

#[test]
fn slow_a_save_plays_again_to_the_same_question() {
    let _serial = crate::common::serial();
    let seed = 21;
    let first = page(vec![
        Act::Answer(70),
        Act::Cmd(json!({ "type": "save_state" })),
        Act::Leave,
    ]);
    install(&first, seed, "a");
    let end = start(&setup(seed, 4), None);
    assert_eq!(end["end"]["left"], true);
    let (save, question, events) = {
        let first = first.lock().unwrap();
        let save = first.replies.last().unwrap()["save"].clone();
        assert_eq!(save["answers"].as_array().unwrap().len(), 70);
        (
            save,
            last_question(&first.messages).clone(),
            events_of(&first.messages),
        )
    };

    let second = page(vec![Act::Leave]);
    install(&second, seed, "a");
    let end = crate::common::resume(&save);
    assert_eq!(end["end"]["left"], true);
    let second = second.lock().unwrap();
    // It was told how far it got, and checked that the game came back to the saved position.
    assert!(second.progress.len() >= 3, "{:?}", second.progress);
    assert_eq!(
        second.progress.last().unwrap().1 as usize,
        save["decisions"].as_array().unwrap().len()
    );
    assert_eq!(
        second.asks[0]["resume_check"]["ok"], true,
        "{}",
        second.asks[0]["resume_check"]
    );
    // The first thing it gets is a full snapshot, and it is the position that was saved.
    let snapshot = &second.messages[0];
    assert_eq!(snapshot["type"], "initial_snapshot");
    assert_eq!(snapshot["state"], question["state"]);
    assert_eq!(snapshot["pending_choice"], question["pending_choice"]);
    assert_eq!(snapshot["history"]["cursor"], question["history"]["cursor"]);
    let again = events_of(&second.messages);
    assert_eq!(again.len(), events.len());
    for (index, (ours, theirs)) in again.iter().zip(&events).enumerate() {
        assert_eq!(ours, theirs, "event {index} (with its time of day)");
    }
}

#[test]
fn a_save_names_its_game_and_every_decision_and_plays_again_with_no_seat_deciding() {
    let _serial = crate::common::serial();
    let seed = 11;
    // Fifteen answers, a save, an undo of the last answer, and a save again.
    let first = page(vec![
        Act::Answer(15),
        Act::Cmd(json!({ "type": "save_state" })),
        Act::Probe(Box::new(|page| {
            let (_, version, _, _) = pending(&page.messages).unwrap();
            page.script.push_front(Act::Cmd(json!({
                "type": "undo", "protocol_version": 4, "game_id": GAME,
                "expected_version": version,
            })));
        })),
        Act::Cmd(json!({ "type": "save_state" })),
        Act::Leave,
    ]);
    install(&first, seed, "a");
    start(&setup(seed, 4), None);
    let (before, save, question) = {
        let first = first.lock().unwrap();
        let saves: Vec<&Value> = first
            .replies
            .iter()
            .filter(|reply| reply["save"].is_object())
            .map(|reply| &reply["save"])
            .collect();
        (
            saves[0].clone(),
            saves[1].clone(),
            last_question(&first.messages).clone(),
        )
    };
    assert_eq!(save["format"], 2);
    assert_eq!(save["seed"], seed);
    assert_eq!(save["players"], 4);
    assert_eq!(save["humans"], 1);
    assert_eq!(save["game"]["setup"]["human_seat"], 0);
    assert_eq!(save["table"]["player_count"], 4);
    assert_eq!(save["finished"], false);
    for gone in ["setup", "log", "human_seat", "game_id"] {
        assert!(save.get(gone).is_none(), "{gone} is not in a save");
    }
    // The undo cuts the decisions at the answer that it takes back.
    let cut = save["decisions"].as_array().unwrap();
    let all = before["decisions"].as_array().unwrap();
    assert!(cut.len() < all.len(), "{} of {}", cut.len(), all.len());
    assert_eq!(cut[..], all[..cut.len()]);
    assert!(
        save["answers"].as_array().unwrap().len() < before["answers"].as_array().unwrap().len()
    );

    // The save alone plays the game again: every decision is given, and no seat decides.
    let second = page(vec![Act::Leave]);
    install(&second, seed, "a");
    let picks = ti4_wasm::picks_native();
    let end = crate::common::resume(&save);
    assert_eq!(end["end"]["left"], true, "{end}");
    assert_eq!(ti4_wasm::picks_native(), picks, "a seat decided again");
    let second = second.lock().unwrap();
    assert_eq!(
        second.asks[0]["resume_check"]["ok"], true,
        "{}",
        second.asks[0]["resume_check"]
    );
    let snapshot = &second.messages[0];
    assert_eq!(snapshot["type"], "initial_snapshot");
    assert_eq!(snapshot["state"], question["state"]);
    assert_eq!(snapshot["pending_choice"], question["pending_choice"]);
    let again = send(&json!({ "type": "save_state" }))["save"].clone();
    assert_eq!(again["decisions"], save["decisions"]);
    assert_eq!(again["decisions_hash"], save["decisions_hash"]);
}

/// A new game records the rules version it plays; a format 2 save made before the field existed
/// has none and resumes as version 0 (the legacy rules), and saves again without it.
#[test]
fn a_save_without_a_rules_version_resumes_as_a_legacy_game() {
    let _serial = crate::common::serial();
    // Seed 12: the first 27 decisions of seed 11 use Saar's agent, whose boost mark is written
    // differently from rules version 2 on, so a save of it is no longer a legacy game.
    let seed = 12;
    let first = page(vec![
        Act::Answer(8),
        Act::Cmd(json!({ "type": "save_state" })),
        Act::Leave,
    ]);
    install(&first, seed, "a");
    start(&setup(seed, 4), None);
    let mut save = {
        let first = first.lock().unwrap();
        first
            .replies
            .iter()
            .find(|reply| reply["save"].is_object())
            .map(|reply| reply["save"].clone())
            .expect("a save")
    };
    assert_eq!(save["rules_version"], ti4_model::state::RULES_VERSION);

    // A save of the current game resumes as the current game.
    let current = page(vec![Act::Leave]);
    install(&current, seed, "a");
    let end = crate::common::resume(&save);
    assert_eq!(end["end"]["left"], true, "{end}");
    let again = send(&json!({ "type": "save_state" }))["save"].clone();
    assert_eq!(again["rules_version"], ti4_model::state::RULES_VERSION);
    assert_eq!(
        current.lock().unwrap().messages[0]["state"]["rules_version"],
        ti4_model::state::RULES_VERSION
    );

    // The same save as an older build wrote it: no field.
    save.as_object_mut().unwrap().remove("rules_version");
    let legacy = page(vec![Act::Leave]);
    install(&legacy, seed, "a");
    let end = crate::common::resume(&save);
    assert_eq!(end["end"]["left"], true, "{end}");
    let again = send(&json!({ "type": "save_state" }))["save"].clone();
    assert!(again.get("rules_version").is_none(), "{again}");
    let legacy = legacy.lock().unwrap();
    assert!(
        legacy.messages[0]["state"].get("rules_version").is_none(),
        "a legacy game's state is written without the field"
    );
    assert_eq!(legacy.asks[0]["resume_check"]["ok"], true, "{}", legacy.asks[0]["resume_check"]);
}

#[test]
fn a_save_of_another_format_is_not_read() {
    let _serial = crate::common::serial();
    let request = json!({ "setup": setup(3, 3), "resume": { "format": 1, "log": [] } });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    assert!(status < 0);
    assert!(
        host_api::out_text().contains("format 1; this engine reads format 2"),
        "{}",
        host_api::out_text()
    );
}

#[test]
fn slow_a_rewind_and_a_redo_play_the_game_again_to_the_same_end() {
    let _serial = crate::common::serial();
    let seed = 8;
    let reference = page(vec![]);
    install(&reference, seed, "a");
    start(&setup(seed, 3), None);
    let want = decisions_hash();

    // Rewind to the cursor of the 12th question while at the 40th.
    let rewinding = page(vec![
        Act::Answer(40),
        Act::Probe(Box::new(|page| {
            let twelfth = pending(page.asks[12]["messages"].as_array().unwrap()).unwrap();
            page.script.push_front(Act::Cmd(json!({
                "type": "history", "action": { "restore_cursor": { "cursor": twelfth.3 } },
            })));
        })),
    ]);
    install(&rewinding, seed, "a");
    let end = start(&setup(seed, 3), None);
    assert_eq!(
        end["end"]["left"], true,
        "a rewind ends the run and asks for a new one"
    );
    let restart = rewinding.lock().unwrap().replies.last().unwrap()["restart"].clone();
    assert_eq!(restart["generation"], 1);
    let last_version = rewinding.lock().unwrap().asks.last().unwrap()["messages"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|m| m["game_version"].as_u64())
        .max()
        .unwrap();
    assert!(restart["version"].as_u64().unwrap() >= last_version);
    let answers = restart["answers"].as_array().unwrap().len();
    let redo = restart["redo"].as_array().unwrap().len();
    assert!(answers <= 12 && answers > 0, "{answers}");
    assert_eq!(answers + redo, 40, "what went back is kept for a redo");

    // Play again from there: one redo, then on to the end.
    let again = page(vec![Act::Probe(Box::new(move |page| {
        let snapshot = &page.messages[0];
        assert_eq!(snapshot["type"], "initial_snapshot");
        assert_eq!(snapshot["history"]["generation"], 1);
        assert_eq!(snapshot["history"]["redo_count"], redo);
        page.script
            .push_front(Act::Cmd(json!({ "type": "history", "action": "redo" })));
    }))]);
    install(&again, seed, "a");
    let resume = restart.clone();
    let end = start(&setup(seed, 3), Some(&resume));
    assert_eq!(end["end"]["left"], true);
    let redone = again.lock().unwrap().replies.last().unwrap()["restart"].clone();
    assert_eq!(redone["answers"].as_array().unwrap().len(), answers + 1);
    assert_eq!(redone["redo"].as_array().unwrap().len(), redo - 1);

    let rest = page(vec![]);
    install(&rest, seed, "a");
    let resume = redone.clone();
    let end = start(&setup(seed, 3), Some(&resume));
    assert_eq!(end["end"]["finished"], true);
    assert_eq!(decisions_hash(), want);
}

#[test]
fn slow_an_undo_of_the_last_turn_goes_back_to_its_start_and_the_game_goes_on_the_same() {
    let _serial = crate::common::serial();
    let seed = 8;
    let reference = page(vec![]);
    install(&reference, seed, "a");
    start(&setup(seed, 3), None);
    let want = decisions_hash();

    // Far enough in for the person to have taken turns: ask to go back over the last one.
    let undoing = page(vec![
        Act::Answer(90),
        Act::Probe(Box::new(|page| {
            page.script.push_front(Act::Cmd(json!({
                "type": "history", "action": { "undo_turn": { "turns": 1 } },
            })));
        })),
    ]);
    install(&undoing, seed, "a");
    let end = start(&setup(seed, 3), None);
    assert_eq!(end["end"]["left"], true, "a turn undo ends the run and asks for a new one");
    let restart = undoing.lock().unwrap().replies.last().unwrap()["restart"].clone();
    assert_eq!(restart["generation"], 1);
    let answers = restart["answers"].as_array().unwrap().len();
    let redo = restart["redo"].as_array().unwrap().len();
    assert!(answers > 0 && answers < 90, "{answers}");
    assert!(redo > 0, "what went back is kept for a redo");

    // The new run waits at the start of the turn, and plays on to the same end.
    let again = page(vec![Act::Probe(Box::new(|page| {
        let (_, _, choice, _) = pending(&page.messages).unwrap();
        assert_eq!(choice.prompt, "action phase", "the turn is asked again from its start");
    }))]);
    install(&again, seed, "a");
    let resume = restart.clone();
    let end = start(&setup(seed, 3), Some(&resume));
    assert_eq!(end["end"]["finished"], true);
    assert_eq!(decisions_hash(), want, "the same answers give the same game");
}

#[test]
fn a_turn_undo_without_a_turn_is_refused() {
    let _serial = crate::common::serial();
    let page = page(vec![
        Act::Probe(Box::new(|page| {
            page.script.push_front(Act::Cmd(json!({
                "type": "history", "action": { "undo_turn": { "turns": 1 } },
            })));
        })),
        Act::Answer(1),
    ]);
    install(&page, 8, "a");
    let _ = start(&setup(8, 3), None);
    let page = page.lock().unwrap();
    assert!(page.replies.iter().all(|r| r["restart"].is_null()));
}

#[test]
fn slow_queries_and_settings_answer_in_the_servers_shapes() {
    let _serial = crate::common::serial();
    let seed = 4;
    let card = ti4_content::ContentStore::embedded()
        .records(ti4_model::content_types::ContentType::ActionCards)[0]
        .text("name")
        .unwrap()
        .to_owned();
    let queried = page(vec![
        // The third time the person takes an action: bots have played in between.
        Act::Until(Box::new({
            let seen = std::sync::atomic::AtomicUsize::new(0);
            move |choice| {
                choice.prompt == "action phase"
                    && seen.fetch_add(1, std::sync::atomic::Ordering::Relaxed) >= 2
            }
        })),
        Act::Cmd(
            json!({ "type": "query_reach", "protocol_version": 4, "game_id": GAME,
                         "request_id": 9, "system_id": "18" }),
        ),
        Act::Cmd(
            json!({ "type": "query_reach", "protocol_version": 4, "game_id": GAME,
                         "request_id": 10, "system_id": "nowhere" }),
        ),
        Act::Cmd(json!({ "type": "subscribe", "protocol_version": 4, "game_id": GAME })),
        Act::Cmd(
            json!({ "type": "run_draft", "protocol_version": 4, "game_id": GAME,
                         "request_id": "d1", "script": [] }),
        ),
        Act::Cmd(
            json!({ "type": "set_reaction_mode", "protocol_version": 4, "game_id": GAME,
                         "card": card, "mode": "never" }),
        ),
        Act::Leave,
    ]);
    install(&queried, seed, "a");
    start(&setup(seed, 4), None);
    let page = queried.lock().unwrap();
    let find = |kind: &str| -> Vec<&Value> {
        page.messages.iter().filter(|m| m["type"] == kind).collect()
    };
    let reach = find("reach_into");
    assert_eq!(reach.len(), 2);
    assert_eq!(reach[0]["request_id"], 9);
    assert_eq!(reach[0]["refused"], Value::Null, "{}", reach[0]);
    assert_eq!(reach[1]["refused"], "unknown_system");
    // A new connection gets the position that the last update showed.
    let snapshots = find("initial_snapshot");
    assert_eq!(snapshots.len(), 2);
    let at = page
        .messages
        .iter()
        .rposition(|m| m["type"] == "initial_snapshot")
        .unwrap();
    let before = last_question(&page.messages[..at]);
    assert_eq!(snapshots[1]["state"], before["state"]);
    assert_eq!(snapshots[1]["pending_choice"], before["pending_choice"]);
    let outcome = find("draft_outcome");
    assert_eq!(outcome.len(), 1);
    assert_eq!(outcome[0]["request_id"], "d1");
    // The copy of the game that the draft is run on is the live game, though the other seats
    // are rule bots that decided since the last checkpoint.
    let at = page
        .messages
        .iter()
        .position(|m| m["type"] == "draft_outcome")
        .unwrap();
    let live = last_question(&page.messages[..at]);
    let (drafted, now) = (&outcome[0]["outcome"]["update"]["view"], &live["view"]);
    let differing: Vec<&String> = drafted
        .as_object()
        .unwrap()
        .keys()
        .filter(|key| drafted[key.as_str()] != now[key.as_str()])
        .collect();
    assert!(
        differing.is_empty(),
        "the draft starts where the game is: {differing:?}"
    );
    let update = find("state_update");
    assert_eq!(
        update.last().unwrap()["reaction_modes"][&card],
        "never",
        "the setting is in the next update"
    );
    drop(page);
    // The Never set is part of the save.
    let save = send(&json!({ "type": "save_state" }));
    assert_eq!(save["save"]["never"], json!([card]));
    assert!(host_api::out_text().contains("never"));
}
