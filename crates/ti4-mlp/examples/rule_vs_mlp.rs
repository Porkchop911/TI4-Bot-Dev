//! The hand-written rule bot against the learned policy at one table: three seats each.
//!
//! Seats alternate rule / MLP, and every seed is played twice with the two sides swapped, so each
//! seat (its faction, its slice of the map, its place in the order) is held once by either kind.
//! The difference between the two games of a seed is therefore the kinds, not the seats.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example rule_vs_mlp -- \
//!   --bundle out/<run>/checkpoints/checkpoint-N --roster wide --diplomacy --seeds 24 --rounds 4
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;

use rayon::prelude::*;
use ti4_content::ContentStore;
use ti4_engine::choice::Decider;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};

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

fn refuse(reason: &str) -> ! {
    eprintln!("\nREFUSED: {reason}");
    std::process::exit(2);
}

fn number<T: std::str::FromStr>(flag: &str, fallback: T) -> T {
    argument(flag).map_or(fallback, |value| {
        value
            .parse()
            .unwrap_or_else(|_| refuse(&format!("{flag} expects a number")))
    })
}

/// One seat of one game.
struct Row {
    seed: u64,
    swap: bool,
    seat: usize,
    rule: bool,
    faction: String,
    vp: i64,
    cleared: bool,
    won: bool,
}

#[expect(clippy::cast_precision_loss, reason = "counts and points are small")]
fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.iter().sum::<f64>() / values.len() as f64
}

#[allow(clippy::cast_precision_loss, reason = "counts are small")]
fn standard_error(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return f64::NAN;
    }
    let m = mean(values);
    let variance = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    (variance / values.len() as f64).sqrt()
}

#[expect(
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    reason = "one measurement: the games and the table they print belong together"
)]
fn main() {
    let bundle_path = argument("--bundle").unwrap_or_else(|| refuse("--bundle is required"));
    let seeds: u64 = number("--seeds", 24);
    let seed_base: u64 = number("--seed-base", 900_000_000);
    let rounds: u32 = number("--rounds", 4);
    let temperature: f64 = number("--temperature", 0.001);
    let max_steps: usize = number("--max-steps", 40_000);
    let diplomacy = std::env::args().any(|a| a == "--diplomacy");
    let roster = argument("--roster").map_or(ti4_engine::seating::FactionRoster::InScope, |value| {
        ti4_training::rollout::parse_roster(&value).unwrap_or_else(|error| refuse(&error))
    });
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    let per_seat = argument("--per-seat");

    ti4_tensor::configure_deterministic(20_260_826)
        .unwrap_or_else(|error| refuse(&format!("configuring the backend: {error}")));
    let content = ContentStore::embedded();
    let loaded = ti4_mlp::bundle::read(std::path::Path::new(&bundle_path))
        .unwrap_or_else(|error| refuse(&format!("reading {bundle_path}: {error}")));
    let vocabulary = loaded.vocabulary;
    let pool = Arc::new(
        ti4_sim::MapPool::from_reader(std::io::Cursor::new(
            ti4_sim::artifacts::read_and_verify_pool_role(
                std::path::Path::new(&pool_path),
                &[ti4_sim::artifacts::ArtifactRole::Validation],
            )
            .unwrap_or_else(|error| refuse(&format!("{pool_path}: {error}"))),
        ))
        .unwrap_or_else(|error| refuse(&format!("parsing the pool: {error}"))),
    );
    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();

    println!("rule bot against MLP, three seats each");
    println!("  mlp         {bundle_path} at temperature {temperature}");
    println!("  roster      {roster:?}, diplomacy {diplomacy}, {rounds} rounds");
    println!("  maps        {pool_path} (Validation)");
    println!("  games       {seeds} seeds x 2 (sides swapped) from seed {seed_base}\n");

    let jobs: Vec<(u64, bool)> = (seed_base..seed_base + seeds)
        .flat_map(|seed| [(seed, false), (seed, true)])
        .collect();
    let workers = rayon::current_num_threads().max(1);
    let per_worker = jobs.len().div_ceil(workers).max(1);
    let chunks: Vec<(ti4_mlp::Actor, Vec<(u64, bool)>)> = jobs
        .chunks(per_worker)
        .map(|chunk| (loaded.actor.inference_copy(), chunk.to_vec()))
        .collect();

    let started = std::time::Instant::now();
    let harvest: Vec<(Vec<Row>, Vec<String>)> = chunks
        .into_par_iter()
        .map(|(local, chunk)| {
            let local = std::rc::Rc::new(local);
            let (mut rows, mut failures) = (Vec::new(), Vec::new());
            for (seed, swap) in chunk {
                let is_rule = |index: usize| index.is_multiple_of(2) != swap;
                let factions = match ti4_training::rollout::game_factions(
                    content, roster, &FACTIONS, &players, DEFAULT, seed,
                ) {
                    Ok(factions) => factions,
                    Err(error) => {
                        failures.push(format!("{seed}/{swap}: {error}"));
                        continue;
                    }
                };
                let seated: BTreeMap<PlayerId, FactionId> = players
                    .iter()
                    .enumerate()
                    .map(|(index, player)| {
                        (
                            player.clone(),
                            ti4_training::rollout::seated_faction(&factions, seed, 0, index),
                        )
                    })
                    .collect();
                let (rollout, _) =
                    ti4_training::rollout::play_with_capabilities_and_decider_factory_digest(
                        content,
                        &players,
                        &seated,
                        DEFAULT,
                        seed,
                        ti4_training::rollout::Horizon {
                            rounds,
                            steps: max_steps,
                        },
                        ti4_engine::opening::DEFAULT_REQUIREMENT,
                        &ti4_training::rollout::OpeningMap::PythonPool {
                            pool: Arc::clone(&pool),
                            tile_seed_offset: 0,
                        },
                        ti4_training::rollout::SimulationCapabilities { diplomacy },
                        false,
                        |baselines| {
                            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> =
                                BTreeMap::new();
                            for (index, player) in players.iter().enumerate() {
                                if is_rule(index) {
                                    deciders.insert(
                                        player.clone(),
                                        Box::new(
                                            ti4_server::session::random_bot::RuleBotDecider::new(
                                                player.clone(),
                                                seed,
                                            ),
                                        ),
                                    );
                                    continue;
                                }
                                let row = ti4_mlp::FactionRow::of(seated[player].as_str())
                                    .map_err(|error| format!("{player}: {error}"))?;
                                let baseline = baselines
                                    .get(player)
                                    .copied()
                                    .ok_or_else(|| format!("{player} has no setup baseline"))?;
                                let stream = seed
                                    .wrapping_mul(1_000_003)
                                    .wrapping_add(u64::try_from(index).unwrap_or(0));
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
                    // A game that stopped is reported, never mixed in: its points describe a
                    // shorter game than the others.
                    failures.push(format!("{seed}/{swap}: {error}"));
                    continue;
                }
                let best = rollout
                    .seats
                    .iter()
                    .map(|seat| seat.episode.final_progress.victory_points)
                    .max()
                    .unwrap_or(0);
                let leaders = rollout
                    .seats
                    .iter()
                    .filter(|seat| seat.episode.final_progress.victory_points == best)
                    .count();
                for (index, seat) in rollout.seats.iter().enumerate() {
                    let vp = seat.episode.final_progress.victory_points;
                    rows.push(Row {
                        seed,
                        swap,
                        seat: index,
                        rule: is_rule(index),
                        faction: seat.faction.to_string(),
                        vp,
                        cleared: seat.episode.cleared,
                        // Ties are not wins.
                        won: vp == best && leaders == 1,
                    });
                }
            }
            (rows, failures)
        })
        .collect();

    let mut rows: Vec<Row> = Vec::new();
    let mut failures: Vec<String> = Vec::new();
    for (chunk, failed) in harvest {
        rows.extend(chunk);
        failures.extend(failed);
    }
    rows.sort_by_key(|row| (row.seed, row.swap, row.seat));
    let games = rows.len() / FACTIONS.len();

    println!("  kind   seats   mean VP   cleared   games won");
    for (name, rule) in [("rule", true), ("mlp", false)] {
        let mine: Vec<&Row> = rows.iter().filter(|row| row.rule == rule).collect();
        let vp: Vec<f64> = mine.iter().map(|row| row.vp as f64).collect();
        let cleared = mine.iter().filter(|row| row.cleared).count();
        let won = mine.iter().filter(|row| row.won).count();
        println!(
            "  {name:<5} {:>6}   {:>6.3}   {:>6.1}%   {won:>4} of {games}",
            mine.len(),
            mean(&vp),
            100.0 * cleared as f64 / mine.len().max(1) as f64,
        );
    }
    let tied = games - rows.iter().filter(|row| row.won).count();
    println!("  games with no single leader: {tied}");

    // Paired by seed: once both swaps are in, each kind has held every seat of that map once, so
    // a seed's mean difference has the seats, factions and map cancelled out.
    let mut by_seed: BTreeMap<u64, (Vec<f64>, Vec<f64>, [bool; 2])> = BTreeMap::new();
    for row in &rows {
        let entry = by_seed.entry(row.seed).or_default();
        if row.rule {
            entry.0.push(row.vp as f64);
        } else {
            entry.1.push(row.vp as f64);
        }
        entry.2[usize::from(row.swap)] = true;
    }
    let paired: Vec<f64> = by_seed
        .values()
        .filter(|(_, _, both)| both[0] && both[1])
        .map(|(rule, mlp, _)| mean(mlp) - mean(rule))
        .collect();
    println!(
        "\n  MLP minus rule, VP per seat, paired over {} complete seeds: {:+.3} +- {:.3} (s.e.)",
        paired.len(),
        mean(&paired),
        standard_error(&paired)
    );
    println!(
        "  {games} games measured, {} failed, in {:.1?}",
        failures.len(),
        started.elapsed()
    );
    for failure in &failures {
        println!("  FAILED {failure}");
    }

    if let Some(path) = per_seat {
        let mut body = String::from("seed swap seat kind faction victory_points cleared won\n");
        for row in &rows {
            body.push_str(&format!(
                "{} {} {} {} {} {} {} {}\n",
                row.seed,
                u8::from(row.swap),
                row.seat,
                if row.rule { "rule" } else { "mlp" },
                row.faction,
                row.vp,
                u8::from(row.cleared),
                u8::from(row.won)
            ));
        }
        std::fs::write(&path, body)
            .unwrap_or_else(|error| refuse(&format!("writing {path}: {error}")));
        println!("  wrote {} seat-games to {path}", rows.len());
    }
}
