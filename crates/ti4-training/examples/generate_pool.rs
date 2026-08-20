//! Build a map pool that draws from every non-home system the content offers.
//!
//! The `save52` pools permute one fixed 37-tile board: 6 home slots, Mecatol Rex, and the same 30
//! other tiles on every arrangement. That makes two board properties constant rather than sampled
//! -- which systems appear at all, and how many of them carry no planets -- so no filter can
//! select on them and every measurement averages over one tile set.
//!
//! This samples the 30 non-home, non-centre positions from the full candidate set (every blue,
//! red system that is not a home system, a hyperlane, or Mecatol -- fracture tiles are excluded
//! because the game places those itself through events), then keeps a board only if it satisfies
//! both constraints:
//!
//! * a planetless-system count inside the requested band, counted over the 31 non-home positions
//!   (Mecatol carries a planet, so in practice this counts the 30 sampled tiles);
//! * no two anomalies adjacent, over the 72 non-home hex adjacencies;
//! * every home slot can reach at least three planets across at least two systems among its own
//!   neighbours, which is what the Stage-1 bar (gain 3 planets, hold 3 systems) needs to be
//!   achievable from that seat at all. Planets inside a supernova or an asteroid field are not
//!   counted: a supernova cannot be entered, and an asteroid field needs Antimass Deflectors,
//!   which two of the six factions do not start with. Pass `--count-anomaly-planets` to count
//!   them anyway.
//!
//! Home slots keep whatever placeholder the source pool used: the galaxy builder replaces them
//! with faction home systems, so their contents are never played.
//!
//! Usage:
//!   `generate_pool --template <pool.json[.gz]> --out <pool.json> [--boards 4000]
//!                  [--min 8] [--max 12] [--seed 1] [--allow-touching]`
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ti4_content::ContentStore;
use ti4_model::content_types::FULL;

/// The six axial neighbours of a hex.
const NEIGHBOURS: [(i64, i64); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// Mecatol Rex, pinned to the centre as the board's fixed point.
const MECATOL: &str = "18";

/// A small deterministic generator, so a pool can be reproduced from its seed alone.
struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            usize::try_from(self.next() % bound as u64).unwrap_or(0)
        }
    }

    /// Partial Fisher-Yates: shuffles enough of the slice to make the first `take` uniform.
    fn shuffle_prefix<T>(&mut self, items: &mut [T], take: usize) {
        for index in 0..take.min(items.len()) {
            let pick = index + self.below(items.len() - index);
            items.swap(index, pick);
        }
    }
}

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn read_pool(path: &Path) -> serde_json::Value {
    let bytes = std::fs::read(path).expect("read template");
    if path.extension().is_some_and(|e| e == "gz") {
        let decoder = flate2::read::GzDecoder::new(&bytes[..]);
        serde_json::from_reader(decoder).expect("parse gzipped template")
    } else {
        serde_json::from_slice(&bytes).expect("parse template")
    }
}

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

#[expect(clippy::too_many_lines, reason = "one generator, the reporting kept visible")]
fn main() {
    let store = ContentStore::embedded();
    let template = argument("--template")
        .unwrap_or_else(|| "out/pools/save52_e400_train.json.gz".to_owned());
    let output = argument("--out").expect("--out");
    let boards: usize = argument("--boards")
        .and_then(|v| v.parse().ok())
        .unwrap_or(4000);
    let min: usize = argument("--min").and_then(|v| v.parse().ok()).unwrap_or(8);
    let max: usize = argument("--max").and_then(|v| v.parse().ok()).unwrap_or(12);
    let seed: u64 = argument("--seed").and_then(|v| v.parse().ok()).unwrap_or(1);
    let allow_touching = std::env::args().any(|a| a == "--allow-touching");
    let count_anomaly_planets = std::env::args().any(|a| a == "--count-anomaly-planets");
    let reach_planets: usize = argument("--reach-planets")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);
    let reach_systems: usize = argument("--reach-systems")
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);

    let mut pool = read_pool(Path::new(&template));
    let coords: Vec<Vec<i64>> = serde_json::from_value(pool["coords"].clone()).expect("coords");
    let slots: Vec<Vec<i64>> = serde_json::from_value(pool["slots"].clone()).expect("slots");
    let home: Vec<bool> = coords.iter().map(|c| slots.contains(c)).collect();
    let centre = coords
        .iter()
        .position(|c| c == &vec![0, 0])
        .expect("a centre coordinate");

    // Non-home adjacencies, each edge once.
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

    // Each home slot's own neighbours. A home slot sits on a board corner, so it has three.
    let home_neighbours: Vec<Vec<usize>> = coords
        .iter()
        .enumerate()
        .filter(|(index, _)| home[*index])
        .map(|(_, c)| {
            NEIGHBOURS
                .iter()
                .filter_map(|(dq, dr)| position.get(&(c[0] + dq, c[1] + dr)).copied())
                .filter(|other| !home[*other])
                .collect()
        })
        .collect();

    // Candidates: every non-home, non-hyperlane, non-Mecatol system the content offers.
    let systems: Vec<serde_json::Value> = {
        let text = include_str!("../../ti4-content/content/systems.json");
        serde_json::from_str(text).expect("systems")
    };
    let mut candidates: Vec<String> = Vec::new();
    let mut planetless: BTreeMap<String, bool> = BTreeMap::new();
    let mut anomaly: BTreeMap<String, bool> = BTreeMap::new();
    let mut backs: BTreeMap<String, usize> = BTreeMap::new();
    // Planets a neighbouring seat could actually take in the opening.
    let mut reachable: BTreeMap<String, usize> = BTreeMap::new();
    for record in &systems {
        let Some(id) = record.get("id").and_then(serde_json::Value::as_str) else {
            continue;
        };
        if id == MECATOL {
            continue;
        }
        if record
            .get("isHyperlane")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            continue;
        }
        let back = record
            .get("tileBack")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("none");
        // Blue and red only. Fracture tiles are placed dynamically by in-game events, so a
        // static pool must never pre-place one; hyperlanes and green home systems are likewise
        // not board draws.
        let usable = back == "blue" || back == "red";
        if !usable {
            continue;
        }
        // The system must resolve under the sources the engine plays with, or the pool will not
        // validate.
        let Some(system) = ti4_content::galaxy::system(store, id, FULL) else {
            continue;
        };
        planetless.insert(id.to_owned(), system.planets().is_empty());
        anomaly.insert(id.to_owned(), is_anomaly(record));
        let blocked = !count_anomaly_planets
            && ["isSupernova", "isAsteroidField"].iter().any(|flag| {
                record
                    .get(*flag)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            });
        reachable.insert(
            id.to_owned(),
            if blocked { 0 } else { system.planets().len() },
        );
        *backs.entry(back.to_owned()).or_default() += 1;
        candidates.push(id.to_owned());
    }

    let fill: Vec<usize> = (0..coords.len())
        .filter(|index| !home[*index] && *index != centre)
        .collect();
    let planetless_available = candidates.iter().filter(|id| planetless[*id]).count();
    let anomalies_available = candidates.iter().filter(|id| anomaly[*id]).count();

    println!("template {template}");
    println!(
        "  {} coordinates, {} home slots, centre at index {centre}, {} tiles to fill",
        coords.len(),
        slots.len(),
        fill.len()
    );
    println!("  {} non-home adjacencies tested", edges.len());
    println!(
        "\ncandidate systems: {} ({backs:?})",
        candidates.len()
    );
    println!("  planetless among them: {planetless_available}");
    println!("  anomalies among them:  {anomalies_available}");
    assert!(
        candidates.len() >= fill.len(),
        "not enough candidate systems to fill a board"
    );

    let placeholders: Vec<String> = {
        let first: Vec<Vec<String>> =
            serde_json::from_value(pool["arrangements"].clone()).expect("arrangements");
        first.first().cloned().expect("a template arrangement")
    };

    let mut random = Random(seed | 1);
    let mut arrangements: Vec<Vec<String>> = Vec::with_capacity(boards);
    let mut attempts = 0usize;
    let mut rejected_band = 0usize;
    let mut rejected_touching = 0usize;
    let mut rejected_reach = 0usize;
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut histogram: BTreeMap<usize, usize> = BTreeMap::new();

    // A bound, so an impossible band fails loudly instead of spinning.
    let ceiling = boards.saturating_mul(2000).max(200_000);
    while arrangements.len() < boards && attempts < ceiling {
        attempts += 1;
        let mut deck = candidates.clone();
        random.shuffle_prefix(&mut deck, fill.len());

        let mut board = placeholders.clone();
        board[centre] = MECATOL.to_owned();
        for (slot, tile) in fill.iter().zip(deck.iter()) {
            board[*slot] = tile.clone();
        }

        let empties = (0..board.len())
            .filter(|index| !home[*index] && planetless.get(&board[*index]).copied().unwrap_or(false))
            .count();
        *histogram.entry(empties).or_default() += 1;
        if empties < min || empties > max {
            rejected_band += 1;
            continue;
        }
        if !allow_touching
            && edges.iter().any(|(a, b)| {
                anomaly.get(&board[*a]).copied().unwrap_or(false)
                    && anomaly.get(&board[*b]).copied().unwrap_or(false)
            })
        {
            rejected_touching += 1;
            continue;
        }
        // Stage-1 achievability, per seat: enough planets next door, spread over enough systems.
        let starved = home_neighbours.iter().any(|neighbours| {
            let planets: usize = neighbours
                .iter()
                .map(|index| reachable.get(&board[*index]).copied().unwrap_or(0))
                .sum();
            let systems = neighbours
                .iter()
                .filter(|index| reachable.get(&board[**index]).copied().unwrap_or(0) > 0)
                .count();
            planets < reach_planets || systems < reach_systems
        });
        if starved {
            rejected_reach += 1;
            continue;
        }
        if seen.insert(board.clone()) {
            arrangements.push(board);
        }
    }

    println!(
        "\n{attempts} draws -> {} boards\n  {rejected_band} outside the planetless band\n  {rejected_touching} with two anomalies adjacent\n  {rejected_reach} with a home short of {reach_planets} planets across {reach_systems} systems",
        arrangements.len()
    );
    println!("planetless-count distribution over all draws:");
    for (count, n) in &histogram {
        let marker = if *count >= min && *count <= max { "  <-- kept" } else { "" };
        #[expect(clippy::cast_precision_loss, reason = "small counts")]
        let share = 100.0 * *n as f64 / attempts as f64;
        println!("  {count:>3}  {n:>7}  {share:>5.1}%{marker}");
    }
    assert_eq!(
        arrangements.len(),
        boards,
        "could not generate the requested number of boards within the attempt ceiling"
    );

    pool["arrangements"] = serde_json::to_value(&arrangements).expect("serialize arrangements");
    std::fs::write(&output, serde_json::to_string(&pool).expect("serialize pool"))
        .expect("write pool");

    // The pool is only useful if the engine accepts it.
    let written = ti4_sim::MapPool::load(Path::new(&output)).expect("reload written pool");
    written
        .validate_systems(store, FULL)
        .expect("written pool validates against the engine sources");
    println!(
        "wrote {output}: {} boards, {} home slots, validated",
        written.len(),
        written.home_slots()
    );
}
