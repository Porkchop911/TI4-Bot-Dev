//! How much is the speaker seat worth, and is it separable from map position?
//!
//! `start_game_seeded` is called with `speaker: None`, which resolves to the first player -- always
//! `seat0`. Factions rotate through the seats, so each is speaker in exactly one of its six games
//! per seed and the comparison between factions is balanced. That much is fine.
//!
//! What is *not* separable is speaker from board position. Home systems are placed into map slots
//! by seat index, so the faction in seat0 is simultaneously the speaker and the occupant of map
//! slot 0. Whatever seat0 is worth, this measurement cannot say how much of it is picking a
//! strategy card first and how much is standing where slot 0 stands.
//!
//! Reported per physical seat, pooled over all six factions, so faction strength cancels.
use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::learned::Profile;
use ti4_training::rollout::{Horizon, play_rotated_save54_pool_batch};

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(
            "out/pools/save52_e400_holdout.json.gz",
        ))
        .expect("pool"),
    );
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/prod/stage1_ppo_s0.json".to_owned());
    let document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("parse");
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(document["profiles"].clone()).expect("profiles");
    // `blank` as the second argument plays untrained weights. If the seat pattern survives that,
    // it is structural -- turn order or setup -- rather than something the policies learned.
    let use_blank = std::env::args().any(|a| a == "blank");
    let profiles: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|f| {
            let profile = if use_blank {
                ti4_policy::learned::blank_explicit_profile(f.as_str())
            } else {
                loaded[f.as_str()].clone()
            };
            (f.clone(), profile)
        })
        .collect();

    let seeds: Vec<u64> = (98_000_000..98_000_150).collect();
    let games = play_rotated_save54_pool_batch(
        store,
        &factions,
        &profiles,
        FULL,
        &seeds,
        Horizon::opening(),
        ti4_engine::opening::DEFAULT_REQUIREMENT,
        Arc::clone(&pool),
        20_000_000,
    );

    // Keyed by physical seat, pooled over every faction that occupied it.
    let mut grid: BTreeMap<(String, String), (usize, usize)> = BTreeMap::new();
    let mut cleared: BTreeMap<String, (usize, usize, f64)> = BTreeMap::new();
    for game in &games {
        for seat in &game.seats {
            let row = cleared
                .entry(seat.player.to_string())
                .or_insert((0, 0, 0.0));
            row.0 += 1;
            let cell = grid
                .entry((seat.faction.to_string(), seat.player.to_string()))
                .or_insert((0, 0));
            cell.0 += 1;
            cell.1 += usize::from(seat.episode.cleared);
            row.1 += usize::from(seat.episode.cleared);
            #[expect(clippy::cast_precision_loss, reason = "planet counts are tiny")]
            let gained = seat.episode.final_progress.planets_gained as f64;
            row.2 += gained;
        }
    }
    println!("checkpoint: {path}");
    println!("{} games, {} seats each\n", games.len(), factions.len());
    println!("seat0 is ALWAYS the speaker, and always occupies map slot 0.\n");
    println!(
        "{:<8} {:>7} {:>10} {:>9}   {}",
        "seat", "games", "clearance", "planets", "role"
    );
    println!("{}", "-".repeat(56));
    for (seat, (games_played, wins, planets)) in &cleared {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let n = *games_played as f64;
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let w = *wins as f64;
        let role = if seat == "seat0" {
            "SPEAKER + map slot 0"
        } else {
            ""
        };
        println!(
            "{seat:<8} {games_played:>7} {:>10.4} {:>9.2}   {role}",
            w / n,
            planets / n
        );
    }
    println!(
        "
CLEARANCE, faction x seat"
    );
    let seats: Vec<String> = cleared.keys().cloned().collect();
    print!("{:<9}", "faction");
    for s in &seats {
        print!("{s:>9}");
    }
    println!();
    let mut current = String::new();
    for ((faction, _), _) in &grid {
        if *faction == current {
            continue;
        }
        current.clone_from(faction);
        print!("{faction:<9}");
        for s in &seats {
            let (n, w) = grid
                .get(&(faction.clone(), s.clone()))
                .copied()
                .unwrap_or((0, 0));
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let share = if n == 0 { 0.0 } else { w as f64 / n as f64 };
            print!("{share:>9.3}");
        }
        println!();
    }
    let values: Vec<f64> = cleared
        .values()
        .map(|(g, w, _)| {
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let (n, x) = (*g as f64, *w as f64);
            x / n
        })
        .collect();
    let speaker = values.first().copied().unwrap_or(0.0);
    let rest: f64 = values.iter().skip(1).sum::<f64>()
        / f64::from(u32::try_from(values.len().saturating_sub(1).max(1)).unwrap_or(1));
    println!(
        "\nspeaker seat {speaker:.4} against non-speaker mean {rest:.4}  =>  {:+.4}",
        speaker - rest
    );
    println!(
        "spread across seats: {:.4}",
        values.iter().copied().fold(0.0_f64, f64::max)
            - values.iter().copied().fold(1.0_f64, f64::min)
    );
}
