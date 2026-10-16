//! The local play of web2 through the hosted exports: several seats of the page, a change of
//! them, the stepping of the other seats, and a save that has every decision of the game.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_wasm::host_api;

use crate::common::{GAME, send, serial};

/// What the page saw of one question: the seat it was shown as, the seat that is asked (none
/// while the game waits for another seat), and the seat that the game waits for.
#[derive(Debug, Clone, PartialEq)]
struct Seen {
    viewer: String,
    asked: Option<String>,
    waiting: Option<String>,
    nonce: Option<String>,
    /// The page may take an answer back.
    can_undo: bool,
    version: u64,
}

/// The last update of an envelope.
fn update(envelope: &Value) -> Option<&Value> {
    envelope["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|message| {
            matches!(
                message["type"].as_str(),
                Some("state_update" | "initial_snapshot")
            )
        })
}

/// What the page does at a question: the command to send, or `None` to leave.
type Act = Box<dyn FnMut(usize, &Seen) -> Option<Value> + Send>;

/// Plays `request` with `act` answering each question; returns what was seen and the `end`
/// envelope.
fn play(request: &Value, mut act: Act) -> (Vec<Seen>, Value) {
    let seen = Arc::new(Mutex::new(Vec::<Seen>::new()));
    let sink = seen.clone();
    ti4_wasm::set_native_host(move |output| {
        let envelope: Value = serde_json::from_str(output).expect("an envelope");
        if envelope["kind"] == "progress" {
            return host_api::CONTINUE;
        }
        let update = update(&envelope).expect("an ask has an update");
        let pending = &update["pending_choice"];
        let now = Seen {
            viewer: update["viewer"]["seat"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            asked: pending["choice"]["player"].as_str().map(str::to_owned),
            waiting: envelope["waiting"].as_str().map(str::to_owned),
            nonce: pending["nonce"].as_str().map(str::to_owned),
            can_undo: update["history"]["can_undo"].as_bool().unwrap_or(false),
            version: update["game_version"].as_u64().unwrap_or(0),
        };
        let count = {
            let mut seen = sink.lock().unwrap();
            seen.push(now.clone());
            seen.len()
        };
        let command = match act(count, &now) {
            Some(command) => command,
            None => return -1,
        };
        let command = if command["type"] == "submit_choice" {
            let mut command = command;
            command["nonce"] = pending["nonce"].clone();
            command["expected_version"] = update["game_version"].clone();
            command["option_id"] = pending["choice"]["options"][0]["id"].clone();
            command
        } else {
            command
        };
        let reply = send(&command);
        i32::try_from(reply["resume"].as_i64().unwrap_or_else(|| {
            panic!("{command} does not resume the game: {reply}");
        }))
        .unwrap()
    });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    let out = host_api::out_text();
    assert!(status >= 0, "start failed: {out}");
    let seen = seen.lock().unwrap().clone();
    (seen, serde_json::from_str(&out).unwrap())
}

fn answer() -> Value {
    json!({ "type": "submit_choice", "protocol_version": 4, "game_id": GAME })
}

fn save() -> Value {
    send(&json!({ "type": "save_state" }))["save"].clone()
}

#[test]
fn the_page_plays_several_seats_and_changes_them_and_a_save_plays_the_same_game() {
    let _serial = serial();
    ti4_wasm::set_round_limit_native(Some(1));
    // Seats a and c are played here. After ten answers seat e is played in place of the seat
    // that is shown.
    let local = json!({ "local": { "seed": 3, "players": 8, "humans": 0b101 } });
    let (seen, end) = play(
        &local,
        Box::new(|count, now| match count {
            10 => Some(json!({ "type": "switch_seat", "seat": "e" })),
            20 => None,
            _ => {
                assert!(now.waiting.is_none(), "no step without stepping");
                Some(answer())
            }
        }),
    );
    assert!(end["end"]["left"].as_bool().unwrap(), "{end}");
    let left = seen[9].viewer.clone();
    for (index, now) in seen.iter().enumerate() {
        // The page is shown the seat that it is asked.
        assert_eq!(now.asked.as_ref(), Some(&now.viewer), "question {index}");
        let seats: &[&str] = if index < 10 {
            &["a", "c"]
        } else if left == "a" {
            &["c", "e"]
        } else {
            &["a", "e"]
        };
        assert!(
            seats.contains(&now.viewer.as_str()),
            "question {index}: {now:?}"
        );
    }
    assert!(seen[..10].iter().any(|now| now.viewer == "a"));
    assert!(seen[..10].iter().any(|now| now.viewer == "c"));
    assert!(seen[10..].iter().any(|now| now.viewer == "e"));
    let saved = save();
    let humans = if left == "a" { 0b10100 } else { 0b10001 };
    assert_eq!(
        saved["switches"][0]["humans"], humans,
        "{}",
        saved["switches"]
    );
    assert_eq!(saved["format"], 2);
    assert_eq!(saved["game"]["local"]["humans"], 0b101);
    assert_eq!(saved["humans"], 0b101);
    assert!(saved.get("table").is_none(), "a local game is no table");

    // The save gives every decision again, and the seat changes: the game asks the question
    // that was open, of the same seat, with the same number, and its decisions are the same.
    let mut resumed = local.clone();
    resumed["resume"] = saved.clone();
    let (again, _) = play(&resumed, Box::new(|_, _| None));
    let last = seen.last().unwrap();
    assert_eq!(again[0].asked, last.asked);
    assert_eq!(again[0].nonce, last.nonce);
    assert_eq!(save()["decisions"], saved["decisions"]);
    ti4_wasm::set_round_limit_native(None);
}

#[test]
fn an_undo_of_the_page_goes_over_its_other_seats_and_stops_at_a_change_of_them() {
    let _serial = serial();
    ti4_wasm::set_round_limit_native(Some(1));
    // Seats a and c are played here (hot seat). At the tenth question seat e is played in place
    // of the seat that is shown; at the twelfth the page undoes its answer of the eleventh.
    let local = json!({ "local": { "seed": 3, "players": 8, "humans": 0b101 } });
    let (seen, _) = play(
        &local,
        Box::new(|count, now| match count {
            10 => Some(json!({ "type": "switch_seat", "seat": "e" })),
            12 => {
                assert!(now.can_undo, "an answer since the change: {now:?}");
                Some(json!({
                    "type": "undo", "protocol_version": 4, "game_id": GAME,
                    "expected_version": now.version,
                }))
            }
            13 => None,
            _ => Some(answer()),
        }),
    );
    assert_eq!(seen.len(), 13, "{seen:#?}");
    // Before the change every question but the first has an answer of the page to take back,
    // the answer of its other seat too: a die, a shuffle and the other seats do not cut.
    assert!(!seen[0].can_undo);
    for (index, now) in seen.iter().enumerate().take(10).skip(1) {
        assert!(now.can_undo, "question {index}: {now:?}");
    }
    assert!(
        (1..10).any(|index| seen[index].viewer != seen[index - 1].viewer),
        "the page answers for both of its seats: {seen:#?}"
    );
    // The seat changed: the answers from before it stay.
    assert!(!seen[10].can_undo, "{:?}", seen[10]);
    // The undo opens the question of the eleventh again, and nothing is left to take back.
    assert_eq!(seen[12].asked, seen[10].asked);
    assert_eq!(seen[12].nonce, seen[10].nonce);
    assert!(!seen[12].can_undo, "{:?}", seen[12]);
    ti4_wasm::set_round_limit_native(None);
}

#[test]
fn the_other_seats_wait_for_a_step_and_play_the_same_game() {
    let _serial = serial();
    ti4_wasm::set_round_limit_native(Some(1));
    let game = |stepping: u32| {
        let request =
            json!({ "local": { "seed": 5, "players": 4, "humans": 1 }, "stepping": stepping });
        let answers = Arc::new(Mutex::new(0_usize));
        let counted = answers.clone();
        let (seen, _) = play(
            &request,
            Box::new(move |_, now| {
                if now.waiting.is_some() {
                    return Some(json!({ "type": "step" }));
                }
                let mut answers = counted.lock().unwrap();
                *answers += 1;
                (*answers <= 6).then(answer)
            }),
        );
        (seen, save()["decisions"].clone())
    };
    let (run, played) = game(0);
    let (stepped, again) = game(1);
    assert_eq!(played, again, "stepping does not change the game");
    let waits: Vec<&Seen> = stepped.iter().filter(|now| now.waiting.is_some()).collect();
    assert!(!waits.is_empty());
    for now in &waits {
        // The page is shown its own seat, with no choice; the turn is of the seat it waits for.
        assert_eq!(now.viewer, "a");
        assert_eq!(now.asked, None);
        assert_ne!(now.waiting.as_deref(), Some("a"));
    }
    assert_eq!(stepped.len() - waits.len(), run.len());
    ti4_wasm::set_round_limit_native(None);
}

#[test]
fn a_saved_decision_that_the_game_does_not_offer_ends_the_run_and_says_where() {
    let _serial = serial();
    let request = json!({
        "local": { "seed": 3, "players": 8, "humans": 1 },
        "resume": { "decisions": ["no-such"] },
    });
    let (seen, end) = play(&request, Box::new(|_, _| None));
    assert!(seen.is_empty(), "the page is not asked: {seen:?}");
    let diverged = end["end"]["diverged"].as_str().unwrap();
    assert!(
        diverged.starts_with("decision 1 of 1 was \"no-such\", but"),
        "{diverged}"
    );
}
