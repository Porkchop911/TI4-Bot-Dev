//! Bluffing a reaction card (M18): the declared hold stalls the live game, and nothing about it
//! reaches the engine, the decision log, the event log, storage or other seats.

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::{PublicTurnStatus, ViewerRole};
use ti4_model::id::PlayerId;
use ti4_server::dev::launch_scenario;
use ti4_server::session::bluff::BluffPolicy;
use ti4_server::session::registry::HistoryAction;
use ti4_server::session::{GameRegistry, GameSession, MockClient};
use ti4_server::storage::FileGameStore;

const SEED: u64 = 54_322;

struct Seats {
    sol: PlayerId,
    hacan: PlayerId,
    letnev: PlayerId,
}

fn seats(session: &GameSession) -> Seats {
    let state = session.current_state();
    let by = |faction: &str| {
        state
            .players
            .iter()
            .find(|player| player.faction.as_str() == faction)
            .map(|player| player.id.clone())
            .unwrap_or_else(|| panic!("a {faction} seat"))
    };
    Seats {
        sol: by("sol"),
        hacan: by("hacan"),
        letnev: by("letnev"),
    }
}

fn short_policy() -> BluffPolicy {
    BluffPolicy {
        min_hold: Duration::from_millis(150),
        max_hold: Duration::from_millis(250),
        hard_cap: Duration::from_secs(1),
        poll: Duration::from_millis(10),
        ..BluffPolicy::default()
    }
}

/// Answer the pending decision with its first option, watching for a bluff hold while waiting.
/// Returns the seats seen holding.
fn drive(session: &GameSession, steps: usize) -> Vec<PlayerId> {
    let mut held = Vec::new();
    for _ in 0..steps {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if let Some(seat) = session.bluff_holding() {
                if held.last() != Some(&seat) {
                    held.push(seat);
                }
            }
            if let Some((seat, nonce, version)) = session.current_pending_decision() {
                let Some(envelope) = session
                    .get_snapshot(&ViewerRole::Player(seat.clone()))
                    .pending_choice
                else {
                    continue;
                };
                let option = envelope.choice.options[0].id.clone();
                if session
                    .submit_choice(&seat, &nonce, version, &option)
                    .is_ok()
                {
                    break;
                }
            }
            if session.is_finished() {
                return held;
            }
            assert!(
                Instant::now() < deadline,
                "no decision: {:?}",
                session.error()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    held
}

fn wait_idle(session: &GameSession) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.bluff_holding().is_some() || session.current_pending_decision().is_none() {
        assert!(Instant::now() < deadline, "never settled");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Player and game ids differ between launches; name seats by faction and the game "G".
fn canon(session: &GameSession, text: String) -> String {
    let mut text = text.replace(session.id(), "G");
    for player in &session.current_state().players {
        text = text.replace(player.id.as_str(), player.faction.as_str());
    }
    text
}

/// Everything compared between two runs of the same game: decisions, state, events (no clocks).
fn fingerprint(session: &GameSession) -> String {
    let decisions = serde_json::to_string(&session.decision_log()).expect("json");
    // Re-parsed so that maps keyed by (random) player ids compare in a seat-independent order.
    let state: serde_json::Value = serde_json::from_str(&canon(
        session,
        serde_json::to_string(&session.current_state()).expect("json"),
    ))
    .expect("parses");
    canon(
        session,
        format!("{decisions}\n{event}", event = event_shape(session)),
    ) + &state.to_string()
}

/// What the event log shows, minus wall-clock fields.
fn event_shape(session: &GameSession) -> String {
    session
        .event_log()
        .iter()
        .map(|entry| {
            format!(
                "{}|{:?}|{:?}|{:?}|{:?}",
                entry.id, entry.event, entry.decision_count, entry.detail, entry.actor
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn launch(registry: &Arc<GameRegistry>) -> (Arc<GameSession>, Seats, String) {
    let launch =
        launch_scenario(registry, "ongoing_combat_four_views", Some(SEED)).expect("launch");
    let session = registry.get_game(&launch.game_id).expect("game");
    let seats = seats(&session);
    (session, seats, launch.player_session)
}

fn declare(session: &GameSession, seat: &PlayerId, triggers: &[&str]) -> Result<(), String> {
    let triggers: Vec<String> = triggers.iter().map(|t| (*t).to_owned()).collect();
    session.set_reaction_intent(seat, &triggers)
}

#[test]
fn a_declared_bluff_holds_the_live_game_and_leaves_no_trace_in_the_history() {
    let control_registry = Arc::new(GameRegistry::new());
    let (control, _, _) = launch(&control_registry);
    drive(&control, 10);
    wait_idle(&control);

    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    session.set_bluff_policy(short_policy());
    let letnev = MockClient::connect(session.clone(), ViewerRole::Player(seats.letnev.clone()));
    let sol = MockClient::connect(session.clone(), ViewerRole::Player(seats.sol.clone()));
    let spectator = MockClient::connect(session.clone(), ViewerRole::Spectator);
    declare(&session, &seats.letnev, &["space_combat"]).expect("Letnev holds Sabotage");
    let held = drive(&session, 10);
    wait_idle(&session);

    assert!(
        held.contains(&seats.letnev),
        "the declared window stalled the game: {held:?}"
    );
    assert!(held.iter().all(|seat| seat == &seats.letnev));

    // The bluff left no trace in anything that is saved, replayed or compared.
    assert_eq!(fingerprint(&session), fingerprint(&control));
    assert_eq!(session.decision_log().len(), control.decision_log().len());

    // Letnev alone hears about its own settings; the others see only the ordinary status.
    let mine = letnev.drain_messages();
    assert!(mine.iter().any(|m| matches!(m,
        ServerMessage::ReactionIntentState(s) if s.holding && s.triggers == ["space_combat"])));
    assert!(mine.iter().any(|m| matches!(m,
        ServerMessage::ReactionIntentState(s) if !s.holding)));
    for other in [&sol, &spectator] {
        let seen = other.drain_messages();
        assert!(
            seen.iter()
                .all(|m| !matches!(m, ServerMessage::ReactionIntentState(_))),
            "other viewers get no intent message"
        );
        let wire = serde_json::to_string(&seen).expect("encodes");
        assert!(!wire.contains("reaction_intent") && !wire.contains("\"triggers\""));
        assert!(seen.iter().any(|m| matches!(m,
            ServerMessage::TurnStatus(t) if matches!(&t.status,
                PublicTurnStatus::WaitingForReactions { .. }))));
    }
}

#[test]
fn the_banner_is_the_status_a_real_question_sends() {
    use ti4_engine::choice::{Choice, ChoiceOption};
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_host::projection::{
        project_turn_status, project_turn_status_for, waiting_for_player_status,
        waiting_for_reactions_status,
    };
    let state = {
        let registry = Arc::new(GameRegistry::new());
        launch(&registry).0.current_state()
    };
    let seat = state.players[0].id.clone();
    let other = state.players[1].id.clone();
    let window = Choice::new(
        seat.clone(),
        "when ACTION_CARD_PLAYED",
        vec![ChoiceOption::decline()],
    )
    .contextualized(DecisionContext::new(
        seat.clone(),
        DecisionSource::Reaction("ACTION_CARD_PLAYED".to_owned()),
        "reaction_when_ACTION_CARD_PLAYED",
        state.phase,
        state.round,
    ));
    // What every other seat and a spectator gets from a real reaction wait is, byte for byte,
    // what a bluff hold sends.
    let hold = serde_json::to_string(&waiting_for_reactions_status(&state)).unwrap();
    for viewer in [ViewerRole::Player(other), ViewerRole::Spectator] {
        let real = project_turn_status_for(&state, Some(&window), &viewer);
        assert_eq!(serde_json::to_string(&real).unwrap(), hold);
        assert!(!hold.contains(seat.as_str()));
    }
    assert_eq!(
        project_turn_status(&state, Some(&window)),
        waiting_for_reactions_status(&state)
    );
    // The asked seat keeps its own prompt status.
    assert_eq!(
        project_turn_status_for(&state, Some(&window), &ViewerRole::Player(seat.clone())),
        waiting_for_player_status(&state, &seat)
    );
    // An ordinary question still names its seat for everybody.
    let ordinary = Choice::new(seat.clone(), "x", vec![]);
    assert_eq!(
        project_turn_status(&state, Some(&ordinary)),
        waiting_for_player_status(&state, &seat)
    );
}

#[test]
fn an_undeclared_trigger_or_an_ineligible_seat_never_stalls() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    session.set_bluff_policy(short_policy());
    // A kind that never opens in this combat.
    declare(&session, &seats.letnev, &["agenda"]).expect("declared");
    let held = drive(&session, 8);
    assert!(held.is_empty(), "{held:?}");

    // A seat with no cards cannot declare, and the refusal is clear.
    let error = declare(&session, &seats.hacan, &["space_combat"]).unwrap_err();
    assert!(error.contains("no action cards"), "{error}");
    assert!(
        session
            .reaction_intent_state(&seats.hacan)
            .triggers
            .is_empty()
    );
    assert!(!session.reaction_intent_state(&seats.hacan).eligible);

    // A seat that set a card to Never offer cannot bluff either.
    session
        .set_reaction_mode(
            &seats.sol,
            "Sabotage",
            ti4_model::state::ReactionMode::Never,
        )
        .expect("mode");
    let error = declare(&session, &seats.sol, &["movement"]).unwrap_err();
    assert!(error.contains("Never"), "{error}");
}

#[test]
fn a_seat_that_turns_to_never_after_declaring_is_not_held() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    session.set_bluff_policy(short_policy());
    declare(&session, &seats.letnev, &["space_combat"]).expect("declared");
    session
        .set_reaction_mode(
            &seats.letnev,
            "Sabotage",
            ti4_model::state::ReactionMode::Never,
        )
        .expect("mode");
    let held = drive(&session, 8);
    assert!(held.is_empty(), "Never must not stall: {held:?}");
}

#[test]
fn validation_cooldown_and_idempotent_resend() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    let letnev = &seats.letnev;
    assert!(
        declare(&session, letnev, &["nonsense"])
            .unwrap_err()
            .contains("unknown")
    );
    assert!(
        declare(
            &session,
            letnev,
            &["agenda", "movement", "production", "turn_end"]
        )
        .unwrap_err()
        .contains("at most 3")
    );
    assert!(session.reaction_intent_state(letnev).triggers.is_empty());

    // The first declaration is free; an identical resend (a reconnect) is a no-op.
    declare(&session, letnev, &["movement", "agenda"]).expect("first");
    declare(&session, letnev, &["agenda", "movement", "agenda"]).expect("idempotent resend");
    let state = session.reaction_intent_state(letnev);
    assert_eq!(state.triggers, ["agenda", "movement"]);
    assert_eq!(state.locked_until_round, Some(2));

    // A change in the same round is refused with a clear message and changes nothing.
    let error = declare(&session, letnev, &["production"]).unwrap_err();
    assert!(error.contains("after round 1"), "{error}");
    assert_eq!(
        session.reaction_intent_state(letnev).triggers,
        ["agenda", "movement"]
    );
    // Clearing is always possible, and does not re-open changes.
    declare(&session, letnev, &[]).expect("clear");
    assert!(session.reaction_intent_state(letnev).triggers.is_empty());
    assert!(declare(&session, letnev, &["production"]).is_err());
    // The resend of the (now empty) declaration stays a no-op.
    declare(&session, letnev, &[]).expect("idempotent clear");
}

#[test]
fn pass_ends_the_hold_early_and_stop_interrupts_it_at_once() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    // A hold that would otherwise last many seconds.
    session.set_bluff_policy(BluffPolicy {
        min_hold: Duration::from_secs(6),
        max_hold: Duration::from_secs(6),
        ..BluffPolicy::default()
    });
    declare(&session, &seats.letnev, &["space_combat"]).expect("declared");
    let started = Instant::now();
    let holder = {
        let session = session.clone();
        std::thread::spawn(move || drive(&session, 10))
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.bluff_holding().is_none() {
        assert!(Instant::now() < deadline, "no hold began");
        std::thread::sleep(Duration::from_millis(5));
    }
    // Someone else's Pass does nothing.
    session.pass_reaction_hold(&seats.sol);
    std::thread::sleep(Duration::from_millis(100));
    assert!(session.bluff_holding().is_some());
    // The bluffer's Pass ends it now.
    let passed_at = Instant::now();
    session.pass_reaction_hold(&seats.letnev);
    while session.bluff_holding().is_some() {
        assert!(
            passed_at.elapsed() < Duration::from_secs(1),
            "Pass was not prompt"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "ended long before the 6 s hold"
    );

    let _ = holder;
}

#[test]
fn a_stop_interrupts_a_running_hold_at_once() {
    // Undo, redo, restart and turn redo all stop the session first.
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    session.set_bluff_policy(BluffPolicy {
        min_hold: Duration::from_secs(6),
        max_hold: Duration::from_secs(6),
        ..BluffPolicy::default()
    });
    declare(&session, &seats.letnev, &["space_combat"]).expect("declared");
    let driver = {
        let session = session.clone();
        std::thread::spawn(move || drive(&session, 10))
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.bluff_holding().is_none() {
        assert!(Instant::now() < deadline, "no hold began");
        std::thread::sleep(Duration::from_millis(5));
    }
    let stopped_at = Instant::now();
    session.stop();
    assert!(
        stopped_at.elapsed() < Duration::from_secs(2),
        "stop waited out the hold"
    );
    assert!(session.bluff_holding().is_none());
    let _ = driver;
}

#[test]
fn the_default_hold_lasts_two_to_four_seconds() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    declare(&session, &seats.letnev, &["space_combat"]).expect("declared");
    let holder = {
        let session = session.clone();
        std::thread::spawn(move || drive(&session, 1))
    };
    let deadline = Instant::now() + Duration::from_secs(10);
    while session.bluff_holding().is_none() {
        assert!(Instant::now() < deadline, "no hold began");
        std::thread::sleep(Duration::from_millis(5));
    }
    let began = Instant::now();
    while session.bluff_holding().is_some() {
        assert!(
            began.elapsed() < Duration::from_secs(8),
            "exceeded the hard cap"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
    let lasted = began.elapsed();
    assert!(
        lasted >= Duration::from_millis(1_900) && lasted <= Duration::from_millis(4_300),
        "{lasted:?}"
    );
    let _ = holder.join();
}

#[test]
fn the_budget_runs_out_and_the_bluffer_is_told_privately() {
    let registry = Arc::new(GameRegistry::new());
    let (session, seats, _) = launch(&registry);
    // Room for exactly one stall in the round.
    session.set_bluff_policy(BluffPolicy {
        seat_round_budget: Duration::from_millis(300),
        ..short_policy()
    });
    let letnev = MockClient::connect(session.clone(), ViewerRole::Player(seats.letnev.clone()));
    declare(&session, &seats.letnev, &["space_combat"]).expect("declared");
    assert!(!session.reaction_intent_state(&seats.letnev).budget_used_up);
    let held = drive(&session, 14);
    assert_eq!(
        held,
        vec![seats.letnev.clone()],
        "one hold, then the budget is gone"
    );
    wait_idle(&session);
    assert!(session.reaction_intent_state(&seats.letnev).budget_used_up);
    assert!(letnev.drain_messages().iter().any(|m| matches!(m,
        ServerMessage::ReactionIntentState(s) if s.budget_used_up)));
    // Nobody else learns of it.
    assert!(!session.reaction_intent_state(&seats.sol).budget_used_up);
}

fn tree(dir: &Path) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(path) = stack.pop() {
        for entry in fs::read_dir(&path).expect("dir") {
            let entry = entry.expect("entry").path();
            if entry.is_dir() {
                stack.push(entry);
            } else {
                out.push((
                    entry
                        .strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    fs::read(&entry).expect("file"),
                ));
            }
        }
    }
    out.sort();
    out
}

#[test]
fn storage_is_the_same_with_and_without_a_bluff_and_a_restart_forgets_it() {
    let run = |bluff: bool| {
        let dir = std::env::temp_dir().join(format!("ti4_bluff_{:032x}", rand::random::<u128>()));
        let store = Arc::new(FileGameStore::new(&dir).expect("store"));
        let registry = Arc::new(GameRegistry::new().with_store(store.clone()));
        let (session, seats, _) = launch(&registry);
        session.set_bluff_policy(short_policy());
        if bluff {
            declare(&session, &seats.letnev, &["space_combat", "movement"]).expect("declared");
        }
        let held = drive(&session, 10);
        wait_idle(&session);
        assert_eq!(!held.is_empty(), bluff);
        let game_id = session.id().to_owned();
        let files = tree(&dir.join(&game_id));
        let decisions = fingerprint(&session);
        drop((session, registry));
        (dir, game_id, files, decisions)
    };
    let (control_dir, _, control_files, control_decisions) = run(false);
    let (bluff_dir, bluff_game, bluff_files, bluff_decisions) = run(true);

    assert_eq!(control_decisions, bluff_decisions);
    let names =
        |files: &[(String, Vec<u8>)]| files.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>();
    assert_eq!(names(&control_files), names(&bluff_files), "no extra file");
    // Ids differ per launch, so compare file sizes: a stored bluff would add bytes.
    for ((name, left), (_, right)) in control_files.iter().zip(&bluff_files) {
        assert_eq!(left.len(), right.len(), "{name} differs in size");
    }
    for (name, bytes) in &bluff_files {
        let text = String::from_utf8_lossy(bytes);
        assert!(
            !text.contains("space_combat\"") && !text.contains("intent") && !text.contains("bluff"),
            "{name} mentions the bluff"
        );
    }

    // After a restart the declaration is gone, and the replayed game is identical.
    let store = Arc::new(FileGameStore::new(&bluff_dir).expect("store"));
    let restarted = Arc::new(GameRegistry::new().with_store(store));
    let report = restarted.recover_all_games_report().expect("recover");
    assert!(report.failed.is_empty(), "{:?}", report.failed);
    let recovered = restarted.get_game(&bluff_game).expect("recovered");
    recovered.wait_replayed().expect("replayed");
    assert_eq!(fingerprint(&recovered), bluff_decisions);
    let letnev = seats_of(&recovered).letnev;
    assert!(recovered.reaction_intent_state(&letnev).triggers.is_empty());
    let _ = fs::remove_dir_all(control_dir);
    let _ = fs::remove_dir_all(bluff_dir);
}

fn seats_of(session: &GameSession) -> Seats {
    seats(session)
}

#[test]
fn undoing_with_a_bluff_active_gives_the_same_history_as_without() {
    let outcome = |bluff: bool| {
        let registry = Arc::new(GameRegistry::new());
        let (session, seats, host) = launch(&registry);
        session.set_bluff_policy(short_policy());
        if bluff {
            declare(&session, &seats.letnev, &["space_combat"]).expect("declared");
        }
        drive(&session, 8);
        wait_idle(&session);
        let game_id = session.id().to_owned();
        let version = session.game_version();
        let cursor = session.decision_log().len() - 3;
        let snapshot = registry
            .change_history(
                &game_id,
                &host,
                version,
                HistoryAction::RestoreCursor { cursor },
            )
            .expect("undo");
        let after = registry.get_game(&game_id).expect("replacement");
        after.wait_replayed().expect("replayed");
        // The replacement forgot the declaration, as documented.
        assert!(
            after
                .reaction_intent_state(&seats.letnev)
                .triggers
                .is_empty()
        );
        let _ = snapshot;
        // And it replays the same game.
        fingerprint(&after)
    };
    let (plain, bluffed) = (outcome(false), outcome(true));
    if plain != bluffed {
        let at = plain
            .bytes()
            .zip(bluffed.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or(0);
        panic!(
            "differ at {at}: {} <> {}",
            &plain[at.saturating_sub(150)..(at + 150).min(plain.len())],
            &bluffed[at.saturating_sub(150)..(at + 150).min(bluffed.len())]
        );
    }
}
