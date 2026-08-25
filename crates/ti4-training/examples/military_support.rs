//! Does Sol's Military Support note actually leave Sol, and does Sol actually lose the token?
//!
//! `pnms:sol:*` in a trajectory is an offer *composition*, not a completed deal -- the partner
//! still has to accept -- so counting those overstates transfers. The note's position is state, so
//! this reads it: at every decision, whether `ms:sol` is held by someone other than Sol, and what
//! Sol's strategy pool holds. The note goes home immediately after it fires
//! (`promissory.rs:390`), so each observed away-spell is one firing.
//!
//! **Where the private read lives.** `ms:sol` has `playArea = false`: while it is held, its
//! position is *private* — a live decider may not see who holds another player's in-hand note, and
//! no observation accessor exposes it (F-M09-021-1 AA1). This diagnostic is offline: main drives
//! the game step by step against full state it owns, so it reads the note position from that state
//! at visible cost — one named read per decision in [`drive`] — instead of smuggling it through a
//! decider's observation. The policy side (plain `LearnedBot`s) sees nothing private.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_engine::choice::{SeededRandom, Table};
use ti4_engine::game::Game;
use ti4_engine::setup::start_game_seeded;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::Profile;

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;
/// Same per-round safety bound `Horizon::rounds` uses, so a stalled game fails loudly instead of
/// spinning.
const STEPS_PER_ROUND_BOUND: usize = 125_000;

/// What one game showed about the note.
#[derive(Default)]
struct Watch {
    /// Away-spells: transitions from Sol holding it to somebody else holding it.
    departures: usize,
    /// Whether it is away right now, so a spell is counted once.
    away: bool,
    /// Who has held it (by faction name — the old watch reported by faction).
    holders: BTreeMap<String, usize>,
    /// Sol's strategy pool at the last decision.
    tokens: i32,
    /// Drops in Sol's strategy pool observed while the note was away.
    token_drops: usize,
}

/// One decision's position-at-decision-time, read from full state before the step that resolves it.
struct DecisionSample {
    holder: Option<PlayerId>,
    /// The holder's faction name — the old watch reported by faction, so the output stays
    /// comparable across this rework.
    holder_name: Option<String>,
    tokens: i32,
}

impl Watch {
    /// Fold one sampled decision in — the same edge logic the old in-decider watch used.
    fn observe(&mut self, sample: &DecisionSample, sol: &PlayerId) {
        let away = sample.holder.as_ref().is_some_and(|who| who != sol);
        if away && !self.away {
            self.departures += 1;
            if let Some(name) = &sample.holder_name {
                *self.holders.entry(name.clone()).or_default() += 1;
            }
        }
        if away && sample.tokens < self.tokens {
            self.token_drops += 1;
        }
        self.away = away;
        self.tokens = sample.tokens;
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// The faction's home system name.
///
/// # Panics
/// If the corpus record is missing or has no home system — both are content errors, not game states.
fn home_of(content: &ContentStore, faction: &FactionId) -> String {
    ti4_content::factions::get(content, faction.as_str())
        .and_then(|record| record.home_system())
        .map(str::to_owned)
        .expect("faction has a corpus record with a home system")
}

/// Build one game with plain learned bots seated — the rollout's `PythonPool` setup path: fresh
/// state, seats named, notes re-dealt under their real owners (the setup deal ran before seating),
/// board drawn by seed.
fn build_game<'a>(
    content: &'a ContentStore,
    players: &[PlayerId],
    factions: &BTreeMap<PlayerId, FactionId>,
    seed: u64,
    pool: &ti4_sim::MapPool,
    loaded: &BTreeMap<String, Profile>,
) -> Result<Game<'a>, String> {
    let mut state = start_game_seeded(content, players, FULL, None, seed)
        .map_err(|error| format!("setup: {error}"))?;
    for (player, faction) in factions {
        if let Some(seat) = state.player_mut(player) {
            seat.faction = faction.clone();
        }
    }
    ti4_engine::promissory::deal(&mut state, content, FULL);
    let homes: Vec<String> = players
        .iter()
        .map(|player| home_of(content, &factions[player]))
        .collect();
    let borrowed: Vec<&str> = homes.iter().map(String::as_str).collect();
    let galaxy = pool
        .galaxy(
            content,
            FULL,
            seed.wrapping_add(TILE_SEED_OFFSET),
            &borrowed,
        )
        .map_err(|error| format!("pool: {error}"))?;
    for (player, faction) in factions {
        ti4_engine::seating::deploy(&mut state, content, player, faction, FULL)
            .map_err(|error| format!("deploy: {error}"))?;
    }

    // The policy side is plain learned bots — no wrapper, nothing private reaches them.
    let mut table = Table::with_default(Box::new(SeededRandom::new(seed)));
    for (index, player) in players.iter().enumerate() {
        let profile = loaded[&factions[player].to_string()].clone();
        let stream = seed
            .wrapping_mul(1_000_003)
            .wrapping_add(u64::try_from(index).unwrap_or(0));
        table.seat(
            player.clone(),
            Box::new(LearnedBot::from_shared(Arc::new(profile), stream)),
        );
    }
    Ok(Game::with_table(state, content, table)
        .with_sources(FULL)
        .with_galaxy(galaxy))
}

/// Drive one game to its round cap, sampling the note's holder at every decision.
///
/// `resolved_choice` is true exactly when the step asked a decider and applied its answer — the
/// same moments the old in-decider watch sampled — so the pre-step read is the position at
/// decision time, for every window kind (secondary windows included).
fn drive(
    game: &mut Game<'_>,
    sol: &PlayerId,
    factions: &BTreeMap<PlayerId, FactionId>,
    rounds: u32,
) -> Result<Watch, String> {
    let start_round = game.state.round;
    let target_round = start_round.saturating_add(rounds);
    let step_bound = STEPS_PER_ROUND_BOUND * usize::try_from(rounds).unwrap_or(0);
    let mut watch = Watch::default();
    let mut steps = 0usize;
    loop {
        if game.state.finished || game.state.round >= target_round {
            break;
        }
        // The one private read this diagnostic makes, at visible cost: the note's holder.
        // (Sol's strategy pool is faceup and could come from any public view.)
        let holder = game.state.promissory_notes.get("ms:sol").cloned();
        let sample = DecisionSample {
            holder_name: holder.as_ref().map(|who| {
                factions
                    .get(who)
                    .map_or_else(|| who.to_string(), ToString::to_string)
            }),
            tokens: game
                .state
                .player(sol)
                .map_or(0, |seat| seat.strategic_tokens),
            holder,
        };
        let result = game.step();
        if let Some(error) = &result.error {
            return Err(format!("game died at step {steps}: {error}"));
        }
        steps += 1;
        if steps >= step_bound {
            return Err(format!("step bound hit at step {steps}"));
        }
        if result.resolved_choice {
            watch.observe(&sample, sol);
        }
    }
    Ok(watch)
}

fn main() {
    let content = ContentStore::embedded();
    let checkpoint = argument("--checkpoint").expect("--checkpoint");
    let rounds: u32 = argument("--rounds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let seeds: u64 = argument("--seeds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let pool = std::sync::Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"),
    );

    let mut games = 0usize;
    let mut with_departure = 0usize;
    let mut departures = 0usize;
    let mut token_drops = 0usize;
    let mut holders: BTreeMap<String, usize> = BTreeMap::new();

    for seed in 98_000_000..98_000_000 + seeds {
        for rotation in 0..FACTIONS.len() {
            let players: Vec<PlayerId> = (0..FACTIONS.len())
                .map(|index| PlayerId::new(format!("seat{index}")))
                .collect();
            let mut factions = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                factions.insert(
                    player.clone(),
                    FactionId::new(FACTIONS[(index + rotation) % FACTIONS.len()]),
                );
            }
            let sol = players
                .iter()
                .find(|p| factions[*p].as_str() == "sol")
                .expect("sol is seated")
                .clone();

            let mut game = match build_game(content, &players, &factions, seed, &pool, &loaded) {
                Ok(game) => game,
                Err(error) => {
                    eprintln!("seed {seed}, rotation {rotation}: {error}");
                    continue;
                }
            };
            match drive(&mut game, &sol, &factions, rounds) {
                Ok(watch) => {
                    games += 1;
                    departures += watch.departures;
                    token_drops += watch.token_drops;
                    if watch.departures > 0 {
                        with_departure += 1;
                    }
                    for (who, count) in &watch.holders {
                        *holders.entry(who.clone()).or_default() += count;
                    }
                }
                Err(error) => eprintln!("seed {seed}, rotation {rotation}: {error}"),
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let share = |value: usize| 100.0 * value as f64 / games.max(1) as f64;
    println!("{games} games, {rounds} rounds, pool {pool_path}");
    println!("checkpoint {checkpoint}\n");
    println!("Military Support (ms:sol), read from state:");
    println!(
        "  games where it ever left Sol: {with_departure} ({:.1}%)",
        share(with_departure)
    );
    println!(
        "  total departures:             {departures} ({:.3} per game)",
        {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let per = departures as f64 / games.max(1) as f64;
            per
        }
    );
    println!("  held by: {holders:?}");
    println!("  Sol strategy-pool drops seen while it was away: {token_drops}");
}
