//! Take-when-offered for each secondary, per faction.
//!
//! "% of that faction's seats" conflates two things: how often the card was played at all, and
//! how often the seat said yes when asked. A preference change can only move the second. This
//! reports both, so a rate that looks low because the window rarely opens is not mistaken for a
//! rate that is low because the policy declines.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

/// The verb each card's accepting option carries. Leadership pairs "spend" with "influence" and
/// must be tested before Technology, whose accepting option is bare "spend".
const CARDS: [(&str, &str); 7] = [
    ("produce", "Warf"),
    ("place", "Cons"),
    ("replenish", "Trad"),
    ("ready", "Dipl"),
    ("draw", "Draw"),
    ("influence", "Lead"),
    ("spend", "Tech"),
];

fn card_of(words: &BTreeSet<String>) -> Option<&'static str> {
    CARDS
        .iter()
        .find(|(token, _)| words.contains(*token))
        .map(|(_, card)| *card)
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let path = std::env::args()
        .skip(1)
        .find(|a| a.ends_with(".json"))
        .expect("checkpoint path");
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(4);
    let only_round: Option<u32> =
        std::env::args().find_map(|a| a.strip_prefix("--round=").and_then(|v| v.parse().ok()));
    let seeds: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(40);
    let pool_path =
        argument("--map-pool").unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
    ti4_training::rollout::set_seat_scramble(true);

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(&pool_path)).expect("pool"));
    let seed_block: Vec<u64> = (98_000_000..98_000_000 + seeds).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seed_block,
        Horizon::rounds(rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    let mut offered: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
    let mut taken: BTreeMap<(String, &'static str), usize> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            let faction = seat.faction.to_string();
            for step in &seat.trajectory {
                if step.head != "secondary" {
                    continue;
                }
                if only_round.is_some_and(|r| r != step.progress.round_number) {
                    continue;
                }
                // The card is identified from the accepting option in the legal set, so a decline
                // is still attributed to the right card.
                let mut card = None;
                for (id, vector) in &step.legal {
                    if id == "no" || id == "decline" {
                        continue;
                    }
                    let words: BTreeSet<String> = vector
                        .iter()
                        .filter_map(|(slot, _)| {
                            ti4_policy::intern::name_of(*slot)
                                .strip_prefix("option:")
                                .map(str::to_owned)
                        })
                        .collect();
                    card = card_of(&words);
                }
                let Some(card) = card else { continue };
                *offered.entry((faction.clone(), card)).or_default() += 1;
                if step.chosen != "no" && step.chosen != "decline" {
                    *taken.entry((faction.clone(), card)).or_default() += 1;
                }
            }
        }
    }

    let scope = only_round.map_or_else(|| format!("rounds 1..{rounds}"), |r| format!("round {r}"));
    println!("{} games, {scope}, {path}\n", games.len());
    let labels = ["Lead", "Dipl", "Cons", "Trad", "Warf", "Tech", "Draw"];
    println!("TAKE-WHEN-OFFERED (offers per seat in brackets)");
    print!("{:<10}", "faction");
    for label in labels {
        print!("{label:>16}");
    }
    println!();
    for faction in &factions {
        let name = faction.to_string();
        let seats = games.len();
        print!("{name:<10}");
        for card in labels {
            let asked = offered.get(&(name.clone(), card)).copied().unwrap_or(0);
            let got = taken.get(&(name.clone(), card)).copied().unwrap_or(0);
            if asked == 0 {
                print!("{:>16}", "-");
            } else {
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let rate = 100.0 * got as f64 / asked as f64;
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let per_seat = asked as f64 / seats.max(1) as f64;
                print!("{:>11}", format!("{rate:.0}% [{per_seat:.2}]"));
                print!("{:>5}", "");
            }
        }
        println!();
    }
}
