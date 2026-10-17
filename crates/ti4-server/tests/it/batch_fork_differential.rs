//! The batch check from a step-boundary copy must mean exactly what the full replay means.
//!
//! For decisions across recorded games, build plans of every batch kind from what the engine
//! offered at that decision, run them through the forked check and through the full replay, and
//! require the same selected options, recorded decisions, boundary state, transitions, winner,
//! interruption or failure. Copies are taken the way the session worker takes them (at the
//! boundary before the step that contains the decision), so decisions in the middle of a step
//! exercise the replayed tail.
//!
//! Size: `TI4_BATCH_DIFF_POINTS` (decisions per game, `all` for every decision with a batchable
//! offer; default is small so debug builds stay quick).

use crate::support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Table};
use ti4_engine::game::Game;
use ti4_host::batch::{
    BatchKind, MovementPlan, MovementStep, SimulationMode, simulate_with, simulation_difference,
};
use ti4_host::step_snapshot::StepSnapshot;
use ti4_server::session::SessionConfig;

use crate::support::{Forced, Source, generated, strict_saved_games};

/// Remembers every offer the engine made, in decision order.
struct Recording {
    inner: Forced,
    offers: Arc<Mutex<Vec<Choice>>>,
}

impl Decider for Recording {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose(choice)?;
        self.offers.lock().unwrap().push(choice.clone());
        Ok(answer)
    }
}

fn step_of(option: &ChoiceOption) -> Option<(BatchKind, MovementStep)> {
    let text = |key: &str| {
        option
            .payload
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    };
    let damaged = option
        .payload
        .get("damaged")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let kind = option.kind.as_str();
    let id = option.id.as_str();
    Some(match (kind, id) {
        ("move", _) => (
            BatchKind::TacticalMovement,
            MovementStep::Move {
                origin: text("origin")?,
                unit: text("unit")?,
                damaged,
            },
        ),
        ("load", _) => (
            BatchKind::TacticalMovement,
            MovementStep::Load {
                origin: text("system")?,
                unit: text("unit")?,
                source: text("source"),
                damaged,
                galvanized: None,
                pickup_system: text("pickup_system"),
            },
        ),
        (_, "done_loading") => (BatchKind::TacticalMovement, MovementStep::DoneLoading),
        (_, "done_moving") => (BatchKind::TacticalMovement, MovementStep::DoneMoving),
        ("pay", "trade_good") => (BatchKind::Payment, MovementStep::TradeGood),
        ("pay", _) => (
            BatchKind::Payment,
            MovementStep::Exhaust {
                planet: id.strip_prefix("exhaust|")?.to_owned(),
            },
        ),
        ("vote_planet", _) => (
            BatchKind::AgendaVotePlanets,
            MovementStep::VotePlanet {
                planet: id.to_owned(),
            },
        ),
        (_, "decline") => (BatchKind::AgendaVotePlanets, MovementStep::DoneVoting),
        (_, "done_producing") => (BatchKind::Production, MovementStep::DoneProducing),
        ("produce", _) => (
            BatchKind::Production,
            MovementStep::Produce {
                unit: text("unit")?,
                count: u32::try_from(option.payload.get("count")?.as_u64()?).ok()?,
                exchange: option
                    .payload
                    .get("exchange")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            },
        ),
        ("ready", _) => (
            BatchKind::ReadyPlanets,
            MovementStep::ReadyPlanet {
                planet: id.to_owned(),
            },
        ),
        ("pool", _) => (
            BatchKind::Tokens,
            MovementStep::Pool {
                pool: id.to_owned(),
            },
        ),
        (k, "yes" | "no") if k == ti4_engine::strategy::STRATEGY_KIND => (
            BatchKind::Tokens,
            MovementStep::Purchase { buy: id == "yes" },
        ),
        (k, _) if k == ti4_engine::combat::SUSTAIN_KIND => (
            BatchKind::Casualties,
            MovementStep::Sustain {
                unit: text("unit")?,
            },
        ),
        (k, _)
            if k == ti4_engine::combat::CASUALTY_KIND
                || k == ti4_engine::invasion::GROUND_CASUALTY_KIND =>
        {
            (
                BatchKind::Casualties,
                MovementStep::Destroy {
                    unit: text("unit")?,
                    damaged,
                },
            )
        }
        _ => return None,
    })
}

fn terminal(kind: BatchKind) -> Option<MovementStep> {
    match kind {
        BatchKind::TacticalMovement => Some(MovementStep::DoneMoving),
        BatchKind::AgendaVotePlanets => Some(MovementStep::DoneVoting),
        BatchKind::Production => Some(MovementStep::DoneProducing),
        BatchKind::Payment
        | BatchKind::Casualties
        | BatchKind::Tokens
        | BatchKind::ReadyPlanets => None,
    }
}

/// Plans of every kind the offer supports, plus deliberately wrong ones (a step the engine does
/// not offer, a plan of a kind the decision is not part of), so failures are compared too.
fn plans_for(choice: &Choice, destination: &str) -> Vec<MovementPlan> {
    let steps: Vec<(BatchKind, MovementStep)> = choice.options.iter().filter_map(step_of).collect();
    let mut plans = Vec::new();
    let mut seen = Vec::new();
    for (kind, first) in &steps {
        if seen.contains(kind) {
            continue;
        }
        seen.push(*kind);
        // Prefer a real step over the terminal one, so the plan does something.
        let step = steps
            .iter()
            .filter(|(other, _)| other == kind)
            .map(|(_, step)| step)
            .find(|step| {
                terminal(*kind)
                    .is_none_or(|end| std::mem::discriminant(&end) != std::mem::discriminant(*step))
            })
            .unwrap_or(first);
        let destination = matches!(kind, BatchKind::TacticalMovement | BatchKind::Production)
            .then(|| destination.to_owned())
            .unwrap_or_default();
        let mut one = vec![step.clone()];
        one.extend(
            terminal(*kind)
                .filter(|end| std::mem::discriminant(end) != std::mem::discriminant(step)),
        );
        plans.push(MovementPlan {
            kind: *kind,
            destination: destination.clone(),
            steps: one,
        });
        if let Some(end) = terminal(*kind) {
            plans.push(MovementPlan {
                kind: *kind,
                destination: destination.clone(),
                steps: vec![end],
            });
        }
        if *kind == BatchKind::TacticalMovement {
            plans.push(MovementPlan {
                kind: *kind,
                destination,
                steps: vec![
                    MovementStep::Move {
                        origin: "nowhere".into(),
                        unit: "ghost".into(),
                        damaged: false,
                    },
                    MovementStep::DoneMoving,
                ],
            });
        }
    }
    if !steps.is_empty() {
        // A kind this decision is not part of: must fail identically on both paths.
        plans.push(MovementPlan {
            kind: BatchKind::Payment,
            destination: String::new(),
            steps: vec![MovementStep::TradeGood],
        });
    }
    plans
}

#[derive(Default, Debug)]
struct Coverage {
    points: usize,
    plans: usize,
    forked: usize,
    mid_step: usize,
    ok: usize,
    failed: usize,
    kinds: BTreeMap<String, usize>,
    /// Plans that were accepted, by kind (the rest failed the same way on both paths).
    ok_kinds: BTreeMap<String, usize>,
}

/// How many decisions per game to check (`None`: all that offer something batchable).
fn point_budget(default: usize) -> Option<usize> {
    match std::env::var("TI4_BATCH_DIFF_POINTS") {
        Ok(value) if value == "all" => None,
        Ok(value) => value.parse().ok().or(Some(default)),
        Err(_) => Some(default),
    }
}

fn differential(source: &Source, default_points: usize) -> Coverage {
    let total = source.records.len();
    // Pass 1: what the engine offered at each decision, and where its steps began.
    let offers = Arc::new(Mutex::new(Vec::new()));
    let mut boundaries = Vec::new();
    {
        let force = ti4_host::rng_force::RngForce::new(&source.marks);
        let table = Table::with_default(Box::new(Recording {
            inner: support::forced_decider(source, 0, force.clone()),
            offers: offers.clone(),
        }));
        let mut game = Game::with_table(source.state.clone(), ContentStore::embedded(), table)
            .with_galaxy(source.galaxy.clone());
        if let Some(force) = &force {
            force.attach(&mut game);
        }
        while game.table.log.records.len() < total {
            boundaries.push(game.table.log.records.len());
            if game.step().error.is_some() {
                break;
            }
        }
    }
    let offers = offers.lock().unwrap().clone();
    assert!(
        offers.len() >= total,
        "{}: offers {} of {total}",
        source.name,
        offers.len()
    );

    // Which decisions to check: spread over the batchable ones, every kind represented.
    let batchable: Vec<usize> = (0..total)
        .filter(|index| offers[*index].options.iter().any(|o| step_of(o).is_some()))
        .collect();
    let chosen: Vec<usize> = match point_budget(default_points) {
        None => batchable.clone(),
        Some(budget) => {
            let mut picked = Vec::new();
            let mut by_kind: BTreeMap<String, Vec<usize>> = BTreeMap::new();
            for index in &batchable {
                let kind = offers[*index]
                    .options
                    .iter()
                    .find_map(step_of)
                    .map(|(kind, _)| format!("{kind:?}"))
                    .unwrap_or_default();
                by_kind.entry(kind).or_default().push(*index);
            }
            let per_kind = budget.div_ceil(by_kind.len().max(1)).max(1);
            for indexes in by_kind.values() {
                let stride = (indexes.len() / per_kind).max(1);
                picked.extend(indexes.iter().step_by(stride).take(per_kind).copied());
            }
            picked.sort_unstable();
            picked
        }
    };

    // Pass 2: walk the game; at the boundary before each chosen decision's step, take the copy
    // the worker would have taken and check the decision against it.
    let mut config = SessionConfig::new("differential", source.state.clone())
        .with_galaxy(source.galaxy.clone(), Vec::new());
    config.rng_marks = source.marks.clone();
    let mut coverage = Coverage::default();
    let mut game = support::start_game(source);
    let mut next_point = 0usize;
    for (position, &boundary) in boundaries.iter().enumerate() {
        // Steps that record nothing leave the boundary where it was: the first of them is the
        // copy the worker keeps, and a decision belongs to the window of the step that asks it.
        let window_end = boundaries[position..]
            .iter()
            .copied()
            .find(|later| *later > boundary)
            .unwrap_or(total);
        let mut due = Vec::new();
        if position > 0 && boundaries[position - 1] == boundary {
            if game.step().error.is_some() {
                break;
            }
            continue;
        }
        while next_point < chosen.len() && chosen[next_point] < window_end {
            if chosen[next_point] >= boundary {
                due.push(chosen[next_point]);
            }
            next_point += 1;
        }
        if !due.is_empty() {
            let copy = StepSnapshot::capture(&mut game, config.history_generation)
                .expect("a copy at a step boundary");
            assert_eq!(copy.log_len, boundary);
            let destination = game
                .state
                .active_system
                .as_ref()
                .map_or_else(|| "0".to_owned(), ToString::to_string);
            for point in due {
                let choice = &offers[point];
                let actor = choice.player.clone();
                coverage.points += 1;
                if point > boundary {
                    coverage.mid_step += 1;
                }
                for plan in plans_for(choice, &destination) {
                    coverage.plans += 1;
                    *coverage
                        .kinds
                        .entry(format!("{:?}", plan.kind))
                        .or_default() += 1;
                    let prefix = &source.records[..point];
                    let forked = simulate_with(
                        &config,
                        prefix,
                        &actor,
                        &plan,
                        Some(&copy),
                        SimulationMode::Auto,
                    );
                    let full = simulate_with(
                        &config,
                        prefix,
                        &actor,
                        &plan,
                        None,
                        SimulationMode::FullReplay,
                    );
                    if let Some(difference) = simulation_difference(&forked, &full) {
                        panic!(
                            "{}: decision {point} (copy at {boundary}) plan {plan:?}: {difference}",
                            source.name
                        );
                    }
                    match &forked {
                        Ok(simulation) => {
                            coverage.ok += 1;
                            *coverage
                                .ok_kinds
                                .entry(format!("{:?}", plan.kind))
                                .or_default() += 1;
                            assert_eq!(simulation.forked_at, Some(boundary));
                            coverage.forked += 1;
                        }
                        Err(_) => coverage.failed += 1,
                    }
                }
            }
        }
        if game.step().error.is_some() {
            break;
        }
    }
    coverage
}

#[test]
fn slow_forked_batch_checks_match_the_full_replay_on_a_generated_game() {
    let source = generated("generated-31", 31, 700, 0);
    let coverage = differential(&source, 40);
    eprintln!("{coverage:?}");
    assert!(coverage.ok > 10 && coverage.failed > 5, "{coverage:?}");
    assert!(coverage.kinds.len() >= 3, "{coverage:?}");
    assert!(coverage.ok_kinds.len() >= 3, "{coverage:?}");
    assert!(coverage.mid_step > 0, "{coverage:?}");
}

#[test]
fn slow_forked_batch_checks_match_the_full_replay_with_forced_rng_positions() {
    let source = generated("generated-marks-12", 12, 450, 17);
    let coverage = differential(&source, 30);
    eprintln!("{coverage:?}");
    assert!(coverage.ok > 5, "{coverage:?}");
}

#[test]
fn forked_batch_checks_match_the_full_replay_on_saved_real_games() {
    let games = strict_saved_games(3);
    if games.is_empty() {
        eprintln!("no saved games on this machine; skipped");
        return;
    }
    for source in games {
        let coverage = differential(&source, 6);
        eprintln!("{}: {coverage:?}", source.name);
        assert!(coverage.plans > 0, "{coverage:?}");
    }
}

/// A movement batch check near the end of a game, with the copy the worker would hold.
struct Late {
    config: SessionConfig,
    actor: ti4_model::id::PlayerId,
    plan: MovementPlan,
    copy: StepSnapshot,
    capture: std::time::Duration,
    point: usize,
    boundary: usize,
}

fn late_check(source: &Source) -> Late {
    let total = source.records.len();
    let mut config = SessionConfig::new("timing", source.state.clone())
        .with_galaxy(source.galaxy.clone(), Vec::new());
    config.rng_marks = source.marks.clone();
    // The worker's copy: the last step boundary before a late batchable decision.
    let offers = Arc::new(Mutex::new(Vec::new()));
    let mut boundaries = Vec::new();
    {
        let table = Table::with_default(Box::new(Recording {
            inner: support::forced_decider(&source, 0, None),
            offers: offers.clone(),
        }));
        let mut game = Game::with_table(source.state.clone(), ContentStore::embedded(), table)
            .with_galaxy(source.galaxy.clone());
        while game.table.log.records.len() < total {
            boundaries.push(game.table.log.records.len());
            if game.step().error.is_some() {
                break;
            }
        }
    }
    let offers = offers.lock().unwrap().clone();
    let point = (0..total)
        .rev()
        .find(|index| {
            *index + 40 < total
                && plans_for(&offers[*index], "0")
                    .iter()
                    .any(|p| p.kind == BatchKind::TacticalMovement)
        })
        .expect("a late movement decision");
    let boundary = boundaries
        .iter()
        .copied()
        .filter(|b| *b <= point)
        .max()
        .unwrap();
    let mut game = support::start_game(source);
    while game.table.log.records.len() < boundary {
        assert!(game.step().error.is_none());
    }
    let copy_started = std::time::Instant::now();
    let copy = StepSnapshot::capture(&mut game, 0).unwrap();
    let capture = copy_started.elapsed();
    let actor = offers[point].player.clone();
    let plan = plans_for(&offers[point], "0")
        .into_iter()
        .find(|p| p.kind == BatchKind::TacticalMovement)
        .unwrap();
    Late {
        config,
        actor,
        plan,
        copy,
        capture,
        point,
        boundary,
    }
}

/// Timing helper (`-- --ignored --nocapture`, ideally with `--release` and again without):
/// a batch check near the end of the longest strictly replayable saved game, forked and in full.
#[test]
#[ignore = "measurement"]
fn time_a_late_game_batch_check() {
    let Some(source) = strict_saved_games(1).into_iter().next() else {
        eprintln!("no saved game");
        return;
    };
    let total = source.records.len();
    let Late {
        config,
        actor,
        plan,
        copy,
        capture,
        point,
        boundary,
    } = late_check(&source);
    let prefix = &source.records[..point];
    let time = |snapshot: Option<&StepSnapshot>, mode| {
        let mut best = std::time::Duration::MAX;
        let mut total_time = std::time::Duration::ZERO;
        for _ in 0..3 {
            let started = std::time::Instant::now();
            let result = simulate_with(&config, prefix, &actor, &plan, snapshot, mode);
            let took = started.elapsed();
            best = best.min(took);
            total_time += took;
            std::hint::black_box(&result);
        }
        (best, total_time / 3)
    };
    let (full_best, full_mean) = time(None, SimulationMode::FullReplay);
    let (fork_best, fork_mean) = time(Some(&copy), SimulationMode::Auto);
    eprintln!(
        "TIMING {total} decisions, check at decision {point} (copy at {boundary}, tail {}): \
         full replay best {full_best:?} mean {full_mean:?}; forked best {fork_best:?} mean {fork_mean:?}; \
         one capture {capture:?}",
        point - boundary
    );
}

/// Run 04-0023 (2026-10-07): a round 7 batch replayed ~1650 recorded choices and hit the flat
/// 15 s budget ("simulation timed out"); `replay_budget` then made the budget grow with the
/// game. With a step-boundary copy only the tail is replayed, so a long history is accepted
/// in a small fraction of even the flat 15 s, whatever its length.
#[test]
fn slow_a_long_history_batch_check_is_accepted_far_inside_the_old_budget() {
    let source = generated("generated-long", 31, 900, 0);
    assert!(source.records.len() > 400, "{}", source.records.len());
    let late = late_check(&source);
    let prefix = &source.records[..late.point];
    assert!(prefix.len() > 400);
    let started = std::time::Instant::now();
    let forked = simulate_with(
        &late.config,
        prefix,
        &late.actor,
        &late.plan,
        Some(&late.copy),
        SimulationMode::Auto,
    );
    let took = started.elapsed();
    let simulation = forked
        .as_ref()
        .unwrap_or_else(|failure| panic!("{failure:?}"));
    assert_eq!(simulation.forked_at, Some(late.boundary));
    assert!(
        took < ti4_host::batch::replay_budget(0) / 5,
        "forked check of {} decisions took {took:?}",
        prefix.len()
    );
    // Same answer as replaying all of it.
    let full = simulate_with(
        &late.config,
        prefix,
        &late.actor,
        &late.plan,
        None,
        SimulationMode::FullReplay,
    );
    assert_eq!(simulation_difference(&forked, &full), None);
    eprintln!(
        "{} decisions: forked {took:?}, capture {:?}",
        prefix.len(),
        late.capture
    );
}

#[test]
fn a_copy_that_does_not_belong_to_the_history_is_not_used() {
    let source = generated("generated-5", 5, 160, 0);
    let mut game = support::start_game(&source);
    while game.table.log.records.len() < 80 {
        assert!(game.step().error.is_none());
    }
    let copy = StepSnapshot::capture(&mut game, 3).unwrap();
    let at = copy.log_len;
    let prefix = &source.records[..at + 5];
    assert!(copy.is_valid_for(3, prefix));
    assert!(!copy.is_valid_for(4, prefix), "another history generation");
    assert!(
        !copy.is_valid_for(3, &source.records[..at - 1]),
        "a shorter log"
    );
    let mut other = source.records[..at + 5].to_vec();
    other[at - 1].chosen.push('x');
    assert!(!copy.is_valid_for(3, &other), "a different log");
    // An unusable copy means a full replay, with the same answer.
    let config = SessionConfig::new("stale", source.state.clone())
        .with_galaxy(source.galaxy.clone(), Vec::new());
    let plan = MovementPlan {
        kind: BatchKind::TacticalMovement,
        destination: "0".into(),
        steps: vec![MovementStep::DoneMoving],
    };
    let actor = source.records[at].player.clone();
    let stale = simulate_with(
        &config,
        prefix,
        &actor,
        &plan,
        Some(&copy),
        SimulationMode::Auto,
    );
    let full = simulate_with(
        &config,
        prefix,
        &actor,
        &plan,
        None,
        SimulationMode::FullReplay,
    );
    assert_eq!(config.history_generation, 0);
    assert_eq!(simulation_difference(&stale, &full), None);
    if let Ok(simulation) = &stale {
        assert_eq!(
            simulation.forked_at, None,
            "generation 3 copy, generation 0 history"
        );
    }
}
