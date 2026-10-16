//! Random bot seats: whole games played by `SeatController::BotRandom`, determinism, recovery,
//! undo/redo and the soak. Delay 0 everywhere (`with_random_bot_delay_ms`).

use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_model::id::PlayerId;
use ti4_server::session::registry::HistoryAction;
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

pub fn players(n: usize) -> Vec<PlayerId> {
    (1..=n).map(|i| PlayerId::new(format!("p{i}"))).collect()
}

/// A config over `n` seats with the given controllers, delay 0.
pub fn config_with(
    id: &str,
    seed: u64,
    card_set: &str,
    controllers: &[SeatController],
) -> SessionConfig {
    let ids = players(controllers.len());
    let (state, galaxy) = ti4_server::map::create_game_with_options(
        ContentStore::embedded(),
        &ids,
        seed,
        None,
        None,
        Some(card_set),
    )
    .expect("create game");
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
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

/// An all-bot config (every seat `BotRandom`), delay 0.
pub fn bot_config(id: &str, n: usize, seed: u64, card_set: &str) -> SessionConfig {
    config_with(id, seed, card_set, &vec![SeatController::BotRandom; n])
}

#[derive(Debug)]
pub struct Outcome {
    pub decisions: Vec<DecisionRecord>,
    pub finished: bool,
    pub error: Option<String>,
    pub stalled: bool,
    pub elapsed: Duration,
    pub round: u32,
}

/// Run a session until it finishes, errors, reaches `cap` decisions, or makes no progress for
/// `stall` (or takes longer than `wall`).
pub fn drive(session: &GameSession, cap: usize, stall: Duration, wall: Duration) -> Outcome {
    let start = Instant::now();
    let mut last_len = 0;
    let mut last_progress = Instant::now();
    let mut stalled = false;
    loop {
        let len = session.decision_log().len();
        if len != last_len {
            last_len = len;
            last_progress = Instant::now();
        }
        if session.is_finished() || session.error().is_some() || len >= cap {
            break;
        }
        if last_progress.elapsed() > stall || start.elapsed() > wall {
            stalled = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let state = session.current_state();
    Outcome {
        decisions: session.decision_log(),
        finished: session.is_finished(),
        error: session.error(),
        stalled,
        elapsed: start.elapsed(),
        round: state.round,
    }
}

fn run_to_end(id: &str, n: usize, seed: u64, card_set: &str, cap: usize) -> Outcome {
    let session = GameSession::start(bot_config(id, n, seed, card_set));
    let out = drive(
        &session,
        cap,
        Duration::from_secs(30),
        Duration::from_secs(600),
    );
    session.stop();
    out
}

#[test]
fn slow_an_all_bot_game_plays_to_game_over_and_reports_speed() {
    let out = run_to_end("rb_one", 4, 7, "te", 6000);
    eprintln!(
        "decisions={} finished={} round={} err={:?} stalled={} elapsed={:?} ({:.0}/s)",
        out.decisions.len(),
        out.finished,
        out.round,
        out.error,
        out.stalled,
        out.elapsed,
        out.decisions.len() as f64 / out.elapsed.as_secs_f64()
    );
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(!out.stalled, "stalled");
    assert!(out.finished, "no game over within the cap");
    assert!(out.decisions.len() > 300, "implausibly short game");
    assert!(out.elapsed < Duration::from_secs(120), "{:?}", out.elapsed);
}

#[test]
fn slow_every_bot_decision_is_one_of_the_offered_options() {
    let out = run_to_end("rb_legal", 3, 11, "pok", 6000);
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(!out.decisions.is_empty());
    for (index, record) in out.decisions.iter().enumerate() {
        assert!(
            record.offered.contains(&record.chosen),
            "decision {index} chose {:?} outside {:?}",
            record.chosen,
            record.offered
        );
    }
    // Reaction windows and secondaries are asked of the bots on the way.
    let subtypes: std::collections::BTreeSet<_> = out
        .decisions
        .iter()
        .filter_map(|r| r.context.as_ref().map(|c| c.subtype.clone()))
        .collect();
    assert!(
        subtypes.iter().any(|s| s.contains("reaction")),
        "no reaction window was asked: {subtypes:?}"
    );
}

#[test]
fn slow_the_same_seed_plays_the_same_game_and_another_seed_a_different_one() {
    let a = run_to_end("rb_det_a", 4, 21, "te", 6000);
    let b = run_to_end("rb_det_b", 4, 21, "te", 6000);
    let c = run_to_end("rb_det_c", 4, 22, "te", 6000);
    assert!(a.finished && b.finished);
    assert_eq!(a.decisions, b.decisions, "same seed, different game");
    assert_ne!(a.decisions, c.decisions);
}

#[test]
fn slow_a_bot_game_recovered_from_disk_mid_way_continues_to_the_same_end() {
    let reference = run_to_end("rb_rec", 4, 31, "te", 6000);
    assert!(reference.finished);
    let dir = std::env::temp_dir().join(format!("ti4_random_bots_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let session = registry
        .create_game(bot_config("rb_rec", 4, 31, "te").with_store(store.clone()))
        .expect("create");
    // Let it play part of the game, then stop it as a restart would.
    let deadline = Instant::now() + Duration::from_secs(60);
    while session.decision_log().len() < 400 {
        assert!(Instant::now() < deadline, "no progress");
        std::thread::sleep(Duration::from_millis(5));
    }
    session.stop();
    let at_stop = session.decision_log();
    drop(session);
    drop(registry);

    // Recovered seats come from the saved record and read the process-wide delay.
    ti4_server::session::random_bot::set_random_bot_delay_override(Some(0));
    let registry = GameRegistry::new().with_store(store.clone());
    registry.recover_all_games().expect("recover");
    let session = registry.get_game("rb_rec").expect("recovered");
    let out = drive(
        &session,
        6000,
        Duration::from_secs(60),
        Duration::from_secs(900),
    );
    session.stop();
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(out.finished, "recovered game did not finish");
    assert!(out.decisions.len() >= at_stop.len());
    assert_eq!(
        out.decisions[..at_stop.len()],
        at_stop[..],
        "recorded answers were not kept"
    );
    // New decisions after the restart are the ones the uninterrupted game made.
    assert_eq!(out.decisions, reference.decisions);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_think_delay_spaces_decisions_and_a_stop_interrupts_it() {
    let config = bot_config("rb_delay", 3, 5, "te").with_random_bot_delay_ms(200);
    let session = GameSession::start(config);
    std::thread::sleep(Duration::from_millis(1000));
    let seen = session.decision_log().len();
    // About 200 ms (jittered 0.6-1.4) per decision: far fewer than delay 0 does in a second.
    assert!(
        (1..=14).contains(&seen),
        "{seen} decisions in 1 s at 200 ms"
    );
    let stopped = Instant::now();
    session.stop();
    assert!(
        stopped.elapsed() < Duration::from_millis(500),
        "stop waited for the delay"
    );
}

/// The pick of a seat is drawn before its wait, so a game is the same with every wait.
#[test]
fn the_same_game_with_a_wait_of_0_and_of_20_ms() {
    let play = |id: &str, delay_ms: u64| {
        let config = bot_config(id, 3, 5, "te").with_random_bot_delay_ms(delay_ms);
        let session = GameSession::start(config);
        let out = drive(
            &session,
            30,
            Duration::from_secs(20),
            Duration::from_secs(60),
        );
        session.stop();
        assert!(out.error.is_none(), "{:?}", out.error);
        out.decisions[..30].to_vec()
    };
    assert_eq!(play("rb_wait_0", 0), play("rb_wait_20", 20));
}

/// While a seat that the server plays takes its time, the other seats are told who the game
/// waits for, as when a person is asked.
#[test]
fn a_seat_that_waits_is_named_to_the_other_seats() {
    use ti4_server::protocol::server::ServerMessage;
    use ti4_server::protocol::status::{PublicTurnStatus, ViewerRole};

    let config = bot_config("rb_wait_named", 3, 5, "te").with_random_bot_delay_ms(150);
    let session = GameSession::start(config);
    let watching = session.subscribe(ViewerRole::Spectator);
    let deadline = Instant::now() + Duration::from_secs(10);
    let named = loop {
        assert!(Instant::now() < deadline, "no seat was named");
        match watching.try_recv() {
            Ok(ServerMessage::TurnStatus(message)) => {
                if let PublicTurnStatus::WaitingForDecision { seat, .. } = message.status {
                    break seat;
                }
            }
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(5)),
        }
    };
    session.stop();
    assert!(players(3).contains(&named), "{named}");
}

/// Host seat human (p1), the other two seats random bots.
struct HostTable {
    registry: Arc<GameRegistry>,
    id: String,
    host_token: String,
}

fn host_table(id: &str, seed: u64) -> HostTable {
    host_table_with_delay(id, seed, 0)
}

fn host_table_with_delay(id: &str, seed: u64, delay_ms: u64) -> HostTable {
    let registry = Arc::new(GameRegistry::new());
    let config = config_with(
        id,
        seed,
        "te",
        &[
            SeatController::Human,
            SeatController::BotRandom,
            SeatController::BotRandom,
        ],
    )
    .with_random_bot_delay_ms(delay_ms);
    let session = registry.create_game(config).unwrap();
    let host_token = session.seat_tokens()[&PlayerId::new("p1")].clone();
    HostTable {
        registry,
        id: id.to_owned(),
        host_token,
    }
}

impl HostTable {
    fn session(&self) -> Arc<GameSession> {
        self.registry.get_game(&self.id).expect("game")
    }

    /// Answer the host's pending decision with its first option.
    fn answer(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let session = self.session();
            if let Some((seat, nonce, version)) = session.current_pending_decision() {
                assert_eq!(seat.as_str(), "p1", "only the host seat is a person");
                let option = session
                    .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                        seat.clone(),
                    ))
                    .pending_choice
                    .expect("pending")
                    .choice
                    .options[0]
                    .id
                    .clone();
                if session
                    .submit_choice(&seat, &nonce, version, &option)
                    .is_ok()
                {
                    return;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out: {:?}",
                session.error()
            );
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.session().history_ready() {
            assert!(Instant::now() < deadline, "history never became ready");
            std::thread::sleep(Duration::from_millis(3));
        }
    }

    fn history(&self, action: HistoryAction) -> Result<(), String> {
        self.settle();
        let version = self.session().game_version();
        self.registry
            .change_history(&self.id, &self.host_token, version, action)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }
}

#[test]
fn undo_steps_past_bot_decisions_to_the_hosts_own_and_redo_returns() {
    let t = host_table("rb_undo", 3);
    for _ in 0..25 {
        t.answer();
    }
    t.settle();
    let before = t.session().decision_log();
    assert!(
        before.iter().any(|r| r.player.as_str() != "p1"),
        "no bot decided"
    );
    t.history(HistoryAction::Undo).expect("undo");
    t.settle();
    let after = t.session().decision_log();
    assert!(after.len() < before.len());
    assert_eq!(after[..], before[..after.len()]);
    // The first removed decision is the host's: the host is asked again, not a bot re-answering.
    assert_eq!(before[after.len()].player.as_str(), "p1");
    assert_eq!(
        t.session()
            .current_pending_decision()
            .expect("host asked")
            .0
            .as_str(),
        "p1"
    );
    assert!(!t.session().redo_decisions().is_empty(), "redo was lost");
    t.history(HistoryAction::Redo).expect("redo");
    t.settle();
    let redone = t.session().decision_log();
    assert!(redone.len() > after.len());
    assert_eq!(redone[..after.len() + 1], before[..after.len() + 1]);
}

/// Wait until the host is asked again (every bot decision between has been made).
fn wait_for_host_ask(t: &HostTable) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if t.session()
            .current_pending_decision()
            .is_some_and(|(seat, _, _)| seat.as_str() == "p1")
            && t.session().history_ready()
        {
            return;
        }
        assert!(Instant::now() < deadline, "the host was never asked");
        std::thread::sleep(Duration::from_millis(3));
    }
}

#[test]
fn slow_redo_survives_the_bots_redeciding_and_replays_the_recorded_timeline() {
    for seed in [3, 11] {
        let t = host_table(&format!("rb_redo_{seed}"), seed);
        for _ in 0..30 {
            t.answer();
        }
        wait_for_host_ask(&t);
        let before = t.session().decision_log();
        // Undo several times so the redo stack spans host and bot decisions.
        for _ in 0..4 {
            t.history(HistoryAction::Undo).expect("undo");
            wait_for_host_ask(&t);
        }
        let undone = t.session().decision_log();
        assert!(undone.len() < before.len());
        let total = undone.len() + t.session().redo_decisions().len();
        assert_eq!(total, before.len(), "undo must keep the whole future");
        let mut redos = 0;
        while !t.session().redo_decisions().is_empty() {
            let at = t.session().decision_log().len();
            t.history(HistoryAction::Redo).expect("redo");
            wait_for_host_ask(&t);
            let log = t.session().decision_log();
            let left = t.session().redo_decisions().len();
            assert_eq!(log[..], before[..log.len().min(before.len())], "diverged");
            // Redo advances at least one decision and never drops the recorded rest.
            assert!(log.len() > at);
            assert_eq!(
                log.len() + left,
                before.len(),
                "redo lost entries: cursor {} redo {left} (seed {seed})",
                log.len()
            );
            redos += 1;
            assert!(redos < 200);
        }
        assert_eq!(t.session().decision_log()[..], before[..]);
        assert!(t.session().error().is_none());
    }
}

#[test]
fn undo_while_a_bot_is_thinking_never_loses_a_bot_decision_or_errors_mid_rewind() {
    let t = host_table_with_delay("rb_undo_race", 5, 25);
    for _ in 0..12 {
        t.answer();
    }
    let mut undone = 0;
    for _ in 0..40 {
        // No settle: the bots are most likely mid-think, or just decided.
        let session = t.session();
        let version = session.game_version();
        let before = session.decision_log();
        match t
            .registry
            .change_history(&t.id, &t.host_token, version, HistoryAction::Undo)
        {
            Ok(_) => {
                undone += 1;
                let after = t.session().decision_log();
                assert!(after.len() < before.len());
                assert_eq!(after[..], before[..after.len()], "undo rewrote the past");
            }
            Err(error) => {
                let text = format!("{error:?}");
                assert!(
                    text.contains("Game advanced or a decision is in flight"),
                    "unexpected failure: {text}"
                );
                assert!(!text.contains("during rewind"), "{text}");
            }
        }
        std::thread::sleep(Duration::from_millis(7));
        if t.session().decision_log().is_empty() {
            break;
        }
        if t.session()
            .current_pending_decision()
            .is_some_and(|(seat, _, _)| seat.as_str() == "p1")
            && undone % 2 == 1
        {
            t.answer();
        }
    }
    assert!(undone > 0, "no undo ever went through");
    assert!(t.session().error().is_none(), "{:?}", t.session().error());
}

#[test]
fn the_host_can_redo_a_bot_seats_turn_and_the_bot_plays_the_new_turn_itself() {
    let t = host_table("rb_turn_redo", 5);
    let bot_turns_done = |log: &[DecisionRecord]| {
        ti4_server::session::turn_redo::find_turns(log)
            .iter()
            .filter(|s| s.seat.as_str() == "p2" && s.complete)
            .count()
    };
    for _ in 0..300 {
        t.settle();
        let log = t.session().decision_log();
        if bot_turns_done(&log) >= 2 && log.len() > 40 {
            break;
        }
        t.answer();
    }
    t.settle();
    let version = t.session().game_version();
    t.registry
        .turn_redo_request(&t.id, &t.host_token, version, Some("p2"), None)
        .expect("host redoes the bot's turn");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let status = t
            .registry
            .turn_redo_status(&t.id, &t.host_token)
            .unwrap()
            .expect("in flight");
        if status.turn_complete {
            break;
        }
        if t.session().current_pending_decision().is_some() {
            t.answer();
        }
        assert!(
            Instant::now() < deadline,
            "the bot's new turn never completed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(t.session().error().is_none(), "{:?}", t.session().error());
}

#[test]
fn a_bot_seat_has_no_usable_credential() {
    let t = host_table("rb_cred", 9);
    let tokens = t.session().seat_tokens();
    let bot_token = tokens[&PlayerId::new("p2")].clone();
    assert!(t.session().viewer_for_seat_token(&bot_token).is_none());
    assert!(t.session().viewer_for_seat_token(&t.host_token).is_some());
}

#[test]
fn a_movement_batch_commits_beside_bot_seats_and_undoes() {
    use ti4_server::session::batch::{BatchKind, BatchRequest, MovementPlan, MovementStep};
    let t = host_table("rb_batch", 17);
    // Play the host's first options, preferring a tactical action, until it must move ships.
    let mut request = None;
    for _ in 0..80 {
        t.settle();
        let session = t.session();
        let (seat, nonce, version) = session.current_pending_decision().expect("host asked");
        let choice = session
            .get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ))
            .pending_choice
            .unwrap()
            .choice;
        if choice
            .context
            .as_ref()
            .is_some_and(|c| c.subtype == "movement_step")
        {
            let destination = session
                .current_state()
                .active_system
                .unwrap()
                .as_str()
                .to_owned();
            request = Some(BatchRequest {
                request_id: "bots_batch".into(),
                expected_version: version,
                nonce,
                plan: MovementPlan {
                    kind: BatchKind::TacticalMovement,
                    destination,
                    steps: vec![MovementStep::DoneMoving],
                },
            });
            break;
        }
        let option = choice
            .options
            .iter()
            .find(|o| o.id == "tactical" || o.id == "22")
            .unwrap_or(&choice.options[0])
            .id
            .clone();
        session
            .submit_choice(&seat, &nonce, version, &option)
            .unwrap();
    }
    let request = request.expect("the host never reached a movement step");
    let before = t.session().decision_log();
    let result = t.registry.submit_batch(&t.id, &t.host_token, request);
    assert!(result.is_ok(), "{result:?}");
    let after = t.registry.get_game(&t.id).unwrap();
    assert!(after.decision_log().len() > before.len());
    assert_eq!(after.decision_log()[..before.len()], before[..]);
    t.settle();
    t.history(HistoryAction::UndoBatch).expect("undo the batch");
    t.settle();
    assert_eq!(t.session().decision_log(), before);
    assert!(t.session().error().is_none(), "{:?}", t.session().error());
}

/// 20 all-bot games, 3 and 4 players, both card sets. Run with `-- --ignored --nocapture`.
#[test]
#[ignore = "soak: about a minute in debug builds"]
fn soak_twenty_games() {
    let mut finished = 0;
    let mut errors = Vec::new();
    let mut stalls = Vec::new();
    let mut total_decisions = 0usize;
    let started = Instant::now();
    for game in 0..20u64 {
        let n = 3 + (game as usize % 2);
        let card_set = if game % 4 < 2 { "te" } else { "pok" };
        let seed = 9000 + game * 7;
        let session = GameSession::start(bot_config(&format!("soak_{game}"), n, seed, card_set));
        let out = drive(
            &session,
            4000,
            Duration::from_secs(20),
            Duration::from_secs(300),
        );
        session.stop();
        total_decisions += out.decisions.len();
        eprintln!(
            "soak game {game:2} seed {seed} {n}p {card_set}: {} decisions, round {}, finished={} err={:?} stalled={} {:.1}s",
            out.decisions.len(),
            out.round,
            out.finished,
            out.error,
            out.stalled,
            out.elapsed.as_secs_f64()
        );
        if out.finished {
            finished += 1;
        }
        if let Some(error) = out.error {
            errors.push((game, error));
        }
        if out.stalled {
            stalls.push(game);
        }
    }
    eprintln!(
        "SOAK finished={finished}/20 errors={errors:?} stalls={stalls:?} decisions={total_decisions} wall={:.1}s",
        started.elapsed().as_secs_f64()
    );
    assert!(errors.is_empty() && stalls.is_empty());
}
