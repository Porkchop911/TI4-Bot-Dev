//! Which technologies a seat actually holds, read from game state rather than from choices.
//!
//! Counting chosen option ids is not the same as counting techs acquired: most technology aliases
//! are two or three letters and collide with option ids in other namespaces, and a chosen option
//! is an intent, not an outcome. This probe wraps each seat's bot and records the seat's own
//! `technologies` set at its last observed decision, so the tally comes from what the engine says
//! the player owns.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice};
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::inference::LearnedBot;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, OpeningMap, play_with_deciders};

const FACTIONS: [&str; 6] = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
const TILE_SEED_OFFSET: u64 = 20_000_000;

/// Records the seat's own technology set at every decision it sees; the last one stands.
struct OwnedTechBot {
    inner: LearnedBot,
    player: PlayerId,
    owned: Rc<RefCell<BTreeSet<String>>>,
}

impl Decider for OwnedTechBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if let Some(seat) = seen.seat(&self.player) {
            *self.owned.borrow_mut() = seat
                .technologies
                .iter()
                .map(std::string::ToString::to_string)
                .collect();
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
    let seeds: u64 = argument("--seeds").and_then(|v| v.parse().ok()).unwrap_or(40);
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/save52_e400_holdout.json.gz".to_owned());

    let names: BTreeMap<String, String> = {
        let raw = include_str!("../../ti4-content/content/technologies.json");
        let list: Vec<serde_json::Value> = serde_json::from_str(raw).expect("technologies");
        list.iter()
            .filter_map(|t| {
                Some((
                    t.get("alias")?.as_str()?.to_owned(),
                    t.get("name")?.as_str()?.to_owned(),
                ))
            })
            .collect()
    };

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let table = document
        .get("profiles")
        .or_else(|| document.get("learner_profiles"))
        .expect("profiles");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(table.clone()).expect("profile table");

    let pool = std::sync::Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"),
    );
    pool.validate_systems(content, FULL).expect("pool validate");

    // Per faction: how many seats were observed, and how often each technology was held at the end.
    let mut held: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut seats_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_techs: BTreeMap<String, usize> = BTreeMap::new();

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
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            let mut sinks: BTreeMap<PlayerId, Rc<RefCell<BTreeSet<String>>>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let faction = factions[player].to_string();
                let profile = loaded[&faction].clone();
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let owned = Rc::new(RefCell::new(BTreeSet::new()));
                deciders.insert(
                    player.clone(),
                    Box::new(OwnedTechBot {
                        inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                        player: player.clone(),
                        owned: Rc::clone(&owned),
                    }),
                );
                sinks.insert(player.clone(), owned);
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
            for (player, owned) in &sinks {
                let faction = factions[player].to_string();
                *seats_seen.entry(faction.clone()).or_default() += 1;
                let set = owned.borrow();
                *total_techs.entry(faction.clone()).or_default() += set.len();
                let row = held.entry(faction).or_default();
                for alias in set.iter() {
                    let label = names.get(alias).cloned().unwrap_or_else(|| alias.clone());
                    *row.entry(label).or_default() += 1;
                }
            }
        }
    }

    println!("technologies HELD at the last observed decision, {rounds} rounds");
    for faction in FACTIONS {
        let seats = seats_seen.get(faction).copied().unwrap_or(0).max(1);
        let total = total_techs.get(faction).copied().unwrap_or(0);
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let mean = total as f64 / seats as f64;
        println!("\n{faction}  ({seats} seats, {mean:.2} technologies per seat)");
        let empty = BTreeMap::new();
        let row = held.get(faction).unwrap_or(&empty);
        let mut rows: Vec<_> = row.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (tech, count) in rows.into_iter().take(10) {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = 100.0 * *count as f64 / seats as f64;
            println!("    {tech:<34} {rate:>5.0}% of seats");
        }
    }
}
