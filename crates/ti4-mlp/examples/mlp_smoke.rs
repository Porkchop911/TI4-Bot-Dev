//! Can the MLP play a game? — the §7.1 legality smoke, at its smallest.
//!
//! Six MLP bots, one real six-player game, every decision scored by the actor against the
//! M09-024b2 vocabulary. The weights are zero, so the policy is uniform over each legal set; that
//! is the point. This proves the chain — engine choice → bound observation → projected features →
//! dense columns → trunk → readout → softmax → a legal answer — actually connects, before anything
//! is trained and before there is a checkpoint format to load.
//!
//! # What the coverage number is, and is not
//!
//! By default this runs a **discovery-regression** smoke: the seed is inside M09-024b2's own
//! discovery range on the same training pool and extractor, so 100% vocabulary coverage is expected
//! *by construction* and a shortfall would mean discovery or the projection had regressed. It is
//! not independent coverage. `--seed` outside `202_608_210..202_608_338`, or a different pool,
//! measures something else and is labelled as such in the output.
//!
//! ```text
//! cargo run --release -p ti4-mlp --example mlp_smoke -- \
//!     --slots out/vocabulary/slots.json \
//!     --map-pool out/pools/full_np8_12_train.json
//! ```

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use ti4_content::ContentStore;
use ti4_engine::choice::{Decider, SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::setup::start_game_seeded;
use ti4_mlp::bot::MlpBot;
use ti4_mlp::{Actor, FactionRow, Width};
use ti4_model::content_types::DEFAULT;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::vocabulary::Vocabulary;

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

#[expect(
    clippy::too_many_lines,
    reason = "a linear smoke script: it reads in the order the game is set up and played"
)]
fn main() {
    let content = ContentStore::embedded();
    let slots = argument("--slots").unwrap_or_else(|| "out/vocabulary/slots.json".to_owned());
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_train.json".to_owned());
    let rounds: u32 = argument("--rounds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    // Proves the fail-closed path is real rather than asserted: one bot is given a temperature the
    // actor must refuse, so every one of its decisions becomes a counted fallback and the run must
    // exit non-zero (F-M09-026-3).
    let force_failure = std::env::args().any(|a| a == "--force-inference-failure");
    let seed: u64 = argument("--seed")
        .and_then(|v| v.parse().ok())
        .unwrap_or(202_608_210);

    ti4_tensor::configure_deterministic(i64::try_from(seed).unwrap_or(i64::MAX))
        .expect("deterministic configuration");

    let text = std::fs::read_to_string(&slots).expect("read slots.json");
    let vocabulary = Vocabulary::from_json(&text).expect("slots.json is a valid vocabulary");
    println!(
        "vocabulary: {} slots, V_cap {}, registry v{}",
        vocabulary.slot_count(),
        vocabulary.capacity(),
        vocabulary.oov_registry_version()
    );

    let capacity = i64::try_from(vocabulary.capacity()).expect("capacity fits");
    let backend = ti4_tensor::backend();
    println!(
        "backend: cuda {} · intra-op {} · width 256 · {} heads · 33 seats",
        backend.cuda,
        backend.intra_op_threads,
        ti4_mlp::heads().len()
    );

    let players: Vec<PlayerId> = (0..FACTIONS.len())
        .map(|index| PlayerId::new(format!("seat{index}")))
        .collect();
    let factions: BTreeMap<PlayerId, FactionId> = players
        .iter()
        .enumerate()
        .map(|(index, player)| (player.clone(), FactionId::new(FACTIONS[index])))
        .collect();

    let mut state = start_game_seeded(content, &players, DEFAULT, None, seed).expect("setup");
    for (player, faction) in &factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }
    ti4_engine::promissory::deal(&mut state, content, DEFAULT);
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"));
    let homes: Vec<String> = players
        .iter()
        .map(|player| {
            ti4_content::factions::get(content, factions[player].as_str())
                .and_then(|f| f.home_system())
                .expect("home")
                .to_owned()
        })
        .collect();
    let borrowed: Vec<&str> = homes.iter().map(String::as_str).collect();
    let galaxy = pool
        .galaxy(
            content,
            DEFAULT,
            seed.wrapping_add(TILE_SEED_OFFSET),
            &borrowed,
        )
        .expect("galaxy");
    for (player, faction) in &factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, DEFAULT).expect("deploy");
    }

    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    let mut counters = Vec::new();
    for (index, player) in players.iter().enumerate() {
        // The conditioning key is the **faction identity**, resolved through the pinned roster —
        // never the physical seat index, which changes with rotation (F-M09-026-2).
        let identity = FactionRow::of(factions[player].as_str())
            .expect("every seated faction is in the roster");
        let mut bot = MlpBot::new(
            Actor::zeros(Width::W256, capacity, 33),
            Vocabulary::from_json(&text).expect("vocabulary"),
            identity,
            seed.wrapping_mul(1_000_003)
                .wrapping_add(u64::try_from(index).unwrap_or(0)),
        );
        if force_failure && index == 0 {
            bot = bot.at_temperature(0.0);
        }
        counters.push(Arc::clone(&bot.counters));
        table.seat(player.clone(), Box::new(bot) as Box<dyn Decider>);
    }

    let mut game = Game::with_table(state, content, table)
        .with_sources(DEFAULT)
        .with_galaxy(galaxy);

    let started = std::time::Instant::now();
    let start_round = game.state.round;
    let target = start_round.saturating_add(rounds);
    let mut steps = 0usize;
    let mut resolved = 0usize;
    while !game.state.finished && game.state.round < target {
        let result = game.step();
        if let Some(error) = &result.error {
            eprintln!("game died at step {steps}: {error}");
            std::process::exit(2);
        }
        if result.resolved_choice {
            resolved += 1;
        }
        steps += 1;
        if steps > 500_000 {
            eprintln!("step bound hit");
            std::process::exit(3);
        }
    }

    // The round *state* is not the number of rounds played: the game starts at round 1, so a
    // four-round horizon ends with the counter reading 5. Reporting the counter as "played 5
    // rounds" overstated it by one (F-M09-026-6).
    let completed = game.state.round.saturating_sub(start_round);
    println!(
        "
completed {completed} of {rounds} rounds (round state {start_round} -> {}): {steps} steps, {resolved} resolved choices, {:.1?}",
        game.state.round,
        started.elapsed()
    );
    println!("finished: {}", game.state.finished);
    let decisions: usize = counters
        .iter()
        .map(|c| c.decisions.load(Ordering::Relaxed))
        .sum();
    let assigned: usize = counters
        .iter()
        .map(|c| c.assigned.load(Ordering::Relaxed))
        .sum();
    let oov: usize = counters.iter().map(|c| c.oov.load(Ordering::Relaxed)).sum();
    let looked_up = assigned + oov;
    #[expect(clippy::cast_precision_loss, reason = "reporting only")]
    let coverage = if looked_up == 0 {
        0.0
    } else {
        100.0 * (assigned as f64) / (looked_up as f64)
    };
    let fallbacks: usize = counters
        .iter()
        .map(|c| c.fallbacks.load(Ordering::Relaxed))
        .sum();
    let inside_discovery = (202_608_210..202_608_338).contains(&seed);
    println!(
        "model answered {decisions} decisions, {fallbacks} fallbacks; {looked_up} feature lookups, {coverage:.2}% assigned, {oov} OOV"
    );
    println!(
        "coverage reading: {}",
        if inside_discovery {
            "discovery-regression (seed inside M09-024b2's discovery range: 100% is expected by construction; a shortfall means discovery or the projection regressed)"
        } else {
            "independent (this seed is outside the discovery range)"
        }
    );

    // Fail closed. A run in which the model never answered, or fell back even once, is not a
    // successful smoke — it is a failure that happens to have produced legal moves.
    let mut refusals: Vec<String> = Vec::new();
    if fallbacks != 0 {
        refusals.push(format!("{fallbacks} inference fallbacks"));
    }
    if decisions == 0 {
        refusals.push("the model answered no decisions".to_owned());
    }
    if looked_up == 0 {
        refusals.push("no feature lookups were made".to_owned());
    }
    if completed != rounds && !game.state.finished {
        refusals.push(format!("completed {completed} of {rounds} rounds"));
    }
    if !refusals.is_empty() {
        eprintln!("SMOKE FAILED: {}", refusals.join("; "));
        std::process::exit(4);
    }

    for player in &players {
        let seat = game.state.player(player).expect("seated");
        println!(
            "  {:<6} {:<8} {:>2} VP",
            player.as_str(),
            seat.faction.as_str(),
            seat.victory_points
        );
    }
}
