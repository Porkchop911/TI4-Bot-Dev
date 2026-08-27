//! What the openings that do not clear actually look like.
//!
//! Both training lineages plateau in the high eighties, and a clearance percentage cannot say
//! whether the missing tenth is bad play or unreachable positions. The bar has three independent
//! parts — three planets gained, three distinct systems, one unit built — and a seat clears only by
//! meeting all of them. Which part misses, and by how much, is the difference between "the policy
//! is not trying hard enough" and "this opening could not be cleared by anyone".
//!
//! A seat one planet short of three is a different failure from a seat that gained none: the first
//! is a near miss, the second suggests the position never offered the planets. The report keeps
//! them apart rather than averaging them into a shortfall.
//!
//! # Spread
//!
//! The bar wants three planets across *three distinct systems*, so a seat that concentrates its
//! forces cannot clear it however much it moves. The operator's account of the usual failure is
//! exactly that: both capacity ships sent to one system, or every infantry landed on one planet.
//!
//! That is measurable at the end of the round. `SystemState::units` is the space area, so distinct
//! systems holding a seat's ships is where its fleet ended up; `planet_units` is ground forces, so
//! distinct systems holding its infantry is where it can actually take planets. Concentration shows
//! as a low count with a high maximum, and the report compares cleared seats against failed ones —
//! an absolute number would say nothing without something to be high or low against.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(argument) = args.next() {
        if argument == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// How one faction's openings went.
#[derive(Default)]
struct Failures {
    games: usize,
    cleared: usize,
    /// Failures in which each part was the one missing, counted per part, so a seat missing two
    /// parts is counted in both.
    missing_planets: usize,
    missing_systems: usize,
    missing_units: usize,
    /// Failures by how many parts were missed at once.
    missed_one: usize,
    missed_two: usize,
    missed_three: usize,
    /// Among failures, how far short each part was.
    planet_short: BTreeMap<usize, usize>,
    system_short: BTreeMap<usize, usize>,
    /// Planets gained in failures, so a near miss is distinguishable from a standstill.
    gained: BTreeMap<usize, usize>,
    /// Force spread at the end of the round, kept separately for cleared and failed seats.
    spread_cleared: Spread,
    spread_failed: Spread,
}

/// Where a seat's forces ended up.
#[derive(Default)]
struct Spread {
    seats: usize,
    /// Distinct systems holding this seat's ships.
    ship_systems: usize,
    /// Distinct systems holding this seat's ground forces.
    ground_systems: usize,
    /// Ground forces in the single most crowded system.
    biggest_stack: usize,
    /// Seats whose ground forces all ended in one system.
    ground_in_one: usize,
    /// Seats whose ships all ended in one system.
    ships_in_one: usize,
}

impl Spread {
    fn add(&mut self, ships: usize, ground: usize, biggest: usize) {
        self.seats += 1;
        self.ship_systems += ships;
        self.ground_systems += ground;
        self.biggest_stack += biggest;
        self.ground_in_one += usize::from(ground <= 1);
        self.ships_in_one += usize::from(ships <= 1);
    }
}

/// Where one seat's forces are, at the end of the round.
///
/// Ships and ground forces are counted apart because they fail differently: ships that all end in
/// one system cannot reach planets elsewhere, and ground forces that all land on one planet cannot
/// hold three.
fn spread_of(state: &ti4_model::state::GameState, player: &PlayerId) -> (usize, usize, usize) {
    let mut ship_systems = 0;
    let mut ground_systems = 0;
    let mut biggest = 0;
    for system in state.board.values() {
        if !system.units_of(player).is_empty() {
            ship_systems += 1;
        }
        let ground = system
            .planet_units
            .values()
            .flatten()
            .filter(|unit| &unit.owner == player)
            .count();
        if ground > 0 {
            ground_systems += 1;
        }
        biggest = biggest.max(ground);
    }
    (ship_systems, ground_systems, biggest)
}

fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: failures belong to a specific policy"));
    let seeds: u64 = argument("--seeds").map_or(200, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(695_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });

    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = std::rc::Rc::new(
        loaded
            .actor
            .inference_copy()
            .to_device(ti4_tensor::Device::Cpu),
    );

    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    let content = ContentStore::embedded();
    let factions: Vec<FactionId> = FACTIONS.iter().map(|name| FactionId::new(*name)).collect();

    println!("opening failures");
    println!("  bundle      {bundle_path}");
    println!(
        "  sample      {seeds} seeds x {} rotations, one round",
        FACTIONS.len()
    );
    println!("  bar         3 planets gained, 3 systems, 1 unit gained");

    let mut tallies: BTreeMap<String, Failures> = BTreeMap::new();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let (_events, _picks, assignments, openings, final_state) =
                ti4_training::rollout::audit_game_with_deciders(
                    content,
                    &factions,
                    DEFAULT,
                    seed,
                    rotation,
                    ti4_training::rollout::Horizon {
                        rounds: 1,
                        steps: 200_000,
                    },
                    &ti4_training::rollout::OpeningMap::PythonPool {
                        pool: Arc::clone(&pool),
                        tile_seed_offset: TILE_SEED_OFFSET,
                    },
                    |seated| {
                        let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                        for (index, (player, faction)) in seated.iter().enumerate() {
                            let row = ti4_mlp::FactionRow::of(faction.as_str())
                                .map_err(|error| format!("{player}: {error}"))?;
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                &actor,
                                vocabulary.clone(),
                                row,
                                stream,
                            )
                            .seat();
                            deciders.insert(player.clone(), decider);
                        }
                        Ok(deciders)
                    },
                )
                .unwrap_or_else(|error| refuse(&error));

            for (player, opening) in &openings {
                let Some(faction) = assignments.get(player) else {
                    continue;
                };
                let tally = tallies.entry(faction.to_string()).or_default();
                tally.games += 1;
                let (ships, ground, biggest) = spread_of(&final_state, player);
                if opening.cleared() {
                    tally.cleared += 1;
                    tally.spread_cleared.add(ships, ground, biggest);
                    continue;
                }
                tally.spread_failed.add(ships, ground, biggest);
                let mut missed = 0;
                if !opening.planets_ok() {
                    tally.missing_planets += 1;
                    missed += 1;
                }
                if !opening.systems_ok() {
                    tally.missing_systems += 1;
                    missed += 1;
                }
                if !opening.units_ok() {
                    tally.missing_units += 1;
                    missed += 1;
                }
                match missed {
                    1 => tally.missed_one += 1,
                    2 => tally.missed_two += 1,
                    _ => tally.missed_three += 1,
                }
                *tally
                    .planet_short
                    .entry(opening.planet_shortfall())
                    .or_default() += 1;
                *tally
                    .system_short
                    .entry(opening.system_shortfall())
                    .or_default() += 1;
                *tally.gained.entry(opening.planets_gained).or_default() += 1;
            }
        }
    }

    report(&tallies);
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts are exact in f64 far beyond any sample size"
)]
fn mean(total: usize, count: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    total as f64 / count as f64
}

#[expect(
    clippy::cast_precision_loss,
    reason = "counts are exact in f64 far beyond any sample size"
)]
fn share(part: usize, whole: usize) -> f64 {
    if whole == 0 {
        return 0.0;
    }
    part as f64 / whole as f64 * 100.0
}

fn report(tallies: &BTreeMap<String, Failures>) {
    println!();
    println!("  which part of the bar the failures missed (share of failures)");
    println!();
    println!(
        "  {:<10} {:>8} {:>9} {:>9} {:>8} {:>8} {:>9} {:>8} {:>8}",
        "faction", "games", "cleared", "failed", "planets", "systems", "units", "1 part", "2+"
    );
    let mut total = Failures::default();
    for (faction, tally) in tallies {
        let failed = tally.games - tally.cleared;
        total.games += tally.games;
        total.cleared += tally.cleared;
        total.missing_planets += tally.missing_planets;
        total.missing_systems += tally.missing_systems;
        total.missing_units += tally.missing_units;
        total.missed_one += tally.missed_one;
        total.missed_two += tally.missed_two;
        total.missed_three += tally.missed_three;
        println!(
            "  {:<10} {:>8} {:>8.1}% {:>9} {:>7.1}% {:>7.1}% {:>8.1}% {:>7.1}% {:>7.1}%",
            faction,
            tally.games,
            share(tally.cleared, tally.games),
            failed,
            share(tally.missing_planets, failed),
            share(tally.missing_systems, failed),
            share(tally.missing_units, failed),
            share(tally.missed_one, failed),
            share(tally.missed_two + tally.missed_three, failed),
        );
    }
    let failed = total.games - total.cleared;
    println!(
        "  {:<10} {:>8} {:>8.1}% {:>9} {:>7.1}% {:>7.1}% {:>8.1}% {:>7.1}% {:>7.1}%",
        "table",
        total.games,
        share(total.cleared, total.games),
        failed,
        share(total.missing_planets, failed),
        share(total.missing_systems, failed),
        share(total.missing_units, failed),
        share(total.missed_one, failed),
        share(total.missed_two + total.missed_three, failed),
    );

    println!();
    println!("  where the forces ended up, cleared against failed (means per seat)");
    println!();
    println!(
        "  {:<10} {:>19} {:>21} {:>19} {:>21}",
        "faction", "ship systems", "ground systems", "biggest stack", "all ground in one"
    );
    println!(
        "  {:<10} {:>9} {:>9} {:>10} {:>10} {:>9} {:>9} {:>10} {:>10}",
        "", "cleared", "failed", "cleared", "failed", "cleared", "failed", "cleared", "failed"
    );
    for (faction, tally) in tallies {
        let c = &tally.spread_cleared;
        let f = &tally.spread_failed;
        println!(
            "  {:<10} {:>9.2} {:>9.2} {:>10.2} {:>10.2} {:>9.2} {:>9.2} {:>9.1}% {:>9.1}%",
            faction,
            mean(c.ship_systems, c.seats),
            mean(f.ship_systems, f.seats),
            mean(c.ground_systems, c.seats),
            mean(f.ground_systems, f.seats),
            mean(c.biggest_stack, c.seats),
            mean(f.biggest_stack, f.seats),
            share(c.ground_in_one, c.seats),
            share(f.ground_in_one, f.seats),
        );
    }

    println!();
    println!("  planets gained in a failed opening (bar is 3; share of that faction's failures)");
    println!();
    print!("  {:<10}", "faction");
    for gained in 0..=3usize {
        print!(" {:>9}", format!("{gained} gained"));
    }
    println!();
    for (faction, tally) in tallies {
        let failed = tally.games - tally.cleared;
        print!("  {faction:<10}");
        for gained in 0..=3usize {
            let count = tally.gained.get(&gained).copied().unwrap_or(0);
            print!(" {:>8.1}%", share(count, failed));
        }
        println!();
    }
}
