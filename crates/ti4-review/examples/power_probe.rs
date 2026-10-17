//! Dump the replayer's power report for sampled frames, next to the trainer's summary, so the
//! overlay's numbers can be checked against the board without the GUI.
//!
//! ```text
//! cargo run --release -p ti4-review --example power_probe -- <session> [every]
//! cargo run --release -p ti4-review --example power_probe -- <session> 1 <frame> <seat>
//! ```

use ti4_content::ContentStore;
use ti4_engine::choice::Observed;
use ti4_model::content_types::FULL;
use ti4_model::id::SystemId;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let path = args.get(1).expect("session path");
    let every: usize = args.get(2).and_then(|v| v.parse().ok()).unwrap_or(50);
    if let (Some(at), Some(who)) = (args.get(3), args.get(4)) {
        let session = ti4_review::load_session(std::path::Path::new(path)).expect("session");
        let content = ContentStore::embedded();
        let galaxy = ti4_review::power::galaxy_of(&session, content).expect("map");
        let frame = &session.frames[at.parse::<usize>().unwrap()];
        let seen = Observed::new(&frame.state, content, FULL, Some(&galaxy));
        let player = ti4_model::id::PlayerId::new(who.as_str());
        println!(
            "faction {:?} supply {}",
            seen.seat(&player).map(|s| s.faction.to_string()),
            seen.fleet_supply_limit(&player)
        );
        for id in galaxy.system_ids() {
            let st = seen.system(&SystemId::new(id));
            let space: Vec<_> = st
                .units_of(&player)
                .iter()
                .map(|u| u.type_id.to_string())
                .collect();
            let ground: Vec<_> = st
                .planet_units
                .values()
                .flatten()
                .filter(|u| u.owner == player)
                .map(|u| u.type_id.to_string())
                .collect();
            if !space.is_empty() || !ground.is_empty() {
                println!(
                    "  at {id}: space {space:?} ground {ground:?} tokens {:?}",
                    st.command_tokens
                );
            }
        }
        for id in ["72", "100", "16"] {
            println!("  adj {id}: {:?}", galaxy.adjacent(id));
        }
        for t in &session.board {
            print!(
                "{}@({},{}){} ",
                t.system,
                t.q,
                t.r,
                t.special_area.as_deref().unwrap_or("")
            );
        }
        println!();
        let map = ti4_policy::power_map::power_map(&seen, &player);
        for (id, p) in &map {
            if p.space_projection_units.count() > 0 {
                let movers: Vec<_> = seen
                    .movable_into(&player, id)
                    .iter()
                    .map(|m| {
                        format!(
                            "{}@{}{}{}",
                            m.unit.type_id,
                            m.origin.as_str(),
                            if m.gravity_drive { "+gd" } else { "" },
                            if m.ionian { "+ion" } else { "" }
                        )
                    })
                    .collect();
                println!(
                    "  -> {}: proj {:?} arriving {} movers {:?}",
                    id.as_str(),
                    p.space_projection_units.units,
                    p.arriving_ships,
                    movers
                );
            }
        }
        return;
    }
    let session = ti4_review::load_session(std::path::Path::new(path)).expect("session");
    let content = ContentStore::embedded();
    let galaxy = ti4_review::power::galaxy_of(&session, content).expect("map");
    println!(
        "board tiles {} galaxy systems {}",
        session.board.len(),
        galaxy.system_ids().len()
    );
    for frame in session.frames.iter().step_by(every) {
        let report = ti4_review::power::report(&galaxy, frame);
        let seen = Observed::new(&frame.state, content, FULL, Some(&galaxy));
        println!(
            "--- frame {} round {} {:?}",
            frame.index, frame.round, frame.phase
        );
        for seat in &report.seats {
            let trainer = ti4_policy::power_map::summary(&seen, &seat.player);
            let units: usize = galaxy
                .system_ids()
                .into_iter()
                .map(|id| seen.system(&SystemId::new(id)).units_of(&seat.player).len())
                .sum();
            let reach = seat
                .systems
                .values()
                .filter(|s| s.projection_index > 0.0)
                .count();
            let bad: Vec<String> = report.maps[&seat.player]
                .iter()
                .filter(|(id, p)| {
                    p.space_defence_units.count() > 0
                        && seat
                            .systems
                            .get(*id)
                            .is_none_or(|s| s.projection_index <= 0.0)
                })
                .map(|(id, p)| format!("{}{:?}", id.as_str(), p.space_defence_units.units))
                .collect();
            if !bad.is_empty() {
                println!(
                    "BAD {} {:?} {:?}",
                    seat.player.as_str(),
                    seen.seat(&seat.player).map(|s| s.faction.to_string()),
                    bad
                );
            }
            let mut top: Vec<_> = seat
                .systems
                .iter()
                .filter(|(_, s)| s.opportunity > 0.0)
                .map(|(id, s)| format!("{}:{:.1}/{:.0}", id.as_str(), s.opportunity, s.takeable))
                .collect();
            top.truncate(6);
            println!(
                "{:>8} units {:>3} opp {:>6.2} (trainer {:>6.2}) reach {:>2} proj>0 {:>2} exposure {:>5.2} | {}",
                seat.player.as_str(),
                units,
                seat.opportunity,
                trainer.opportunity,
                seat.reach_systems,
                reach,
                seat.exposure,
                top.join(" ")
            );
        }
    }
}

/// `power_probe <session> <frame> <seat>`: one seat's units and every system it projects into.
#[allow(dead_code)]
pub fn detail() {}
