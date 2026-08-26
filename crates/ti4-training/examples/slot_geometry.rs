//! Why is one home slot so much worse than the others?
//!
//! Clearance pooled over factions differs by physical seat far more than it differs by faction, so
//! whatever the seats are is a larger effect than which faction plays them. Since home systems are
//! placed into map slots by seat index, seat N is map slot N, and this counts what each slot can
//! actually reach: planets within one and two hexes, which is what a one-round opening can take.
use std::collections::BTreeSet;
use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;

fn main() {
    let store = ContentStore::embedded();
    let pool = Arc::new(
        ti4_sim::MapPool::load(std::path::Path::new(
            "out/pools/save52_e400_holdout.json.gz",
        ))
        .expect("pool"),
    );
    let names: Vec<String> = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"]
        .iter()
        .filter_map(|faction| {
            ti4_content::factions::get(store, faction)
                .and_then(|record| record.home_system())
                .map(str::to_owned)
        })
        .collect();
    let borrowed: Vec<&str> = names.iter().map(String::as_str).collect();

    let boards = 60;
    let mut within1 = vec![0.0_f64; 6];
    let mut within2 = vec![0.0_f64; 6];
    let mut anomalies = vec![0.0_f64; 6];
    for seed in 98_000_000_u64..98_000_000 + boards {
        let galaxy = pool
            .galaxy(store, FULL, seed + 20_000_000, &borrowed)
            .expect("galaxy");
        for (slot, home) in borrowed.iter().enumerate() {
            let ids: Vec<&str> = galaxy.system_ids();
            let (mut near, mut mid, mut bad) = (0usize, 0usize, 0usize);
            for id in &ids {
                if borrowed.contains(id) {
                    continue; // another player's home is not a gainable planet
                }
                let Some(distance) = galaxy.distance(home, id) else {
                    continue;
                };
                let Some(record) = ti4_content::galaxy::system(store, id, FULL) else {
                    continue;
                };
                let planets = record.planets().len();
                if distance <= 1 {
                    near += planets;
                    if record.is_anomaly() {
                        bad += 1;
                    }
                }
                if distance <= 2 {
                    mid += planets;
                }
            }
            #[expect(clippy::cast_precision_loss, reason = "small counts")]
            let (n, m, b) = (near as f64, mid as f64, bad as f64);
            within1[slot] += n;
            within2[slot] += m;
            anomalies[slot] += b;
        }
    }
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let n = boards as f64;
    println!("mean over {boards} held-out boards, per home slot (= physical seat)\n");
    println!(
        "{:<7} {:>14} {:>14} {:>16}",
        "slot", "planets <=1 hex", "planets <=2 hex", "adjacent anomalies"
    );
    println!("{}", "-".repeat(56));
    let unique: BTreeSet<&str> = borrowed.iter().copied().collect();
    for slot in 0..6 {
        println!(
            "seat{slot:<3} {:>14.2} {:>14.2} {:>16.2}",
            within1[slot] / n,
            within2[slot] / n,
            anomalies[slot] / n
        );
    }
    let _ = unique;
}
