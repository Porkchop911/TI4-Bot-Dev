//! Read a captured upload into engine state and report what came across.
//!
//! ```text
//! python bridge/run_bridge.py --capture out/bridge-captures   # then !gamedata localhost in TTS
//! cargo run -p ti4-bridge --example import_capture -- out/bridge-captures/<file>.json
//! ```
//!
//! With no argument it takes the most recent capture in `out/bridge-captures`, or reads the live
//! bridge's `/latest` when given `--live`.
//!
//! This exists because the fixtures the importer was built against are from a pinned historical
//! repository, and the mod has been developed since. A capture from the table you are actually
//! going to play on is the only thing that can say whether that still holds.

use std::path::PathBuf;

use ti4_bridge::client::BridgeClient;
use ti4_bridge::import::{Telemetry, import, seats_with_nothing};
use ti4_model::content_types::FULL;

// One long function on purpose: it is a report, read top to bottom in the order it prints, and
// splitting it into helpers that are each called once would scatter that order across the file.
#[expect(clippy::too_many_lines, reason = "a report reads in printing order")]
fn main() -> std::process::ExitCode {
    // `--tile N` may appear anywhere; the remaining positional argument, if any, is the capture.
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let tile = arguments
        .iter()
        .position(|a| a == "--tile")
        .and_then(|index| arguments.get(index + 1))
        .cloned();
    let argument = arguments
        .iter()
        .find(|a| a.as_str() != "--tile" && Some(a.as_str()) != tile.as_deref());
    let payload = match argument.map(String::as_str) {
        Some("--live") => match BridgeClient::default().latest() {
            Ok(map) => serde_json::Value::Object(map),
            Err(error) => {
                eprintln!("reading the live bridge: {error}");
                return std::process::ExitCode::FAILURE;
            }
        },
        Some(path) => match read(&PathBuf::from(path)) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("{error}");
                return std::process::ExitCode::FAILURE;
            }
        },
        None => match newest() {
            Ok(value) => value,
            Err(error) => {
                eprintln!("{error}");
                return std::process::ExitCode::FAILURE;
            }
        },
    };

    let telemetry: Telemetry = match serde_json::from_value(payload) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("this capture is not telemetry the bridge reads: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    let store = ti4_content::ContentStore::embedded();
    let imported = match import(store, &telemetry, FULL) {
        Ok(imported) => imported,
        Err(error) => {
            eprintln!("import failed: {error}");
            return std::process::ExitCode::FAILURE;
        }
    };

    println!("imported a table");
    println!("  round      {}", imported.state.round);
    println!("  speaker    {}", imported.state.speaker);
    println!(
        "  active     {}",
        imported
            .state
            .active
            .as_ref()
            .map_or("nobody".to_owned(), ToString::to_string)
    );
    println!(
        "  custodians {}",
        if imported.state.custodians_removed {
            "lifted, so every round has an agenda phase"
        } else {
            "still on Mecatol"
        }
    );
    println!(
        "  tiles      {} placed, {} unknown",
        imported.galaxy.system_ids().len(),
        imported
            .gaps
            .iter()
            .filter(|g| matches!(g, ti4_bridge::import::Gap::UnknownTile(_)))
            .count()
    );

    println!("\n  seat        colour   VP  TG  cmd(t/f/s)  techs  planets  units");
    for (colour, seat) in &imported.seats {
        let Some(player) = imported.state.players.iter().find(|p| &p.id == seat) else {
            continue;
        };
        let planets = imported
            .state
            .board
            .values()
            .flat_map(|system| system.planet_control.values())
            .filter(|controller| *controller == seat)
            .count();
        let units: usize = imported
            .state
            .board
            .values()
            .map(|system| {
                system
                    .units
                    .iter()
                    .chain(system.planet_units.values().flatten())
                    .filter(|unit| &unit.owner == seat)
                    .count()
            })
            .sum();
        println!(
            "  {:<11} {:<8} {:>2}  {:>2}  {}/{}/{}       {:>4}  {:>7}  {:>5}",
            seat.to_string(),
            colour.name(),
            player.victory_points,
            player.trade_goods,
            player.tactic_tokens,
            player.fleet_tokens,
            player.strategic_tokens,
            player.technologies.len(),
            planets,
            units
        );
    }

    // `--tile 64` dumps one system, which is how a suspect import gets diagnosed against the
    // board a human is looking at.
    if let Some(wanted) = &tile {
        print_tile(&imported, wanted);
    }

    let stranded = seats_with_nothing(&imported.state);
    if stranded.is_empty() {
        println!("\n  every seat has pieces on the board");
    } else {
        println!("\n  WARNING: {stranded:?} hold nothing anywhere -- the import is wrong");
    }

    println!("\n  what the table could not say:");
    for gap in &imported.gaps {
        println!("    - {gap}");
    }

    if stranded.is_empty() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}

/// One system in full: what stands in space, and who holds each planet.
fn print_tile(imported: &ti4_bridge::import::Imported, tile: &str) {
    let padded = format!("{tile:0>2}");
    let Some((id, system)) = imported
        .state
        .board
        .iter()
        .find(|(id, _)| id.as_str() == tile || id.as_str() == padded)
    else {
        println!(
            "
  tile {tile} holds nothing"
        );
        return;
    };
    println!(
        "
  tile {id}"
    );
    let mut space: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for unit in &system.units {
        *space
            .entry(format!("{} {}", unit.owner, unit.type_id))
            .or_default() += 1;
    }
    println!("    space units   {space:?}");
    println!(
        "    command tokens {:?}",
        system
            .command_tokens
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    );
    for (planet, controller) in &system.planet_control {
        let ground: Vec<String> = system
            .planet_units
            .get(planet)
            .map(|units| {
                units
                    .iter()
                    .map(|unit| format!("{} {}", unit.owner, unit.type_id))
                    .collect()
            })
            .unwrap_or_default();
        println!("    {planet}: controlled by {controller}, ground {ground:?}");
    }
}

fn read(path: &PathBuf) -> Result<serde_json::Value, String> {
    let text =
        std::fs::read_to_string(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("parsing {}: {e}", path.display()))
}

fn newest() -> Result<serde_json::Value, String> {
    let directory = PathBuf::from("out/bridge-captures");
    let mut captures: Vec<PathBuf> = std::fs::read_dir(&directory)
        .map_err(|e| format!("reading {}: {e}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some("json"))
        .collect();
    captures.sort();
    let path = captures
        .last()
        .ok_or_else(|| format!("no captures in {}", directory.display()))?;
    println!("reading {}\n", path.display());
    read(path)
}
