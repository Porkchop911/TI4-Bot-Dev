//! Backward planning for the objectives that count structures: Build Defenses (4) and Construct
//! Massive Cities (7), Improve Infrastructure (planets away from home) and Protect the Border,
//! and the secrets that count PDS (Establish a Perimeter) or space docks (Fuel the War Machine).
//!
//! A structure comes from Construction: the primary of the seat's own card places two (a PDS or
//! a space dock, then a PDS), the secondary of another seat's card one (a strategy token). The
//! planner counts the structures still missing, the places that can take them (a planet it
//! controls, outside its home for the away goals, without a structure yet for those; two PDS and
//! one dock at most on a planet), and the placements it has before the status phase; the steer is
//! the Construction card, the cost the strategy tokens of the secondaries. Where the placements
//! go is the existing structure choice's business (`objectives::pick_structure`).
//!
//! # Objectives (exactly; checked by `covers_exactly_the_documented_objectives`)
//!
//! | alias | name | kind | points | family |
//! |---|---|---|---|---|
//! | `build_defenses` | Build Defenses | public | 1 | `structures` |
//! | `infrastructure` | Improve Infrastructure | public | 1 | `structures_away` |
//! | `massive_cities` | Construct Massive Cities | public | 2 | `structures` |
//! | `protect_border` | Protect the Border | public | 2 | `structures_away` |
//! | `eap` | Establish a Perimeter | secret | 1 | `units_pds` |
//! | `fwm` | Fuel the War Machine | secret | 1 | `units_spacedock` |

use ti4_engine::choice::{Observed, SeatObservation};
use ti4_model::id::PlayerId;

use super::{Cost, ObjectivePlanner, Plan, PlanCtx, Steer};

/// The family tokens this group plans.
pub const IDS: &[&str] = &[
    "structures",
    "structures_away",
    "units_pds",
    "units_spacedock",
];

/// Placements of the Construction primary of the seat's own card.
pub const OWN_PLACEMENTS: usize = 2;
/// Probability that the seat's own Construction primary is played this round.
pub const OWN_CARD_P: f64 = 0.95;
/// Probability that another seat plays Construction and the secondary is open.
pub const SECONDARY_P: f64 = 0.7;
/// Most PDS and space docks on one planet.
const PDS_PER_PLANET: usize = 2;
const DOCKS_PER_PLANET: usize = 1;

/// What the seat's planets can still take.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Room {
    /// PDS places left.
    pub pds: usize,
    /// Space dock places left.
    pub docks: usize,
    /// Structure places left (PDS and docks together).
    pub any: usize,
    /// Places left outside home that hold no structure yet.
    pub fresh_planets: usize,
}

/// Placements before the status phase, as (probability, tokens), best first.
#[must_use]
pub fn placements(own_open: bool, rivals_open: usize, strategic_tokens: i32) -> Vec<(f64, i32)> {
    let mut out: Vec<(f64, i32)> = Vec::new();
    if own_open {
        out.extend(std::iter::repeat_n((OWN_CARD_P, 0), OWN_PLACEMENTS));
    }
    let mut tokens = strategic_tokens;
    for _ in 0..rivals_open {
        if tokens >= 1 {
            tokens -= 1;
            out.push((SECONDARY_P, 1));
        }
    }
    out.sort_by(|a, b| b.0.total_cmp(&a.0));
    out
}

/// The probability and token cost of `k` placements, if the seat has that many.
#[must_use]
pub fn route_of(sources: &[(f64, i32)], k: usize) -> Option<(f64, i32)> {
    if k == 0 || k > sources.len() {
        return None;
    }
    let used = &sources[..k];
    Some((
        used.iter().map(|s| s.0).product(),
        used.iter().map(|s| s.1).sum(),
    ))
}

/// How many more places the goal's family can use at most.
#[must_use]
pub fn capacity(family: &str, room: Room) -> usize {
    match family {
        "structures" => room.any,
        "structures_away" => room.fresh_planets,
        "units_pds" => room.pds,
        "units_spacedock" => room.docks,
        _ => 0,
    }
}

fn room_of(seen: &Observed<'_>, me: &PlayerId) -> Room {
    let home = crate::home_system(seen, me);
    let mut room = Room::default();
    for (system, record) in seen.board() {
        let away = home.as_ref() != Some(system);
        for (planet, owner) in &record.planet_control {
            if owner != me {
                continue;
            }
            let (mut pds, mut docks) = (0usize, 0usize);
            for unit in record
                .planet_units
                .get(planet)
                .map_or(&[][..], Vec::as_slice)
            {
                if &unit.owner != me {
                    continue;
                }
                let id = unit.type_id.as_str();
                if id.contains("pds") {
                    pds += 1;
                } else if id.contains("spacedock") || id.contains("space_dock") {
                    docks += 1;
                }
            }
            let free_pds = PDS_PER_PLANET.saturating_sub(pds);
            let free_docks = DOCKS_PER_PLANET.saturating_sub(docks);
            room.pds += free_pds;
            room.docks += free_docks;
            room.any += free_pds + free_docks;
            if away && pds + docks == 0 {
                room.fresh_planets += 1;
            }
        }
    }
    room
}

pub fn construction_open(seen: &Observed<'_>, seat: &ti4_engine::choice::PublicSeat<'_>) -> bool {
    seat.strategy_cards.iter().any(|c| {
        !seat.exhausted_strategy_cards.contains(c)
            && ti4_engine::strategy_cards::card_name(seen.content(), c.as_str()).as_deref()
                == Some("Construction")
    })
}

/// The aliases this group plans, as documented at the top of the file.
pub const COVERS: &[&str] = &[
    "build_defenses",
    "infrastructure",
    "massive_cities",
    "protect_border",
    "eap",
    "fwm",
];

/// The structures group.
pub struct Structures;

impl ObjectivePlanner for Structures {
    fn objective_ids(&self) -> &[&str] {
        IDS
    }

    fn plan(&self, obs: &SeatObservation<'_>, ctx: &PlanCtx) -> Option<Plan> {
        let observed = obs.observed();
        let gap = ctx.goal.gap();
        if gap <= 0.0 {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small count"
        )]
        let missing = gap.ceil() as usize;
        let room = room_of(observed, &ctx.me);
        if capacity(&ctx.goal.family, room) < missing {
            return None;
        }
        let seat = observed.seat(&ctx.me)?;
        let own_open = construction_open(observed, &seat);
        let rivals = observed
            .players()
            .into_iter()
            .filter(|p| **p != ctx.me)
            .filter_map(|p| observed.seat(p))
            .filter(|s| !s.passed && construction_open(observed, s))
            .count();
        let sources = placements(own_open, rivals, seat.strategic_tokens);
        let (probability, tokens) = route_of(&sources, missing)?;
        Some(Plan {
            objective: ctx.goal.alias.clone(),
            points: ctx.goal.points,
            probability,
            cost: Cost {
                tokens,
                resources: 0,
                activations: 0,
            },
            steer: Steer {
                cards: if own_open {
                    vec!["Construction".to_owned()]
                } else {
                    Vec::new()
                },
                ..Steer::default()
            },
            reason: format!("{missing} structure(s) from Construction"),
            payment: false,
            secret: ctx.goal.secret,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placements_come_from_the_own_card_then_secondaries() {
        let s = placements(true, 1, 1);
        assert_eq!(s.len(), 3);
        let (p, tokens) = route_of(&s, 3).unwrap();
        assert!((p - 0.95 * 0.95 * 0.7).abs() < 1e-9 && tokens == 1);
        // Without the card and without a token: nothing.
        assert!(placements(false, 2, 0).is_empty());
        assert!(route_of(&placements(true, 0, 0), 3).is_none());
        assert!(route_of(&s, 0).is_none());
    }

    #[test]
    fn capacity_follows_the_family() {
        let room = Room {
            pds: 5,
            docks: 1,
            any: 6,
            fresh_planets: 2,
        };
        assert_eq!(capacity("structures", room), 6);
        assert_eq!(capacity("structures_away", room), 2);
        assert_eq!(capacity("units_pds", room), 5);
        assert_eq!(capacity("units_spacedock", room), 1);
        assert_eq!(capacity("fleet_in_one_system", room), 0);
    }

    #[test]
    fn covers_exactly_the_documented_objectives() {
        assert_eq!(
            crate::backward::coverage::matched(&Structures),
            crate::backward::coverage::sorted(COVERS)
        );
    }
}
