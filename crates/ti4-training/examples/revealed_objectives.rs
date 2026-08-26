//! Which public objectives actually reach the table, against which get scored.
//!
//! A scored-objective tally cannot tell "never revealed" from "revealed and never satisfied", and
//! those are different problems: the first is deck construction, the second is policy or map. This
//! reads `revealed_objectives` from state at every decision, so the denominator is what the game
//! actually put in front of the seats.
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

struct Seen {
    revealed: BTreeSet<String>,
    scored: BTreeSet<String>,
}

struct SeeBot {
    inner: LearnedBot,
    player: PlayerId,
    seen: Rc<RefCell<Seen>>,
}

impl Decider for SeeBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &ti4_engine::choice::SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        {
            let mut record = self.seen.borrow_mut();
            for id in seen.revealed_objectives() {
                record.revealed.insert(id.to_string());
            }
            for id in seen.scored_by(&self.player) {
                record.scored.insert(id.to_string());
            }
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
    let rounds: u32 = argument("--rounds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let seeds: u64 = argument("--seeds")
        .and_then(|v| v.parse().ok())
        .unwrap_or(25);
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());

    let catalogue: BTreeMap<String, (String, i64)> = {
        let raw = include_str!("../../ti4-content/content/public_objectives.json");
        let list: Vec<serde_json::Value> = serde_json::from_str(raw).expect("objectives");
        list.into_iter()
            .filter_map(|o| {
                Some((
                    o.get("alias")?.as_str()?.to_owned(),
                    (
                        o.get("name")?.as_str()?.to_owned(),
                        o.get("points")
                            .and_then(serde_json::Value::as_i64)
                            .unwrap_or(1),
                    ),
                ))
            })
            .collect()
    };

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&checkpoint).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let pool = std::sync::Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"),
    );

    let mut games = 0usize;
    let mut revealed: BTreeMap<String, usize> = BTreeMap::new();
    let mut scored: BTreeMap<String, usize> = BTreeMap::new();
    let mut revealed_per_game = 0usize;

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
            let record = Rc::new(RefCell::new(Seen {
                revealed: BTreeSet::new(),
                scored: BTreeSet::new(),
            }));
            let mut deciders: BTreeMap<PlayerId, Box<dyn Decider>> = BTreeMap::new();
            for (index, player) in players.iter().enumerate() {
                let profile = loaded[&factions[player].to_string()].clone();
                let stream = seed
                    .wrapping_mul(1_000_003)
                    .wrapping_add(u64::try_from(index).unwrap_or(0));
                deciders.insert(
                    player.clone(),
                    Box::new(SeeBot {
                        inner: LearnedBot::from_shared(std::sync::Arc::new(profile), stream),
                        player: player.clone(),
                        seen: Rc::clone(&record),
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
            let record = record.borrow();
            revealed_per_game += record.revealed.len();
            for id in &record.revealed {
                *revealed.entry(id.clone()).or_default() += 1;
            }
            for id in &record.scored {
                *scored.entry(id.clone()).or_default() += 1;
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let per_game = revealed_per_game as f64 / games.max(1) as f64;
    println!("{games} games, {rounds} rounds, pool {pool_path}");
    println!("checkpoint {checkpoint}");
    println!("\n{per_game:.2} public objectives revealed per game\n");

    println!(
        "{:<28} {:>4} {:>10} {:>10} {:>9}",
        "objective", "pts", "revealed", "scored", "hit rate"
    );
    let mut rows: Vec<_> = catalogue.iter().collect();
    rows.sort_by_key(|(alias, (_, points))| {
        (*points, revealed.get(*alias).map_or(0, |n| usize::MAX - n))
    });
    for (alias, (name, points)) in rows {
        let shown = revealed.get(alias).copied().unwrap_or(0);
        let got = scored.get(alias).copied().unwrap_or(0);
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let rate = if shown == 0 {
            f64::NAN
        } else {
            100.0 * got as f64 / shown as f64
        };
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let share = 100.0 * shown as f64 / games.max(1) as f64;
        if shown == 0 {
            println!(
                "{name:<28} {points:>4} {shown:>10} {got:>10} {:>9}",
                "never shown"
            );
        } else {
            println!("{name:<28} {points:>4} {shown:>6} ({share:>3.0}%) {got:>10} {rate:>8.0}%");
        }
    }
}
