//! A replacement session (recovery, rewind, turn redo, restore) used to replay the whole history
//! twice more after the caller's own check: once privately for the position at the first live
//! question and once in the worker. One replay now copies the game near its end and the worker
//! forks that copy. These tests pin that the fork is taken (step counts, not wall clock) and that
//! it lands on exactly the position a full replay reaches.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_host::protocol::server::GameEvent;
use ti4_host::protocol::status::ViewerRole;
use ti4_host::replay::{replay_session_forced, replay_session_with_copy};
use ti4_host::rng_force::RngMarks;
use ti4_model::id::PlayerId;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

fn base_config() -> SessionConfig {
    let players: Vec<PlayerId> = ["p1", "p2", "p3"].into_iter().map(PlayerId::new).collect();
    let content = ContentStore::embedded();
    let (state, galaxy) =
        ti4_host::map::create_game_with_template(content, &players, 515_151, None)
            .expect("create map");
    let tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    SessionConfig::new("game_replay_copy", state)
        .with_galaxy(galaxy, tiles)
        .with_player_ids(players.clone())
        .with_seat(players[0].clone(), SeatController::Human)
        .with_seat(players[1].clone(), SeatController::Human)
        .with_seat(players[2].clone(), SeatController::BotFirstOption)
}

/// Play until at least `count` decisions are recorded, the humans always taking the first option.
fn play(count: usize) -> (Vec<DecisionRecord>, Vec<GameEvent>) {
    let session = Arc::new(GameSession::start(base_config()));
    let deadline = Instant::now() + Duration::from_secs(120);
    while session.decision_log().len() < count {
        assert!(Instant::now() < deadline, "the game stalled");
        assert!(session.error().is_none(), "{:?}", session.error());
        if let Some((seat, nonce, version)) = session.current_pending_decision() {
            let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
            if let Some(pending) = snapshot.pending_choice {
                let _ =
                    session.submit_choice(&seat, &nonce, version, &pending.choice.options[0].id);
                continue;
            }
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    session.stop();
    // The log can run past `count` inside one step; stop at a step the humans answered.
    (session.decision_log(), session.event_log())
}

struct Resumed {
    resumed_from: Option<usize>,
    position: serde_json::Value,
}

fn resume(decisions: &[DecisionRecord], events: &[GameEvent], step_snapshots: bool) -> Resumed {
    let mut config = base_config().with_prior_history(decisions.to_vec(), events.to_vec());
    config.step_snapshots = step_snapshots;
    let session = GameSession::start(config);
    session.wait_replayed().expect("replayed");
    let (seat, _, version) = session
        .current_pending_decision()
        .expect("a human is asked");
    let snapshot = session.get_snapshot(&ViewerRole::Player(seat.clone()));
    let position = serde_json::json!({
        "seat": seat,
        "version": version,
        "state": snapshot.state,
        "pending": snapshot.pending_choice.map(|pending| pending.choice),
        "decisions": session.decision_log(),
    });
    let resumed_from = session.resumed_from();
    session.stop();
    Resumed {
        resumed_from,
        position,
    }
}

#[test]
fn a_replacement_session_forks_the_copy_and_reaches_the_full_replay_position() {
    let (decisions, events) = play(150);
    let forked = resume(&decisions, &events, true);
    let full = resume(&decisions, &events, false);

    assert_eq!(
        full.resumed_from, None,
        "without copies the worker replays it all"
    );
    let at = forked
        .resumed_from
        .expect("the worker forks the copy its private replay took");
    assert!(
        at <= decisions.len() && at + 64 >= decisions.len(),
        "the copy is near the end of the history: {at} of {}",
        decisions.len()
    );
    assert_eq!(forked.position, full.position);
}

#[test]
fn the_checking_replay_copies_the_game_without_changing_its_report() {
    let (decisions, _) = play(120);
    let config = base_config();
    let plain = replay_session_forced(
        &config.state,
        config.galaxy.as_ref(),
        &decisions,
        &RngMarks::new(),
    )
    .expect("replay");
    let (report, copy) = replay_session_with_copy(
        &config.state,
        config.galaxy.as_ref(),
        &decisions,
        &RngMarks::new(),
        1,
    )
    .expect("replay");
    assert_eq!(report, plain);
    assert!(report.hashes_match);
    let copy = copy.expect("a step boundary near the end was copied");
    assert!(copy.is_prefix_of(&decisions));
    assert!(copy.log_len + 64 >= decisions.len());
}
