//! The updates of `fixtures/protocol/update-*.json`: one update of a game that the hosted exports
//! play, as the page is shown it. The tests of web2 read them. `cargo wire-web` writes them (this
//! test, with `TI4_WRITE_WIRE`); otherwise the test fails while one differs from what the engine
//! plays now.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_wasm::host_api;

use crate::common::{GAME, send, serial};

/// The fields of a `state_update` that are the update of the page (`SessionUpdate`, what
/// `updateOf` in `web2/src/session/serverMessages.ts` reads).
const UPDATE: [&str; 13] = [
    "viewer",
    "view",
    "pending_choice",
    "turn_status",
    "tactical",
    "exploration",
    "action_cards",
    "secret_objectives",
    "score",
    "agendas",
    "secondary",
    "reaction",
    "seats",
];

/// One recorded update: the game, the question that is recorded, and the answers to the first
/// questions, so that the game reaches it. A question without an answer here is answered by a
/// fixed rule.
struct Recording {
    name: &'static str,
    /// `{"seed", "players", "humans"}` or `{"scenario", "humans"}`.
    game: Value,
    question: At,
    answers: &'static [&'static str],
}

/// Which question a recording keeps.
#[derive(Clone, Copy)]
enum At {
    /// The question of this number, counted from 1: a short script that leads to it.
    Question(usize),
    /// The first turn menu of the human seat in this round that still offers its strategic
    /// action. Found by what it shows, not by its number, so that a change of how many questions
    /// come before it (a rule asked in fewer steps) keeps the same kind of position.
    TurnMenu { round: u64 },
}

impl At {
    fn is(self, asked: usize, update: &Value) -> bool {
        match self {
            Self::Question(wanted) => asked == wanted,
            Self::TurnMenu { round } => {
                let choice = &update["pending_choice"]["choice"];
                let offers = |id: &str| {
                    choice["options"]
                        .as_array()
                        .is_some_and(|options| options.iter().any(|o| o["id"] == id))
                };
                update["view"]["round"] == round
                    && update["view"]["phase"] == "action"
                    && choice["player"] == update["viewer"]["seat"]
                    && offers("strategic")
                    && offers("tactical")
            }
        }
    }
}

fn recordings() -> Vec<Recording> {
    let seed = |seed: u64| json!({ "seed": seed, "players": 8, "humans": 1 });
    let late = json!({ "scenario": "late-game", "humans": 1 });
    vec![
        Recording {
            name: "update-activation",
            game: seed(3),
            question: At::Question(3),
            answers: &["pok8imperial", "tactical"],
        },
        Recording {
            name: "update-movement",
            game: seed(3),
            question: At::Question(4),
            answers: &["pok8imperial", "tactical", "23"],
        },
        Recording {
            name: "update-load",
            game: seed(3),
            question: At::Question(5),
            answers: &["pok8imperial", "tactical", "23", "move|01|0"],
        },
        Recording {
            name: "update-landing",
            game: seed(3),
            question: At::Question(9),
            answers: &[
                "pok8imperial",
                "tactical",
                "23",
                "move|01|0",
                "load|3",
                "load|4",
                "done_loading",
                "done_moving",
            ],
        },
        Recording {
            name: "update-menu",
            game: seed(3),
            question: At::TurnMenu { round: 2 },
            answers: &[],
        },
        Recording {
            name: "update-battle",
            game: late.clone(),
            question: At::Question(3),
            answers: &["tactical", "32"],
        },
        Recording {
            name: "update-production",
            game: late,
            question: At::Question(5),
            answers: &["tactical", "72", "done_moving", "decline"],
        },
    ]
}

/// What the host was told at each question, and how the recording ended.
#[derive(Default)]
struct Run {
    /// One line for each question: its number, the seat, the prompt and the option ids.
    asked: Vec<String>,
    update: Option<Value>,
    failure: Option<String>,
}

/// The update of a question, and its line for `Run::asked`.
fn question(envelope: &Value, asked: usize) -> Option<(Value, Value, String)> {
    let message = envelope["messages"]
        .as_array()?
        .iter()
        .rev()
        .find(|message| {
            matches!(
                message["type"].as_str(),
                Some("state_update" | "initial_snapshot")
            )
        })?;
    let update: serde_json::Map<String, Value> = UPDATE
        .iter()
        .filter_map(|key| {
            let value = message.get(*key).filter(|value| !value.is_null())?;
            Some(((*key).to_owned(), value.clone()))
        })
        .collect();
    let choice = &message["pending_choice"]["choice"];
    let ids: Vec<&str> = choice["options"]
        .as_array()
        .map(|options| options.iter().filter_map(|o| o["id"].as_str()).collect())
        .unwrap_or_default();
    let line = format!(
        "{asked:3} {} {:?} {ids:?}",
        choice["player"].as_str().unwrap_or("?"),
        choice["prompt"].as_str().unwrap_or("?")
    );
    Some((message.clone(), Value::Object(update), line))
}

/// Plays the game of `recording` up to its question and gives the update of that question. The
/// host must not panic (it is called across the wasm boundary): a failure is kept and the game
/// is left.
fn record(recording: &Recording) -> Result<Value, String> {
    let run = Arc::new(Mutex::new(Run::default()));
    let sink = run.clone();
    let mut answers = recording.answers.iter();
    let wanted = recording.question;
    let mut asked = 0_usize;
    ti4_wasm::set_native_host(move |output| {
        let mut run = sink
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Ok(envelope) = serde_json::from_str::<Value>(output) else {
            run.failure = Some(format!("an output that is not JSON: {output}"));
            return -1;
        };
        if envelope["kind"] == "progress" {
            return host_api::CONTINUE;
        }
        asked += 1;
        let Some((message, update, line)) = question(&envelope, asked) else {
            run.failure = Some(format!("an ask without an update: {output}"));
            return -1;
        };
        run.asked.push(line);
        if wanted.is(asked, &update) {
            run.update = Some(update);
            return -1;
        }
        let pending = &message["pending_choice"];
        let options = pending["choice"]["options"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let id = match answers.next() {
            Some(id) => (*id).to_owned(),
            // Not always the first option, so that the game moves on.
            None => options
                .get(asked * 7 % options.len().max(1))
                .and_then(|option| option["id"].as_str())
                .unwrap_or_default()
                .to_owned(),
        };
        let answered = send(&json!({
            "type": "submit_choice",
            "protocol_version": ti4_host::PROTOCOL_VERSION,
            "game_id": GAME,
            "nonce": pending["nonce"],
            "expected_version": message["game_version"],
            "option_id": id,
        }));
        match answered["resume"]
            .as_i64()
            .and_then(|code| i32::try_from(code).ok())
        {
            Some(code) => code,
            None => {
                run.failure = Some(format!(
                    "question {asked} does not offer {id:?}: {answered}"
                ));
                -1
            }
        }
    });
    let len = host_api::put_input(&json!({ "local": recording.game }).to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).expect("a short request"));
    let run = std::mem::take(
        &mut *run
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    let failure = if status < 0 {
        Some(format!("the game does not start: {}", host_api::out_text()))
    } else {
        run.failure
    };
    match (failure, run.update) {
        (None, Some(update)) => Ok(update),
        (failure, _) => Err(format!(
            "{}: {}\nthe questions:\n{}",
            recording.name,
            failure.unwrap_or_else(|| "the game ended before its question".to_owned()),
            run.asked.join("\n")
        )),
    }
}

#[test]
fn protocol_fixtures_are_current() {
    let _serial = serial();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/protocol");
    let write = std::env::var_os("TI4_WRITE_WIRE").is_some_and(|value| value == "1");
    let mut stale = Vec::new();
    for recording in recordings() {
        let update = record(&recording).unwrap_or_else(|failure| panic!("{failure}"));
        let path = dir.join(format!("{}.json", recording.name));
        if write {
            std::fs::write(&path, format!("{update}\n")).expect("the fixture is written");
            continue;
        }
        let kept: Value = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(Value::Null);
        if kept != update {
            stale.push(recording.name);
        }
    }
    assert!(
        stale.is_empty(),
        "fixtures/protocol differs from what the engine plays now; write it again with \
         `cargo wire-web`: {stale:?}"
    );
}
