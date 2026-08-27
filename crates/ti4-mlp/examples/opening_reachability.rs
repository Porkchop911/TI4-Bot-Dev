//! Were the openings that failed actually clearable?
//!
//! Four independently trained policies converge on 86–91% clearance, and a clearance number cannot
//! say whether the missing tenth is bad play or positions that no play could clear. Everything
//! downstream depends on which: destination features are worth building if the failures were
//! winnable, and worth nothing if they were not.
//!
//! # What this measures, and what it does not
//!
//! For each seat that failed, the same position is replayed several times with **that seat alone**
//! sampling at a raised temperature, every other seat left on the policy and its original stream.
//! If any replay clears, that opening was clearable — a constructive proof, one line at a time.
//!
//! The result is therefore a **lower bound** on the achievable rate. Finding a clearing line proves
//! the position was winnable; failing to find one proves only that this search did not find one.
//! Reporting it as "the ceiling" would invert the logic, so the report says "at least".
//!
//! # Why temperature rather than a scored search
//!
//! A beam guided by the opening potential would search harder, and would answer a different
//! question: whether a hand-written scoring rule can clear the position. Sampling the trained
//! policy hotter keeps the search inside plausible play and introduces no rule that was not already
//! learned. It is a weaker searcher and a cleaner measurement.
//!
//! Perturbing one seat at a time is the other half of that. Letting every seat explore at once
//! would change the contention the failing seat faced, and a position cleared against different
//! opponents is not the position that failed.

use std::collections::{BTreeMap, BTreeSet};
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

#[derive(Default)]
struct Reach {
    seats: usize,
    cleared: usize,
    failed: usize,
    /// Failures for which some replay found a clearing line.
    recoverable: usize,
    /// Attempts spent before the first success, summed over recovered seats.
    attempts_to_first: usize,
}

struct Setup {
    actor: std::rc::Rc<ti4_mlp::Actor>,
    vocabulary: ti4_policy::vocabulary::Vocabulary,
    pool: Arc<ti4_sim::MapPool>,
    content: &'static ContentStore,
    factions: Vec<FactionId>,
}

/// Play one game, optionally sampling a single seat hotter.
///
/// `hot` names the seat to perturb and the stream to perturb it with. Every other seat keeps the
/// temperature and stream it had in the baseline run, so the position under test is the one that
/// failed rather than a different one.
fn play(
    setup: &Setup,
    seed: u64,
    rotation: usize,
    hot: Option<(&PlayerId, f64, u64)>,
) -> BTreeMap<PlayerId, (FactionId, bool)> {
    let (_events, _picks, assignments, openings, _final) =
        ti4_training::rollout::audit_game_with_deciders(
            setup.content,
            &setup.factions,
            DEFAULT,
            seed,
            rotation,
            ti4_training::rollout::Horizon {
                rounds: 1,
                steps: 200_000,
            },
            &ti4_training::rollout::OpeningMap::PythonPool {
                pool: Arc::clone(&setup.pool),
                tile_seed_offset: TILE_SEED_OFFSET,
            },
            |seated, baselines| {
                let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                for (index, (player, faction)) in seated.iter().enumerate() {
                    let row = ti4_mlp::FactionRow::of(faction.as_str())
                        .map_err(|error| format!("{player}: {error}"))?;
                    let baseline = baselines
                        .get(player)
                        .copied()
                        .ok_or_else(|| format!("{player} has no setup baseline"))?;
                    let ordinary = seed
                        .wrapping_mul(1_000_003)
                        .wrapping_add(u64::try_from(index).unwrap_or(0));
                    let (temperature, stream) = match hot {
                        Some((target, temperature, stream)) if target == player => {
                            (temperature, stream)
                        }
                        _ => (1.0, ordinary),
                    };
                    let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                        &setup.actor,
                        setup.vocabulary.clone(),
                        row,
                        stream,
                    )
                    .from_setup(baseline)
                    .at_temperature(temperature)
                    .seat();
                    deciders.insert(player.clone(), decider);
                }
                Ok(deciders)
            },
        )
        .unwrap_or_else(|error| refuse(&error));

    openings
        .into_iter()
        .map(|(player, opening)| {
            let faction = assignments
                .get(&player)
                .cloned()
                .unwrap_or_else(|| FactionId::new(""));
            (player, (faction, opening.cleared()))
        })
        .collect()
}

fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| {
        refuse("--bundle is required: reachability is measured against a policy")
    });
    let seeds: u64 = argument("--seeds").map_or(60, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds expects a positive integer"))
    });
    let seed_base: u64 = argument("--seed-base").map_or(700_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base expects an unsigned integer"))
    });
    let attempts: usize = argument("--attempts").map_or(40, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--attempts expects a positive integer"))
    });
    let temperature: f64 = argument("--temperature").map_or(2.5, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--temperature expects a number"))
    });

    ti4_tensor::configure_deterministic(20_260_821)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Train],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));

    let setup = Setup {
        actor: std::rc::Rc::new(
            loaded
                .actor
                .inference_copy()
                .to_device(ti4_tensor::Device::Cpu),
        ),
        vocabulary: loaded.vocabulary,
        pool: Arc::new(
            ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
                .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
        ),
        content: ContentStore::embedded(),
        factions: FACTIONS.iter().map(|name| FactionId::new(*name)).collect(),
    };

    println!("opening reachability");
    println!("  bundle      {bundle_path}");
    println!(
        "  sample      {seeds} seeds x {} rotations, one round",
        FACTIONS.len()
    );
    println!("  search      {attempts} replays per failed seat at temperature {temperature}");
    println!("  note        a clearing line proves the position was clearable; not finding one");
    println!("              proves only that this search did not, so the result is a lower bound");

    let mut tallies: BTreeMap<String, Reach> = BTreeMap::new();
    let started = std::time::Instant::now();

    for seed in seed_base..seed_base + seeds {
        for rotation in 0..FACTIONS.len() {
            let baseline = play(&setup, seed, rotation, None);
            let failed: Vec<(PlayerId, FactionId)> = baseline
                .iter()
                .filter(|(_, (_, cleared))| !cleared)
                .map(|(player, (faction, _))| (player.clone(), faction.clone()))
                .collect();
            for (player, (faction, cleared)) in &baseline {
                let tally = tallies.entry(faction.to_string()).or_default();
                tally.seats += 1;
                if *cleared {
                    tally.cleared += 1;
                } else {
                    tally.failed += 1;
                }
                let _ = player;
            }

            for (player, faction) in failed {
                let mut recovered = None;
                for attempt in 0..attempts {
                    // A stream disjoint from every ordinary one, so a "hot" replay is a different
                    // sample rather than the same one at a different temperature.
                    let stream = seed
                        .wrapping_mul(7_777_777)
                        .wrapping_add(u64::try_from(attempt).unwrap_or(0))
                        .wrapping_add(0x5EED_0000_0000);
                    let outcome =
                        play(&setup, seed, rotation, Some((&player, temperature, stream)));
                    if outcome.get(&player).is_some_and(|(_, cleared)| *cleared) {
                        recovered = Some(attempt + 1);
                        break;
                    }
                }
                if let Some(spent) = recovered {
                    let tally = tallies.entry(faction.to_string()).or_default();
                    tally.recoverable += 1;
                    tally.attempts_to_first += spent;
                }
            }
        }
    }

    report(&tallies, started.elapsed().as_secs_f64());
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

fn report(tallies: &BTreeMap<String, Reach>, seconds: f64) {
    println!();
    println!(
        "  {:<10} {:>8} {:>9} {:>8} {:>13} {:>12} {:>10}",
        "faction", "seats", "cleared", "failed", "recoverable", "at least", "tries"
    );
    let mut total = Reach::default();
    for (faction, tally) in tallies {
        total.seats += tally.seats;
        total.cleared += tally.cleared;
        total.failed += tally.failed;
        total.recoverable += tally.recoverable;
        total.attempts_to_first += tally.attempts_to_first;
        println!(
            "  {:<10} {:>8} {:>8.1}% {:>8} {:>7} {:>4.0}% {:>11.1}% {:>10.1}",
            faction,
            tally.seats,
            share(tally.cleared, tally.seats),
            tally.failed,
            tally.recoverable,
            share(tally.recoverable, tally.failed),
            share(tally.cleared + tally.recoverable, tally.seats),
            mean(tally.attempts_to_first, tally.recoverable),
        );
    }
    println!(
        "  {:<10} {:>8} {:>8.1}% {:>8} {:>7} {:>4.0}% {:>11.1}% {:>10.1}",
        "table",
        total.seats,
        share(total.cleared, total.seats),
        total.failed,
        total.recoverable,
        share(total.recoverable, total.failed),
        share(total.cleared + total.recoverable, total.seats),
        mean(total.attempts_to_first, total.recoverable),
    );
    println!();
    println!(
        "  \"at least\" is the clearance a perfect chooser would reach among the lines this search\n  \
         actually found. The true achievable rate is at least that and may be higher."
    );
    println!("  wall time   {seconds:.1}s");
}
