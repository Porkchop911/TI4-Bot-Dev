//! Play a captured table forward in pure simulation, so a real game can be compared against it.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example mlp_simulate -- \
//!     --bundle out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428 \
//!     --capture out/bridge-captures/<start>.json \
//!     --rounds 1 --out out/divergence/simulated.json
//! ```
//!
//! # What this is for
//!
//! The bridge lets the MLP play on a real table. This plays *the same policy from the same board*
//! with no table at all: the engine decides and applies its own results, and nothing physical is
//! involved. Running both from one starting capture and comparing the end states is the sharpest
//! test the bridge has — it asks whether the table is a faithful mirror of the rules, or whether
//! commands are being dropped, mis-executed, or applied to the wrong seat.
//!
//! A divergence is not automatically a bridge fault. The engine is authoritative for the *rules*
//! and the table for *what is physically there*, so a difference means one of them is wrong about
//! the game and they want opposite fixes. Naming which fields differ is the whole point; nothing
//! here corrects anything.
//!
//! # Determinism
//!
//! Every seat is the same checkpoint at the same temperature, so at 0.001 this is as close to
//! deterministic as the policy gets. Dice and deck draws are not: the engine's own RNG is seeded
//! from `--seed`, and a real table rolls its own. Combat outcomes are therefore expected to differ
//! and are reported separately from state the policy controls.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_model::content_types::FULL;
use ti4_model::id::PlayerId;

const MAX_STEPS: usize = 400_000;
const REPEATED_CYCLE_LIMIT: usize = 10;

#[derive(Clone)]
struct AnsweredChoice {
    choice: Choice,
    answer: ChoiceOption,
}

struct TracedDecider {
    inner: Box<dyn Decider>,
    latest: Rc<RefCell<Option<AnsweredChoice>>>,
}

impl TracedDecider {
    fn record(
        &self,
        choice: &Choice,
        answer: Result<ChoiceOption, IllegalChoice>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if let Ok(answer) = &answer {
            *self.latest.borrow_mut() = Some(AnsweredChoice {
                choice: choice.clone(),
                answer: answer.clone(),
            });
        }
        answer
    }
}

impl Decider for TracedDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose(choice);
        self.record(choice, answer)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let answer = self.inner.choose_seeing(choice, seen);
        self.record(choice, answer)
    }
}

fn argument(name: &str) -> Option<String> {
    let mut arguments = std::env::args().skip(1);
    while let Some(found) = arguments.next() {
        if found == name {
            return arguments.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("REFUSED: {reason}");
    std::process::exit(1)
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let capture_path = argument("--capture").unwrap_or_else(|| refuse("--capture is required"));
    let rounds: u32 = argument("--rounds").map_or(1, |value| {
        value.parse().unwrap_or_else(|_| refuse("--rounds"))
    });
    let temperature: f64 = argument("--temperature").map_or(0.001, |value| {
        value.parse().unwrap_or_else(|_| refuse("--temperature"))
    });
    let seed: u64 = argument("--seed").map_or(20_260_909, |value| {
        value.parse().unwrap_or_else(|_| refuse("--seed"))
    });
    let out = argument("--out");

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let store = ti4_content::ContentStore::embedded();

    let text = std::fs::read_to_string(&capture_path)
        .unwrap_or_else(|error| refuse(&format!("reading {capture_path}: {error}")));
    let telemetry: ti4_bridge::import::Telemetry = serde_json::from_str(&text)
        .unwrap_or_else(|error| refuse(&format!("{capture_path} is not telemetry: {error}")));
    let imported = ti4_bridge::import::import(store, &telemetry, FULL)
        .unwrap_or_else(|error| refuse(&format!("importing the table: {error}")));

    let bundle = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let actor = std::rc::Rc::new(bundle.actor);
    let vocabulary = bundle.vocabulary;

    println!("simulating from {capture_path}");
    println!("  round      {}", imported.state.round);
    println!("  seats      {}", imported.state.players.len());
    println!("  policy     {bundle_path} at temperature {temperature}");
    println!("  rounds     {rounds}");

    // Every seat is the same checkpoint, exactly as the bridge seats them.
    let mut game = ti4_engine::game::Game::with_seeded_random(imported.state.clone(), store, seed)
        .with_galaxy(imported.galaxy.clone());
    let mut statuses = Vec::new();
    let latest = Rc::new(RefCell::new(None));
    for player in &imported.state.players {
        let row = ti4_mlp::FactionRow::of(player.id.as_str())
            .unwrap_or_else(|error| refuse(&format!("{}: {error}", player.id)));
        let (decider, status) =
            ti4_mlp::bot::MlpBot::sharing(&actor, vocabulary.clone(), row, seed)
                .at_temperature(temperature)
                .seat();
        game.table.seat(
            player.id.clone(),
            Box::new(TracedDecider {
                inner: decider,
                latest: Rc::clone(&latest),
            }),
        );
        statuses.push(status);
    }

    let before = snapshot(&imported.state);
    let target_round = game.state.round.saturating_add(rounds);
    let mut steps = 0usize;
    let mut seen_cycles: BTreeMap<(String, String, String), usize> = BTreeMap::new();
    while game.state.round < target_round && !game.state.finished && steps < MAX_STEPS {
        *latest.borrow_mut() = None;
        let result = game.step();
        steps += 1;
        if let Some(error) = result.error {
            println!("\n  the run stopped early at step {steps}: {error}");
            break;
        }

        let answered = latest.borrow().clone();
        let cycle = answered.as_ref().map(|answered| {
            (
                progress_key(&game.state),
                serde_json::to_string(&answered.choice).expect("Choice serialises"),
                answered.answer.id.clone(),
            )
        });
        let repetitions = cycle
            .map(|cycle| {
                let repetitions = seen_cycles.entry(cycle).or_default();
                *repetitions += 1;
                *repetitions
            })
            .unwrap_or(0);

        if repetitions >= REPEATED_CYCLE_LIMIT {
            println!(
                "\n  repeated an identical semantic state/decision/answer cycle {repetitions} times at step {steps}"
            );
            if let Some(answered) = answered {
                println!("  player      {}", answered.choice.player);
                println!("  prompt      {:?}", answered.choice.prompt);
                println!(
                    "  selected    {} ({:?})",
                    answered.answer.id, answered.answer.label
                );
                println!("  options");
                for option in &answered.choice.options {
                    println!(
                        "    id={:?} kind={:?} label={:?} payload={}",
                        option.id,
                        option.kind,
                        option.label,
                        serde_json::to_string(&option.payload).expect("payload serialises")
                    );
                }
            } else {
                println!("  no decision was offered; an internal phase/window transition stalled");
            }
            println!("  active_system {:?}", game.state.active_system);
            println!("  pending       {:?}", game.state.pending);
            for player in &game.state.players {
                println!(
                    "  seat {:<12} passed={} cards={:?} exhausted={:?} tactics={}",
                    player.id.to_string(),
                    player.passed,
                    player.strategy_cards,
                    player.exhausted_strategy_cards,
                    player.tactic_tokens,
                );
            }
            break;
        }
    }
    if steps >= MAX_STEPS {
        println!(
            "\n  the run stopped early: game did not progress within {MAX_STEPS} steps (round {}, phase {:?})",
            game.state.round, game.state.phase
        );
    }
    println!("\n  steps      {steps}");
    let after = snapshot(&game.state);

    let assigned: usize = statuses
        .iter()
        .map(|s| {
            s.counters()
                .assigned
                .load(std::sync::atomic::Ordering::Relaxed)
        })
        .sum();
    let oov: usize = statuses
        .iter()
        .map(|s| s.counters().oov.load(std::sync::atomic::Ordering::Relaxed))
        .sum();
    println!(
        "\n  features   {assigned} assigned, {oov} out of vocabulary ({:.2}% known)",
        if assigned + oov == 0 {
            0.0
        } else {
            100.0 * assigned as f64 / (assigned + oov) as f64
        }
    );

    println!("\n  seat        VP  TG  cmd(t/f/s)  techs  planets");
    for (seat, row) in &after {
        println!(
            "  {:<11} {:>2}  {:>2}  {}/{}/{}       {:>4}  {:>7}",
            seat.to_string(),
            row.victory_points,
            row.trade_goods,
            row.tactics,
            row.fleet,
            row.strategy,
            row.technologies,
            row.planets
        );
    }
    let moved: usize = after
        .iter()
        .filter(|(seat, row)| before.get(*seat).is_none_or(|was| was != *row))
        .count();
    println!("\n  {moved} of {} seats changed", after.len());

    if let Some(path) = out {
        let document = serde_json::json!({
            "source": capture_path,
            "bundle": bundle_path,
            "temperature": temperature,
            "rounds": rounds,
            "seed": seed,
            "round_after": game.state.round,
            "seats": after
                .iter()
                .map(|(seat, row)| (seat.to_string(), row.to_json()))
                .collect::<serde_json::Map<_, _>>(),
        });
        if let Some(parent) = std::path::Path::new(&path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(
            &path,
            serde_json::to_string_pretty(&document).expect("json"),
        )
        .unwrap_or_else(|error| refuse(&format!("writing {path}: {error}")));
        println!("  wrote {path}");
    }
}

/// A bounded fingerprint of fields that must move when an action or continuation advances.
///
/// Serialising the full imported `GameState` here is needlessly expensive and can overflow the
/// Windows process stack: it contains every deck plus large nested resolution records. This key is
/// diagnostic rather than a replay contract, so it deliberately records the progress-bearing
/// state while the exact choice and answer are fingerprinted separately.
fn progress_key(state: &ti4_model::state::GameState) -> String {
    let players: Vec<_> = state
        .players
        .iter()
        .map(|player| {
            serde_json::json!({
                "id": player.id,
                "passed": player.passed,
                "victory_points": player.victory_points,
                "trade_goods": player.trade_goods,
                "commodities": player.commodities,
                "tokens": [player.tactic_tokens, player.fleet_tokens, player.strategic_tokens],
                "strategy_cards": player.strategy_cards,
                "exhausted_strategy_cards": player.exhausted_strategy_cards,
                "technologies": player.technologies,
            })
        })
        .collect();
    let board: serde_json::Map<String, serde_json::Value> = snapshot(state)
        .into_iter()
        .map(|(seat, row)| (seat.to_string(), row.to_json()))
        .collect();
    serde_json::json!({
        "round": state.round,
        "phase": state.phase,
        "active": state.active,
        "active_system": state.active_system,
        "pending": state.pending,
        "players": players,
        "board": board,
    })
    .to_string()
}

/// The per-seat facts a table can also report, so the two sides are comparable.
///
/// Deliberately not the whole state: a `GameState` carries decks and hidden hands the table cannot
/// show, and comparing those would report a divergence on every field the bridge already declares
/// as a gap.
// Built by hand rather than derived: `serde`'s derive is not a dependency of this crate, and one
// nine-field row does not justify making it one.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Row {
    victory_points: i32,
    trade_goods: i32,
    commodities: i32,
    tactics: i32,
    fleet: i32,
    strategy: i32,
    technologies: usize,
    planets: usize,
    units: usize,
}

impl Row {
    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "victory_points": self.victory_points,
            "trade_goods": self.trade_goods,
            "commodities": self.commodities,
            "tactics": self.tactics,
            "fleet": self.fleet,
            "strategy": self.strategy,
            "technologies": self.technologies,
            "planets": self.planets,
            "units": self.units,
        })
    }
}

fn snapshot(state: &ti4_model::state::GameState) -> BTreeMap<PlayerId, Row> {
    state
        .players
        .iter()
        .map(|player| {
            let planets = state
                .board
                .values()
                .flat_map(|system| system.planet_control.values())
                .filter(|controller| **controller == player.id)
                .count();
            let units = state
                .board
                .values()
                .map(|system| {
                    system
                        .units
                        .iter()
                        .chain(system.planet_units.values().flatten())
                        .filter(|unit| unit.owner == player.id)
                        .count()
                })
                .sum();
            (
                player.id.clone(),
                Row {
                    victory_points: player.victory_points,
                    trade_goods: player.trade_goods,
                    commodities: player.commodities,
                    tactics: player.tactic_tokens,
                    fleet: player.fleet_tokens,
                    strategy: player.strategic_tokens,
                    technologies: player.technologies.len(),
                    planets,
                    units,
                },
            )
        })
        .collect()
}
