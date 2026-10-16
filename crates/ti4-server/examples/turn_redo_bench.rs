//! Times game recovery and one turn redo (request, then restore) on a COPY of a saved game.
//!
//! ```text
//! turn_redo_bench <store root> <game id> <seat player id>       # recover, redo, restore
//! turn_redo_bench <store root> <game id> -                      # recover only
//! turn_redo_bench <store root> <game id> <seat> grow<N>         # play on to N decisions
//! turn_redo_bench <store root> <game id> - <N>                  # cut the history to N decisions
//! ```
//!
//! Every mode but "recover only" rewrites the saved game, so point it at a copy. `grow` lets the
//! bot seats play themselves and answers the given seat pseudo-randomly; `<N>` keeps the first N
//! decisions (a save that no longer replays to its end still has a long replayable prefix). Lines
//! starting with `BENCH` are the timings.
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_host::protocol::status::ViewerRole;
use ti4_model::id::PlayerId;
use ti4_server::session::{GameRegistry, GameSession, random_bot};
use ti4_server::storage::FileGameStore;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (root, game_id, seat) = (&args[1], &args[2], &args[3]);
    let store = Arc::new(FileGameStore::new(root).expect("store"));
    if let Some(len) = args.get(4).and_then(|v| v.parse::<usize>().ok()) {
        truncate(&store, game_id, len);
        return;
    }
    let registry = GameRegistry::new().with_store(store.clone());
    let started = Instant::now();
    let report = registry.recover_all_games_report().expect("recovery");
    eprintln!(
        "BENCH recover_all_games: {:?} (recovered {}, failed {})",
        started.elapsed(),
        report.recovered.len(),
        report.failed.len()
    );
    for failure in &report.failed {
        eprintln!(
            "BENCH failed {} {}: {}",
            failure.game_id, failure.stage, failure.error
        );
    }
    if seat == "-" {
        return;
    }
    let session = registry.get_game(game_id).expect("session");
    let started = Instant::now();
    session.wait_replayed().expect("replayed");
    eprintln!(
        "BENCH wait_replayed after recovery: {:?}",
        started.elapsed()
    );
    if let Some(target) = args
        .get(4)
        .and_then(|v| v.strip_prefix("grow"))
        .and_then(|v| v.parse::<usize>().ok())
    {
        grow(&session, &PlayerId::new(seat.clone()), target);
        return;
    }

    let credential = store
        .load_player_sessions(game_id)
        .expect("sessions")
        .sessions
        .into_iter()
        .find(|(id, _)| id.as_str() == seat)
        .map(|(_, s)| s.as_str().to_owned())
        .expect("seat credential");
    let started = Instant::now();
    let snapshot = registry
        .turn_redo_request(game_id, &credential, session.game_version(), None, None)
        .expect("turn redo request");
    eprintln!("BENCH turn_redo_request: {:?}", started.elapsed());
    let started = Instant::now();
    registry
        .turn_redo_restore(game_id, &credential, snapshot.game_version)
        .expect("turn redo restore");
    eprintln!("BENCH turn_redo_restore: {:?}", started.elapsed());
    registry.get_game(game_id).expect("session").stop();
}

/// Keep the first `len` decisions of the saved history (and the events, batches and marks up to
/// there).
fn truncate(store: &FileGameStore, game_id: &str, len: usize) {
    let mut history = store.load_history(game_id).expect("load").expect("history");
    history.decisions.truncate(len);
    history.redo.clear();
    history.redo_events.clear();
    let split = ti4_host::history::turn_redo::events_through(&history.events, 0, len);
    history.events.truncate(split);
    history.batches.retain(|b| b.end_cursor <= len);
    history.rng_marks.truncate_to(len);
    store.save_history(game_id, &history).expect("save");
    eprintln!("BENCH truncated to {len} decisions");
}

/// Play on until `target` decisions are recorded: bots move themselves, `seat` picks
/// pseudo-randomly.
fn grow(session: &GameSession, seat: &PlayerId, target: usize) {
    random_bot::set_random_bot_delay_override(Some(0));
    let mut last = (session.decision_log().len(), Instant::now());
    loop {
        let len = session.decision_log().len();
        if len >= target || session.is_finished() || session.error().is_some() {
            eprintln!("BENCH grow stopped at {len}, error {:?}", session.error());
            break;
        }
        if len != last.0 {
            last = (len, Instant::now());
        } else if last.1.elapsed() > Duration::from_secs(60) {
            eprintln!("BENCH grow stalled at {len}");
            break;
        }
        if session
            .pending_prompt()
            .is_some_and(|(who, _)| &who == seat)
        {
            let snap = session.get_snapshot(&ViewerRole::Player(seat.clone()));
            if let Some(pending) = snap.pending_choice {
                let mut rng = random_bot::decision_rng(7, 0, len);
                let index = random_bot::pick_index(&pending.choice, &mut rng);
                let _ = session.submit_choice(
                    seat,
                    &pending.nonce,
                    snap.game_version,
                    &pending.choice.options[index].id,
                );
                continue;
            }
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    session.stop();
}
