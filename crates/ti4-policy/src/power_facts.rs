//! Power projection as actor features: what an activation or a move does to the seat's position.
//!
//! The critic already carries the seat's power summary ([`crate::critic::POWER_FACTS`]). These facts
//! give the *actor* the same picture per option, the way [`crate::battle::decision_facts`] gives it
//! battle odds:
//!
//! - every decision: the seat's own opportunity, best single target and reach;
//! - choosing a system to activate: what one activation could expect to take there, what the seat
//!   holds there and how likely an opponent is to take it, how many ships could arrive, and the
//!   blowback -- the strongest opponent's chance to beat the seat's projected fleet once it is there;
//! - moving a ship: what the seat holds where the ship leaves from, and how exposed that becomes
//!   with the ship gone.
//!
//! A bundle opts in by having [`NAMES`] in its vocabulary; one without them sees exactly the vectors
//! it always did. Values are scaled to sit near the other facts: planet value in tens, chances as
//! probabilities, counts in tens.

use std::collections::BTreeMap;

use ti4_engine::choice::{Choice, Observed};
use ti4_model::id::{PlayerId, SystemId};

use crate::fleet_strength::Descriptors;
use crate::power_map::{self, GROUND_CALIBRATION, SPACE_CALIBRATION, Summary, SystemPower};

/// Every fact name this module can emit. The first is the gate a bundle's vocabulary is checked for.
///
/// Option facts sit in the `action-plan` family and seat facts in `seat-state`, the same split the
/// battle and opening facts use: a seat fact is the same for every option and says where the seat
/// stands; an action fact differs between options and says what each would do.
pub const NAMES: [&str; 11] = [
    "action-plan:power-target-opportunity",
    "action-plan:power-target-take",
    "action-plan:power-target-held",
    "action-plan:power-target-threat",
    "action-plan:power-target-blowback",
    "action-plan:power-target-arriving",
    "action-plan:power-origin-held",
    "action-plan:power-origin-exposed",
    "seat-state:power-opportunity",
    "seat-state:power-best-target",
    "seat-state:power-reach",
];

/// Every seat's full power map (projection included) and the planet value each holds per system.
#[derive(Debug, Clone, Default)]
pub struct TablePower {
    pub maps: BTreeMap<PlayerId, BTreeMap<SystemId, SystemPower>>,
    pub held: BTreeMap<(PlayerId, SystemId), f64>,
}

impl TablePower {
    /// The whole table, computed fresh: a movement search for every seat and system.
    #[must_use]
    pub fn of(seen: &Observed<'_>) -> Self {
        let content = seen.content();
        let sources = seen.sources();
        let maps = seen
            .players()
            .into_iter()
            .map(|p| (p.clone(), power_map::power_map(seen, p)))
            .collect();
        let mut held: BTreeMap<(PlayerId, SystemId), f64> = BTreeMap::new();
        if let Some(galaxy) = seen.galaxy() {
            for id in galaxy.system_ids() {
                let system = SystemId::new(id);
                for (planet, owner) in &seen.system(&system).planet_control {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "planet values are single digits"
                    )]
                    let value = ti4_content::galaxy::planet(content, planet.as_str(), sources)
                        .map_or(0, |p| p.resources() + p.influence())
                        as f64;
                    *held.entry((owner.clone(), system.clone())).or_default() += value;
                }
            }
        }
        Self { maps, held }
    }

    fn holds(&self, player: &PlayerId, system: &SystemId) -> f64 {
        self.held
            .get(&(player.clone(), system.clone()))
            .copied()
            .unwrap_or(0.0)
    }

    /// The likeliest opponent's chance to take `player`'s presence in `system`, against `defence`
    /// standing in for the seat's ships and ground forces there.
    fn threat(&self, player: &PlayerId, system: &SystemId, defence: &SystemPower) -> f64 {
        let holds = self.holds(player, system) > 0.0;
        self.maps
            .iter()
            .filter(|(p, _)| *p != player)
            .filter_map(|(_, map)| map.get(system))
            .filter(|theirs| theirs.space_projection_units.count() > 0)
            .map(|theirs| {
                let space = if defence.space_defence.durability > 0.0 {
                    power_map::win_probability(
                        &theirs.space_projection,
                        &defence.space_defence,
                        SPACE_CALIBRATION,
                    )
                } else {
                    1.0
                };
                if !holds {
                    return if defence.space_defence.durability > 0.0 {
                        space
                    } else {
                        0.0
                    };
                }
                if theirs.ground_projection_units.count() == 0 {
                    return 0.0;
                }
                let ground = if defence.ground_defence.durability > 0.0 {
                    let mut landing = theirs.ground_projection;
                    landing.cannon = if defence.shielded {
                        0.0
                    } else {
                        theirs.bombardment
                    };
                    power_map::win_probability(
                        &landing,
                        &defence.ground_defence,
                        GROUND_CALIBRATION,
                    )
                } else {
                    1.0
                };
                space * ground
            })
            .fold(0.0, f64::max)
    }
}

fn scaled(value: f64) -> f64 {
    value / 10.0
}

/// The facts for one decision, one list per option (empty when nothing applies to it).
///
/// `summary` is the seat's own power summary (cheap, from a [`power_map::PowerCache`]); `table` is
/// every seat's map, needed only for activation and movement options -- callers compute it once at
/// the activation decision and keep it for the moves that follow.
#[must_use]
pub fn decision_facts(
    seen: &Observed<'_>,
    choice: &Choice,
    player: &PlayerId,
    summary: &Summary,
    table: Option<&TablePower>,
) -> Vec<Vec<(&'static str, f64)>> {
    let seat = [
        (NAMES[8], scaled(summary.opportunity)),
        (NAMES[9], scaled(summary.best_target)),
        (NAMES[10], scaled(summary.reach_systems)),
    ];
    let content = seen.content();
    let sources = seen.sources();
    let faction = seen
        .seat(player)
        .map(|s| s.faction.as_str().to_owned())
        .unwrap_or_default();
    let own_targets = table.map(|t| {
        let others: Vec<(PlayerId, BTreeMap<SystemId, SystemPower>)> = t
            .maps
            .iter()
            .filter(|(p, _)| *p != player)
            .map(|(p, m)| (p.clone(), m.clone()))
            .collect();
        let own = t.maps.get(player).cloned().unwrap_or_default();
        power_map::opportunity_by_system(seen, player, &own, &others)
    });
    choice
        .options
        .iter()
        .map(|option| {
            let mut facts: Vec<(&'static str, f64)> = seat.to_vec();
            let Some(table) = table else { return facts };
            let Some(mine) = table.maps.get(player) else {
                return facts;
            };
            if option.kind == ti4_engine::tactical::ACTIVATE_KIND {
                let system = SystemId::new(option.id.clone());
                let Some(here) = mine.get(&system) else {
                    return facts;
                };
                let opportunity = own_targets
                    .as_ref()
                    .and_then(|t| t.get(&system))
                    .copied()
                    .unwrap_or(0.0);
                let held = table.holds(player, &system);
                let total = planets_value(seen, &system);
                let takeable = (total - held).max(0.0);
                facts.push((NAMES[0], scaled(opportunity)));
                if takeable > 0.0 {
                    facts.push((NAMES[1], (opportunity / takeable).clamp(0.0, 1.0)));
                }
                if held > 0.0 {
                    facts.push((NAMES[2], scaled(held)));
                }
                let threat = table.threat(player, &system, here);
                if threat > 0.0 {
                    facts.push((NAMES[3], threat));
                }
                // After the activation the seat stands there with its whole projection.
                let mut arrived = here.clone();
                arrived.space_defence = here.space_projection;
                // The projection already counts the ships' own space cannon; add the planets' guns.
                arrived.space_defence.cannon += power_cannon(here);
                arrived.ground_defence = here.ground_projection;
                let blowback = table.threat(player, &system, &arrived);
                if blowback > 0.0 {
                    facts.push((NAMES[4], blowback));
                }
                #[expect(clippy::cast_precision_loss, reason = "ship counts are small")]
                facts.push((NAMES[5], scaled(here.arriving_ships as f64)));
            } else if option.kind == ti4_engine::tactical::MOVE_KIND {
                let Some(origin) = option
                    .payload
                    .get("origin")
                    .and_then(serde_json::Value::as_str)
                else {
                    return facts;
                };
                let origin = SystemId::new(origin);
                let held = table.holds(player, &origin);
                if held > 0.0 {
                    facts.push((NAMES[6], scaled(held)));
                    if let (Some(there), Some(unit)) = (
                        mine.get(&origin),
                        option
                            .payload
                            .get("unit")
                            .and_then(serde_json::Value::as_str),
                    ) {
                        let mut left = there.clone();
                        left.space_defence = without_one(
                            content,
                            sources,
                            &faction,
                            &there.space_defence_units,
                            unit,
                        );
                        left.space_defence.cannon += power_cannon(there);
                        let exposed = table.threat(player, &origin, &left);
                        facts.push((NAMES[7], held / 10.0 * exposed));
                    }
                }
            }
            facts
        })
        .collect()
}

/// Space cannon from the seat's planets in a system: what `space_defence.cannon` adds beyond the
/// ships themselves.
fn power_cannon(power: &SystemPower) -> f64 {
    power.ground_defence.cannon
}

fn without_one(
    content: &ti4_content::ContentStore,
    sources: ti4_model::content_types::SourceSet,
    faction: &str,
    units: &power_map::Composition,
    unit: &str,
) -> Descriptors {
    let mut remaining: Vec<(String, usize)> = units.units.clone();
    if let Some(entry) = remaining.iter_mut().find(|(id, n)| id == unit && *n > 0) {
        entry.1 -= 1;
    }
    remaining.retain(|(_, n)| *n > 0);
    crate::fleet_strength::descriptors(content, sources, faction, &remaining, &units.damaged)
}

fn planets_value(seen: &Observed<'_>, system: &SystemId) -> f64 {
    let content = seen.content();
    let sources = seen.sources();
    let planets = ti4_content::galaxy::system(content, system.as_str(), sources)
        .map(|s| s.planets())
        .unwrap_or_default();
    #[expect(
        clippy::cast_precision_loss,
        reason = "planet values are single digits"
    )]
    planets
        .iter()
        .map(|p| {
            ti4_content::galaxy::planet(content, p, sources)
                .map_or(0, |p| p.resources() + p.influence()) as f64
        })
        .sum()
}
