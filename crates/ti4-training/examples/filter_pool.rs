//! Filter a map pool: planetless-system count, and no two anomalies touching.
//!
//! A pool arrangement is a list of tile ids, one per coordinate. Two properties of a board are
//! left entirely to the generator and vary widely across the pool, so every measurement taken on
//! it averages over boards that play very differently:
//!
//! * how many systems carry no planets at all -- empty space, anomalies, wormhole tiles -- which
//!   sets how much of the board is contestable rather than merely traversable;
//! * whether anomalies clump. Adjacent anomalies compound: two nebulae or an asteroid field
//!   beside a supernova can wall off a region that a single one only slows.
//!
//! Home slots are excluded from both tests. Those tiles are replaced by faction home systems when
//! the galaxy is built, so whatever sits in them in the artifact is never played.
//!
//! Usage:
//!   `filter_pool --in <pool.json[.gz]> --out <pool.json> [--min 8] [--max 12] [--allow-touching]`
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;

/// The six axial neighbours of a hex.
const NEIGHBOURS: [(i64, i64); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn read_pool(path: &Path) -> serde_json::Value {
    let bytes = std::fs::read(path).expect("read pool");
    if path.extension().is_some_and(|e| e == "gz") {
        let decoder = flate2::read::GzDecoder::new(&bytes[..]);
        serde_json::from_reader(decoder).expect("parse gzipped pool")
    } else {
        serde_json::from_slice(&bytes).expect("parse pool")
    }
}

/// Whether a system is an anomaly, from the content record's own flags.
fn is_anomaly(raw: &serde_json::Value) -> bool {
    [
        "isAsteroidField",
        "isNebula",
        "isGravityRift",
        "isSupernova",
        "isScar",
    ]
    .iter()
    .any(|flag| {
        raw.get(*flag)
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
    })
}

#[expect(
    clippy::too_many_lines,
    reason = "one pass, the reporting kept visible"
)]
fn main() {
    let store = ContentStore::embedded();
    let input = argument("--in").expect("--in");
    let output = argument("--out").expect("--out");
    let min: usize = argument("--min").and_then(|v| v.parse().ok()).unwrap_or(8);
    let max: usize = argument("--max").and_then(|v| v.parse().ok()).unwrap_or(12);
    let allow_touching = std::env::args().any(|a| a == "--allow-touching");

    let mut pool = read_pool(Path::new(&input));
    let coords: Vec<Vec<i64>> = serde_json::from_value(pool["coords"].clone()).expect("coords");
    let slots: Vec<Vec<i64>> = serde_json::from_value(pool["slots"].clone()).expect("slots");
    let home: Vec<bool> = coords.iter().map(|c| slots.contains(c)).collect();
    assert_eq!(
        home.iter().filter(|h| **h).count(),
        slots.len(),
        "every home slot must match a pool coordinate"
    );

    // Adjacent index pairs, both non-home. Built once from the axial coordinates; a pair is kept
    // in one direction only so each edge is tested a single time.
    let position: BTreeMap<(i64, i64), usize> = coords
        .iter()
        .enumerate()
        .map(|(index, c)| ((c[0], c[1]), index))
        .collect();
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for (index, c) in coords.iter().enumerate() {
        if home[index] {
            continue;
        }
        for (dq, dr) in NEIGHBOURS {
            if let Some(other) = position.get(&(c[0] + dq, c[1] + dr)) {
                if *other > index && !home[*other] {
                    edges.push((index, *other));
                }
            }
        }
    }
    println!(
        "board geometry: {} coordinates, {} home slots, {} non-home adjacencies",
        coords.len(),
        slots.len(),
        edges.len()
    );

    // Anomaly and planet-count lookups, resolved once per distinct tile id.
    let raw: BTreeMap<String, serde_json::Value> = {
        let text = include_str!("../../ti4-content/content/systems.json");
        let list: Vec<serde_json::Value> = serde_json::from_str(text).expect("systems");
        list.into_iter()
            .filter_map(|s| Some((s.get("id")?.as_str()?.to_owned(), s)))
            .collect()
    };
    let mut planetless: BTreeMap<String, bool> = BTreeMap::new();
    let mut anomaly: BTreeMap<String, bool> = BTreeMap::new();
    let mut classify = |tile: &String| -> (bool, bool) {
        let empty = *planetless.entry(tile.clone()).or_insert_with(|| {
            ti4_content::galaxy::system(store, tile, FULL).map_or(0, |s| s.planets().len()) == 0
        });
        let anom = *anomaly
            .entry(tile.clone())
            .or_insert_with(|| raw.get(tile).is_some_and(is_anomaly));
        (empty, anom)
    };

    let arrangements: Vec<Vec<String>> =
        serde_json::from_value(pool["arrangements"].clone()).expect("arrangements");

    let mut histogram: BTreeMap<usize, usize> = BTreeMap::new();
    let mut touching_boards = 0usize;
    let mut band_only = 0usize;
    let mut kept: Vec<Vec<String>> = Vec::new();
    let mut anomalies_seen: BTreeSet<String> = BTreeSet::new();

    for arrangement in &arrangements {
        let flags: Vec<(bool, bool)> = arrangement.iter().map(&mut classify).collect();
        let empties = arrangement
            .iter()
            .enumerate()
            .filter(|(index, _)| !home[*index] && flags[*index].0)
            .count();
        *histogram.entry(empties).or_default() += 1;
        for (index, tile) in arrangement.iter().enumerate() {
            if !home[index] && flags[index].1 {
                anomalies_seen.insert(tile.clone());
            }
        }
        let touching = edges.iter().any(|(a, b)| flags[*a].1 && flags[*b].1);
        if touching {
            touching_boards += 1;
        }
        let in_band = empties >= min && empties <= max;
        if in_band {
            band_only += 1;
        }
        if in_band && (allow_touching || !touching) {
            kept.push(arrangement.clone());
        }
    }

    let total = arrangements.len();
    #[expect(clippy::cast_precision_loss, reason = "small counts")]
    let share = |value: usize| 100.0 * value as f64 / total as f64;

    println!("\n{input}: {total} arrangements");
    println!("\nplanetless non-home systems per board:");
    for (count, boards) in &histogram {
        let marker = if *count >= min && *count <= max {
            "  <-- in band"
        } else {
            ""
        };
        println!(
            "  {count:>3}  {boards:>6}  {:>5.1}%{marker}",
            share(*boards)
        );
    }
    println!("\nanomaly tiles used off the home slots: {anomalies_seen:?}");
    println!(
        "boards with two anomalies adjacent: {touching_boards} ({:.1}%)",
        share(touching_boards)
    );
    println!(
        "boards in the {min}..={max} band:        {band_only} ({:.1}%)",
        share(band_only)
    );
    println!(
        "kept (band{}):            {} ({:.1}%)",
        if allow_touching {
            ""
        } else {
            ", no touching anomalies"
        },
        kept.len(),
        share(kept.len())
    );
    assert!(
        !kept.is_empty(),
        "the filter keeps no boards; widen the band"
    );

    pool["arrangements"] = serde_json::to_value(&kept).expect("serialize arrangements");
    std::fs::write(
        &output,
        serde_json::to_string(&pool).expect("serialize pool"),
    )
    .expect("write pool");
    println!("wrote {output}");
}
