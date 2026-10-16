//! Did the power map say a fleet could reach the system it then actually moved into?
//!
//! For every tactical action in a saved session, compares the board before the action with the board
//! after it: non-fighter ships of the acting seat that left some systems and arrived in another. If
//! ships arrived but the power map at the frame before the activation showed nothing arriving there,
//! the projection missed a legal move. Each miss is printed with what the map and the board showed.
//!
//! ```text
//! cargo run --release -p ti4-review --example projection_vs_moves -- <session.review>
//! ```

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::Observed;
use ti4_model::content_types::FULL;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

fn ships(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (system, board) in &state.board {
        for unit in board.units_of(player) {
            let Some(k) = ti4_content::units::unit_type(content, unit.type_id.as_str(), FULL)
            else {
                continue;
            };
            if k.is_ship() && !k.is_fighter() {
                out.entry(system.to_string())
                    .or_default()
                    .push(unit.type_id.to_string());
            }
        }
    }
    out
}

fn main() {
    let path = std::env::args().nth(1).expect("session path");
    let session = ti4_review::load_session(std::path::Path::new(&path)).expect("session");
    let content = ContentStore::embedded();
    let galaxy = ti4_review::power::galaxy_of(&session, content).expect("map");
    let (mut checked, mut missed) = (0, 0);
    for frame in &session.frames {
        let Some(summary) = &frame.action_summary else {
            continue;
        };
        if !summary.details.iter().any(|d| d.starts_with("Activated")) {
            continue;
        }
        let (Some(before), Some(after)) = (
            session.frames.get(summary.start_frame),
            session.frames.get(summary.end_frame),
        ) else {
            continue;
        };
        let actor = PlayerId::new(&summary.actor);
        let (pre, post) = (
            ships(&before.state, content, &actor),
            ships(&after.state, content, &actor),
        );
        let count = |m: &BTreeMap<String, Vec<String>>, s: &str| m.get(s).map_or(0, Vec::len);
        let systems: std::collections::BTreeSet<&String> = pre.keys().chain(post.keys()).collect();
        let left: usize = systems
            .iter()
            .map(|s| count(&pre, s).saturating_sub(count(&post, s)))
            .sum();
        let Some((destination, gained)) = systems
            .iter()
            .map(|s| (*s, count(&post, s).saturating_sub(count(&pre, s))))
            .max_by_key(|(_, g)| *g)
        else {
            continue;
        };
        let moved = left.min(gained);
        if moved == 0 {
            continue;
        }
        checked += 1;
        let report = ti4_review::power::report(&galaxy, before);
        let target = SystemId::new(destination.as_str());
        let arriving = report
            .maps
            .get(&actor)
            .and_then(|m| m.get(&target))
            .map_or(0, |p| p.arriving_ships);
        if arriving == 0 {
            missed += 1;
            let seen = Observed::new(&before.state, content, FULL, Some(&galaxy));
            let origins: Vec<String> = systems
                .iter()
                .filter(|s| count(&pre, s) > count(&post, s))
                .map(|s| {
                    let token = seen
                        .system(&SystemId::new(s.as_str()))
                        .command_tokens
                        .contains(&actor);
                    format!(
                        "{s}{} {:?}",
                        if token { "(own token)" } else { "" },
                        pre.get(*s)
                    )
                })
                .collect();
            let movable = seen.movable_into(&actor, &target).len();
            println!(
                "MISS frame {}..{} {} moved {moved} into {destination} from [{}] | map arriving 0, movable_into now {movable}, \
                 phase {:?}, active system {:?}, target token {}",
                summary.start_frame,
                summary.end_frame,
                actor,
                origins.join("; "),
                before.phase,
                before.state.active_system,
                seen.system(&target).command_tokens.contains(&actor),
            );
        }
    }
    println!(
        "tactical actions with ships moving: {checked}, projection showed nothing arriving: {missed}"
    );
}
