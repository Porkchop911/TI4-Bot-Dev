//! Strategy picks and secondaries per round, not just round one.
//!
//! Every table in this programme so far has read the opening round, because that is where the
//! clearance bar lives. Stage 2 plays four rounds and drafts a fresh card in each of them, so a
//! policy that looks committed to one card in round one may be doing something else entirely
//! later. This splits both the draft and the follow decisions by round number.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "out/pools/save52_e400_holdout.json.gz";

const CARDS: [(&str, &str); 8] = [
    ("pok1leadership", "Lead"),
    ("pok2diplomacy", "Dipl"),
    ("pok3politics", "Poli"),
    ("te4construction", "Cons"),
    ("pok5trade", "Trad"),
    ("te6warfare", "Warf"),
    ("pok7technology", "Tech"),
    ("pok8imperial", "Impe"),
];

fn card_label(id: &str) -> &'static str {
    CARDS
        .iter()
        .find(|(alias, _)| id.starts_with(&alias[..7.min(alias.len())]))
        .map_or("????", |(_, short)| *short)
}

/// Which secondary an accepting option belongs to, from the verb it carries. Imperial and
/// Politics both say "draw" and cannot be separated at the option level, so they share a column.
fn secondary_of(words: &BTreeSet<String>) -> Option<&'static str> {
    // Built from what the options actually carry, not from the card text. Option features come
    // from the option id and label only (features.rs:290), never the prompt, so Technology --
    // whose label is bare "spend" -- was invisible to a mapping that looked for "research".
    // Leadership carries "influence" alongside "spend" and must be tested first.
    for (token, card) in [
        ("produce", "Warf"),
        ("build", "Cons"),
        ("place", "Cons"),
        ("replenish", "Trad"),
        ("ready", "Dipl"),
        // Imperial and Politics both label their accepting option "draw" and carry no word that
        // separates them; they share a column.
        ("draw", "Draw"),
        ("influence", "Lead"),
        ("spend", "Tech"),
    ] {
        if words.contains(token) {
            return Some(card);
        }
    }
    None
}

#[expect(clippy::too_many_lines, reason = "one probe, two tables, kept visible")]
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
    let seeds: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(100);
    ti4_training::rollout::set_seat_scramble(true);

    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| (f.clone(), loaded[f.as_str()].clone()))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
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

    // (faction, round) -> card -> count
    let mut picks: BTreeMap<(FactionId, u32), BTreeMap<&'static str, usize>> = BTreeMap::new();
    let mut follows: BTreeMap<(FactionId, u32), BTreeMap<&'static str, usize>> = BTreeMap::new();
    // A seat is counted once per round in which it was asked to follow anything.
    let mut seats: BTreeMap<(FactionId, u32), usize> = BTreeMap::new();
    let mut offers: BTreeMap<(FactionId, u32), usize> = BTreeMap::new();
    let mut declines: BTreeMap<(FactionId, u32), usize> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            let mut seen_round: BTreeSet<u32> = BTreeSet::new();
            for step in &seat.trajectory {
                let round = step.progress.round_number;
                match step.head.as_str() {
                    "strategy" => {
                        *picks
                            .entry((seat.faction.clone(), round))
                            .or_default()
                            .entry(card_label(&step.chosen))
                            .or_default() += 1;
                    }
                    "secondary" => {
                        if seen_round.insert(round) {
                            *seats.entry((seat.faction.clone(), round)).or_default() += 1;
                        }
                        *offers.entry((seat.faction.clone(), round)).or_default() += 1;
                        if step.chosen == "no" || step.chosen == "decline" {
                            *declines.entry((seat.faction.clone(), round)).or_default() += 1;
                        } else if let Some(vector) = step.legal.get(&step.chosen) {
                            let words: BTreeSet<String> = vector
                                .iter()
                                .filter_map(|(slot, _)| {
                                    ti4_policy::intern::name_of(*slot)
                                        .strip_prefix("option:")
                                        .map(str::to_owned)
                                })
                                .collect();
                            if let Some(card) = secondary_of(&words) {
                                *follows
                                    .entry((seat.faction.clone(), round))
                                    .or_default()
                                    .entry(card)
                                    .or_default() += 1;
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    println!("{} games, {rounds} rounds, {path}", games.len());

    println!("\nSTRATEGY PICKS BY ROUND (% of that faction's picks in that round)");
    let labels: Vec<&str> = CARDS.iter().map(|(_, short)| *short).collect();
    for faction in &factions {
        println!("\n{faction}");
        print!("{:<8}", "round");
        for label in &labels {
            print!("{label:>7}");
        }
        println!("{:>9}", "picks");
        for round in 1..=rounds {
            let empty = BTreeMap::new();
            let row = picks.get(&(faction.clone(), round)).unwrap_or(&empty);
            let total: usize = row.values().sum();
            if total == 0 {
                continue;
            }
            print!("{round:<8}");
            for label in &labels {
                let n = row.get(label).copied().unwrap_or(0);
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let rate = 100.0 * n as f64 / total as f64;
                print!("{rate:>6.1} ");
            }
            println!("{total:>8}");
        }
    }

    println!("\nSECONDARIES FOLLOWED BY ROUND (% of that faction's seats in that round)");
    let seconds = ["Lead", "Dipl", "Cons", "Trad", "Warf", "Tech", "Draw"];
    for faction in &factions {
        println!("\n{faction}");
        print!("{:<8}", "round");
        for label in seconds {
            print!("{label:>7}");
        }
        println!("{:>10}{:>10}", "offers", "declined");
        for round in 1..=rounds {
            let count = seats.get(&(faction.clone(), round)).copied().unwrap_or(0);
            if count == 0 {
                continue;
            }
            print!("{round:<8}");
            let empty = BTreeMap::new();
            let row = follows.get(&(faction.clone(), round)).unwrap_or(&empty);
            for label in seconds {
                let n = row.get(label).copied().unwrap_or(0);
                #[expect(clippy::cast_precision_loss, reason = "counts are small")]
                let rate = 100.0 * n as f64 / count as f64;
                print!("{rate:>6.0} ");
            }
            let asked = offers.get(&(faction.clone(), round)).copied().unwrap_or(0);
            let no = declines
                .get(&(faction.clone(), round))
                .copied()
                .unwrap_or(0);
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let per_seat = asked as f64 / count as f64;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let declined = 100.0 * no as f64 / asked.max(1) as f64;
            println!("{per_seat:>10.2}{declined:>9.0}%");
        }
    }
}
