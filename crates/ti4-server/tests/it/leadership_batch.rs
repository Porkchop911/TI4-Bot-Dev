//! Leadership's purchase loop answered as one batch: free tokens, purchases, each purchase's
//! payment and its pool, in the order the engine asks them.

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
    p2_token: String,
    path: std::path::PathBuf,
    store: Arc<FileGameStore>,
}

/// Two seats; p1 holds Leadership and has 7 trade goods (plus Jord's 2 influence), p2 has 3.
/// Returns with p1's first "gain a command token" question pending.
fn at_leadership(game_id: &str, p2_goods: i32) -> Table {
    let path = std::env::temp_dir().join(format!("ti4_lead_{:032x}", rand::random::<u128>()));
    let store = Arc::new(FileGameStore::new(&path).unwrap());
    let registry = GameRegistry::new().with_store(store.clone());
    let players = vec![PlayerId::new("p1"), PlayerId::new("p2")];
    let (mut state, galaxy) =
        ti4_server::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
    state.player_mut(&players[0]).unwrap().trade_goods = 7;
    state.player_mut(&players[1]).unwrap().trade_goods = p2_goods;
    let tiles = ti4_server::map::build_board_tiles(ContentStore::embedded(), &galaxy);
    let config = SessionConfig::new(game_id, state)
        .with_seed(42)
        .with_player_ids(players.clone())
        .with_galaxy(galaxy, tiles)
        .with_seat(players[0].clone(), SeatController::Human)
        .with_seat(players[1].clone(), SeatController::Human);
    let session = registry.create_game(config).unwrap();
    let p1_token = session.seat_tokens()[&players[0]].clone();
    let p2_token = session.seat_tokens()[&players[1]].clone();
    loop {
        let (seat, nonce, version, choice) = current(&session);
        if choice.prompt.starts_with("gain a command token") {
            break;
        }
        let pick = choice
            .options
            .iter()
            .find(|o| o.id == "strategic")
            .unwrap_or(&choice.options[0]);
        session
            .submit_choice(&seat, &nonce, version, &pick.id)
            .unwrap();
    }
    Table {
        registry,
        session,
        p1_token,
        p2_token,
        path,
        store,
    }
}

/// The batch replaces the worker, so look the session up again after submitting one.
fn live(table: &Table, game_id: &str) -> Arc<GameSession> {
    table.registry.get_game(game_id).unwrap()
}

fn pool(id: &str) -> MovementStep {
    MovementStep::Pool { pool: id.into() }
}

fn buy(buy: bool) -> MovementStep {
    MovementStep::Purchase { buy }
}

/// What the web panel does: spend the biggest planets first, then trade goods, in order.
fn payment_for(
    details: &serde_json::Map<String, serde_json::Value>,
    tokens: i64,
) -> Vec<MovementStep> {
    let purchase = &details["purchase"];
    let mut planets: Vec<(String, i64)> = purchase["planets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["id"].as_str().unwrap().to_owned(),
                p["worth"].as_i64().unwrap(),
            )
        })
        .collect();
    planets.sort_by_key(|(_, worth)| std::cmp::Reverse(*worth));
    let mut steps = Vec::new();
    let mut paid = 0;
    for (id, worth) in planets {
        if paid >= 3 * tokens {
            break;
        }
        paid += worth;
        steps.push(MovementStep::Exhaust { planet: id });
    }
    while paid < 3 * tokens {
        paid += purchase["trade_good_worth"].as_i64().unwrap();
        steps.push(MovementStep::TradeGood);
    }
    steps
}

fn request(id: &str, pending: &Pending, steps: Vec<MovementStep>) -> BatchRequest {
    BatchRequest {
        request_id: id.into(),
        expected_version: pending.2,
        nonce: pending.1.clone(),
        plan: MovementPlan {
            kind: BatchKind::Tokens,
            destination: String::new(),
            steps,
        },
    }
}

/// Free tokens, two purchases (Jord plus a trade good, then three trade goods), pools, and the
/// final "no" the engine asks because three influence are still left.
fn two_purchases() -> Vec<MovementStep> {
    vec![
        pool("tactic_tokens"),
        pool("fleet_tokens"),
        pool("fleet_tokens"),
        buy(true),
        MovementStep::Exhaust {
            planet: "jord".into(),
        },
        MovementStep::TradeGood,
        pool("strategic_tokens"),
        buy(true),
        MovementStep::TradeGood,
        MovementStep::TradeGood,
        MovementStep::TradeGood,
        pool("strategic_tokens"),
        buy(false),
    ]
}

#[test]
fn a_plan_with_purchases_and_assignments_applies_atomically() {
    let table = at_leadership("lead_ok", 0);
    let pending = current(&table.session);
    let before = table.session.current_state();
    let log_before = table.session.decision_log().len();
    let p1 = PlayerId::new("p1");
    // The panel plans from what the question states; the hand-built plan agrees with it.
    assert_eq!(pending.3.details["purchase"]["max"], 3);
    assert!(matches!(
        &payment_for(&pending.3.details, 2)[0],
        MovementStep::Exhaust { planet } if planet == "jord"
    ));

    let result = table
        .registry
        .submit_batch(
            "lead_ok",
            &table.p1_token,
            request("ok", &pending, two_purchases()),
        )
        .expect("a legal plan");
    assert!(result.interrupted.is_none());

    let session = live(&table, "lead_ok");
    let after = session.current_state();
    let (b, a) = (before.player(&p1).unwrap(), after.player(&p1).unwrap());
    assert_eq!(a.tactic_tokens, b.tactic_tokens + 1);
    assert_eq!(a.fleet_tokens, b.fleet_tokens + 2);
    assert_eq!(a.strategic_tokens, b.strategic_tokens + 2);
    assert_eq!(
        a.trade_goods,
        b.trade_goods - 4,
        "one good beside Jord, three for the second"
    );
    assert!(after.exhausted_planets.contains("jord"));
    // Free tokens (3), then per purchase: yes, a payment decision or none, pool; and the "no".
    assert!(session.decision_log().len() > log_before + 3 + 2 * 2);
    assert_eq!(
        session.decision_log().last().unwrap().prompt,
        "spend 3 influence for a command token"
    );
    assert_eq!(session.decision_log().last().unwrap().chosen, "no");
    // The turn moved on: nothing of Leadership is left pending.
    assert_ne!(
        current(&session).3.prompt,
        "spend 3 influence for a command token"
    );
    table.session.stop();
    std::fs::remove_dir_all(&table.path).unwrap();
}

#[test]
fn plans_the_engine_would_not_accept_change_nothing() {
    let table = at_leadership("lead_bad", 0);
    let pending = current(&table.session);
    let before = table.session.current_state();
    let log_before = table.session.decision_log();

    let unaffordable = vec![
        pool("tactic_tokens"),
        pool("tactic_tokens"),
        pool("tactic_tokens"),
        buy(true),
        MovementStep::Exhaust {
            planet: "jord".into(),
        },
        MovementStep::TradeGood,
        pool("tactic_tokens"),
        buy(true),
        MovementStep::TradeGood,
        MovementStep::TradeGood,
        MovementStep::TradeGood,
        pool("tactic_tokens"),
        buy(true),
        MovementStep::TradeGood,
        MovementStep::TradeGood,
        MovementStep::TradeGood,
        pool("tactic_tokens"),
        buy(true), // a fourth purchase: only nine influence were available
        MovementStep::TradeGood,
        pool("tactic_tokens"),
    ];
    let extra_free_token = vec![
        pool("tactic_tokens"),
        pool("tactic_tokens"),
        pool("tactic_tokens"),
        pool("tactic_tokens"), // the engine asks the purchase question here, not a pool
    ];
    let wrong_payment = vec![
        pool("tactic_tokens"),
        pool("tactic_tokens"),
        pool("tactic_tokens"),
        buy(true),
        MovementStep::Exhaust {
            planet: "moon_of_nowhere".into(),
        },
        pool("tactic_tokens"),
    ];
    // Three purchases spend all nine influence, so the engine never asks the closing question.
    let trailing_no_never_asked = {
        let mut steps = two_purchases();
        steps.truncate(12);
        steps.extend([
            buy(true),
            MovementStep::TradeGood,
            MovementStep::TradeGood,
            MovementStep::TradeGood,
            pool("tactic_tokens"),
            buy(false),
        ]);
        steps
    };
    for (name, steps) in [
        ("unaffordable", unaffordable),
        ("extra_free_token", extra_free_token),
        ("wrong_payment", wrong_payment),
        ("trailing_no_never_asked", trailing_no_never_asked),
    ] {
        let rejected = table.registry.submit_batch(
            "lead_bad",
            &table.p1_token,
            request(name, &pending, steps),
        );
        assert!(rejected.is_err(), "{name} must be rejected");
        assert!(
            table.session.current_state().identical(&before),
            "{name} changed the state"
        );
        assert_eq!(
            table.session.decision_log(),
            log_before,
            "{name} changed the log"
        );
    }
    table.session.stop();
    std::fs::remove_dir_all(&table.path).unwrap();
}

#[test]
fn a_recovered_session_replays_the_purchase_batch() {
    let table = at_leadership("lead_replay", 0);
    let pending = current(&table.session);
    table
        .registry
        .submit_batch(
            "lead_replay",
            &table.p1_token,
            request("ok", &pending, two_purchases()),
        )
        .unwrap();
    let recovered = table.store.recover_session("lead_replay").unwrap();
    recovered.wait_replayed().unwrap();
    let session = live(&table, "lead_replay");
    assert_eq!(recovered.decision_log(), session.decision_log());
    assert_eq!(recovered.batches().len(), 1);
    assert_eq!(recovered.current_state(), session.current_state());
    recovered.stop();
    table.session.stop();
    std::fs::remove_dir_all(&table.path).unwrap();
}

#[test]
fn the_secondary_window_plans_its_purchase_from_the_one_question() {
    let table = at_leadership("lead_secondary", 3);
    let p2 = PlayerId::new("p2");
    let pending = current(&table.session);
    // Take the three free tokens and decline further purchases; the follower is asked next.
    table
        .registry
        .submit_batch(
            "lead_secondary",
            &table.p1_token,
            request(
                "primary",
                &pending,
                vec![
                    pool("tactic_tokens"),
                    pool("tactic_tokens"),
                    pool("tactic_tokens"),
                    buy(false),
                ],
            ),
        )
        .unwrap();
    let window = current(&live(&table, "lead_secondary"));
    assert_eq!(window.0, p2);
    assert_eq!(window.3.prompt, "spend 3 influence for a command token");
    assert_eq!(window.3.details["kind"], "strategy_secondary");
    let max = window.3.details["purchase"]["max"].as_i64().unwrap();
    assert!(max >= 1);
    let before = live(&table, "lead_secondary").current_state();

    let mut steps = vec![buy(true)];
    steps.extend(payment_for(&window.3.details, 1));
    steps.push(pool("fleet_tokens"));
    if max > 1 {
        steps.push(buy(false));
    }
    table
        .registry
        .submit_batch(
            "lead_secondary",
            &table.p2_token,
            request("secondary", &window, steps),
        )
        .expect("the window's yes, its payment and its pool in one plan");
    let after = live(&table, "lead_secondary").current_state();
    assert_eq!(
        after.player(&p2).unwrap().fleet_tokens,
        before.player(&p2).unwrap().fleet_tokens + 1
    );
    table.session.stop();
    std::fs::remove_dir_all(&table.path).unwrap();
}
