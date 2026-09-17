//! Check the lean arena simulator against ti4calc on sampled positions.
//!
//! `--write <scenarios.json> [--count N] [--max-ships N] [--max-fighters N]` samples positions from
//! the six factions, with and without upgrades, half of each side carrying its flagship, ships
//! possibly starting damaged. `tools/ti4calc/arena_runner.ts` turns that file into ti4calc odds.
//! `--check <scenarios.json> <results.json> [--seeds N] [--rolls N]` compares them with the lean
//! simulator and reports the gaps, grouped by which flagship is in the fight.
//! `--write-ground` and `--check-ground` do the same for a ground combat on one planet: the
//! invader's ground forces and bombarding ships against the defender's ground forces and PDS.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::POK;
use ti4_training::battle_arena::{self as arena, GroundSide, Profile, Rng, SHIP_TYPES, Side};

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

fn ground_write(content: &ContentStore, path: &str) {
    let count = parse("--count", 300usize);
    let profiles = Profile::all(true);
    let mut rng = Rng::new(arena::fnv("ti4calc-ground"));
    let mut out = Vec::new();
    for _ in 0..count {
        let attacker_profile = profiles[rng.below(profiles.len())];
        let defender_profile = profiles[rng.below(profiles.len())];
        let side = |profile: Profile, rng: &mut Rng, invading: bool| {
            let mut units = serde_json::Map::new();
            let mut upgrades = serde_json::Map::new();
            let mut damaged = serde_json::Map::new();
            let infantry = if invading {
                1 + rng.below(6)
            } else {
                rng.below(6)
            };
            let mechs = rng.below(3);
            if infantry > 0 {
                units.insert("infantry".to_owned(), infantry.into());
                if profile.upgraded {
                    upgrades.insert("infantry".to_owned(), true.into());
                }
            }
            if mechs > 0 {
                units.insert("mech".to_owned(), mechs.into());
                let hurt = rng.below(mechs + 1);
                if hurt > 0 && !invading {
                    damaged.insert("mech".to_owned(), hurt.into());
                }
            }
            if invading {
                for (base, most) in [("dreadnought", 3), ("warsun", 2)] {
                    let n = rng.below(most);
                    if n > 0 {
                        units.insert(base.to_owned(), n.into());
                        if profile.upgraded && base == "dreadnought" {
                            upgrades.insert(base.to_owned(), true.into());
                        }
                    }
                }
                if profile.faction == "letnev" && rng.below(3) == 0 {
                    units.insert("flagship".to_owned(), 1.into());
                }
            } else {
                let pds = rng.below(3);
                if pds > 0 {
                    units.insert("pds".to_owned(), pds.into());
                    if profile.upgraded {
                        upgrades.insert("pds".to_owned(), true.into());
                    }
                }
            }
            serde_json::json!({
                "faction": profile.faction,
                "upgraded": profile.upgraded,
                "units": units,
                "upgrades": upgrades,
                "damaged": damaged,
            })
        };
        let attacker = side(attacker_profile, &mut rng, true);
        let defender = side(defender_profile, &mut rng, false);
        out.push(
            serde_json::json!({ "attacker": attacker, "defender": defender, "place": "ground" }),
        );
    }
    let _ = content;
    std::fs::write(path, serde_json::to_string(&out).expect("serialises")).expect("written");
    println!("wrote {count} ground scenarios to {path}");
}

/// A lean ground side from a scenario side: ground forces, and (invading) bombardment, or
/// (defending) space cannon defense. Returns the side, the faction and whether PDS stand there.
fn ground_side(
    content: &ContentStore,
    value: &serde_json::Value,
    shielded: bool,
    mech_bombards: bool,
) -> (GroundSide, String, bool) {
    let faction = value["faction"].as_str().expect("faction");
    let upgraded = value["upgraded"].as_bool().unwrap_or(false);
    let profile = Profile::all(true)
        .into_iter()
        .find(|p| p.faction == faction && p.upgraded == upgraded)
        .expect("profile");
    let count = |base: &str| {
        value["units"][base]
            .as_u64()
            .map_or(0, |n| usize::try_from(n).unwrap_or(0))
    };
    let mut forces = Vec::new();
    for base in ["infantry", "mech"] {
        if count(base) > 0 {
            forces.push((profile.unit_for(content, base), count(base)));
        }
    }
    let mut hurt = Vec::new();
    if let Some(n) = value["damaged"]["mech"].as_u64() {
        hurt.push((
            profile.unit_for(content, "mech"),
            usize::try_from(n).unwrap_or(0),
        ));
    }
    let mut side = GroundSide::of(content, &forces, &hurt, faction);
    let ships: Vec<(String, usize)> = ["dreadnought", "warsun", "flagship"]
        .iter()
        .filter(|base| count(base) > 0)
        .map(|base| (profile.unit_for(content, base), count(base)))
        .collect();
    let strips = count("warsun") > 0 || (faction == "letnev" && count("flagship") > 0);
    let allowed = !shielded || strips;
    // The engine lets the L1Z1X mech bombard from the space area before it lands ("while not
    // participating in ground combat ... as if it were a ship"); ti4calc does not model it.
    if mech_bombards && allowed && faction == "l1z1x" && count("mech") > 0 {
        side = side.with_bombardment(
            content,
            &[(profile.unit_for(content, "mech"), count("mech"))],
            true,
            false,
        );
    }
    if !ships.is_empty() {
        side = side.with_bombardment(content, &ships, allowed, allowed && faction == "l1z1x");
    }
    let mut guns = Vec::new();
    if count("pds") > 0 {
        guns.push((profile.unit_for(content, "pds"), count("pds")));
    }
    let side = side.with_defense_guns(content, &guns);
    (side, faction.to_owned(), count("pds") > 0)
}

fn ground_check(content: &ContentStore, scenarios: &str, results: &str) {
    let seeds = parse("--seeds", 20_000u64);
    let rolls = parse("--rolls", 10_000u64);
    let scenarios: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(scenarios).expect("scenarios"))
            .expect("json");
    let results: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(results).expect("results")).expect("json");
    assert_eq!(scenarios.len(), results.len(), "one result per scenario");
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let (n_lean, n_calc) = (seeds as f64, rolls as f64);
    let mut groups: BTreeMap<String, (f64, f64, usize)> = BTreeMap::new();
    let mut rows = Vec::new();
    for (index, (scenario, result)) in scenarios.iter().zip(&results).enumerate() {
        let (_, _, shielded) = ground_side(content, &scenario["defender"], false, false);
        let (a, a_faction, _) = ground_side(content, &scenario["attacker"], shielded, false);
        let (d, d_faction, _) = ground_side(content, &scenario["defender"], false, false);
        let mut wins = 0u64;
        let mut rng = Rng::new(arena::fnv(&format!("ground-{index}")));
        for _ in 0..seeds {
            if arena::ground_fight(&a, &d, rng.next_u64()).winner == Some("a") {
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
        rows.push((gap / se, lean, calc, index));
        let units = &scenario["attacker"]["units"];
        let bombards = units.get("dreadnought").is_some()
            || units.get("warsun").is_some()
            || units.get("flagship").is_some();
        for key in [
            format!("invader {a_faction}"),
            format!("defender {d_faction}"),
            format!("bombard {bombards} shielded {shielded}"),
            format!(
                "defender mechs {}",
                scenario["defender"]["units"].get("mech").is_some()
            ),
        ] {
            let entry = groups.entry(key).or_insert((0.0, 0.0, 0));
            entry.0 += gap;
            entry.1 += (gap / se).powi(2);
            entry.2 += 1;
        }
    }
    let beyond = rows.iter().filter(|r| r.0.abs() > 3.0).count();
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let mean_abs = rows.iter().map(|r| (r.1 - r.2).abs()).sum::<f64>() / rows.len() as f64;
    println!(
        "ground: lean ({seeds}) vs ti4calc ({rolls}), {} scenarios, invader takes the planet",
        rows.len()
    );
    println!("  mean |gap| {mean_abs:.4}   |z| > 3: {beyond}");
    println!("  group                                n   mean gap   mean z^2");
    for (key, (gap, z2, n)) in &groups {
        #[expect(clippy::cast_precision_loss, reason = "counts are small")]
        let nf = *n as f64;
        println!("  {key:<34} {n:>4}   {:+.4}   {:>8.2}", gap / nf, z2 / nf);
    }
    rows.sort_by(|x, y| y.0.abs().total_cmp(&x.0.abs()));
    println!("  worst:");
    for (z, lean, calc, index) in rows.iter().take(8) {
        println!(
            "    z {z:+6.1}  lean {lean:.3}  ti4calc {calc:.3}  {} vs {}",
            scenarios[*index]["attacker"], scenarios[*index]["defender"]
        );
    }
}

/// Commits every ground force to one planet and declines everything optional.
struct CommitAll {
    planet: String,
}

impl ti4_engine::choice::Decider for CommitAll {
    fn choose(
        &mut self,
        choice: &ti4_engine::choice::Choice,
    ) -> Result<ti4_engine::choice::ChoiceOption, ti4_engine::choice::IllegalChoice> {
        let landing = choice.options.iter().find(|option| {
            option.kind == "commit"
                && option
                    .payload
                    .get("planet")
                    .and_then(serde_json::Value::as_str)
                    == Some(self.planet.as_str())
        });
        let pick = landing
            .or_else(|| choice.options.iter().find(|option| option.is_decline()))
            .or_else(|| choice.options.first())
            .cloned();
        pick.ok_or_else(|| ti4_engine::choice::IllegalChoice::NoOptions {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
        })
    }
}

/// A system with exactly one planet, not a home system, not Mecatol Rex, not an anomaly.
fn lone_planet(content: &ContentStore) -> (String, String) {
    let planets = ti4_content::galaxy::all_planets(content, POK);
    let away = ti4_engine::fixtures::non_home_planets(usize::MAX);
    let systems = ti4_content::galaxy::all_systems(content, POK);
    planets
        .iter()
        .filter(|(id, planet)| {
            !planet.is_placed_during_play()
                && away.iter().any(|away| away == *id)
                && planet.system_id().is_some_and(|system| {
                    system != ti4_engine::seating::MECATOL
                        && systems.get(system).is_some_and(|s| !s.is_anomaly())
                        && planets
                            .values()
                            .filter(|other| other.system_id() == Some(system))
                            .count()
                            == 1
                })
        })
        .map(|(id, planet)| {
            (
                planet.system_id().expect("filtered").to_owned(),
                (*id).to_owned(),
            )
        })
        .next()
        .expect("a lone planet exists")
}

/// One engine invasion of a scenario; whether the invader took the planet.
fn engine_ground(
    content: &'static ContentStore,
    scenario: &serde_json::Value,
    target: &(String, String),
    seed: u64,
) -> bool {
    use ti4_model::id::{FactionId, PlanetId, PlayerId, SystemId};
    let (a, d) = (PlayerId::new("a"), PlayerId::new("b"));
    let mut state = ti4_engine::fixtures::game(&["a", "b"]);
    let (system, planet) = (SystemId::new(&target.0), PlanetId::new(&target.1));
    let mut setup = |side: &serde_json::Value, owner: &PlayerId, invading: bool| {
        let faction = side["faction"].as_str().expect("faction");
        let upgraded = side["upgraded"].as_bool().unwrap_or(false);
        if let Some(seat) = state.player_mut(owner) {
            seat.faction = FactionId::new(faction);
        }
        let profile = Profile::all(true)
            .into_iter()
            .find(|p| p.faction == faction && p.upgraded == upgraded)
            .expect("profile");
        for (base, count) in side["units"].as_object().expect("units") {
            let id = profile.unit_for(content, base);
            let count = usize::try_from(count.as_u64().unwrap_or(0)).unwrap_or(0);
            let ground = matches!(base.as_str(), "infantry" | "mech" | "pds");
            if invading || !ground {
                ti4_engine::fixtures::put(&mut state, &system, &id, owner, count);
            } else {
                ti4_engine::fixtures::put_on_planet(
                    &mut state, &system, &planet, &id, owner, count,
                );
            }
        }
        if let Some(hurt) = side["damaged"]["mech"].as_u64() {
            let id = profile.unit_for(content, "mech");
            for _ in 0..hurt {
                let board = state.system_state(&system);
                let fresh = board
                    .on_planet_of(&planet, owner)
                    .into_iter()
                    .find(|unit| unit.type_id.as_str() == id && !unit.sustained_damage)
                    .cloned();
                if let Some(fresh) = fresh {
                    state.system_mut(&system).replace_planet_unit(
                        &planet,
                        &fresh,
                        fresh.sustained(),
                    );
                }
            }
        }
    };
    setup(&scenario["attacker"], &a, true);
    setup(&scenario["defender"], &d, false);
    state
        .system_mut(&system)
        .set_control(planet.clone(), d.clone());
    state.active = Some(a.clone());
    let mut table = ti4_engine::choice::Table::with_default(Box::new(CommitAll {
        planet: target.1.clone(),
    }));
    let mut dice = ti4_engine::dice::Dice::new();
    let mut rng = ti4_engine::rng::GameRng::new(seed);
    let report = ti4_engine::invasion::resolve(
        &mut state, content, POK, &mut table, &mut dice, &mut rng, &system, &a,
    )
    .expect("the invasion resolves");
    report.captured.iter().any(|(taken, _)| taken == &planet)
}

fn ground_engine_check(content: &'static ContentStore, scenarios: &str) {
    use rayon::prelude::*;
    let reps = parse("--reps", 1_500u64);
    let seeds = parse("--seeds", 20_000u64);
    let count = parse("--count", 150usize);
    let target = lone_planet(content);
    let scenarios: Vec<serde_json::Value> =
        serde_json::from_str(&std::fs::read_to_string(scenarios).expect("scenarios"))
            .expect("json");
    let picked: Vec<(usize, &serde_json::Value)> =
        scenarios.iter().enumerate().take(count).collect();
    println!(
        "ground: lean ({seeds}) vs engine ({reps}) on {} / {}, {} scenarios",
        target.0,
        target.1,
        picked.len()
    );
    let rows: Vec<(f64, f64, f64, usize, String)> = picked
        .par_iter()
        .map(|(index, scenario)| {
            let (_, _, shielded) = ground_side(content, &scenario["defender"], false, false);
            let (a, a_faction, _) = ground_side(content, &scenario["attacker"], shielded, true);
            let (d, _, _) = ground_side(content, &scenario["defender"], false, false);
            let mut rng = Rng::new(arena::fnv(&format!("ground-{index}")));
            let lean_wins = (0..seeds)
                .filter(|_| arena::ground_fight(&a, &d, rng.next_u64()).winner == Some("a"))
                .count();
            let engine_wins = (0..reps)
                .filter(|rep| {
                    engine_ground(content, scenario, &target, rep.wrapping_mul(7_919) + 17)
                })
                .count();
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let (lean, engine) = (
                lean_wins as f64 / seeds as f64,
                engine_wins as f64 / reps as f64,
            );
            #[expect(clippy::cast_precision_loss, reason = "counts are small")]
            let se = (lean * (1.0 - lean) / seeds as f64 + engine * (1.0 - engine) / reps as f64)
                .sqrt()
                .max(1e-9);
            ((lean - engine) / se, lean, engine, *index, a_faction)
        })
        .collect();
    let beyond = rows.iter().filter(|r| r.0.abs() > 3.0).count();
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let mean_abs = rows.iter().map(|r| (r.1 - r.2).abs()).sum::<f64>() / rows.len() as f64;
    #[expect(clippy::cast_precision_loss, reason = "counts are small")]
    let z2 = rows.iter().map(|r| r.0 * r.0).sum::<f64>() / rows.len() as f64;
    println!("  mean |gap| {mean_abs:.4}   mean z^2 {z2:.2}   |z| > 3: {beyond}");
    let mut sorted = rows;
    sorted.sort_by(|x, y| y.0.abs().total_cmp(&x.0.abs()));
    for (z, lean, engine, index, _) in sorted.iter().take(6) {
        println!(
            "    z {z:+6.1}  lean {lean:.3}  engine {engine:.3}  {} vs {}",
            scenarios[*index]["attacker"], scenarios[*index]["defender"]
        );
    }
}

fn main() {
    let content = ContentStore::embedded();
    if let Some(path) = argument("--check-ground-engine") {
        ground_engine_check(content, &path);
    } else if let Some(path) = argument("--write-ground") {
        ground_write(content, &path);
    } else if let (Some(scenarios), Some(results)) = (
        argument_at("--check-ground", 1),
        argument_at("--check-ground", 2),
    ) {
        ground_check(content, &scenarios, &results);
    } else if let Some(path) = argument("--write") {
        write(content, &path);
    } else if let (Some(scenarios), Some(results)) =
        (argument_at("--check", 1), argument_at("--check", 2))
    {
        check(content, &scenarios, &results);
    } else {
        eprintln!("usage: --write <scenarios.json> | --check <scenarios.json> <results.json>");
    }
}
