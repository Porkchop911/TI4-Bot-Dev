//! Starting technology on the server: a table that starts with the choice asks the Winnu seat
//! before the first strategy card, the other seats wait, the answers are decisions of the log (so a
//! recovery replays them), and a game whose opening state has no picks plays as it always did.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::ViewerRole;
use ti4_host::replay::replay_session;
use ti4_host::setup::{self, SpeakerOrder, TableSetup};
use ti4_model::id::{PlayerId, TechnologyId};
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

fn players() -> Vec<PlayerId> {
    (1..=3).map(|i| PlayerId::new(format!("p{i}"))).collect()
}

fn table(with_choice: bool) -> TableSetup {
    let table = TableSetup {
        speaker_order: SpeakerOrder::Seat,
        ..TableSetup::new(3, 77)
    };
    if with_choice {
        table.with_starting_tech()
    } else {
        table
    }
}

fn opening(with_choice: bool) -> (ti4_model::state::GameState, ti4_content::galaxy::Galaxy) {
    let table = table(with_choice);
    let chosen = vec![
        Some("winnu".to_owned()),
        Some("argent".to_owned()),
        Some("sol".to_owned()),
    ];
    let drawn = setup::draw(&table, &chosen).expect("draw");
    setup::create(ContentStore::embedded(), &table, &players(), &drawn).expect("create")
}

#[test]
fn the_winnu_seat_is_asked_first_and_the_answers_are_decisions_of_the_log() {
    let ps = players();
    let (state, galaxy) = opening(true);
    assert_eq!(
        state.starting_tech_picks.len(),
        2,
        "Winnu and Argent owe picks"
    );
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new("game_starting_tech", state.clone())
        .with_seed(77)
        .with_player_ids(ps.clone())
        .with_galaxy(galaxy.clone(), tiles)
        .with_seat(ps[0].clone(), SeatController::Human)
        .with_seat(ps[1].clone(), SeatController::SMART)
        .with_seat(ps[2].clone(), SeatController::Human);
    let session = Arc::new(GameSession::start(config));
    let winnu = MockClient::connect(session.clone(), ViewerRole::Player(ps[0].clone()));
    let sol = MockClient::connect(session.clone(), ViewerRole::Player(ps[2].clone()));

    let mut asked = None;
    for _ in 0..500 {
        asked = winnu.snapshot().pending_choice;
        if asked.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pending = asked.expect("the Winnu are asked");
    assert_eq!(pending.choice.player, ps[0]);
    assert_eq!(
        pending.choice.context.as_ref().unwrap().subtype,
        "starting_technology"
    );
    assert_eq!(pending.choice.options.len(), 8);
    assert!(
        sol.snapshot().pending_choice.is_none(),
        "the other seats wait for them"
    );
    let version = winnu.snapshot().game_version;
    winnu
        .submit(&pending.nonce, version, "det")
        .expect("the Winnu take Dark Energy Tap");

    // The Argent bot chooses its two, then the draft opens for the first seat.
    let mut subtype = String::new();
    for _ in 0..500 {
        if let Some(p) = winnu.snapshot().pending_choice {
            subtype = p.choice.context.map(|c| c.subtype).unwrap_or_default();
            if subtype == "draft_strategy_card" {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(subtype, "draft_strategy_card");
    let now = session.current_state();
    assert!(now.starting_tech_picks.is_empty());
    let held = |p: &PlayerId| now.player(p).unwrap().technologies.clone();
    assert_eq!(held(&ps[0]), [TechnologyId::new("det")].into());
    assert_eq!(held(&ps[1]).len(), 2, "the Argent Flight took two");
    assert!(
        held(&ps[1])
            .iter()
            .all(|t| ["nm", "st", "ps"].contains(&t.as_str()))
    );

    // Recovery replays the opening state and the log to the same state and hashes.
    let log = session.decision_log();
    assert_eq!(
        log.iter()
            .filter(|r| r.prompt.contains("starting technology"))
            .count(),
        3
    );
    let report = replay_session(&state, Some(&galaxy), &log).expect("replay");
    assert!(report.hashes_match);
    assert_eq!(report.replayed_hashes, session.decision_hashes());
    for message in sol.drain_messages() {
        let _ = matches!(message, ServerMessage::Event(_));
    }
    session.stop();
}

#[test]
fn an_opening_state_made_without_the_choice_is_the_game_it_always_was() {
    let (state, _) = opening(false);
    assert!(state.starting_tech_picks.is_empty());
    let winnu = state.player(&players()[0]).unwrap();
    assert!(
        winnu.technologies.is_empty(),
        "no choice, no technology: as before"
    );
    // The key is absent from the serialized state, so a stored `init.json` of an older game
    // keeps the bytes (and the checksum) it was written with.
    let json = serde_json::to_string(&state).unwrap();
    assert!(!json.contains("starting_tech_picks"));
    let (with, _) = opening(true);
    assert!(
        serde_json::to_string(&with)
            .unwrap()
            .contains("starting_tech_picks")
    );
}
