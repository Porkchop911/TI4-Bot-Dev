//! A scenario of `ti4_host::view::scenario` in web2's local play: the page opens at the same
//! question as the server's launch of the same scenario and seed, after the same opening.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_host::protocol::status::ViewerRole;
use ti4_host::view::scenario::SCENARIOS;
use ti4_server::session::GameRegistry;
use ti4_wasm::host_api;

use crate::common::{last_question, serial};

/// The first question of a game: its seat, its subtype and its prompt, and the answers of the
/// opening before it.
#[derive(Debug, PartialEq)]
struct Opened {
    seat: String,
    subtype: String,
    prompt: String,
    opening: Vec<String>,
}

/// The first question of the page in a local game of the scenario `id`: the page plays the
/// seats that people play on the server, the rule bot the others.
fn in_the_page(id: &str, humans: &[usize], players: usize) -> Opened {
    let asked = Arc::new(Mutex::new(None::<Value>));
    let sink = asked.clone();
    ti4_wasm::set_native_host(move |output| {
        let envelope: Value = serde_json::from_str(output).expect("an envelope");
        if envelope["kind"] == "progress" {
            return host_api::CONTINUE;
        }
        *sink.lock().unwrap() = Some(envelope);
        -1
    });
    let mask: u32 = humans.iter().map(|place| 1 << place).sum();
    let bots: Vec<Option<&str>> = (0..players)
        .map(|place| (!humans.contains(&place)).then_some("smart"))
        .collect();
    let request = json!({ "local": { "humans": mask, "bots": bots, "scenario": id } });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    assert!(status >= 0, "{id}: {}", host_api::out_text());
    let envelope = asked.lock().unwrap().take().expect("the page is asked");
    let messages = envelope["messages"].as_array().unwrap();
    let question = last_question(messages);
    let choice = &question["pending_choice"]["choice"];
    let saved = host_api::with(|session| session.decision_log().to_vec()).unwrap_or_default();
    Opened {
        seat: choice["player"].as_str().unwrap().to_owned(),
        subtype: choice["context"]["subtype"]
            .as_str()
            .unwrap_or_default()
            .to_owned(),
        prompt: choice["prompt"].as_str().unwrap().to_owned(),
        opening: saved
            .iter()
            .map(|record| format!("{}:{}", record.player.as_str(), record.chosen))
            .collect(),
    }
}

/// The first question after the server's launch, with its seats named as the page names them.
fn on_the_server(id: &str) -> Opened {
    let registry = Arc::new(GameRegistry::new());
    let launch = ti4_server::dev::launch_scenario(&registry, id, Some(42)).expect("a launch");
    let session = registry.get_game(&launch.game_id).unwrap();
    let state = session.current_state();
    let letter = |player: &ti4_model::id::PlayerId| {
        let place = state
            .players
            .iter()
            .position(|seat| seat.id == *player)
            .unwrap();
        ["a", "b", "c", "d", "e", "f", "g", "h"][place].to_owned()
    };
    let (seat, _, _) = session.open_pending_decision().expect("a question");
    let choice = session
        .get_snapshot(&ViewerRole::Player(seat.clone()))
        .pending_choice
        .expect("a question")
        .choice;
    let opened = Opened {
        seat: letter(&seat),
        subtype: choice
            .context
            .map(|context| context.subtype)
            .unwrap_or_default(),
        prompt: choice.prompt,
        opening: session
            .decision_log()
            .iter()
            .map(|record| format!("{}:{}", letter(&record.player), record.chosen))
            .collect(),
    };
    session.stop();
    opened
}

#[test]
fn a_scenario_opens_at_the_same_question_in_the_page_and_on_the_server() {
    let _serial = serial();
    for scenario in SCENARIOS.iter().filter(|scenario| scenario.players == 3) {
        let page = in_the_page(scenario.id, scenario.humans, scenario.players);
        assert_eq!(page, on_the_server(scenario.id), "{}", scenario.id);
    }
}
