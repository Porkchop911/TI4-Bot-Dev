//! Can the board actually satisfy the objectives that were dealt?
//!
//! Clearance needs three gained planets per seat and the boards supply 5.17, so the opening bar is
//! not map-limited. The objectives ask different questions -- planets of one trait, planets with
//! technology specialties, total resources and influence -- and nothing has checked whether the
//! generated boards can answer them. An objective the map cannot satisfy is not a policy failure.
use std::collections::BTreeMap;
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

    // Planet records by id, so a board's tiles can be resolved to traits and specialties.
    let catalogue: BTreeMap<String, serde_json::Value> = {
        let raw = include_str!("../../ti4-content/content/planets.json");
        let list: Vec<serde_json::Value> = serde_json::from_str(raw).expect("planets");
        list.into_iter()
            .filter_map(|p| Some((p.get("id")?.as_str()?.to_owned(), p)))
            .collect()
    };

    let mut boards = 0usize;
    let mut traits: BTreeMap<String, usize> = BTreeMap::new();
    let mut specialties: BTreeMap<String, usize> = BTreeMap::new();
    let mut specialty_planets = 0usize;
    let mut resources = 0i64;
    let mut influence = 0i64;
    let mut legendary = 0usize;
    let mut non_home = 0usize;
    // Planets the content store gives no trait at all: they can never answer a trait objective.
    let mut untyped: BTreeMap<String, usize> = BTreeMap::new();

    for seed in 98_000_000_u64..98_000_050 {
        let galaxy = pool
            .galaxy(store, FULL, seed + 20_000_000, &borrowed)
            .expect("galaxy");
        boards += 1;
        for id in galaxy.system_ids() {
            if borrowed.contains(&id) {
                continue;
            }
            let Some(system) = ti4_content::galaxy::system(store, id, FULL) else {
                continue;
            };
            for planet in system.planets() {
                let Some(record) = catalogue.get(planet) else {
                    continue;
                };
                non_home += 1;
                let kind = record
                    .get("planetType")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("NONE");
                *traits.entry(kind.to_owned()).or_default() += 1;
                if kind == "NONE" {
                    *untyped.entry((*planet).to_owned()).or_default() += 1;
                }
                resources += record
                    .get("resources")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                influence += record
                    .get("influence")
                    .and_then(serde_json::Value::as_i64)
                    .unwrap_or(0);
                if record
                    .get("legendaryAbilityName")
                    .is_some_and(|v| !v.is_null())
                {
                    legendary += 1;
                }
                if let Some(list) = record
                    .get("techSpecialties")
                    .and_then(serde_json::Value::as_array)
                {
                    if !list.is_empty() {
                        specialty_planets += 1;
                    }
                    for entry in list {
                        if let Some(colour) = entry.as_str() {
                            *specialties.entry(colour.to_owned()).or_default() += 1;
                        }
                    }
                }
            }
        }
    }

    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let n = boards as f64;
    let per_board = |value: usize| {
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let v = value as f64;
        v / n
    };

    println!("over {boards} held-out boards, NON-HOME planets only\n");
    println!(
        "  gainable planets        {:.1}  ({:.2} per seat if split six ways)",
        per_board(non_home),
        per_board(non_home) / 6.0
    );
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let res = resources as f64 / n;
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let inf = influence as f64 / n;
    println!(
        "  total resources         {res:.1}  ({:.2} per seat)",
        res / 6.0
    );
    println!(
        "  total influence         {inf:.1}  ({:.2} per seat)",
        inf / 6.0
    );
    println!("  legendary planets       {:.2}", per_board(legendary));
    println!(
        "  planets with a tech specialty  {:.1}  ({:.2} per seat)",
        per_board(specialty_planets),
        per_board(specialty_planets) / 6.0
    );

    println!("\n  by trait (per board):");
    for (kind, count) in &traits {
        println!(
            "    {kind:<12} {:.1}   ({:.2} per seat)",
            per_board(*count),
            per_board(*count) / 6.0
        );
    }
    println!("\n  planets carrying no trait, most common (per board):");
    let mut rows: Vec<_> = untyped.iter().collect();
    rows.sort_by(|a, b| b.1.cmp(a.1));
    for (id, count) in rows.into_iter().take(10) {
        println!("    {id:<20} {:.2}", per_board(*count));
    }

    println!("\n  tech specialties by colour (per board):");
    for (colour, count) in &specialties {
        println!(
            "    {colour:<12} {:.2}  ({:.3} per seat)",
            per_board(*count),
            per_board(*count) / 6.0
        );
    }
}
