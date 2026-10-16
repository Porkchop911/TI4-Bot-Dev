//! The last strategy card of a four-player draft is taken for the picker, who is told so.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::setup::start_game_seeded;
use ti4_model::POK;
use ti4_model::id::PlayerId;
use ti4_server::protocol::server::ServerMessage;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::session::{GameSession, MockClient, SeatController, SessionConfig};

#[test]
fn the_lone_last_card_is_taken_for_its_picker_and_only_that_seat_hears_of_it() {
    let ids: Vec<PlayerId> = ["p1", "p2", "p3", "p4"]
        .iter()
        .map(|id| PlayerId::new(*id))
        .collect();
    let state = start_game_seeded(ContentStore::embedded(), &ids, POK, None, 42).expect("game");
    let mut config = SessionConfig::new("draft_last_card", state);
    for id in &ids {
        config = config.with_seat(id.clone(), SeatController::Human);
    }
    let session = Arc::new(GameSession::start(config));
    let clients: Vec<(PlayerId, MockClient)> = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                MockClient::connect(session.clone(), ViewerRole::Player(id.clone())),
            )
        })
        .collect();
    let spectator = MockClient::connect(session.clone(), ViewerRole::Spectator);

    let mut heard: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    let mut spectator_notes = 0;
    let mut asked = 0;
    let drain = |heard: &mut BTreeMap<String, Vec<(String, String)>>| {
        for (id, client) in &clients {
            while let Ok(message) = client.try_recv() {
                if let ServerMessage::StateUpdate(update) = message {
                    for note in update.auto_resolved {
                        heard
                            .entry(id.to_string())
                            .or_default()
                            .push((note.selected, note.reason));
                    }
                }
            }
        }
    };
    let deadline = Instant::now() + Duration::from_secs(20);
    // Draft the first seven cards; the eighth must never be put to anyone.
    while session.current_state().phase == ti4_model::Phase::Strategy {
        assert!(Instant::now() < deadline, "the draft stalled");
        if let Some((actor, nonce, version)) = session.current_pending_decision() {
            let option = session.current_state().unclaimed_strategy_cards[0].to_string();
            if session
                .submit_choice(&actor, &nonce, version, &option)
                .is_ok()
            {
                asked += 1;
            }
        }
        drain(&mut heard);
        thread::sleep(Duration::from_millis(10));
    }
    // The next state update carries the note.
    while heard.is_empty() && Instant::now() < deadline {
        drain(&mut heard);
        thread::sleep(Duration::from_millis(10));
    }
    while let Ok(message) = spectator.try_recv() {
        if let ServerMessage::StateUpdate(update) = message {
            spectator_notes += update.auto_resolved.len();
        }
    }

    assert_eq!(asked, 7, "seven picks were asked for, the eighth was not");
    assert_eq!(heard.len(), 1, "exactly one seat is told: {heard:?}");
    let notes = heard.values().next().unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].1, "only one strategy card left");
    assert!(
        notes[0].0.contains(". "),
        "the card is named: {}",
        notes[0].0
    );
    assert_eq!(spectator_notes, 0);
    let state = session.current_state();
    assert!(state.unclaimed_strategy_cards.is_empty());
    assert!(state.players.iter().all(|p| p.strategy_cards.len() == 2));
}
