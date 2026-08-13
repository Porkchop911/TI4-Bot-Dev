//! Does Stage 1 from blank weights actually learn anything?
//!
//! Trains on the Stage-1 return and evaluates on held-out seeds against an untrained policy, on
//! the three facts Stage 1 is *about* — planets gained, systems held, units gained — and whether
//! the opening bar was cleared. Victory points are not the question here; the whole reason Stage 1
//! exists is that they are too noisy this early to select on.
//!
//! `cargo run -p ti4-training --example stage1_check --release`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;
use ti4_policy::learned::Profile;
use ti4_training::reward::Stage;
use ti4_training::rollout::{Horizon, play_batch};
use ti4_training::stage1::{Plan, train};

/// Mean planets gained, systems, units gained, and the fraction of seats clearing the bar.
fn evaluate(
    profiles: &BTreeMap<PlayerId, Profile>,
    players: &[PlayerId],
    from: u64,
    games: u64,
) -> (f64, f64, f64, f64) {
    let seeds: Vec<u64> = (from..from + games).collect();
    let rollouts = play_batch(
        ContentStore::embedded(),
        players,
        profiles,
        DEFAULT,
        &seeds,
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
    );
    let (mut planets, mut systems, mut units, mut cleared, mut seats) =
        (0i64, 0i64, 0i64, 0i64, 0i64);
    for rollout in &rollouts {
        for seat in &rollout.seats {
            seats += 1;
            let final_progress = seat.episode.final_progress;
            planets += final_progress.planets_gained;
            systems += final_progress.systems;
            units += final_progress.units_gained;
            cleared += i64::from(seat.episode.cleared);
        }
    }
    let n = f64::from(i32::try_from(seats.max(1)).unwrap_or(1));
    let as_float = |v: i64| f64::from(i32::try_from(v).unwrap_or(0));
    (
        as_float(planets) / n,
        as_float(systems) / n,
        as_float(units) / n,
        as_float(cleared) / n,
    )
}

fn main() {
    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();
    let generations: usize = std::env::args()
        .position(|a| a == "--generations")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(20);

    let plan = Plan {
        stage: Stage::One,
        players: players.clone(),
        generations,
        games: 8,
        seed: 0,
        ..Plan::smoke(Stage::One)
    };

    let started = std::time::Instant::now();
    let run = train(ContentStore::embedded(), &plan);
    println!(
        "stage 1: {} generations x {} games in {:.1}s",
        plan.generations,
        plan.games,
        started.elapsed().as_secs_f64()
    );
    let moved: f64 = run
        .generations
        .iter()
        .map(ti4_training::stage1::Generation::movement)
        .sum();
    let spread: f64 = run
        .generations
        .iter()
        .map(ti4_training::stage1::Generation::best_spread)
        .fold(0.0, f64::max);
    println!("  total weight movement {moved:.3}, best return spread {spread:.3}");

    let held = 20_000u64;
    let games = 40u64;
    let blank: BTreeMap<PlayerId, Profile> = BTreeMap::new();
    let (bp, bs, bu, bc) = evaluate(&blank, &players, held, games);
    let (tp, ts, tu, tc) = evaluate(&run.profiles, &players, held, games);

    println!(
        "\non {games} held-out openings ({} seat-games), per seat:",
        games * 6
    );
    println!("                     blank   trained");
    println!("  planets gained    {bp:>6.3}   {tp:>6.3}");
    println!("  systems held      {bs:>6.3}   {ts:>6.3}");
    println!("  units gained      {bu:>6.3}   {tu:>6.3}");
    println!("  cleared the bar   {bc:>6.3}   {tc:>6.3}");
}
