//! Stage-1 clearance rate against training updates, from blank, six players.
//!
//! The oracle reports this taking several hundred updates, so a short run says nothing. Training
//! continues through the resume path rather than restarting, so the seed schedule keeps moving and
//! no update re-trains on games an earlier one already learned from.
//!
//! `cargo run -p ti4-training --example stage1_curve --release [-- --updates 600 --every 25]`

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::DEFAULT;
use ti4_model::id::PlayerId;
use ti4_policy::learned::Profile;
use ti4_training::reward::Stage;
use ti4_training::rollout::{Horizon, play_batch};
use ti4_training::stage1::{Plan, Start, train};

fn number(name: &str, fallback: usize) -> usize {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(fallback)
}

/// Clearance and its three components, on held-out openings.
fn evaluate(
    profiles: &BTreeMap<PlayerId, Profile>,
    players: &[PlayerId],
    games: u64,
) -> (f64, f64, f64, f64) {
    let seeds: Vec<u64> = (20_000..20_000 + games).collect();
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
    for seat in rollouts.iter().flat_map(|r| r.seats.iter()) {
        seats += 1;
        let p = seat.episode.final_progress;
        planets += p.planets_gained;
        systems += p.systems;
        units += p.units_gained;
        cleared += i64::from(seat.episode.cleared);
    }
    let n = f64::from(i32::try_from(seats.max(1)).unwrap_or(1));
    let f = |v: i64| f64::from(i32::try_from(v).unwrap_or(0));
    (f(cleared) / n, f(planets) / n, f(systems) / n, f(units) / n)
}

fn main() {
    let updates = number("--updates", 600);
    let every = number("--every", 25);
    let games = number("--games", 8) as u64;
    let evaluation_games = number("--eval", 40) as u64;

    let players: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|n| PlayerId::new(*n))
        .collect();

    let base = Plan {
        stage: Stage::One,
        players: players.clone(),
        generations: every,
        games,
        seed: 0,
        ..Plan::smoke(Stage::One)
    };

    println!(
        "stage 1 from blank, {} seats, {games} games/update, evaluated on {evaluation_games} held-out openings ({} seat-games)",
        players.len(),
        evaluation_games * players.len() as u64
    );
    println!("\n updates | clearance | planets | systems |  units | movement");
    println!("---------|-----------|---------|---------|--------|---------");

    let blank: BTreeMap<PlayerId, Profile> = BTreeMap::new();
    let (c, p, s, u) = evaluate(&blank, &players, evaluation_games);
    println!("       0 |    {c:>5.3}  |  {p:>5.3}  |  {s:>5.3}  | {u:>5.3}  |    -");

    let mut profiles: Option<BTreeMap<PlayerId, Profile>> = None;
    let mut done = 0usize;
    let started = std::time::Instant::now();
    while done < updates {
        let plan = match &profiles {
            None => base.clone(),
            Some(fitted) => base.clone().resuming(Start {
                profiles: fitted.clone(),
                generation: done,
            }),
        };
        let run = train(ContentStore::embedded(), &plan);
        let movement: f64 = run
            .generations
            .iter()
            .map(ti4_training::stage1::Generation::movement)
            .sum();
        done += every;
        let (c, p, s, u) = evaluate(&run.profiles, &players, evaluation_games);
        println!(
            "  {done:>6} |    {c:>5.3}  |  {p:>5.3}  |  {s:>5.3}  | {u:>5.3}  |  {movement:>6.2}"
        );
        profiles = Some(run.profiles);
    }
    println!(
        "\n{updates} updates in {:.0}s",
        started.elapsed().as_secs_f64()
    );
}
