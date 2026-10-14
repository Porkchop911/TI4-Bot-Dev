//! Turn redo through the registry and HTTP: permissions, the saved original timeline, restore,
//! chains, lifecycle, recovery and bot seats.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_server::create_app;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::protocol::turn_redo::{TurnRedoStage, TurnRedoStop};
use ti4_server::session::registry::HistoryError;
use ti4_server::session::turn_redo::find_turns;
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

struct Table3 {
    registry: Arc<GameRegistry>,
    id: String,
    seed: u64,
    guest: PlayerId,
    third: PlayerId,
    host_token: String,
    guest_token: String,
}

fn table(id: &str, third: SeatController, store: Option<Arc<FileGameStore>>) -> Table3 {
    let mut registry = GameRegistry::new();
    if let Some(store) = &store {
        registry = registry.with_store(store.clone());
    }
    let registry = Arc::new(registry);
    let (host, guest, third_seat) = (
        PlayerId::new("p1"),
        PlayerId::new("p2"),
        PlayerId::new("p3"),
    );
    let players = vec![host.clone(), guest.clone(), third_seat.clone()];
    let seed = 42;
    let (state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, seed).unwrap();
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let mut config = SessionConfig::new(id, state)
        .with_seed(seed)
        .with_player_ids(players)
        .with_galaxy(galaxy, tiles)
        .with_seat(host.clone(), SeatController::Human)
        .with_seat(guest.clone(), SeatController::Human)
        .with_seat(third_seat.clone(), third);
    if let Some(store) = store {
        config = config.with_store(store);
    }
    let session = registry.create_game(config).unwrap();
    let tokens = session.seat_tokens();
    Table3 {
        registry,
        id: id.to_owned(),
        seed,
        host_token: tokens[&host].clone(),
        guest_token: tokens[&guest].clone(),
        guest,
        third: third_seat,
    }
}

impl Table3 {
    fn session(&self) -> Arc<GameSession> {
        self.registry.get_game(&self.id).expect("game")
    }

    /// Answer the pending decision with option `pick(options)`; waits for one to be pending.
    fn answer(&self, pick: impl Fn(usize) -> usize) -> bool {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let session = self.session();
            if session.is_finished() {
                return false;
            }
            if let Some((seat, nonce, version)) = session.current_pending_decision() {
                let options = session
                    .get_snapshot(&ViewerRole::Player(seat.clone()))
                    .pending_choice
                    .expect("pending choice")
                    .choice
                    .options;
                let option = options[pick(options.len())].id.clone();
                if session
                    .submit_choice(&seat, &nonce, version, &option)
                    .is_ok()
                {
                    return true;
                }
            }
            assert!(
                Instant::now() < deadline,
                "timed out: {:?}",
                session.error()
            );
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    /// Play first options until `until` holds for the decision log.
    fn play_until(&self, until: impl Fn(&[ti4_engine::choice::DecisionRecord]) -> bool) {
        for _ in 0..600 {
            if until(&self.session().decision_log()) {
                return;
            }
            assert!(self.answer(|_| 0), "the game ended first");
        }
        panic!("the condition was not reached");
    }

    fn settle(&self) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !self.session().history_ready() {
            assert!(Instant::now() < deadline, "history never became ready");
            std::thread::sleep(Duration::from_millis(4));
        }
    }

    fn version(&self) -> u64 {
        self.settle();
        self.session().game_version()
    }

    fn request(
        &self,
        token: &str,
        seat: Option<&str>,
        turns: Option<u8>,
    ) -> Result<(), HistoryError> {
        self.registry
            .turn_redo_request(&self.id, token, self.version(), seat, turns)
            .map(|_| ())
    }

    fn status(&self, token: &str) -> Option<ti4_server::protocol::turn_redo::TurnRedoStatus> {
        self.registry.turn_redo_status(&self.id, token).unwrap()
    }
}

/// Both humans have finished a turn, and the game has moved on a few decisions since.
fn two_turns_done(log: &[ti4_engine::choice::DecisionRecord]) -> bool {
    let spans = find_turns(log);
    let done = |seat: &str| {
        spans
            .iter()
            .filter(|t| t.seat.as_str() == seat && t.complete)
            .count()
    };
    done("p1") >= 2 && done("p2") >= 2 && spans.last().is_some_and(|t| t.end + 2 <= log.len())
}

fn json_of<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap()
}

#[test]
fn a_redo_rewinds_the_seat_and_restore_brings_the_original_back_byte_for_byte() {
    let t = table("redo_restore", SeatController::BotFirstOption, None);
    t.play_until(two_turns_done);
    t.settle();
    let session = t.session();
    let log0 = session.decision_log();
    let events0 = session.event_log();
    let (version0, generation0) = (session.game_version(), session.history_status().generation);
    let redo0 = session.redo_decisions();
    let rewound_to = {
        let spans = find_turns(&log0);
        spans
            .iter()
            .rev()
            .find(|s| s.seat == t.guest)
            .unwrap()
            .start
    };

    // The guest redoes their own last turn: allowed, rewinds to its start, seed unchanged.
    t.request(&t.guest_token, None, Some(1))
        .expect("redo own turn");
    let session = t.session();
    assert_eq!(session.decision_log(), log0[..rewound_to].to_vec());
    // Same version semantics as the existing history replacement: an identical game rewound to
    // the same cursor by the host lands on the same version and generation.
    {
        let twin = table("redo_twin", SeatController::BotFirstOption, None);
        twin.play_until(two_turns_done);
        twin.settle();
        assert_eq!(
            twin.session().decision_log(),
            log0,
            "twin games are deterministic"
        );
        twin.registry
            .change_history(
                &twin.id,
                &twin.host_token,
                twin.version(),
                ti4_server::session::registry::HistoryAction::RestoreCursor { cursor: rewound_to },
            )
            .expect("plain rewind");
        assert_eq!(twin.session().game_version(), session.game_version());
        assert_eq!(
            twin.session().history_status().generation,
            session.history_status().generation
        );
    }
    assert!(session.game_version() > version0);
    assert_eq!(
        session.history_status().generation,
        generation0 + 1,
        "clients resync"
    );
    assert_eq!(
        session.restart_config().state.rng_seed,
        session.restart_config().seed.unwrap_or(t.seed)
    );
    assert_eq!(session.restart_config().seed, Some(t.seed));
    let status = t.status(&t.guest_token).expect("a redo is in flight");
    assert_eq!(status.stage, TurnRedoStage::NewTurn);
    assert_eq!(status.seat, "p2");
    assert!(status.can_control && !status.turn_complete);
    assert_eq!(status.original_decisions, log0.len());
    // Everyone authenticated sees the status; only the host and the seat control it.
    assert!(
        !t.status(&t.host_token).map_or(true, |s| !s.can_control),
        "the host controls"
    );
    // The third seat is a bot here; its status read needs a credential too.
    assert!(t.registry.turn_redo_status(&t.id, "nobody").is_err());

    // Restore: byte-identical decision log, events and redo list, one version later.
    let (version1, generation1) = (session.game_version(), session.history_status().generation);
    t.registry
        .turn_redo_restore(&t.id, &t.guest_token, version1)
        .expect("restore");
    let session = t.session();
    assert_eq!(json_of(&session.decision_log()), json_of(&log0));
    assert_eq!(json_of(&session.event_log()), json_of(&events0));
    assert_eq!(json_of(&session.redo_decisions()), json_of(&redo0));
    assert!(session.game_version() > version1);
    assert_eq!(session.history_status().generation, generation1 + 1);
    assert!(t.status(&t.host_token).is_none(), "the original is used up");
    assert!(matches!(
        t.registry
            .turn_redo_restore(&t.id, &t.host_token, t.version()),
        Err(HistoryError::InvalidTarget(_))
    ));
}

#[test]
fn only_the_host_may_redo_another_seats_turn_and_a_stranger_may_do_nothing() {
    let t = table("redo_permissions", SeatController::BotFirstOption, None);
    t.play_until(two_turns_done);
    t.settle();
    let log0 = t.session().decision_log();
    // A guest naming another seat: refused, nothing changes.
    let err = t.request(&t.guest_token, Some("p1"), None).unwrap_err();
    assert!(matches!(err, HistoryError::Forbidden(_)), "{err:?}");
    assert_eq!(t.session().decision_log(), log0);
    assert!(t.status(&t.guest_token).is_none());
    // A stranger is refused everywhere.
    let version = t.version();
    for result in [
        t.registry
            .turn_redo_request(&t.id, "stranger", version, None, None)
            .map(|_| ()),
        t.registry
            .turn_redo_autoplay(&t.id, "stranger", version)
            .map(|_| ()),
        t.registry
            .turn_redo_restore(&t.id, "stranger", version)
            .map(|_| ()),
        t.registry.turn_redo_keep(&t.id, "stranger"),
    ] {
        assert!(
            matches!(result, Err(HistoryError::Forbidden(_))),
            "{result:?}"
        );
    }
    // The host may redo the guest's turn.
    t.request(&t.host_token, Some("p2"), None)
        .expect("host redoes another seat");
    let status = t.status(&t.host_token).unwrap();
    assert_eq!(
        (status.seat.as_str(), status.requested_by.as_str()),
        ("p2", "p1")
    );
    // The other human may read the status but not steer it.
    let other = t.status(&t.guest_token).unwrap();
    assert!(other.can_control, "the redoing seat steers its own redo");
    // The host's own seat is not the redoing seat, but the host controls anyway; a third human
    // would not. A seat that is neither: the bot has no credential, so nothing to test there.
    assert!(t.registry.turn_redo_keep(&t.id, &t.guest_token).is_ok());
    // A bad request: an unknown seat, and too many turns.
    let err = t.request(&t.host_token, Some("nobody"), None).unwrap_err();
    assert!(matches!(err, HistoryError::InvalidTarget(_)), "{err:?}");
    let err = t.request(&t.host_token, None, Some(3)).unwrap_err();
    assert!(matches!(err, HistoryError::InvalidTarget(_)), "{err:?}");
}

#[test]
fn a_new_turn_then_autoplay_keeps_the_round_and_a_second_redo_keeps_the_first_original() {
    let t = table("redo_autoplay", SeatController::BotFirstOption, None);
    t.play_until(two_turns_done);
    t.settle();
    let log0 = t.session().decision_log();
    let events0 = t.session().event_log();

    t.request(&t.host_token, Some("p1"), Some(1))
        .expect("first redo");
    // Autoplay before the new turn is complete is refused.
    let err = t
        .registry
        .turn_redo_autoplay(&t.id, &t.host_token, t.version())
        .unwrap_err();
    assert!(matches!(err, HistoryError::InvalidTarget(_)), "{err:?}");
    // A second request on top of the first: chains onto the same saved original.
    t.request(&t.host_token, Some("p1"), Some(1))
        .expect("second redo");
    let status = t.status(&t.host_token).unwrap();
    assert_eq!(status.redo_count, 2);
    assert_eq!(
        status.original_decisions,
        log0.len(),
        "the original is the one before the first redo"
    );

    // The host plays a different new turn (last option instead of first) until it is complete.
    for _ in 0..200 {
        if t.status(&t.host_token).unwrap().turn_complete {
            break;
        }
        assert!(t.answer(|n| n - 1));
    }
    let status = t.status(&t.host_token).unwrap();
    assert!(status.turn_complete, "the new turn finished");
    t.settle();
    let version = t.session().game_version();
    t.registry
        .turn_redo_autoplay(&t.id, &t.host_token, version)
        .expect("autoplay");
    let session = t.session();
    assert!(session.game_version() > version);
    // A duplicate auto-play (another tab asking for the same round) is not an error, with the
    // stale version or the current one, and applies nothing a second time.
    let after = session.game_version();
    let log_after = json_of(&session.decision_log());
    for expected in [version, after] {
        let again = t
            .registry
            .turn_redo_autoplay(&t.id, &t.host_token, expected)
            .expect("a duplicate auto-play is idempotent");
        assert_eq!(again.game_version, after);
    }
    assert_eq!(t.session().game_version(), after);
    assert_eq!(json_of(&t.session().decision_log()), log_after);
    let status = t
        .status(&t.host_token)
        .expect("still restorable at the hand-off");
    assert_eq!(status.stage, TurnRedoStage::AutoPlayed);
    let outcome = status.outcome.expect("an outcome");
    assert!(outcome.tail_total > 0);
    assert_eq!(status.handoff_len, Some(session.decision_log().len()));
    match &outcome.stop {
        TurnRedoStop::Handoff { seat } => assert_eq!(seat, "p1"),
        TurnRedoStop::Conflict { conflict } => assert!(!conflict.seat.is_empty()),
        TurnRedoStop::TailExhausted => {}
    }
    // The kept part of the round is the original's, decision for decision.
    let new_log = session.decision_log();
    let kept_from = new_log.len() - outcome.kept;
    assert_eq!(
        new_log[kept_from..],
        log0[log0.len() - outcome.tail_total..][..outcome.kept]
    );
    // The log is persisted-consistent: events line up with decisions.
    let resolved = session
        .event_log()
        .iter()
        .filter(|e| {
            matches!(
                e.event,
                ti4_server::protocol::server::GameEventKind::DecisionResolved
            )
        })
        .count();
    assert_eq!(resolved, new_log.len(), "one decision event per decision");

    // Restore goes back to the timeline before BOTH redos.
    let version = t.version();
    t.registry
        .turn_redo_restore(&t.id, &t.host_token, version)
        .expect("restore");
    assert_eq!(json_of(&t.session().decision_log()), json_of(&log0));
    assert_eq!(json_of(&t.session().event_log()), json_of(&events0));
}

/// p1 is in the middle of a turn (its action is under way, the turn not ended yet).
fn p1_mid_turn(log: &[ti4_engine::choice::DecisionRecord]) -> bool {
    let spans = find_turns(log);
    spans
        .iter()
        .filter(|t| t.seat.as_str() == "p1" && t.complete)
        .count()
        >= 1
        && spans
            .last()
            .is_some_and(|t| t.seat.as_str() == "p1" && !t.complete && log.len() > t.start + 1)
}

#[test]
fn redoing_the_turn_in_progress_finishes_without_replacing_the_live_game() {
    let t = table("redo_mid_turn", SeatController::BotFirstOption, None);
    t.play_until(p1_mid_turn);
    t.settle();
    let log0 = t.session().decision_log();
    t.request(&t.host_token, None, None)
        .expect("redo of the turn still being played");
    let status = t.status(&t.host_token).unwrap();
    assert_eq!(status.rewound_to, find_turns(&log0).last().unwrap().start);
    // Nothing was recorded after it, but auto-play still waits for the new turn to end.
    assert!(matches!(
        t.registry
            .turn_redo_autoplay(&t.id, &t.host_token, t.version()),
        Err(HistoryError::InvalidTarget(_))
    ));
    for _ in 0..200 {
        if t.status(&t.host_token).unwrap().turn_complete {
            break;
        }
        assert!(t.answer(|_| 0));
    }
    assert!(t.status(&t.host_token).unwrap().turn_complete);
    // The game goes on live before the client gets round to asking for the auto-play.
    assert!(t.answer(|_| 0));
    t.settle();
    let session = t.session();
    let live = json_of(&session.decision_log());
    let live_len = session.decision_log().len();
    let generation = session.history_status().generation;
    let version = session.game_version();

    let reply = t
        .registry
        .turn_redo_autoplay(&t.id, &t.host_token, version)
        .expect("auto-play with nothing to replay");
    assert_eq!(reply.game_version, version);
    // The same session goes on: no replacement (sockets stay open, nothing in flight is lost)
    // and nothing decided since the new turn ended is cut off.
    assert!(
        Arc::ptr_eq(&session, &t.session()),
        "the live session was replaced"
    );
    assert_eq!(t.session().history_status().generation, generation);
    assert_eq!(t.session().game_version(), version);
    assert_eq!(json_of(&t.session().decision_log()), live);

    let status = t
        .status(&t.host_token)
        .expect("restorable until the next decision");
    assert_eq!(status.stage, TurnRedoStage::AutoPlayed);
    assert_eq!(status.handoff_len, Some(live_len));
    let outcome = status.outcome.expect("an outcome");
    assert_eq!((outcome.kept, outcome.tail_total), (0, 0));
    assert_eq!(outcome.stop, TurnRedoStop::TailExhausted);
    // A second tab asking again gets the same live game back.
    t.registry
        .turn_redo_autoplay(&t.id, &t.host_token, version)
        .expect("a repeated auto-play is a no-op");
    assert!(Arc::ptr_eq(&session, &t.session()));

    // The original turn can still be brought back exactly.
    t.registry
        .turn_redo_restore(&t.id, &t.host_token, t.version())
        .expect("restore");
    assert_eq!(json_of(&t.session().decision_log()), json_of(&log0));
    assert!(t.status(&t.host_token).is_none());
}

#[test]
fn the_original_is_dropped_on_keep_and_once_a_live_decision_passes_the_handoff() {
    // Keep: explicit.
    let t = table("redo_keep", SeatController::BotFirstOption, None);
    t.play_until(two_turns_done);
    t.request(&t.guest_token, None, None).expect("redo");
    t.registry
        .turn_redo_keep(&t.id, &t.guest_token)
        .expect("keep");
    assert!(t.status(&t.guest_token).is_none());
    assert!(matches!(
        t.registry
            .turn_redo_restore(&t.id, &t.guest_token, t.version()),
        Err(HistoryError::InvalidTarget(_))
    ));

    // Past the hand-off: after auto-play, one more live decision ends the restore window.
    let t = table("redo_window", SeatController::BotFirstOption, None);
    t.play_until(two_turns_done);
    t.request(&t.host_token, Some("p1"), None).expect("redo");
    for _ in 0..200 {
        if t.status(&t.host_token).unwrap().turn_complete {
            break;
        }
        assert!(t.answer(|_| 0));
    }
    t.registry
        .turn_redo_autoplay(&t.id, &t.host_token, t.version())
        .expect("autoplay");
    let status = t.status(&t.host_token).expect("restorable at the hand-off");
    let handoff_len = status.handoff_len.unwrap();
    assert!(t.answer(|_| 0), "someone answers live");
    t.settle();
    assert!(t.session().decision_log().len() > handoff_len);
    assert!(
        t.status(&t.host_token).is_none(),
        "the redone timeline moved past the hand-off"
    );
    assert!(matches!(
        t.registry
            .turn_redo_restore(&t.id, &t.host_token, t.version()),
        Err(HistoryError::InvalidTarget(_))
    ));
}

#[test]
fn the_redo_and_its_saved_original_survive_a_restart_with_their_forced_dice() {
    let dir = std::env::temp_dir().join(format!("ti4_turn_redo_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let t = table(
        "redo_recover",
        SeatController::BotFirstOption,
        Some(store.clone()),
    );
    t.play_until(two_turns_done);
    t.settle();
    let log0 = t.session().decision_log();
    t.request(&t.host_token, Some("p1"), None).expect("redo");
    for _ in 0..200 {
        if t.status(&t.host_token).unwrap().turn_complete {
            break;
        }
        assert!(t.answer(|n| n - 1));
    }
    t.registry
        .turn_redo_autoplay(&t.id, &t.host_token, t.version())
        .expect("autoplay");
    t.settle();
    let live = t.session().decision_log();
    let saved = store.load_history(&t.id).unwrap().expect("history saved");
    assert_eq!(saved.decisions, live);
    assert!(
        saved
            .rng_marks
            .deck_plan
            .iter()
            .all(|segment| segment.until != ti4_server::session::rng_force::OPEN),
        "the finished redo's card reservations are closed"
    );
    assert_eq!(saved.rng_marks.deck_plan.len(), 1);
    let plan = saved.rng_marks.deck_plan.clone();
    t.session().stop();

    // A new registry on the same store recovers the redone timeline and the saved original.
    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    let recovered = registry.recover_all_games().expect("recover");
    assert_eq!(recovered, vec![t.id.clone()]);
    let session = registry.get_game(&t.id).unwrap();
    session.wait_replayed().expect("replays with its marks");
    assert_eq!(session.decision_log(), live);
    assert_eq!(
        session.rng_marks().deck_plan,
        plan,
        "the card plan came back from disk"
    );
    let status = registry
        .turn_redo_status(&t.id, &t.host_token)
        .expect("status after restart")
        .expect("the redo is still in flight");
    assert_eq!(status.stage, TurnRedoStage::AutoPlayed);
    registry
        .turn_redo_restore(&t.id, &t.host_token, session.game_version())
        .expect("restore after restart");
    assert_eq!(
        json_of(&registry.get_game(&t.id).unwrap().decision_log()),
        json_of(&log0)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

async fn spawn(registry: Arc<GameRegistry>) -> String {
    let app = create_app(registry);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{address}")
}

async fn post(base: &str, game: &str, token: Option<&str>, body: Value) -> (StatusCode, Value) {
    let mut request = reqwest::Client::new()
        .post(format!("{base}/api/games/{game}/turn-redo"))
        .json(&body);
    if let Some(token) = token {
        request = request.header("x-ti4-player-session", token);
    }
    let response = request.send().await.expect("request");
    let status = response.status();
    let text = response.text().await.unwrap_or_default();
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn http_permissions_and_shapes() {
    let t = table("redo_http", SeatController::BotFirstOption, None);
    let registry = t.registry.clone();
    let (id, host_token, guest_token) = (t.id.clone(), t.host_token.clone(), t.guest_token.clone());
    let t = tokio::task::spawn_blocking(move || {
        t.play_until(two_turns_done);
        t.settle();
        t
    })
    .await
    .unwrap();
    let base = spawn(registry).await;
    let version = t.session().game_version();
    let request = json!({"expected_version": version, "action": "request", "seat": "p1"});

    let (status, _) = post(&base, &id, Some(&guest_token), request.clone()).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "a guest may not redo the host's turn"
    );
    let (status, _) = post(&base, &id, None, request.clone()).await;
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN,
        "{status}"
    );
    let (status, _) = post(&base, "nope", Some(&host_token), request.clone()).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _) = post(
        &base,
        &id,
        Some(&host_token),
        json!({"expected_version": version, "action": "dance"}),
    )
    .await;
    assert!(status.is_client_error(), "{status}");
    let (status, _) = post(
        &base,
        &id,
        Some(&host_token),
        json!({"expected_version": version + 5, "action": "request"}),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "a stale version conflicts");

    let (status, body) = post(&base, &id, Some(&host_token), request).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["game_version"].as_u64().unwrap() > version);

    let client = reqwest::Client::new();
    let get = |token: &str| {
        client
            .get(format!("{base}/api/games/{id}/turn-redo"))
            .header("x-ti4-player-session", token.to_owned())
            .send()
    };
    let response = get(&guest_token).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["status"]["stage"], "new_turn");
    assert_eq!(body["status"]["seat"], "p1");
    assert_eq!(
        body["status"]["can_control"], false,
        "the guest reads but may not steer the host's redo"
    );
    assert_eq!(get("nope").await.unwrap().status(), StatusCode::FORBIDDEN);

    let version = t.version();
    let (status, body) = post(
        &base,
        &id,
        Some(&guest_token),
        json!({"expected_version": version, "action": "restore"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    let (status, body) = post(
        &base,
        &id,
        Some(&host_token),
        json!({"expected_version": version, "action": "restore"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let (status, body) = post(
        &base,
        &id,
        Some(&host_token),
        json!({"expected_version": t.version(), "action": "keep"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["kept"], true);
    let _ = (&t.guest, &t.third);
}

#[test]
fn the_host_can_redo_a_bot_seats_turn_and_the_bot_replays_it_live() {
    let t = table("redo_bot", SeatController::BotFirstOption, None);
    t.play_until(|log| {
        find_turns(log)
            .iter()
            .filter(|s| s.seat.as_str() == "p3" && s.complete)
            .count()
            >= 2
            && log.len() > 40
    });
    t.settle();
    let log0 = t.session().decision_log();
    // The bot has no credential; the guest may not redo its turn, the host may.
    assert!(matches!(
        t.request(&t.guest_token, Some("p3"), None),
        Err(HistoryError::Forbidden(_))
    ));
    t.request(&t.host_token, Some("p3"), None)
        .expect("host redoes the bot's turn");
    // The bot answers its new turn by itself (first options), so the turn completes on its own;
    // a human asked in between (a reaction) answers with the first option.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let status = t.status(&t.host_token).expect("in flight");
        if status.turn_complete {
            break;
        }
        if t.session().current_pending_decision().is_some() {
            let _ = t.answer(|_| 0);
        }
        assert!(
            Instant::now() < deadline,
            "the bot's new turn never completed"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    t.registry
        .turn_redo_autoplay(&t.id, &t.host_token, t.version())
        .expect("autoplay after a bot's turn");
    let status = t.status(&t.host_token).expect("in flight");
    assert_eq!(status.seat, "p3");
    assert_eq!(status.stage, TurnRedoStage::AutoPlayed);
    t.registry
        .turn_redo_restore(&t.id, &t.host_token, t.version())
        .expect("restore");
    assert_eq!(json_of(&t.session().decision_log()), json_of(&log0));
}

#[test]
fn a_redo_in_its_new_turn_recovers_from_disk_with_its_open_card_reservations() {
    let dir = std::env::temp_dir().join(format!("ti4_turn_redo_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&dir).unwrap());
    let t = table(
        "redo_open_recover",
        SeatController::BotFirstOption,
        Some(store.clone()),
    );
    t.play_until(two_turns_done);
    t.settle();
    t.request(&t.host_token, Some("p1"), None).expect("redo");
    t.settle();
    // Half a new turn is played live; the redo is still in its new-turn stage.
    assert!(t.answer(|_| 0));
    t.settle();
    let live = t.session().decision_log();
    let saved = store.load_history(&t.id).unwrap().expect("history saved");
    let open: Vec<_> = saved
        .rng_marks
        .deck_plan
        .iter()
        .filter(|s| s.until == ti4_server::session::rng_force::OPEN)
        .collect();
    assert_eq!(
        open.len(),
        1,
        "the new turn is governed by one open segment"
    );
    t.session().stop();

    let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
    registry.recover_all_games().expect("recover");
    let session = registry.get_game(&t.id).unwrap();
    session
        .wait_replayed()
        .expect("replays with the open segment");
    assert_eq!(session.decision_log(), live);
    let status = registry
        .turn_redo_status(&t.id, &t.host_token)
        .unwrap()
        .expect("still in flight");
    assert_eq!(status.stage, TurnRedoStage::NewTurn);
    let _ = std::fs::remove_dir_all(&dir);
}
