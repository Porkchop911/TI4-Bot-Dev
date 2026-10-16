//! Why a large fleet projects nothing into a system next to it.
//!
//! Reads a saved review session, rebuilds its map, and for sampled frames looks at every seat's
//! fleets of two or more non-fighter ships: each adjacent system where the power map shows no ship
//! arriving is classified by the likeliest reason (command token in the fleet's own system,
//! token in the target, fleet supply, or none of those).
//!
//! ```text
//! cargo run --release -p ti4-review --example projection_gaps -- <session.review> [every]
//! ```

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::Observed;
use ti4_model::content_types::FULL;
use ti4_model::id::SystemId;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("session path");
    let every: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(25);
    let session = ti4_review::load_session(std::path::Path::new(path)).expect("session");
    let content = ContentStore::embedded();
    let galaxy = ti4_review::power::galaxy_of(&session, content).expect("map");
    let kind = |id: &str| ti4_content::units::unit_type(content, id, FULL);
    let mut reasons: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();
    for frame in session.frames.iter().step_by(every) {
        if frame.active.is_some() && frame.action_in_progress.is_some() {
            continue;
        }
        let report = ti4_review::power::report(&galaxy, frame);
        let seen = Observed::new(&frame.state, content, FULL, Some(&galaxy));
        for player in seen.players() {
            let Some(map) = report.maps.get(player) else {
                continue;
            };
            let supply = seen.fleet_supply_limit(player);
            for id in galaxy.system_ids() {
                let here = SystemId::new(id);
                let state = seen.system(&here);
                let ships = state
                    .units_of(player)
                    .into_iter()
                    .filter(|u| {
                        kind(u.type_id.as_str()).is_some_and(|k| k.is_ship() && !k.is_fighter())
                    })
                    .count();
                if ships < 2 {
                    continue;
                }
                for next in galaxy.adjacent(id) {
                    let target = SystemId::new(next);
                    let Some(power) = map.get(&target) else {
                        continue;
                    };
                    if power.arriving_ships > 0 {
                        *reasons.entry("projects (ok)").or_default() += 1;
                        continue;
                    }
                    let target_state = seen.system(&target);
                    let present_there = target_state
                        .units_of(player)
                        .into_iter()
                        .filter(|u| {
                            kind(u.type_id.as_str()).is_some_and(|k| k.is_ship() && !k.is_fighter())
                        })
                        .count();
                    let reason = if state.command_tokens.contains(player) {
                        "own token on the fleet's system (ships there cannot move)"
                    } else if usize::try_from(supply).unwrap_or(0) <= present_there {
                        "fleet supply already full at the target"
                    } else if target_state.command_tokens.contains(player) {
                        "own token on the target (cannot be activated)"
                    } else {
                        "unexplained"
                    };
                    *reasons.entry(reason).or_default() += 1;
                    if reason == "unexplained" && examples.len() < 12 {
                        examples.push(format!(
                            "frame {} round {} {player}: {ships} ships in {id} -> nothing into {next} (supply {supply})",
                            frame.index, frame.round
                        ));
                    }
                }
            }
        }
    }
    for (reason, n) in &reasons {
        println!("{n:>6}  {reason}");
    }
    for e in &examples {
        println!("  {e}");
    }
}
