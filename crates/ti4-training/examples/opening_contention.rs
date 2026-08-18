//! Is a faction's clearance a property of its policy, or of the competition for one shared board?
//!
//! Six seats play the same 6-player map, and the bar is **gain 3 planets, hold 3 systems, gain 1
//! unit**. Planets one seat takes are planets another cannot. So a low per-faction clearance has
//! two very different explanations, and the reports so far have silently assumed the first:
//!
//! 1. the faction's policy is worse, or its features cannot express its opening; or
//! 2. the faction is being outcompeted -- the board cannot supply six seats at once, and this is
//!    the seat that loses.
//!
//! These are distinguished by changing the opponents and nothing else. A trained profile is played
//! against five trained opponents, then against five blank ones, on the same maps and seeds. If
//! clearance jumps when the opposition is removed, the constraint was contention.
//!
//! Also counted: how many planets the board actually offers against how many six clearing seats
//! would need, which bounds mean clearance from above regardless of any policy.

use std::collections::BTreeMap;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::FactionId;
use ti4_policy::learned::{Profile, blank_explicit_profile};
use ti4_training::stage1::evaluate_factions_on_pool;

const POOL: &str = "out/pools/save52_e400_holdout.json.gz";
const TILE_OFFSET: u64 = 20_000_000;

fn load(path: &str, factions: &[FactionId]) -> BTreeMap<FactionId, Profile> {
    let bytes = std::fs::read(path).unwrap_or_else(|error| panic!("read {path}: {error}"));
    let document: serde_json::Value = serde_json::from_slice(&bytes).expect("parse");
    let table = document.get("profiles").unwrap_or(&document);
    let loaded: BTreeMap<String, Profile> =
        serde_json::from_value(table.clone()).expect("profile table");
    factions
        .iter()
        .map(|faction| {
            (
                faction.clone(),
                loaded
                    .get(faction.as_str())
                    .cloned()
                    .unwrap_or_else(|| blank_explicit_profile(faction.as_str())),
            )
        })
        .collect()
}

fn main() {
    let store = ContentStore::embedded();
    let factions: Vec<FactionId> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .map(|name| FactionId::new(*name))
        .collect();
    let pool = Arc::new(ti4_sim::MapPool::load(std::path::Path::new(POOL)).expect("pool"));

    let checkpoint = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "out/prod/stage1_ppo_s0.json".to_owned());
    let trained = load(&checkpoint, &factions);
    let blank: BTreeMap<FactionId, Profile> = factions
        .iter()
        .map(|faction| (faction.clone(), blank_explicit_profile(faction.as_str())))
        .collect();

    let seeds = 100_u64;
    let first = 98_000_000_u64;

    let together = evaluate_factions_on_pool(
        store,
        &factions,
        &trained,
        FULL,
        first,
        seeds,
        Arc::clone(&pool),
        TILE_OFFSET,
    );

    println!("checkpoint: {checkpoint}");
    println!("panel: {seeds} seeds x 6 rotations on held-out boards\n");
    println!("Each faction trained, played against FIVE TRAINED opponents (the usual measurement),");
    println!("then against FIVE BLANK opponents. Same maps, same seeds; only the opposition moves.\n");
    println!(
        "{:<9} {:>10} {:>10} {:>9}   {:>8} {:>8}",
        "faction", "vs trained", "vs blank", "lift", "plan.tr", "plan.bl"
    );
    println!("{}", "-".repeat(62));

    let mut lifts = Vec::new();
    for faction in &factions {
        // One trained seat, five blank. Everything else identical.
        let mut mixed = blank.clone();
        if let Some(profile) = trained.get(faction) {
            mixed.insert(faction.clone(), profile.clone());
        }
        let alone = evaluate_factions_on_pool(
            store,
            &factions,
            &mixed,
            FULL,
            first,
            seeds,
            Arc::clone(&pool),
            TILE_OFFSET,
        );
        let (Some(with), Some(without)) = (together.get(faction), alone.get(faction)) else {
            continue;
        };
        lifts.push((faction.clone(), with.clearance, without.clearance));
        println!(
            "{:<9} {:>10.4} {:>10.4} {:>+9.4}   {:>8.2} {:>8.2}",
            faction.as_str(),
            with.clearance,
            without.clearance,
            without.clearance - with.clearance,
            with.planets_gained,
            without.planets_gained
        );
    }

    let count = f64::from(u32::try_from(lifts.len().max(1)).unwrap_or(1));
    println!(
        "\nmean vs trained {:.4}, mean vs blank {:.4}, mean lift {:+.4}",
        lifts.iter().map(|row| row.1).sum::<f64>() / count,
        lifts.iter().map(|row| row.2).sum::<f64>() / count,
        lifts.iter().map(|row| row.2 - row.1).sum::<f64>() / count
    );

    // How much the board can supply at all. Six seats clearing needs 18 planets gained between
    // them; if the board cannot offer that many reachable ones, mean clearance is capped by the
    // map and no policy can lift it.
    let total_gained: f64 = together.values().map(|row| row.planets_gained).sum();
    println!(
        "\nplanets gained by all six seats together: {total_gained:.2} of the 18 that six clearing \
         seats would need"
    );
    println!(
        "mean systems held {:.2}, mean units gained {:.2}",
        together.values().map(|row| row.systems).sum::<f64>() / 6.0,
        together.values().map(|row| row.units_gained).sum::<f64>() / 6.0
    );
}
