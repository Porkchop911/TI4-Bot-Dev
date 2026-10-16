//! The random seats of a game (`SeatController::BotRandom`): they decide from the seed of the
//! game, and a game that is recovered from its decision log goes on as the game that was never
//! stopped.

use ti4_model::id::PlayerId;
use ti4_server::session::{GameSession, SessionConfig};

use crate::draft_scripts::{asked, config, passive};

/// Plays `seat` passively until it is asked its decision number `own` (counted from 1), and
/// returns the log of the game there.
fn play(
    session: &GameSession,
    seat: &PlayerId,
    own: usize,
) -> Vec<ti4_engine::choice::DecisionRecord> {
    loop {
        let (choice, nonce, version) = asked(session, seat);
        let log = session.decision_log();
        if log.iter().filter(|record| record.player == *seat).count() + 1 >= own {
            return log;
        }
        session
            .submit_choice(seat, &nonce, version, &passive(&choice))
            .expect("the answer is taken");
    }
}

fn game(game_id: &str) -> (SessionConfig, PlayerId) {
    config(game_id, 11)
}

#[test]
fn a_recovered_game_goes_on_as_the_game_that_was_never_stopped() {
    let (straight, seat) = game("random_straight");
    let session = GameSession::start(straight);
    let whole = play(&session, &seat, 14);
    session.stop();
    assert!(
        whole.iter().filter(|record| record.player != seat).count() > 20,
        "the random seats decided"
    );

    // The same game, stopped early: the server keeps its decisions and nothing else.
    let (first, _) = game("random_first");
    let session = GameSession::start(first);
    let saved = play(&session, &seat, 6);
    let events = session.event_log();
    session.stop();
    assert!(saved.len() < whole.len());
    assert_eq!(
        saved[..],
        whole[..saved.len()],
        "the same seed is the same game"
    );

    // The game is played again from the log, and then goes on: every random seat draws next
    // what it would have drawn.
    let (again, _) = game("random_again");
    let session = GameSession::start_recovered(again, saved.clone(), events);
    session.wait_replayed().expect("the replay ends");
    let recovered = play(&session, &seat, 14);
    session.stop();
    assert_eq!(recovered, whole);
}

#[test]
fn two_seeds_are_two_games() {
    let (first, seat) = config("random_one", 11);
    let (second, _) = config("random_two", 12);
    let one = GameSession::start(first);
    let two = GameSession::start(second);
    let of_one = play(&one, &seat, 4);
    let of_two = play(&two, &seat, 4);
    one.stop();
    two.stop();
    assert_ne!(of_one, of_two);
}
