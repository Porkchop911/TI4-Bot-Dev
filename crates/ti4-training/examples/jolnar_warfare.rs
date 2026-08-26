//! What does a seat holding Warfare actually do when it fails to clear?
//!
//! Jol-Nar takes the Warfare primary in most of its games and still misses the opening bar a
//! quarter of the time, missing by 1.9 on average rather than narrowly. The card is sufficient by
//! the rules, so the failure is in what happens after the draft — and the way to find it is to put
//! the seats that cleared next to the seats that did not, holding the card constant.
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

const POOL: &str = "D:/Projects/ti4-engine/data/map_pools/save52_e400_n8192.json.gz";

#[derive(Default)]
struct Tally {
    seats: usize,
    activations: f64,
    landings: f64,
    declined_landings: f64,
    production: f64,
    built: f64,
    turns: f64,
    passes: f64,
    strategic: f64,
    secondaries_taken: f64,
    secondaries_declined: f64,
    planets: f64,
    systems: f64,
    units: f64,
    round1_activations: f64,
    decisions: f64,
    cargo_loaded: f64,
    cargo_declined: f64,
    combats: f64,
    movements: f64,
    moves_declined: f64,
}

impl Tally {
    fn mean(&self, value: f64) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let n = self.seats.max(1) as f64;
        value / n
    }
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    ti4_training::rollout::set_seat_scramble(true);
    let who = std::env::args()
        .find_map(|a| a.strip_prefix("--faction=").map(ToOwned::to_owned))
        .unwrap_or_else(|| "jolnar".to_owned());
    let card = std::env::args()
        .find_map(|a| a.strip_prefix("--card=").map(ToOwned::to_owned))
        .unwrap_or_else(|| "te6warfare".to_owned());
    let path = std::env::args()
        .find(|a| a.ends_with(".json"))
        .unwrap_or_else(|| "out/stage2_clear/C1-s0.json".to_owned());
    let panel: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(120);
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .filter_map(|f| loaded.get(f.as_str()).map(|p| (f.clone(), p.clone())))
        .collect();
    let rounds: u32 = std::env::args()
        .find_map(|a| a.strip_prefix("--rounds=").and_then(|v| v.parse().ok()))
        .unwrap_or(4);
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));
    let seeds: Vec<u64> = (98_000_000..98_000_000 + panel).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::rounds(rounds),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    let target = FactionId::new(who.as_str());
    let (mut cleared, mut failed) = (Tally::default(), Tally::default());
    // What the failing seats chose at their activation decisions, by option count offered.
    let mut failed_activation_options: Vec<usize> = Vec::new();
    let mut cleared_activation_options: Vec<usize> = Vec::new();

    for game in &games {
        for seat in &game.seats {
            if seat.faction != target {
                continue;
            }
            let took = seat
                .trajectory
                .iter()
                .find(|step| step.head == "strategy")
                .is_some_and(|step| step.chosen.starts_with(&card[..7.min(card.len())]));
            // --any drops the card filter: for a faction whose starting units already meet the
            // bar, the card is not the question and conditioning on it only shrinks the sample.
            let invert = std::env::args().any(|a| a == "--invert");
            let unconditional = std::env::args().any(|a| a == "--any");
            if !unconditional && took == invert {
                continue;
            }
            let tally = if seat.episode.cleared {
                &mut cleared
            } else {
                &mut failed
            };
            tally.seats += 1;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let progress = seat.episode.final_progress;
            tally.planets += progress.planets_gained as f64;
            tally.systems += progress.systems as f64;
            tally.units += progress.units_gained as f64;
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let steps = seat.trajectory.len() as f64;
            tally.decisions += steps;
            for step in &seat.trajectory {
                match step.head.as_str() {
                    "activation" => {
                        tally.activations += 1.0;
                        if step.progress.round_number == 1 {
                            tally.round1_activations += 1.0;
                        }
                        if seat.episode.cleared {
                            cleared_activation_options.push(step.legal.len());
                        } else {
                            failed_activation_options.push(step.legal.len());
                        }
                    }
                    "landing" => {
                        if step.chosen.contains("done") || step.chosen.contains("decline") {
                            tally.declined_landings += 1.0;
                        } else {
                            tally.landings += 1.0;
                        }
                    }
                    "production" => {
                        tally.production += 1.0;
                        if !step.chosen.contains("done") && !step.chosen.contains("decline") {
                            tally.built += 1.0;
                        }
                    }
                    "turn" => {
                        tally.turns += 1.0;
                        if step.chosen.contains("pass") {
                            tally.passes += 1.0;
                        } else if step.chosen.contains("strategic") {
                            tally.strategic += 1.0;
                        }
                    }
                    "cargo" => {
                        if step.chosen.contains("done") || step.chosen.contains("decline") {
                            tally.cargo_declined += 1.0;
                        } else {
                            tally.cargo_loaded += 1.0;
                        }
                    }
                    "combat" => tally.combats += 1.0,
                    "movement" => {
                        tally.movements += 1.0;
                        if step.chosen.contains("done") || step.chosen.contains("decline") {
                            tally.moves_declined += 1.0;
                        }
                    }
                    "secondary" => {
                        if step.chosen == "no" {
                            tally.secondaries_declined += 1.0;
                        } else {
                            tally.secondaries_taken += 1.0;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    println!(
        "{who} holding {card}: {} cleared, {} failed, out of {} games\n",
        cleared.seats,
        failed.seats,
        games.len()
    );
    println!(
        "{:<26} {:>10} {:>10} {:>10}",
        "per seat", "cleared", "FAILED", "diff"
    );
    println!("{}", "-".repeat(60));
    let rows: [(&str, fn(&Tally) -> f64); 17] = [
        ("planets gained", |t| t.mean(t.planets)),
        ("systems held", |t| t.mean(t.systems)),
        ("units gained", |t| t.mean(t.units)),
        ("activations", |t| t.mean(t.activations)),
        ("  of them in round 1", |t| t.mean(t.round1_activations)),
        ("landings committed", |t| t.mean(t.landings)),
        ("landings declined", |t| t.mean(t.declined_landings)),
        ("production decisions", |t| t.mean(t.production)),
        ("  units actually built", |t| t.mean(t.built)),
        ("turns taken", |t| t.mean(t.turns)),
        ("  passes", |t| t.mean(t.passes)),
        ("secondaries followed", |t| t.mean(t.secondaries_taken)),
        ("cargo loaded", |t| t.mean(t.cargo_loaded)),
        ("cargo declined", |t| t.mean(t.cargo_declined)),
        ("movement decisions", |t| t.mean(t.movements)),
        ("  moves declined", |t| t.mean(t.moves_declined)),
        ("combat decisions", |t| t.mean(t.combats)),
    ];
    for (label, get) in rows {
        let (a, b) = (get(&cleared), get(&failed));
        println!("{label:<26} {a:>10.2} {b:>10.2} {:>+10.2}", b - a);
    }

    let mean = |v: &Vec<usize>| {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let n = v.len().max(1) as f64;
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let total: f64 = v.iter().map(|x| *x as f64).sum();
        total / n
    };
    println!(
        "\nactivation options offered: cleared {:.1}, failed {:.1}",
        mean(&cleared_activation_options),
        mean(&failed_activation_options)
    );
    let _: BTreeSet<u8> = BTreeSet::new();
}
