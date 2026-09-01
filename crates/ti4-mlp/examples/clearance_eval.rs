//! Measure a bundle's stage-1 opening clearance, at a fixed temperature, on held-out maps.
//!
//! # Why this exists
//!
//! `ppo_update` prints a clearance table, and that table is **training data**: it is tallied from
//! the self-play rollouts the update was computed from, on the training pool, at the training
//! temperature. Three properties make it useless for comparing runs against each other.
//!
//! - It is sampled at whatever temperature the run trains at. A policy scores higher at 0.25 than
//!   at 1.0 because the softmax is sharper, so a 1.0 run and a 0.25 run reporting the same figure
//!   are not equally good. A temperature sweep read off those tables compares the measuring
//!   instrument, not the policies.
//! - It is measured on `full_np8_12_train.json`, the maps the policy is being fitted to.
//! - It moves with the batch: sixteen seeds, in a window ten updates wide.
//!
//! This tool fixes all three: one temperature chosen by the operator regardless of how the bundle
//! was trained, the **Validation** pool, and a seed range the caller holds constant across every
//! bundle it compares.
//!
//! # What it measures
//!
//! Exactly what `ppo_update` tallies -- `episode.cleared` and
//! `episode.final_progress.victory_points` from `play_with_decider_factory`, per seat, against
//! `opening::DEFAULT_REQUIREMENT`. It shares that path deliberately: a re-implementation of the bar
//! would be a second definition of the thing being measured, and the two would drift.
//!
//! Bots are seated **without** `recording_ppo`. Recording stores the sparse features of every legal
//! option at every decision, which is what the importance ratio needs and what made a
//! non-progressing game allocate 53 GB. An evaluation needs none of it.
//!
//! # Usage
//!
//! ```text
//! cargo run --release -p ti4-mlp --example clearance_eval -- \
//!   --bundle out/checkpoints/run-028/checkpoint-60672 --temperature 0.25 --seeds 400
//! ```
//!
//! Inference is CPU-only under 7.1, so this needs no GPU and can run beside a training job.

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

/// The six trained factions, in the fixed order every rotation is taken against.
const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];

/// The offset `ppo_update` uses when drawing a map from the pool. Evaluation must use the same one
/// or it is scoring a different distribution of maps.
const TILE_SEED_OFFSET: u64 = 0;

fn argument(name: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == name {
            return args.next();
        }
    }
    None
}

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

/// One faction's running totals.
#[derive(Default, Clone, Copy)]
struct Tally {
    seats: usize,
    cleared: usize,
    victory_points: f64,
}

impl Tally {
    #[expect(
        clippy::cast_precision_loss,
        reason = "counts here are at most a few hundred thousand"
    )]
    fn clearance(self) -> f64 {
        if self.seats == 0 {
            return 0.0;
        }
        self.cleared as f64 / self.seats as f64 * 100.0
    }

    #[expect(clippy::cast_precision_loss, reason = "as above")]
    fn mean_vp(self) -> f64 {
        if self.seats == 0 {
            return 0.0;
        }
        self.victory_points / self.seats as f64
    }
}

/// The 95% confidence half-width of a clearance figure, in percentage points.
///
/// A sweep is a comparison, and a comparison without an interval invites reading a two-point gap as
/// a result when the sample cannot support it. This is the normal approximation to a binomial
/// proportion: sound at these counts and rates, and **not** sound near 0% or 100%, where a run that
/// has collapsed reports a meaningless interval -- though the collapse is the finding in that case.
///
/// It treats seat-games as independent. They are not quite, because six seats share a map, so this
/// is a lower bound on the true width. A gap no larger than the interval is not a gap.
#[expect(clippy::cast_precision_loss, reason = "counts are small")]
fn half_width(tally: Tally) -> f64 {
    if tally.seats == 0 {
        return 0.0;
    }
    let p = tally.clearance() / 100.0;
    1.96 * (p * (1.0 - p) / tally.seats as f64).sqrt() * 100.0
}

#[expect(
    clippy::too_many_lines,
    reason = "one measurement: the rollout and the table it prints belong together"
)]
fn main() {
    let bundle_path = argument("--bundle")
        .unwrap_or_else(|| refuse("--bundle is required: this measures a policy"));
    let temperature: f64 = argument("--temperature").map_or(0.25, |value| {
        value
            .parse::<f64>()
            .ok()
            .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
            .unwrap_or_else(|| refuse("--temperature must be a positive number"))
    });
    let seeds: u64 = argument("--seeds").map_or(400, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seeds must be a number"))
    });
    // Deliberately far from any training range. Runs so far have consumed 650000000 upward, so
    // starting here makes an accidental overlap impossible rather than merely unlikely.
    let seed_base: u64 = argument("--seed-base").map_or(900_000_000, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--seed-base must be a number"))
    });
    let rounds: u32 = argument("--rounds").map_or(1, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse("--rounds must be a number"))
    });

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();

    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let actor = loaded.actor;

    // The Validation pool, and the role guard is the point: passing the training pool here would
    // produce a number that looks like the others and answers a different question.
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let pool_bytes = ti4_sim::artifacts::read_and_verify_pool_role(
        std::path::Path::new(&pool_path),
        &[ti4_sim::artifacts::ArtifactRole::Validation],
    )
    .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}")));
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(&pool_bytes))
            .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );

    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();

    println!("stage-1 clearance for {bundle_path}");
    println!("  temperature {temperature}");
    println!("  maps        {pool_path} (Validation)");
    println!(
        "  seeds       {seed_base}..{} x {} rotations",
        seed_base + seeds,
        FACTIONS.len()
    );
    println!();

    // One owned actor per worker: `tch::Tensor` is `Send` but not `Sync`, so the actor cannot be
    // borrowed across threads, and a copy per job would allocate one per game instead of one per
    // core.
    let jobs: Vec<(u64, usize)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| (0..FACTIONS.len()).map(move |rotation| (seed, rotation)))
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers);
    let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, usize)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| (actor.inference_copy(), chunk.to_vec()))
        .collect();

    let started = std::time::Instant::now();
    let harvest: Vec<Result<Vec<(String, bool, f64)>, String>> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = std::rc::Rc::new(local);
            let mut seats = Vec::new();
            for (seed, rotation) in chunk {
                let seated: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(index, player)| {
                        (
                            player.clone(),
                            FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]),
                        )
                    })
                    .collect();
                let rollout = ti4_training::rollout::play_with_decider_factory(
                    content,
                    &players,
                    &seated,
                    DEFAULT,
                    seed,
                    ti4_training::rollout::Horizon {
                        rounds,
                        steps: 10_000,
                    },
                    ti4_engine::opening::DEFAULT_REQUIREMENT,
                    &ti4_training::rollout::OpeningMap::PythonPool {
                        pool: Arc::clone(&pool),
                        tile_seed_offset: TILE_SEED_OFFSET,
                    },
                    |baselines| {
                        let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
                        for (index, player) in players.iter().enumerate() {
                            let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                                .map_err(|error| format!("{player}: {error}"))?;
                            let baseline = baselines
                                .get(player)
                                .copied()
                                .ok_or_else(|| format!("{player} has no setup baseline"))?;
                            let stream = seed
                                .wrapping_mul(1_000_003)
                                .wrapping_add(u64::try_from(index).unwrap_or(0));
                            // No `recording_ppo`: an evaluation needs no importance-ratio data, and
                            // recording it allocates the sparse features of every legal option at
                            // every decision.
                            let (decider, _status) = ti4_mlp::bot::MlpBot::sharing(
                                &local,
                                vocabulary.clone(),
                                row,
                                stream,
                            )
                            .at_temperature(temperature)
                            .from_setup(baseline)
                            .seat();
                            deciders.insert(player.clone(), decider);
                        }
                        Ok(deciders)
                    },
                );
                if let Some(error) = &rollout.error {
                    return Err(format!("game {seed}/{rotation} failed: {error}"));
                }
                for seat in &rollout.seats {
                    seats.push((
                        seat.faction.to_string(),
                        seat.episode.cleared,
                        // Victory points are a small non-negative count, so the widening is
                        // exact; `f64::from` does not accept `i64` and a lossless conversion is
                        // what is wanted here rather than a saturating one.
                        #[expect(
                            clippy::cast_precision_loss,
                            reason = "victory points are single digits"
                        )]
                        {
                            seat.episode.final_progress.victory_points as f64
                        },
                    ));
                }
            }
            Ok(seats)
        })
        .collect();

    let mut tallies: BTreeMap<String, Tally> = BTreeMap::new();
    for chunk in harvest {
        for (faction, cleared, points) in chunk.unwrap_or_else(|error| refuse(&error)) {
            let entry = tallies.entry(faction).or_default();
            entry.seats += 1;
            entry.cleared += usize::from(cleared);
            entry.victory_points += points;
        }
    }

    println!("  faction      seats   clearance          mean VP");
    let mut table = Tally::default();
    for (faction, tally) in &tallies {
        table.seats += tally.seats;
        table.cleared += tally.cleared;
        table.victory_points += tally.victory_points;
        println!(
            "  {:<10} {:>6}   {:>6.2}% +-{:>4.2}    {:>6.3}",
            faction,
            tally.seats,
            tally.clearance(),
            half_width(*tally),
            tally.mean_vp()
        );
    }
    println!(
        "  {:<10} {:>6}   {:>6.2}% +-{:>4.2}    {:>6.3}",
        "table",
        table.seats,
        table.clearance(),
        half_width(table),
        table.mean_vp()
    );
    println!("\n  measured in {:.1?}", started.elapsed());
}
