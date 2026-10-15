//! Rule bot seats (`SeatController::SMART`): whole games without illegal answers or stalls,
//! determinism, and a strength comparison against the plain random bot. Delay 0 everywhere.

use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

fn config_with(
    id: &str,
    seed: u64,
    card_set: &str,
    controllers: &[SeatController],
) -> SessionConfig {
    let ids: Vec<PlayerId> = (1..=controllers.len())
        .map(|i| PlayerId::new(format!("p{i}")))
        .collect();
    let (state, galaxy) = ti4_host::map::create_game_with_template_seated(
        ContentStore::embedded(),
        &ids,
        seed,
        None,
        &ti4_engine::seating::seat_in_scope(&ids),
        Some(card_set),
    )
    .expect("create game");
    let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(id, state)
        .with_seed(seed)
        .with_player_ids(ids.clone())
        .with_galaxy(galaxy, tiles)
        .with_random_bot_delay_ms(0);
    for (id, controller) in ids.into_iter().zip(controllers) {
        config = config.with_seat(id, controller.clone());
    }
    config
}

struct Outcome {
    decisions: Vec<DecisionRecord>,
    finished: bool,
    error: Option<String>,
    stalled: bool,
    state: GameState,
}

/// Run until game over, an error, `max_round` is over, or no progress for `stall`.
fn drive(session: &GameSession, max_round: u32, stall: Duration) -> Outcome {
    let mut last_len = 0;
    let mut last_progress = Instant::now();
    let mut stalled = false;
    loop {
        let len = session.decision_log().len();
        if len != last_len {
            last_len = len;
            last_progress = Instant::now();
        }
        if session.is_finished()
            || session.error().is_some()
            || session.current_state().round > max_round
        {
            break;
        }
        if last_progress.elapsed() > stall {
            stalled = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    session.stop();
    Outcome {
        decisions: session.decision_log(),
        finished: session.is_finished(),
        error: session.error(),
        stalled,
        state: session.current_state(),
    }
}

fn play(id: &str, seed: u64, card_set: &str, seats: &[SeatController], max_round: u32) -> Outcome {
    let session = GameSession::start(config_with(id, seed, card_set, seats));
    drive(&session, max_round, Duration::from_secs(60))
}

fn assert_clean(out: &Outcome) {
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(
        !out.stalled,
        "stalled after {} decisions",
        out.decisions.len()
    );
    for (index, record) in out.decisions.iter().enumerate() {
        assert!(
            record.offered.contains(&record.chosen),
            "decision {index} chose {:?} outside {:?}",
            record.chosen,
            record.offered
        );
    }
}

#[test]
fn slow_an_all_smart_table_plays_to_game_over_with_legal_answers() {
    let out = play("sb_full", 7, "te", &vec![SeatController::SMART; 4], 30);
    assert_clean(&out);
    eprintln!(
        "smart table: {} decisions, round {}, finished={}",
        out.decisions.len(),
        out.state.round,
        out.finished
    );
    assert!(out.finished, "no game over by round 30");
}

#[test]
fn slow_a_mixed_pok_table_plays_legally_and_the_same_seed_repeats() {
    let seats = [
        SeatController::SMART,
        SeatController::RANDOM,
        SeatController::SMART,
    ];
    let a = play("sb_det_a", 11, "pok", &seats, 4);
    let b = play("sb_det_b", 11, "pok", &seats, 4);
    assert_clean(&a);
    // A stop lands between two polls, so compare the common prefix.
    let n = a.decisions.len().min(b.decisions.len());
    assert!(n > 200, "implausibly short: {n}");
    assert_eq!(
        a.decisions[..n],
        b.decisions[..n],
        "same seed, different game"
    );
}

/// Planets, victory points and non-fighter ships per seat.
fn standing(state: &GameState, player: &PlayerId) -> (usize, i32) {
    let planets = state.controlled_planets(player).len();
    let vp = state.player(player).map_or(0, |seat| seat.victory_points);
    (planets, vp)
}

/// Smart and random seats alternate, and every seed is played twice with the seats swapped so
/// neither kind keeps the better starting positions. Reports the totals after `ROUNDS` rounds.
#[test]
fn slow_smart_bots_clearly_outplay_random_bots() {
    const ROUNDS: u32 = 4;
    let seeds = [3u64, 17, 29];
    let (mut smart_planets, mut random_planets, mut smart_vp, mut random_vp) = (0, 0, 0, 0);
    let (mut smart_seats, mut random_seats) = (0, 0);
    for seed in seeds {
        for swap in [false, true] {
            let seats: Vec<SeatController> = (0..4)
                .map(|i| {
                    if (i % 2 == 0) == swap {
                        SeatController::RANDOM
                    } else {
                        SeatController::SMART
                    }
                })
                .collect();
            let out = play(&format!("sb_vs_{seed}_{swap}"), seed, "te", &seats, ROUNDS);
            assert_clean(&out);
            let mut line = Vec::new();
            for (i, controller) in seats.iter().enumerate() {
                let player = PlayerId::new(format!("p{}", i + 1));
                let (planets, vp) = standing(&out.state, &player);
                let smart = *controller == SeatController::SMART;
                line.push(format!(
                    "{}:{planets}p/{vp}vp",
                    if smart { "S" } else { "R" }
                ));
                if smart {
                    smart_planets += planets;
                    smart_vp += vp;
                    smart_seats += 1;
                } else {
                    random_planets += planets;
                    random_vp += vp;
                    random_seats += 1;
                }
            }
            let mecatol = out
                .state
                .board
                .values()
                .find_map(|system| {
                    system
                        .planet_control
                        .get(&ti4_model::id::PlanetId::new("mr"))
                })
                .map_or_else(|| "nobody".to_owned(), ToString::to_string);
            eprintln!(
                "seed {seed} swap={swap} round {} finished={}: {} (Mecatol Rex: {mecatol})",
                out.state.round,
                out.finished,
                line.join(" ")
            );
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let avg = |total: f64, seats: usize| total / seats as f64;
    let (sp, rp) = (
        avg(
            f64::from(u32::try_from(smart_planets).unwrap()),
            smart_seats,
        ),
        avg(
            f64::from(u32::try_from(random_planets).unwrap()),
            random_seats,
        ),
    );
    let (sv, rv) = (
        avg(f64::from(smart_vp), smart_seats),
        avg(f64::from(random_vp), random_seats),
    );
    eprintln!(
        "STRENGTH after {ROUNDS} rounds: smart {sp:.2} planets {sv:.2} VP per seat; random {rp:.2} planets {rv:.2} VP per seat"
    );
    assert!(sp > rp * 1.3, "smart planets {sp:.2} vs random {rp:.2}");
    assert!(sv >= rv, "smart VP {sv:.2} vs random {rv:.2}");
}

#[test]
fn slow_smart_seats_are_saved_and_recovered_as_smart_seats_and_play_on_the_same() {
    use std::sync::Arc;
    use ti4_server::session::GameRegistry;
    use ti4_server::storage::FileGameStore;

    let seats = [
        SeatController::SMART,
        SeatController::RANDOM,
        SeatController::SMART,
    ];
    let reference = play("sb_rec", 41, "te", &seats, 3);
    assert_clean(&reference);

    let dir = std::env::temp_dir().join(format!("ti4_smart_bots_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let session = registry
        .create_game(config_with("sb_rec", 41, "te", &seats).with_store(store.clone()))
        .expect("create");
    let deadline = Instant::now() + Duration::from_secs(120);
    while session.decision_log().len() < 250 {
        assert!(Instant::now() < deadline, "no progress");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.stop();
    let at_stop = session.decision_log();
    drop(session);
    drop(registry);

    ti4_server::session::random_bot::set_random_bot_delay_override(Some(0));
    let registry = GameRegistry::new().with_store(store);
    registry.recover_all_games().expect("recover");
    let session = registry.get_game("sb_rec").expect("recovered");
    let recovered_seats = session.lobby_details().1;
    assert_eq!(recovered_seats[&PlayerId::new("p1")], SeatController::SMART);
    assert_eq!(
        recovered_seats[&PlayerId::new("p2")],
        SeatController::RANDOM
    );
    let out = drive(&session, 3, Duration::from_secs(60));
    assert!(out.error.is_none(), "{:?}", out.error);
    assert_eq!(
        out.decisions[..at_stop.len()],
        at_stop[..],
        "recorded answers were not kept"
    );
    let n = out.decisions.len().min(reference.decisions.len());
    assert!(n > at_stop.len(), "nothing played after recovery");
    assert_eq!(
        out.decisions[..n],
        reference.decisions[..n],
        "the recovered bots played differently"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
