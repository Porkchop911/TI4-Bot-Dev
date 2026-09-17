//! Check the lean arena simulator against ti4calc on sampled positions.
//!
//! `--write <scenarios.json> [--count N] [--max-ships N] [--max-fighters N]` samples positions from
//! the six factions, with and without upgrades, half of each side carrying its flagship, ships
//! possibly starting damaged. `tools/ti4calc/arena_runner.ts` turns that file into ti4calc odds.
//! `--check <scenarios.json> <results.json> [--seeds N] [--rolls N]` compares them with the lean
//! simulator and reports the gaps, grouped by which flagship is in the fight.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::POK;
use ti4_training::battle_arena::{self as arena, Profile, Rng, SHIP_TYPES, Side};

fn argument(name: &str) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + 1))
        .cloned()
}

fn argument_at(name: &str, offset: usize) -> Option<String> {
    let args: Vec<String> = std::env::args().collect();
    args.iter()
        .position(|arg| arg == name)
        .and_then(|at| args.get(at + offset))
        .cloned()
}

fn parse<T: std::str::FromStr>(name: &str, default: T) -> T {
    argument(name).map_or(default, |value| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("{name} expects a number"))
    })
}

fn side_json(
    content: &ContentStore,
    profile: Profile,
    composition: &arena::Composition,
    rng: &mut Rng,
    bare: bool,
) -> serde_json::Value {
    let mut units = serde_json::Map::new();
    let mut upgrades = serde_json::Map::new();
    let mut damaged = serde_json::Map::new();
    for (index, base) in SHIP_TYPES.iter().enumerate() {
        let count = if bare { 0 } else { composition[index] };
        if count == 0 {
            continue;
        }
        units.insert((*base).to_owned(), count.into());
        let id = profile.unit_for(content, base);
        let kind = ti4_content::units::unit_type(content, &id, POK).expect("unit exists");
        if profile.upgraded && kind.upgrades_from().is_some() {
            upgrades.insert((*base).to_owned(), true.into());
        }
        if kind.sustain_damage() {
            let hurt = rng.below(count + 1);
            if hurt > 0 {
                damaged.insert((*base).to_owned(), hurt.into());
            }
        }
    }
    // Guns: a third of sides carry PDS (and Xxcha mechs), and some defenders are guns alone.
    if rng.below(3) == 0 || bare {
        let pds = 1 + rng.below(3);
        units.insert("pds".to_owned(), pds.into());
        if profile.upgraded {
            upgrades.insert("pds".to_owned(), true.into());
        }
        if profile.faction == "xxcha" && rng.below(2) == 0 {
            units.insert("mech".to_owned(), (1 + rng.below(2)).into());
        }
    }
    serde_json::json!({
        "faction": profile.faction,
        "upgraded": profile.upgraded,
        "units": units,
        "upgrades": upgrades,
        "damaged": damaged,
    })
}

fn write(content: &ContentStore, path: &str) {
    let count = parse("--count", 300usize);
    let max_ships = parse("--max-ships", 4usize);
    let max_fighters = parse("--max-fighters", 6usize);
    let profiles = Profile::all(true);
    let fleets: Vec<(Profile, Vec<arena::Composition>, Vec<arena::Composition>)> = profiles
        .iter()
        .map(|profile| {
            let all = arena::compositions(content, *profile, max_ships, max_fighters);
            let (with, without): (Vec<_>, Vec<_>) = all.into_iter().partition(|c| c[6] > 0);
            (*profile, with, without)
        })
        .collect();
    let mut rng = Rng::new(arena::fnv("ti4calc-check"));
    let mut out = Vec::new();
    for _ in 0..count {
        let pick = |rng: &mut Rng, bare: bool| {
            let (profile, with, without) = &fleets[rng.below(fleets.len())];
            let pool = if rng.below(2) == 0 { with } else { without };
            let composition = pool[rng.below(pool.len())];
            side_json(content, *profile, &composition, rng, bare)
        };
        let attacker = pick(&mut rng, false);
        let bare = rng.below(8) == 0;
        let defender = pick(&mut rng, bare);
        out.push(serde_json::json!({ "attacker": attacker, "defender": defender }));
    }
    std::fs::write(path, serde_json::to_string(&out).expect("serialises")).expect("written");
    println!("wrote {count} scenarios (max {max_ships} ships, {max_fighters} fighters) to {path}");
}

/// Rebuild a lean side from a scenario side.
fn side(content: &ContentStore, value: &serde_json::Value) -> (Side, String) {
    let faction = value["faction"].as_str().expect("faction");
    let profile = Profile::all(true)
        .into_iter()
        .find(|p| {
            p.faction == faction && p.upgraded == value["upgraded"].as_bool().unwrap_or(false)
        })
        .expect("profile");
    let mut fleet = Vec::new();
    let mut guns = Vec::new();
    let mut hurt = Vec::new();
    let mut flagship = String::new();
    for (base, count) in value["units"].as_object().expect("units") {
        let id = profile.unit_for(content, base);
        let count = usize::try_from(count.as_u64().unwrap_or(0)).unwrap_or(0);
        if matches!(base.as_str(), "pds" | "mech") {
            guns.push((id, count));
            continue;
        }
        if base == "flagship" {
            flagship = faction.to_owned();
        }
        if let Some(n) = value["damaged"][base.as_str()].as_u64() {
            hurt.push((id.clone(), usize::try_from(n).unwrap_or(0)));
        }
        fleet.push((id, count));
    }
    (
        Side::of(content, &fleet, &hurt, faction, true).with_guns(content, &guns),
        flagship,
    )
}

fn check(content: &ContentStore, scenarios: &str, results: &str) {
    let seeds = parse("--seeds", 20_000u64);
    let rolls = parse("--rolls", 10_000u64);
    let attacker_cannon = std::env::args().any(|arg| arg == "--attacker-cannon");
    println!(
        "attacker space cannon: {}",
        if attacker_cannon { "fires" } else { "silent" }
    );
    let scenarios: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(scenarios).expect("scenarios"))
            .expect("json");
    let results: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(results).expect("results")).expect("json");
    assert_eq!(scenarios.len(), results.len(), "one result per scenario");

    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (n_lean, n_calc) = (seeds as f64, rolls as f64);
    let mut rows = Vec::new();
    let mut groups: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    for (index, (scenario, result)) in scenarios.iter().zip(&results).enumerate() {
        let (a, a_flag) = side(content, &scenario["attacker"]);
        let (d, d_flag) = side(content, &scenario["defender"]);
        let mut wins = 0u64;
        let mut rng = Rng::new(arena::fnv(&format!("lean-{index}")));
        for _ in 0..seeds {
            if arena::fight_with(&a, &d, rng.next_u64(), attacker_cannon).0 == Some("a") {
                wins += 1;
            }
        }
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let lean = wins as f64 / n_lean;
        let calc = result["attacker"].as_f64().expect("attacker rate");
        let se = (lean * (1.0 - lean) / n_lean + calc * (1.0 - calc) / n_calc)
            .sqrt()
            .max(1e-9);
        let gap = lean - calc;
        rows.push((gap / se, gap, lean, calc, index));
        let mut keys = vec![
            format!(
                "attacker flagship {}",
                if a_flag.is_empty() { "-" } else { &a_flag }
            ),
            format!(
                "defender flagship {}",
                if d_flag.is_empty() { "-" } else { &d_flag }
            ),
        ];
        if a_flag.is_empty() && d_flag.is_empty() {
            keys.push("no flagship either side".to_owned());
        }
        let armed = |side: &serde_json::Value| {
            side["units"].get("pds").is_some() || side["units"].get("mech").is_some()
        };
        keys.push(format!(
            "guns: attacker {} defender {}{}",
            armed(&scenario["attacker"]),
            armed(&scenario["defender"]),
            if scenario["defender"]["units"]
                .as_object()
                .is_some_and(|units| units.keys().all(|k| k == "pds" || k == "mech"))
            {
                " (guns only)"
            } else {
                ""
            }
        ));
        for key in keys {
            let entry = groups.entry(key).or_insert((0.0, 0.0, 0));
            entry.0 += gap;
            entry.1 += (gap / se).powi(2);
            entry.2 += 1;
        }
    }
    let beyond = rows.iter().filter(|r| r.0.abs() > 3.0).count();
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let mean_abs = rows.iter().map(|r| r.1.abs()).sum::<f64>() / rows.len() as f64;
    println!(
        "lean ({seeds} seeds) vs ti4calc ({rolls} rolls), {} scenarios, attacker win rate",
        rows.len()
    );
    println!("  mean |gap| {mean_abs:.4}   |z| > 3: {beyond}");
    println!();
    println!("  group                          n   mean gap   mean z^2  (1.0 = noise)");
    for (key, (gap, z2, n)) in &groups {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let nf = *n as f64;
        println!("  {key:<28} {n:>4}   {:+.4}   {:>8.2}", gap / nf, z2 / nf);
    }
    rows.sort_by(|x, y| y.0.abs().total_cmp(&x.0.abs()));
    println!();
    println!("  worst:");
    for (z, _, lean, calc, index) in rows.iter().take(10) {
        println!(
            "    z {z:+6.1}  lean {lean:.3}  ti4calc {calc:.3}  {} vs {}",
            scenarios[*index]["attacker"], scenarios[*index]["defender"]
        );
    }
}

fn main() {
    let content = ContentStore::embedded();
    if let Some(path) = argument("--write") {
        write(content, &path);
    } else if let (Some(scenarios), Some(results)) =
        (argument_at("--check", 1), argument_at("--check", 2))
    {
        check(content, &scenarios, &results);
    } else {
        eprintln!("usage: --write <scenarios.json> | --check <scenarios.json> <results.json>");
    }
}
