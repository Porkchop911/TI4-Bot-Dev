//! Diplomacy's "ready which planet" questions answered as one `ready_planets` batch.

use std::sync::Arc;
use std::time::{Duration, Instant};

use ti4_content::ContentStore;
use ti4_engine::choice::Choice;
use ti4_model::id::PlayerId;
use ti4_server::session::batch::{BatchKind, BatchRequest, MovementPlan, MovementStep};
use ti4_server::session::{GameRegistry, GameSession, SeatController, SessionConfig};
use ti4_server::storage::FileGameStore;

type Pending = (PlayerId, String, u64, Choice);

fn current(session: &GameSession) -> Pending {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some((seat, nonce, version)) = session.current_pending_decision() {
            let snapshot = session.get_snapshot(&ti4_server::protocol::status::ViewerRole::Player(
                seat.clone(),
            ));
            return (
                seat,
                nonce,
                version,
                snapshot.pending_choice.unwrap().choice,
            );
        }
        assert!(
            Instant::now() < deadline,
            "timed out: {:?}",
            session.error()
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct Table {
    registry: GameRegistry,
    session: Arc<GameSession>,
    p1_token: String,
    path: std::path::PathBuf,
    exhausted: Vec<String>,
}

/// Two seats; p1 holds Diplomacy with every planet it controls exhausted. Returns with p1's
/// first "ready which planet" question pending (the primary's, after the system pick).
fn at_ready(game_id: &str) -> Table {
    let path = std::env::temp_dir().join(format!("ti4_dipl_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store);
    let players = vec![PlayerId::new("p1"), PlayerId::new("p2")];
    let (mut state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    // Jord is p1's only home planet; two more make a real choice of which two to ready.
    for (system, planet) in
        ti4_content::galaxy::all_planets(ContentStore::embedded(), ti4_model::content_types::POK)
            .iter()
            .filter(|(_, planet)| planet.system_id().is_some() && !planet.is_placed_during_play())
            .filter(|(id, _)| ["lodor", "bereg"].contains(*id))
            .map(|(id, planet)| {
                (
                    ti4_model::id::SystemId::new(planet.system_id().unwrap_or("18")),
                    *id,
                )
            })
    {
        state
            .system_mut(&system)
            .set_control(ti4_model::id::PlanetId::new(planet), players[0].clone());
    }
    let owned: Vec<String> = state
        .controlled_planets(&players[0])
        .into_iter()
        .map(|(_, planet)| planet.to_string())
        .collect();
    for planet in &owned {
        state.exhaust_planet(ti4_model::id::PlanetId::new(planet.clone()));
    }
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new(game_id, state)
        .with_seed(42)
        .with_player_ids(players.clone())
        .with_galaxy(galaxy, tiles)
        .with_seat(players[0].clone(), SeatController::Human)
        .with_seat(players[1].clone(), SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let p1_token = session.seat_tokens()[&players[0]].clone();
    for _ in 0..200 {
        let (seat, nonce, version, choice) = current(&session);
        if seat == players[0] && choice.prompt == "ready which planet" {
            return Table {
                registry,
                session,
                p1_token,
                path,
                exhausted: choice.options.iter().map(|o| o.id.clone()).collect(),
            };
        }
        let pick = choice
            .options
            .iter()
            .find(|o| seat == players[0] && o.id.to_lowercase().contains("diplomacy"))
            .or_else(|| {
                choice
                    .options
                    .iter()
                    .find(|o| o.id.to_lowercase().contains("strategic") || o.id == "yes")
            })
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &pick.id)
            .unwrap();
    }
    panic!("never reached the ready question");
}

fn request(id: &str, pending: &Pending, kind: BatchKind, steps: Vec<MovementStep>) -> BatchRequest {
    BatchRequest {
        request_id: id.into(),
        expected_version: pending.2,
        nonce: pending.1.clone(),
        plan: MovementPlan {
            kind,
            destination: String::new(),
            steps,
        },
    }
}

fn ready(planet: &str) -> MovementStep {
    MovementStep::ReadyPlanet {
        planet: planet.into(),
    }
}

#[test]
fn both_planets_are_readied_by_one_batch() {
    let table = at_ready("dipl_both");
    assert!(table.exhausted.len() >= 2, "{:?}", table.exhausted);
    let pending = current(&table.session);
    let (a, b) = (table.exhausted[1].clone(), table.exhausted[0].clone());
    let log_before = table.session.decision_log().len();
    table
        .registry
        .submit_batch(
            "dipl_both",
            &table.p1_token,
            request(
                "both",
                &pending,
                BatchKind::ReadyPlanets,
                vec![ready(&a), ready(&b)],
            ),
        )
        .expect("a legal plan");
    let session = table.registry.get_game("dipl_both").unwrap();
    let state = session.current_state();
    assert!(
        !state
            .exhausted_planets
            .contains(&ti4_model::id::PlanetId::new(a))
    );
    assert!(
        !state
            .exhausted_planets
            .contains(&ti4_model::id::PlanetId::new(b))
    );
    assert_eq!(
        session.decision_log().len(),
        log_before + 2,
        "one engine answer per planet"
    );
    session.stop();
    std::fs::remove_dir_all(&table.path).unwrap();
}

#[test]
fn a_stale_or_short_plan_changes_nothing() {
    let table = at_ready("dipl_bad");
    let pending = current(&table.session);
    let before = table.session.current_state();
    let log_before = table.session.decision_log().len();
    let first = table.exhausted[0].clone();
    // A planet the engine does not offer.
    let stale = table.registry.submit_batch(
        "dipl_bad",
        &table.p1_token,
        request(
            "stale",
            &pending,
            BatchKind::ReadyPlanets,
            vec![ready("mecatol_rex_nowhere")],
        ),
    );
    assert!(stale.is_err());
    // A plan of one when two are still owed: the second question stays open and is not answered.
    let one = table.registry.submit_batch(
        "dipl_bad",
        &table.p1_token,
        request(
            "one",
            &pending,
            BatchKind::ReadyPlanets,
            vec![ready(&first)],
        ),
    );
    let session = table.registry.get_game("dipl_bad").unwrap();
    if one.is_ok() {
        assert_eq!(session.decision_log().len(), log_before + 1);
        assert_eq!(current(&session).3.prompt, "ready which planet");
    } else {
        assert_eq!(
            session.current_state().exhausted_planets,
            before.exhausted_planets
        );
        assert_eq!(session.decision_log().len(), log_before);
    }
    session.stop();
    std::fs::remove_dir_all(&table.path).unwrap();
}
