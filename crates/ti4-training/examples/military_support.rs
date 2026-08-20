//! Does Sol's Military Support note actually leave Sol, and does Sol actually lose the token?
//!
//! `pnms:sol:*` in a trajectory is an offer *composition*, not a completed deal -- the partner
//! still has to accept -- so counting those overstates transfers. The note's position is state, so
//! this reads it: at every decision, whether `ms:sol` is held by someone other than Sol, and what
//! Sol's strategy pool holds. The note goes home immediately after it fires
//! (`promissory.rs:390`), so each observed away-spell is one firing.
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Observed};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

/// What one game showed about the note.
#[derive(Default)]
struct Watch {
    /// Away-spells: transitions from Sol holding it to somebody else holding it.
    departures: usize,
    /// Whether it is away right now, so a spell is counted once.
    away: bool,
    /// Who has held it.
    holders: BTreeMap<String, usize>,
    /// Sol's strategy pool at the last decision Sol was asked about.
    tokens: i32,
    /// Drops in Sol's strategy pool observed while the note was away.
    token_drops: usize,
}

struct WatchBot {
    inner: LearnedBot,
    sol: PlayerId,
    watch: Rc<RefCell<Watch>>,
}

impl Decider for WatchBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let state = seen.redacted_for(&self.sol);
        let holder = state.promissory_notes.get("ms:sol").cloned();
        let tokens = state
            .player(&self.sol)
            .map_or(0, |seat| seat.strategic_tokens);
        {
            let mut watch = self.watch.borrow_mut();
            let away = holder.as_ref().is_some_and(|who| who != &self.sol);
            if away && !watch.away {
                watch.departures += 1;
                if let Some(who) = &holder {
                    let name = state
                        .player(who)
                        .map(|seat| seat.faction.to_string())
                        .unwrap_or_else(|| who.to_string());
                    *watch.holders.entry(name).or_default() += 1;
                }
            }
            if away && tokens < watch.tokens {
                watch.token_drops += 1;
            }
            watch.away = away;
            watch.tokens = tokens;
        }
        self.inner.choose_seeing(choice, seen)
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let content = ContentStore::embedded();
    let checkpoint = argument("--checkpoint").expect("--checkpoint");
    let rounds: u32 = argument("--rounds").and_then(|v| v.parse().ok()).unwrap_or(4);
    let seeds: u64 = argument("--seeds").and_then(|v| v.parse().ok()).unwrap_or(30);
    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let pool =
        std::sync::Arc::new(ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"));

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
            let watch = Rc::new(RefCell::new(Watch::default()));
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let profile = loaded[&factions[player].to_string()].clone();
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                deciders.insert(
                    player.clone(),
                    Box::new(WatchBot {
                        inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                        sol: sol.clone(),
                        watch: Rc::clone(&watch),
                    }),
                );
            }
            let map = OpeningMap::PythonPool {
                pool: std::sync::Arc::clone(&pool),
                tile_seed_offset: TILE_SEED_OFFSET,
            };
            let rollout = play_with_deciders(
                content,
                &players,
                &factions,
                FULL,
                seed,
                Horizon::rounds(rounds),
                ti4_engine::opening::DEFAULT_REQUIREMENT,
                &map,
                deciders,
            );
            if rollout.error.is_some() {
                continue;
            }
            games += 1;
            let watch = watch.borrow();
            departures += watch.departures;
            token_drops += watch.token_drops;
            if watch.departures > 0 {
                with_departure += 1;
            }
            for (who, count) in &watch.holders {
                *holders.entry(who.clone()).or_default() += count;
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let share = |value: usize| 100.0 * value as f64 / games.max(1) as f64;
    println!("{games} games, {rounds} rounds, pool {pool_path}");
    println!("checkpoint {checkpoint}\n");
    println!("Military Support (ms:sol), read from state:");
    println!("  games where it ever left Sol: {with_departure} ({:.1}%)", share(with_departure));
    println!("  total departures:             {departures} ({:.3} per game)", {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let per = departures as f64 / games.max(1) as f64;
        per
    });
    println!("  held by: {holders:?}");
    println!("  Sol strategy-pool drops seen while it was away: {token_drops}");
}
