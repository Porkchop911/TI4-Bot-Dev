//! Where the victory points come from, read from game state.
//!
//! The Stage-2 reward is VP and every report quotes it, but the number says nothing about which
//! objectives produced it. This wraps each seat's bot and records what that seat has scored at its
//! last observed decision, then names the objectives from the content store. Points credited by
//! something other than an objective -- the Mecatol custodians token, Support for the Throne --
//! show up as the gap between total VP and the summed objective points.
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
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

/// Records the seat's own technology set at every decision it sees; the last one stands.
struct ScoreBot {
    inner: LearnedBot,
    player: PlayerId,
    owned: Rc<RefCell<(BTreeSet<String>, i32, usize)>>,
    secrets_held: usize,
    revealed: usize,
    scoreable: usize,
}

impl Decider for ScoreBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &Observed<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let scored: BTreeSet<String> = seen
            .scored_by(&self.player)
            .iter()
            .map(std::string::ToString::to_string)
            .collect();
        let points = seen.seat(&self.player).map_or(0, |seat| seat.victory_points);
        self.secrets_held = seen
            .seat(&self.player)
            .map_or(0, |seat| seat.secret_objectives_held);
        self.revealed = seen.revealed_objectives().len();
        self.scoreable = seen.scoreable_public(&self.player);
        // Support for the Throne is the one note whose position scores (promissory.rs:231), so
        // count how many of the six are sitting in front of this seat. The redacted view is a
        // clone, so it is only taken when this seat's point total has moved.
        let mut slot = self.owned.borrow_mut();
        if points != slot.1 {
            slot.2 = seen
                .redacted_for(&self.player)
                .support_holders
                .values()
                .filter(|holder| *holder == &self.player)
                .count();
        }
        slot.0 = scored;
        slot.1 = points;
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
    println!("pool {pool_path}");

    // Objective aliases -> (name, points), from both decks.
    let mut catalogue: BTreeMap<String, (String, i32)> = BTreeMap::new();
    for (deck, raw) in [
        ("pub", include_str!("../../ti4-content/content/public_objectives.json")),
        ("SEC", include_str!("../../ti4-content/content/secret_objectives.json")),
    ] {
        let list: Vec<serde_json::Value> = serde_json::from_str(raw).expect("objectives");
        for entry in list {
            if let (Some(alias), Some(name)) = (
                entry.get("alias").and_then(serde_json::Value::as_str),
                entry.get("name").and_then(serde_json::Value::as_str),
            ) {
                let points = entry
                    .get("points")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(1);
                catalogue.insert(
                    alias.to_owned(),
                    (
                        format!("[{deck}] {name}"),
                        i32::try_from(points).unwrap_or(1),
                    ),
                );
            }
        }
    }

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

    let mut scored: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut seats_seen: BTreeMap<String, usize> = BTreeMap::new();
    let mut total_vp: BTreeMap<String, i32> = BTreeMap::new();
    let mut objective_vp: BTreeMap<String, i32> = BTreeMap::new();
    let mut support_vp: BTreeMap<String, i32> = BTreeMap::new();

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
            let mut sinks: BTreeMap<PlayerId, Rc<RefCell<(BTreeSet<String>, i32, usize)>>> =
                BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let faction = factions[player].to_string();
                let profile = loaded[&faction].clone();
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                let owned = Rc::new(RefCell::new((BTreeSet::new(), 0, 0)));
                deciders.insert(
                    player.clone(),
                    Box::new(ScoreBot {
                        inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                        player: player.clone(),
                        owned: Rc::clone(&owned),
                        secrets_held: 0,
                        revealed: 0,
                        scoreable: 0,
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
                let (set, points, supports) = &*owned.borrow();
                *support_vp.entry(faction.clone()).or_default() +=
                    i32::try_from(*supports).unwrap_or(0);
                *total_vp.entry(faction.clone()).or_default() += *points;
                let row = scored.entry(faction.clone()).or_default();
                for alias in set {
                    let (label, value) = catalogue
                        .get(alias)
                        .cloned()
                        .unwrap_or_else(|| (alias.clone(), 1));
                    *row.entry(label).or_default() += 1;
                    *objective_vp.entry(faction.clone()).or_default() += value;
                }
            }
        }
    }

    println!("objectives SCORED at the last observed decision, {rounds} rounds
");
    for faction in FACTIONS {
        let seats = seats_seen.get(faction).copied().unwrap_or(0).max(1);
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let per_seat = |value: i32| f64::from(value) / seats as f64;
        let vp = total_vp.get(faction).copied().unwrap_or(0);
        let obj = objective_vp.get(faction).copied().unwrap_or(0);
        let sup = support_vp.get(faction).copied().unwrap_or(0);
        println!(
            "{faction}  ({seats} seats)  {:.2} VP/seat = {:.2} objectives + {:.2} Support for the Throne + {:.2} other (custodians, agendas)",
            per_seat(vp),
            per_seat(obj),
            per_seat(sup),
            per_seat(vp - obj - sup)
        );
        let empty = BTreeMap::new();
        let row = scored.get(faction).unwrap_or(&empty);
        let mut rows: Vec<_> = row.iter().collect();
        rows.sort_by(|a, b| b.1.cmp(a.1));
        for (objective, count) in rows.into_iter() {
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let rate = 100.0 * *count as f64 / seats as f64;
            println!("    {objective:<38} {rate:>5.1}% of seats");
        }
        println!();
    }
}
