//! Activation rework, Phase 2 gate: is every generated fleet executable by the engine?
//!
//! Plays near-greedy six-seat tables with an arena bundle. At sampled turn starts where the acting
//! seat may take a tactical action, it copies the position and, for every system the engine lets it
//! activate, generates the candidate fleets and carries each one out in a fresh game from that
//! copy: the tactical action, the activation, then the plan answering every movement and cargo
//! prompt. A plan whose move is not offered with no event to explain it is a generation error.
//!
//! Also reports the menu (size, strategies) and what generating it costs per activation decision.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example package_legality -- \
//!   --bundle <arena checkpoint> --seeds 4 --rounds 4 --every 3
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::rc::Rc;
use std::time::Instant;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, Table};
use ti4_engine::game::{Game, TACTICAL_ACTION_ID};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_policy::tactical_plan::{Execution, Package, PlanState, packages_with};
use ti4_training::rollout::{
    OpeningMap, SimulationCapabilities, seated_faction,
    setup_game_with_capabilities_and_decider_factory,
};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn number<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{name} expects a number, got {value}")))
    })
}

fn refuse(message: &str) -> ! {
    eprintln!("REFUSED: {message}");
    std::process::exit(2)
}

/// Takes the tactical action, activates `system`, then follows the plan; anything else gets the
/// first option (or the decline, when there is one).
struct Driver {
    actor: PlayerId,
    system: Option<String>,
    execution: Option<Execution>,
    /// Activation options seen, when probing.
    offered: Rc<std::cell::RefCell<Vec<String>>>,
    /// The seat's ships in the destination when movement finished.
    arrived: Rc<std::cell::RefCell<Option<usize>>>,
    outcome: Rc<std::cell::RefCell<Option<Execution>>>,
    activated: bool,
}

impl Driver {
    fn fallback(choice: &Choice) -> ChoiceOption {
        choice
            .options
            .iter()
            .find(|option| option.is_decline())
            .unwrap_or(&choice.options[0])
            .clone()
    }
}

impl Decider for Driver {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        Ok(Self::fallback(choice))
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.player != self.actor {
            return Ok(Self::fallback(choice));
        }
        if let Some(option) = choice.options.iter().find(|o| o.id == TACTICAL_ACTION_ID)
            && !self.activated
        {
            return Ok(option.clone());
        }
        if choice
            .options
            .iter()
            .any(|o| o.kind == ti4_engine::tactical::ACTIVATE_KIND)
            && !self.activated
        {
            self.activated = true;
            *self.offered.borrow_mut() = choice.options.iter().map(|o| o.id.clone()).collect();
            let wanted = self
                .system
                .clone()
                .unwrap_or_else(|| choice.options[0].id.clone());
            return Ok(choice
                .options
                .iter()
                .find(|o| o.id == wanted)
                .unwrap_or(&choice.options[0])
                .clone());
        }
        if let Some(execution) = &mut self.execution
            && Execution::handles(choice)
        {
            let answer = execution.answer(choice);
            if execution.state == PlanState::Complete
                && let Some(system) = &self.system
            {
                let here = seen.observed().system(&SystemId::new(system.clone()));
                let ships = here.units.iter().filter(|u| u.owner == self.actor).count();
                *self.arrived.borrow_mut() = Some(ships);
            }
            *self.outcome.borrow_mut() = Some(execution.clone());
            if let Some(id) = answer
                && let Some(option) = choice.options.iter().find(|o| o.id == id)
            {
                return Ok(option.clone());
            }
        }
        Ok(Self::fallback(choice))
    }
}

struct Position {
    state: GameState,
    galaxy: Galaxy,
    actor: PlayerId,
}

fn fresh(content: &'static ContentStore, position: &Position, driver: Driver) -> Game<'static> {
    Game::with_table(
        position.state.clone(),
        content,
        Table::with_default(Box::new(driver)),
    )
    .with_galaxy(position.galaxy.clone())
}

fn run(game: &mut Game<'_>) -> Result<(), String> {
    for _ in 0..400 {
        if let Some(error) = game.step().error {
            return Err(format!("{error:?}"));
        }
        if game
            .events
            .iter()
            .any(|e| e == "TACTICAL_ACTION_COMPLETE" || e.starts_with("TURN_ENDED"))
        {
            return Ok(());
        }
    }
    Err("the tactical action did not finish in 400 steps".to_owned())
}

#[derive(Default)]
struct Tally {
    positions: usize,
    systems: usize,
    packages: usize,
    by_strategy: BTreeMap<&'static str, usize>,
    complete: usize,
    explained: BTreeMap<String, usize>,
    errors: Vec<String>,
    dropped_loads: usize,
    wrong_arrivals: usize,
    generation_seconds: f64,
    menu_sizes: Vec<usize>,
    /// Contested systems small enough to enumerate: best-of-all minus best-of-menu efficiency,
    /// and the reference menu's size.
    regrets: Vec<(f64, usize)>,
}

#[expect(clippy::too_many_lines, reason = "one gate, read top to bottom")]
fn check(
    content: &'static ContentStore,
    position: &Position,
    predictor: Option<&ti4_policy::battle::BattlePredictor>,
    tally: &mut Tally,
) {
    // Which systems the engine offers.
    let offered = Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut probe = fresh(
        content,
        position,
        Driver {
            actor: position.actor.clone(),
            system: None,
            execution: None,
            offered: Rc::clone(&offered),
            arrived: Rc::default(),
            outcome: Rc::default(),
            activated: false,
        },
    );
    for _ in 0..20 {
        if !offered.borrow().is_empty() || probe.step().error.is_some() {
            break;
        }
    }
    let systems = offered.borrow().clone();
    if systems.is_empty() {
        return;
    }
    tally.positions += 1;

    let seen = ti4_engine::choice::Observed::new(
        &position.state,
        content,
        ti4_model::POK,
        Some(&position.galaxy),
    );
    let started = Instant::now();
    let threat = ti4_policy::tactical_plan::threats(&seen, &position.actor);
    let menus: Vec<(String, Vec<Package>)> = systems
        .iter()
        .map(|system| {
            (
                system.clone(),
                packages_with(
                    &seen,
                    &position.actor,
                    &SystemId::new(system.clone()),
                    predictor,
                    &threat,
                ),
            )
        })
        .collect();
    tally.generation_seconds += started.elapsed().as_secs_f64();

    for (system, menu) in &menus {
        let Some(all) = ti4_policy::tactical_plan::exhaustive(
            &seen,
            &position.actor,
            &SystemId::new(system.clone()),
            predictor,
            &threat,
            8,
        ) else {
            continue;
        };
        if !all
            .iter()
            .any(|p| p.facts.fight && p.facts.space_win.is_some())
        {
            continue;
        }
        let best = |list: &[Package]| {
            list.iter()
                .filter(|p| !p.moves.is_empty())
                .map(ti4_policy::tactical_plan::efficiency)
                .fold(f64::NEG_INFINITY, f64::max)
        };
        let (reference, shortlisted) = (best(&all), best(menu));
        if reference.is_finite() && shortlisted.is_finite() {
            tally
                .regrets
                .push(((reference - shortlisted).max(0.0), all.len()));
        }
    }

    for (system, menu) in menus {
        tally.systems += 1;
        tally.menu_sizes.push(menu.len());
        for package in menu {
            tally.packages += 1;
            *tally
                .by_strategy
                .entry(package.strategy.label())
                .or_default() += 1;
            let expected = seen
                .system(&SystemId::new(system.clone()))
                .units
                .iter()
                .filter(|u| u.owner == position.actor)
                .count()
                + package.moves.len()
                + package.loads.len();
            let outcome = Rc::new(std::cell::RefCell::new(None));
            let arrived = Rc::new(std::cell::RefCell::new(None));
            let mut game = fresh(
                content,
                position,
                Driver {
                    actor: position.actor.clone(),
                    system: Some(system.clone()),
                    execution: Some(Execution::new(package.clone())),
                    offered: Rc::default(),
                    arrived: Rc::clone(&arrived),
                    outcome: Rc::clone(&outcome),
                    activated: false,
                },
            );
            if let Err(error) = run(&mut game) {
                tally.errors.push(format!(
                    "{system} {}: engine: {error}",
                    package.strategy.label()
                ));
                continue;
            }
            let rift = game.events.iter().any(|e| e == "SHIP_LOST_TO_GRAVITY_RIFT");
            let result = outcome.borrow().clone();
            match result.as_ref().map(|e| e.state.clone()) {
                Some(PlanState::Complete) => {
                    tally.complete += 1;
                    let dropped = result.as_ref().map_or(0, Execution::dropped_loads);
                    tally.dropped_loads += dropped;
                    if !rift && dropped == 0 && *arrived.borrow() != Some(expected) {
                        tally.wrong_arrivals += 1;
                        if tally.wrong_arrivals <= 5 {
                            tally.errors.push(format!(
                                "{system} {}: expected {expected} units there, found {:?}; moves {:?}; loads {:?}; events {:?}",
                                package.strategy.label(),
                                arrived.borrow(),
                                package.moves,
                                package.loads,
                                game.events.iter().rev().take(25).collect::<Vec<_>>()
                            ));
                        }
                    }
                }
                Some(PlanState::NotOffered(reason)) => {
                    tally
                        .errors
                        .push(format!("{system} {}: {reason}", package.strategy.label()));
                }
                Some(PlanState::Executing) | None => {
                    let why = ["CEASEFIRE_USED", "TURN_ENDED_BY_MINISTER_OF_PEACE"]
                        .into_iter()
                        .find(|marker| game.events.iter().any(|e| e == marker))
                        .map_or_else(|| "never asked to move".to_owned(), ToOwned::to_owned);
                    if why == "never asked to move" {
                        tally
                            .errors
                            .push(format!("{system} {}: {why}", package.strategy.label()));
                    } else {
                        *tally.explained.entry(why).or_default() += 1;
                    }
                }
            }
        }
    }
}

#[expect(clippy::too_many_lines, reason = "setup, play and report in one place")]
fn main() {
    let bundle = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seeds: u64 = number("--seeds", 4);
    let rounds: u32 = number("--rounds", 4);
    let every: usize = number("--every", 3);
    let content = ContentStore::embedded();
    ti4_tensor::configure_deterministic(20_260_917)
        .unwrap_or_else(|error| refuse(&format!("backend: {error}")));
    let loaded = ti4_mlp::bundle::read(Path::new(&bundle))
        .unwrap_or_else(|error| refuse(&format!("{bundle}: {error}")));
    let predictor = loaded.actor.battle_predictor().cloned();
    let actor = Rc::new(loaded.actor);
    let vocabulary = loaded.vocabulary;

    let mut tally = Tally::default();
    let clock = Instant::now();
    for seed in 0..seeds {
        let seed = 930_000_000 + seed;
        let factions = FACTIONS.map(FactionId::new);
        let players: Vec<PlayerId> = (0..FACTIONS.len())
            .map(|index| PlayerId::new(format!("seat{index}")))
            .collect();
        let seated: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .enumerate()
            .map(|(index, player)| (player.clone(), seated_faction(&factions, seed, 0, index)))
            .collect();
        let mut game = setup_game_with_capabilities_and_decider_factory(
            content,
            &players,
            &seated,
            DEFAULT,
            seed,
            &OpeningMap::RustVaried,
            SimulationCapabilities { diplomacy: true },
            |baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, player) in players.iter().enumerate() {
                    let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                        .map_err(|error| format!("{player}: {error}"))?;
                    let baseline = baselines.get(player).copied().unwrap_or_default();
                    let stream = seed.wrapping_add(u64::try_from(index).unwrap_or(0));
                    let (decider, _status) =
                        ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, stream)
                            .at_temperature(0.25)
                            .from_setup(baseline)
                            .seat();
                    deciders.insert(player.clone(), decider);
                }
                Ok(deciders)
            },
        )
        .unwrap_or_else(|error| refuse(&format!("setup: {error}")));
        let galaxy = game
            .galaxy()
            .cloned()
            .unwrap_or_else(|| refuse("no galaxy"));
        let target = game.state.round + rounds;
        let mut turns = 0usize;
        let mut steps = 0usize;
        while game.state.round < target && !game.state.finished && steps < 80_000 {
            if let Some(choice) = game.legal_options()
                && choice.options.iter().any(|o| o.id == TACTICAL_ACTION_ID)
                && game.state.active.as_ref() == Some(&choice.player)
            {
                turns += 1;
                if turns.is_multiple_of(every) {
                    let position = Position {
                        state: game.state.clone(),
                        galaxy: galaxy.clone(),
                        actor: choice.player.clone(),
                    };
                    check(content, &position, predictor.as_deref(), &mut tally);
                }
            }
            if let Some(error) = game.step().error {
                refuse(&format!("seed {seed} step {steps}: {error:?}"));
            }
            steps += 1;
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let per_decision_ms = 1000.0 * tally.generation_seconds / tally.positions.max(1) as f64;
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let mean_menu =
        tally.menu_sizes.iter().sum::<usize>() as f64 / tally.menu_sizes.len().max(1) as f64;
    println!(
        "package legality (generator v{})",
        ti4_policy::tactical_plan::GENERATOR_VERSION
    );
    println!(
        "  positions {}  systems {}  packages {}  ({:.1?})",
        tally.positions,
        tally.systems,
        tally.packages,
        clock.elapsed()
    );
    println!(
        "  menu size per system: mean {mean_menu:.2}, max {}",
        tally.menu_sizes.iter().max().unwrap_or(&0)
    );
    println!("  by strategy: {:?}", tally.by_strategy);
    println!(
        "  completed {}  explained interruptions {:?}",
        tally.complete, tally.explained
    );
    println!(
        "  dropped loads {}  wrong arrivals {}",
        tally.dropped_loads, tally.wrong_arrivals
    );
    println!("  generation {per_decision_ms:.2} ms per activation decision (all offered systems)");
    let n = tally.regrets.len().max(1);
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (mean_regret, missed, reference_size) = (
        tally.regrets.iter().map(|(r, _)| r).sum::<f64>() / n as f64,
        tally.regrets.iter().filter(|(r, _)| *r > 0.1).count() as f64 / n as f64,
        tally.regrets.iter().map(|(_, s)| *s).sum::<usize>() as f64 / n as f64,
    );
    println!(
        "  shortlist recall on {} contested systems with <= 8 movable ships: mean regret {mean_regret:.3}, regret > 0.1 in {:.1}%, reference menu {reference_size:.1} fleets vs shortlist",
        tally.regrets.len(),
        100.0 * missed
    );
    println!("  failures {}", tally.errors.len());
    for error in tally.errors.iter().take(20) {
        println!("    {error}");
    }
}
