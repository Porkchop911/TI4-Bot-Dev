//! The objective planner's production weights (bot version 2, [`crate::tuning::planner_weight`]).
//!
//! From the objective round on, each unscored objective (public or its own secret) that more of
//! one kind of unit would bring closer, and that is at most [`crate::tuning::planner_gap`] steps
//! short, raises that unit's production weight by `planner_weight` per point over the gap: a
//! flagship or war sun for Engineer a Marvel / Achieve Supremacy, warships for Raise a Fleet /
//! Command an Armada and the presence objectives, dreadnoughts, PDS, docks, mechs and ground
//! forces for the secrets that count them. Bought objectives and met ones are left out. It reads
//! only the seat's own observation.

use ti4_content::units::UnitType;
use ti4_engine::choice::SeatObservation;
use ti4_engine::objectives::Imagined;
use ti4_model::id::PlayerId;

use crate::{objectives, tuning};

/// Extra production weight per unit kind.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Bias {
    warship: f64,
    capital: f64,
    dreadnought: f64,
    ground: f64,
    mech: f64,
    pds: f64,
    dock: f64,
    /// Unit types a chosen backward plan wants built (bot version 4).
    steered: Vec<String>,
}

impl Bias {
    /// The seat's weights now; none before the objective round or with the planner off.
    #[must_use]
    pub fn of(seen: &SeatObservation<'_>, me: &PlayerId) -> Self {
        let w = tuning::planner_weight();
        let observed = seen.observed();
        let steered = crate::backward::verdict(seen, me)
            .map(|v| v.builds())
            .unwrap_or_default();
        if w <= 0.0 || observed.round() < tuning::objective_round() || !tuning::objectives_enabled()
        {
            return Self {
                steered,
                ..Self::default()
            };
        }
        let mut bias = Self {
            steered,
            ..Self::default()
        };
        for goal in objectives::goals(seen, me, &Imagined::NONE) {
            if goal.satisfied || goal.bought || goal.gap() > tuning::planner_gap() {
                continue;
            }
            let add = w * goal.points / goal.gap().max(1.0);
            let slot = match goal.family.as_str() {
                "fleet_in_one_system" => &mut bias.warship,
                "capital_ship_systems" | "capital_ships_rival_or_mecatol" => &mut bias.capital,
                "ships_in_systems"
                | "planetless_systems"
                | "ship_systems_beside_anomaly"
                | "ship_systems_beside_rival_home"
                | "ships_adjacent_to_mecatol"
                | "rival_dock_systems_with_ships"
                | "mecatol_ships_while_controlling" => &mut bias.warship,
                "units_dreadnought" => &mut bias.dreadnought,
                "units_pds" => &mut bias.pds,
                "units_spacedock" => &mut bias.dock,
                "mechs_on_distinct_planets" => &mut bias.mech,
                "ground_forces_on_one_planet" => &mut bias.ground,
                _ => continue,
            };
            *slot += add;
        }
        bias
    }

    /// The multiplier for producing `kind`.
    #[must_use]
    pub fn weight(&self, kind: UnitType<'_>) -> f64 {
        let id = kind.id();
        let mut w = 1.0;
        if self.steered.iter().any(|b| b == id) {
            w += crate::tuning::bw_build_weight();
        }
        if kind.is_ship() && !kind.is_fighter() {
            w += self.warship;
        }
        if id.contains("flagship") || id.contains("warsun") {
            w += self.capital;
        }
        if id.contains("dreadnought") {
            w += self.dreadnought;
        }
        if kind.is_ground_force() {
            w += self.ground;
            if id.contains("mech") {
                w += self.mech;
            }
        }
        if id.contains("pds") {
            w += self.pds;
        }
        if id.contains("spacedock") || id.contains("space_dock") {
            w += self.dock;
        }
        w
    }
}
