//! Two Sol-specific mechanics: Orbital Drop, and the Military Support note.
//!
//! Orbital Drop is a component action offered as `faction|orbital_drop` whenever Sol holds a
//! strategy token and controls a planet, so it can be counted as offered-versus-taken.
//!
//! Military Support is Sol's promissory note. Whoever holds it removes a token from Sol's
//! strategy pool at the start of Sol's turn, plants two infantry, and returns the card
//! (`promissory.rs:348`). The firing itself is automatic and raises no decision, but the note can
//! only be away from Sol because a transaction moved it, and it goes home immediately after
//! firing -- so the number of accepted `pnms:sol:*` transaction options is the number of times it
//! fires. The token is only actually removed when Sol has one, which the engine checks, so this
//! reports the transfers and the option's share of the offers it was part of.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

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
    let seeds: u64 = std::env::args()
        .find_map(|a| a.strip_prefix("--seeds=").and_then(|v| v.parse().ok()))
        .unwrap_or(60);
    let pool_path = argument("--map-pool")
        .unwrap_or_else(|| "out/pools/full_np8_12_holdout.json".to_owned());
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

    let mut sol_seats = 0usize;
    let mut drop_offered = 0usize;
    let mut drop_taken = 0usize;
    let mut drop_by_round: BTreeMap<u32, usize> = BTreeMap::new();
    // Every promissory note that changed hands, by owning faction, and who took it.
    let mut notes_taken: BTreeMap<String, usize> = BTreeMap::new();
    let mut ms_offered = 0usize;
    let mut ms_taken_by: BTreeMap<String, usize> = BTreeMap::new();

    for game in &games {
        for seat in &game.seats {
            let faction = seat.faction.to_string();
            if faction == "sol" {
                sol_seats += 1;
            }
            for step in &seat.trajectory {
                if faction == "sol" && step.legal.contains_key("faction|orbital_drop") {
                    drop_offered += 1;
                    if step.chosen == "faction|orbital_drop" {
                        drop_taken += 1;
                        *drop_by_round
                            .entry(step.progress.round_number)
                            .or_default() += 1;
                    }
                }
                // Promissory options are `pn{alias}:{faction}:{price}`.
                if step.legal.keys().any(|id| id.starts_with("pnms:sol:")) {
                    ms_offered += 1;
                }
                if let Some(rest) = step.chosen.strip_prefix("pn") {
                    let mut parts = rest.split(':');
                    if let (Some(alias), Some(owner)) = (parts.next(), parts.next()) {
                        *notes_taken
                            .entry(format!("{alias}:{owner}"))
                            .or_default() += 1;
                        if alias == "ms" && owner == "sol" {
                            *ms_taken_by.entry(faction.clone()).or_default() += 1;
                        }
                    }
                }
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let per_seat = |value: usize| value as f64 / sol_seats.max(1) as f64;

    println!("{} games, {rounds} rounds, pool {pool_path}", games.len());
    println!("checkpoint {path}\n");

    println!("ORBITAL DROP ({sol_seats} sol seats)");
    println!("  offered      {drop_offered:>6}   ({:.2} per sol seat)", per_seat(drop_offered));
    println!("  taken        {drop_taken:>6}   ({:.2} per sol seat)", per_seat(drop_taken));
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let rate = 100.0 * drop_taken as f64 / drop_offered.max(1) as f64;
    println!("  taken when offered: {rate:.1}%");
    println!("  by round: {drop_by_round:?}");

    let ms_total: usize = ms_taken_by.values().sum();
    println!("\nMILITARY SUPPORT (sol's note)");
    println!("  decisions where it was on the table: {ms_offered}");
    println!(
        "  times it changed hands:              {ms_total}   ({:.3} per sol seat)",
        per_seat(ms_total)
    );
    println!("  taken by: {ms_taken_by:?}");
    println!(
        "  -> sol loses a strategy token that many times, when it has one to lose"
    );

    println!("\nALL PROMISSORY NOTES TAKEN (alias:owner)");
    let mut rows: Vec<_> = notes_taken.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (note, count) in rows.into_iter().take(15) {
        println!("  {note:<20} {count:>6}");
    }
}
