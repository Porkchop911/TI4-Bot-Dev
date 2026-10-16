//! The hosted game and the server's own session make the same game: for one seed and the same
//! answers of the person, the decisions, the events (ids, kinds, cursors, dice, details) and the
//! final state are equal. Only the clocks differ.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use ti4_engine::choice::Choice;
use ti4_host::protocol::server::GameEvent;
use ti4_model::id::PlayerId;
use ti4_server::{GameSession, SeatController, SessionConfig};
use ti4_wasm::host_api;
use ti4_wasm::host_setup::Setup;

/// Plays the hosted game with the rule bot as the person; returns its answers, decisions, events
/// and final state.
fn hosted(
    setup: &Setup,
) -> (
    Vec<String>,
    Vec<ti4_engine::choice::DecisionRecord>,
    Vec<GameEvent>,
) {
    let answers = Arc::new(Mutex::new(Vec::<String>::new()));
    let seed = setup.seed;
    let game_id = setup.game_id.clone();
    let log = answers.clone();
    let viewer = PlayerId::new("a");
    ti4_wasm::set_native_host(move |text: &str| {
        let envelope: Value = serde_json::from_str(text).unwrap();
        if envelope["kind"] == "progress" {
            return host_api::CONTINUE;
        }
        let messages = envelope["messages"].as_array().unwrap();
        let update = messages
            .iter()
            .rev()
            .find(|m| m["pending_choice"].is_object())
            .expect("a question");
        let choice: Choice =
            serde_json::from_value(update["pending_choice"]["choice"].clone()).unwrap();
        let cursor = usize::try_from(update["history"]["cursor"].as_u64().unwrap_or(0)).unwrap();
        let (picked, _) = ti4_rule_bot::choose(&choice, None, seed, &viewer, cursor);
        let id = choice.options[picked].id.clone();
        let command = json!({
            "type": "submit_choice", "protocol_version": 3, "game_id": game_id,
            "nonce": update["pending_choice"]["nonce"],
            "expected_version": update["game_version"], "option_id": id,
        });
        let len = host_api::put_input(&command.to_string());
        assert!(host_api::command(len) >= 0, "{}", host_api::out_text());
        let answered: Value = serde_json::from_str(&host_api::out_text()).unwrap();
        log.lock().unwrap().push(id);
        i32::try_from(answered["resume"].as_i64().unwrap()).unwrap()
    });
    let request = json!({ "setup": setup });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    assert!(status >= 0, "{}", host_api::out_text());
    let end: Value = serde_json::from_str(&host_api::out_text()).unwrap();
    assert_eq!(end["end"]["finished"], true, "{end}");
    let answers = answers.lock().unwrap().clone();
    let (decisions, events) =
        host_api::with(|s| (s.decision_log().to_vec(), s.events().to_vec())).unwrap();
    (answers, decisions, events)
}

/// The same table through the server's session: seat `a` answers from the script, the others are
/// the server's rule bots.
fn server(
    setup: &Setup,
    answers: Vec<String>,
) -> (
    Vec<ti4_engine::choice::DecisionRecord>,
    Vec<GameEvent>,
    ti4_model::state::GameState,
) {
    let built = setup.build().expect("the table builds");
    let ids: Vec<PlayerId> = (0..built.players)
        .map(|seat| PlayerId::new(["a", "b", "c", "d", "e", "f"][seat]))
        .collect();
    let galaxy = built.galaxy.clone().unwrap();
    let tiles = ti4_view::map::build_board_tiles(ti4_content::ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(setup.game_id.clone(), built.game.state.clone())
        .with_seed(built.seed)
        .with_player_ids(ids.clone())
        .with_galaxy(galaxy, tiles)
        .with_random_bot_delay_ms(0);
    for (index, id) in ids.iter().enumerate() {
        let controller = if index == built.human {
            SeatController::BotScripted(answers.clone())
        } else {
            SeatController::BotSmart
        };
        config = config.with_seat(id.clone(), controller);
    }
    let session = GameSession::start(config);
    let deadline = Instant::now() + Duration::from_secs(300);
    while !session.is_finished() {
        assert!(session.error().is_none(), "{:?}", session.error());
        assert!(Instant::now() < deadline, "the server game did not finish");
        std::thread::sleep(Duration::from_millis(20));
    }
    let out = (
        session.decision_log(),
        session.event_log(),
        session.current_state(),
    );
    session.stop();
    out
}

/// An event without its clocks: the time of day and the version of the update it was made at.
fn plain(event: &GameEvent) -> Value {
    let mut value = serde_json::to_value(event).unwrap();
    let object = value.as_object_mut().unwrap();
    object.remove("timestamp");
    object.remove("version");
    value
}

fn parity(seed: u64, players: usize) {
    let _serial = crate::common::serial();
    let setup: Setup = serde_json::from_value(
        json!({ "game_id": "parity", "seed": seed, "players": players, "human_seat": 0 }),
    )
    .unwrap();
    let (answers, decisions, events) = hosted(&setup);
    let (server_decisions, server_events, _state) = server(&setup, answers);
    assert!(
        decisions.len() > 150,
        "a real game: {} decisions",
        decisions.len()
    );
    assert!(
        events.iter().any(|e| matches!(
            e.event,
            ti4_host::protocol::server::GameEventKind::DiceRolled
        )),
        "dice are events"
    );
    assert_eq!(decisions.len(), server_decisions.len(), "decisions");
    for (index, (ours, theirs)) in decisions.iter().zip(&server_decisions).enumerate() {
        assert_eq!(ours, theirs, "decision {index}");
    }
    assert_eq!(events.len(), server_events.len(), "events");
    for (index, (ours, theirs)) in events.iter().zip(&server_events).enumerate() {
        assert_eq!(plain(ours), plain(theirs), "event {index}");
    }
}

#[test]
fn slow_four_players_make_the_servers_game() {
    parity(11, 4);
}

#[test]
fn slow_three_players_make_the_servers_game() {
    parity(3, 3);
}
