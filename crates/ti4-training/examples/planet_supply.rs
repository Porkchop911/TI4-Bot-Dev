//! How many planets does the board actually offer, against how many six clearing seats need?
//!
//! The opening bar is three planets gained per seat, so all six clearing requires eighteen planets
//! gained between them. If the board cannot supply that, mean clearance is capped by the map and
//! no policy, optimiser, or amount of training can lift it -- which would make every clearance
//! number in this programme a measurement against an unreachable ceiling.
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

    let (mut total, mut home, mut boards) = (0usize, 0usize, 0usize);
    for seed in 98_000_000_u64..98_000_050 {
        let galaxy = pool
            .galaxy(store, FULL, seed + 20_000_000, &borrowed)
            .expect("galaxy");
        let (mut planets, mut in_home) = (0usize, 0usize);
        for id in galaxy.system_ids() {
            let count = ti4_content::galaxy::system(store, id, FULL)
                .map_or(0, |record| record.planets().len());
            planets += count;
            if borrowed.contains(&id) {
                in_home += count;
            }
        }
        total += planets;
        home += in_home;
        boards += 1;
    }
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let n = boards as f64;
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let (t, h) = (total as f64, home as f64);
    println!("over {boards} held-out boards:");
    println!("  mean planets on the board      {:.1}", t / n);
    println!("  of which in the six home systems {:.1}", h / n);
    println!("  NON-HOME, i.e. gainable         {:.1}", (t - h) / n);
    println!("\n  six seats clearing need 18 gained between them");
    println!(
        "  supply per seat if split evenly: {:.2} against the 3 required",
        (t - h) / n / 6.0
    );
}
