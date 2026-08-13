//! What things are worth, to a particular player, in a particular position (M08-002).
//!
//! Ported from the oracle's `engine/valuation.py`.
//!
//! The spec asked for two things that pull against each other: index values in trade-good
//! equivalents, like a chess piece table, and valuations that are contextual and nonlinear rather
//! than fixed. Both, by composition:
//!
//! ```text
//! value = base[item] × product of context multipliers
//! ```
//!
//! The base table is a flat trade-good price and doubles as a debugging baseline — if a bot
//! misbehaves, freezing the multipliers to 1.0 says whether the fault is in the weights or in the
//! situation reading. The multipliers carry everything that depends on the position: a carrier is
//! near-worthless with spare capacity and precious when troops are stranded, a planet is worth
//! more when it completes an objective.
//!
//! Multipliers are named and clamped rather than folded into one opaque score, because a score
//! that cannot be taken apart cannot be explained, and because that is the surface a learned
//! policy is later allowed to nudge.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_engine::choice::Observed;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};

/// Rough trade-good prices. Deliberately close to build cost, adjusted for utility.
///
/// A table rather than a `match`, because two units sharing a price today is a coincidence and
/// not a shared meaning: merging the arms that happen to agree would make repricing one of them
/// silently reprice the other.
pub const BASE_UNIT_VALUE: [(&str, f64); 11] = [
    ("fighter", 0.5),
    ("infantry", 1.0),
    // Hits on 6 where infantry needs 8, and sustains damage, so it is worth well more than its
    // cost of 2 suggests — which is what the cost fallback gave it.
    ("mech", 3.5),
    ("destroyer", 1.5),
    ("cruiser", 2.0),
    ("carrier", 3.5),
    ("dreadnought", 4.0),
    ("warsun", 10.0),
    ("flagship", 8.0),
    ("spacedock", 4.0),
    ("pds", 2.0),
];

/// The printed price of a base unit type, if it has one.
#[must_use]
pub fn base_unit_value(base_type: &str) -> Option<f64> {
    BASE_UNIT_VALUE
        .iter()
        .find(|(name, _)| *name == base_type)
        .map(|(_, value)| *value)
}

/// Bounds on any single multiplier, so no one signal can dominate a score.
pub const MULTIPLIER_FLOOR: f64 = 0.25;
/// The upper bound; see [`MULTIPLIER_FLOOR`].
pub const MULTIPLIER_CEILING: f64 = 4.0;

/// Hold a multiplier inside its bounds.
#[must_use]
pub fn clamp(value: f64) -> f64 {
    value.clamp(MULTIPLIER_FLOOR, MULTIPLIER_CEILING)
}

/// What one unit is worth, by id.
///
/// Unit upgrades and faction variants fall back to their base type, then to cost — so a newly
/// added unit is priced roughly rather than at zero, which would make it free to lose.
#[must_use]
pub fn unit_value(content: &ContentStore, sources: SourceSet, unit_id: &str) -> f64 {
    if let Some(value) = base_unit_value(unit_id) {
        return value;
    }
    // A point lookup against the store's index. This is called once per option on every
    // casualty and production choice, and building the whole catalogue to answer it was the
    // single hottest thing the bot did.
    let Some(stats) = ti4_content::units::unit_type(content, unit_id, sources) else {
        return 1.0;
    };
    base_unit_value(stats.base_type()).unwrap_or_else(|| stats.cost().max(1.0))
}

// -- planets -------------------------------------------------------------------------------------

/// Named, clamped context factors. Named so a score can be explained.
#[must_use]
pub fn planet_multipliers(
    seen: &Observed<'_>,
    player: &PlayerId,
    planet: &ti4_content::galaxy::Planet<'_>,
) -> BTreeMap<&'static str, f64> {
    let mut factors = BTreeMap::new();
    if !planet.tech_specialties().is_empty() {
        factors.insert("tech_specialty", 1.4);
    }
    if planet.is_legendary() {
        factors.insert("legendary", 2.0);
    }
    if planet.id() == "mr" {
        factors.insert("mecatol", 2.5); // a victory point a round via Imperial
    }
    let own_faction = seen
        .seat(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    if planet
        .homeworld_of()
        .is_some_and(|faction| faction != own_faction)
    {
        factors.insert("enemy_homeworld", 1.3);
    }
    if has_unscored_public_goal(
        seen,
        player,
        &[
            "expand_borders",
            "subdue",
            "push_boundaries",
            "ancient_monuments",
            "lost_outposts",
        ],
    ) {
        factors.insert("public_planet_goal", 1.25);
    }
    factors.insert(
        "objective_pressure",
        clamp(1.0 + 0.15 * objective_pressure(seen, player)),
    );
    factors
}

fn has_unscored_public_goal(seen: &Observed<'_>, player: &PlayerId, aliases: &[&str]) -> bool {
    let scored = seen.scored_by(player);
    seen.revealed_objectives()
        .iter()
        .any(|goal| aliases.contains(&goal.as_str()) && !scored.contains(goal))
}

/// How close the player is to a revealed objective they could still score.
fn objective_pressure(seen: &Observed<'_>, player: &PlayerId) -> f64 {
    let scored = seen.scored_by(player);
    let outstanding = seen
        .revealed_objectives()
        .iter()
        .filter(|alias| ti4_engine::objectives::requirement_for(alias).is_some())
        .any(|alias| !scored.contains(alias));
    if !outstanding {
        return 0.0;
    }
    let held = seen.controlled_planets(player).len();
    #[expect(
        clippy::cast_precision_loss,
        reason = "a planet count is far below 2^53"
    )]
    let held = held as f64;
    (held / 3.0).min(2.0)
}

/// What taking (or holding) a planet is worth right now.
#[must_use]
pub fn planet_value(seen: &Observed<'_>, player: &PlayerId, planet: &PlanetId) -> f64 {
    let Some(record) = ti4_content::galaxy::planet(seen.content(), planet.as_str(), seen.sources())
    else {
        return 0.0;
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "printed resource and influence values are single digits"
    )]
    let base = record.resources() as f64 + 0.5 * record.influence() as f64;
    let mut total = base + 1.0; // every planet is worth something as a body count
    for factor in planet_multipliers(seen, player, &record).values() {
        total *= factor;
    }
    total
}

// -- systems -------------------------------------------------------------------------------------

/// What activating a system is worth: what can be taken, less what defends it.
#[must_use]
pub fn system_value(seen: &Observed<'_>, player: &PlayerId, system: &SystemId) -> f64 {
    let Some(galaxy) = seen.galaxy() else {
        return 0.0; // no map, so no board to read
    };
    if galaxy.coord_of(system.as_str()).is_none() {
        return 0.0; // not on this map, so there is nothing here to take
    }
    let board = seen.system(system);
    let content = seen.content();
    let sources = seen.sources();
    let types = ti4_content::units::catalogue(content, sources);

    let mut prize = 0.0;
    for record in ti4_content::galaxy::planets_in(content, system.as_str(), sources) {
        let planet = PlanetId::new(record.id());
        if board.planet_control.get(&planet) == Some(player) {
            continue; // already ours
        }
        prize += planet_value(seen, player, &planet);
    }

    let defenders: f64 = board
        .units
        .iter()
        .filter(|unit| &unit.owner != player)
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ship)
        })
        .map(|unit| unit_value(content, sources, unit.type_id.as_str()))
        .sum();
    let garrison: f64 = board
        .planet_units
        .values()
        .flatten()
        .filter(|unit| &unit.owner != player)
        .map(|unit| unit_value(content, sources, unit.type_id.as_str()))
        .sum();

    prize - 0.6 * defenders - 0.4 * garrison
}

/// What this player's ships in one system are worth.
#[must_use]
pub fn fleet_strength(seen: &Observed<'_>, player: &PlayerId, system: &SystemId) -> f64 {
    let content = seen.content();
    let sources = seen.sources();
    let types = ti4_content::units::catalogue(content, sources);
    seen.system(system)
        .units_of(player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ship)
        })
        .map(|unit| unit_value(content, sources, unit.type_id.as_str()))
        .sum()
}

/// Ground forces sitting on planets with nothing carrying them anywhere.
#[must_use]
pub fn stranded_troops(seen: &Observed<'_>, player: &PlayerId) -> usize {
    let types = ti4_content::units::catalogue(seen.content(), seen.sources());
    seen.board()
        .values()
        .map(|system| {
            system
                .planet_units
                .values()
                .flatten()
                .filter(|unit| &unit.owner == player)
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ground_force)
                })
                .count()
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::POK;

    fn store() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn table() -> ti4_model::state::GameState {
        ti4_engine::fixtures::game(&["a", "b"])
    }

    /// A position with no map. Enough for anything that reads seats and planets.
    fn watching(state: &ti4_model::state::GameState) -> Observed<'_> {
        Observed::new(state, store(), POK, None)
    }

    /// A position on a map.
    fn watching_map<'a>(
        state: &'a ti4_model::state::GameState,
        galaxy: &'a ti4_content::galaxy::Galaxy,
    ) -> Observed<'a> {
        Observed::new(state, store(), POK, Some(galaxy))
    }

    #[test]
    fn a_dreadnought_outprices_a_destroyer_and_a_war_sun_outprices_both() {
        assert!(
            unit_value(store(), POK, "dreadnought") > unit_value(store(), POK, "destroyer"),
            "the price table is not flat"
        );
        assert!(unit_value(store(), POK, "warsun") > unit_value(store(), POK, "dreadnought"));
    }

    #[test]
    fn an_upgrade_is_priced_as_its_base_type_not_at_nothing() {
        // The fallback that matters: an unpriced upgrade valued at zero would be free to lose.
        let types = ti4_content::units::catalogue(store(), POK);
        let upgrade = types
            .values()
            .find(|unit| unit.base_type() == "dreadnought" && unit.id() != "dreadnought")
            .expect("a dreadnought upgrade exists");

        assert!(
            (unit_value(store(), POK, upgrade.id()) - base_unit_value("dreadnought").unwrap())
                .abs()
                < f64::EPSILON,
            "{} priced as its base type",
            upgrade.id()
        );
    }

    #[test]
    fn an_unknown_unit_is_worth_something_rather_than_nothing() {
        assert!(unit_value(store(), POK, "no_such_unit") > 0.0);
    }

    #[test]
    fn mecatol_is_worth_more_than_its_printed_value_suggests() {
        // Its resources and influence are ordinary; what makes it worth taking is Imperial.
        let state = table();
        let player = PlayerId::new("a");
        let mecatol = planet_value(&watching(&state), &player, &PlanetId::new("mr"));

        let all = ti4_content::galaxy::all_planets(store(), POK);
        let record = all.get("mr").expect("Mecatol Rex is in the corpus");
        #[expect(clippy::cast_precision_loss, reason = "single-digit printed values")]
        let printed = record.resources() as f64 + 0.5 * record.influence() as f64 + 1.0;

        assert!(
            mecatol > 2.0 * printed,
            "mecatol {mecatol} against a printed {printed}"
        );
    }

    #[test]
    fn a_multiplier_cannot_run_away_with_the_score() {
        assert!((clamp(100.0) - MULTIPLIER_CEILING).abs() < f64::EPSILON);
        assert!((clamp(-5.0) - MULTIPLIER_FLOOR).abs() < f64::EPSILON);
        assert!((clamp(1.5) - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn the_multipliers_are_named_so_a_score_can_be_explained() {
        // A score that cannot be taken apart cannot be explained, and an unexplainable bot is
        // one nobody can debug.
        let state = table();
        let all = ti4_content::galaxy::all_planets(store(), POK);
        let record = all.get("mr").unwrap();
        let factors = planet_multipliers(&watching(&state), &PlayerId::new("a"), record);

        assert!(factors.contains_key("mecatol"));
        assert!(factors.contains_key("objective_pressure"));
    }

    #[test]
    fn objective_pressure_needs_an_objective_that_is_actually_scorable() {
        // A revealed objective with no registered requirement cannot be scored, so it must not
        // raise the value of taking anything.
        let mut state = table();
        let player = PlayerId::new("a");
        state
            .revealed_objectives
            .push(ti4_model::id::ObjectiveId::new("no_such_objective"));
        assert!(
            objective_pressure(&watching(&state), &player).abs() < f64::EPSILON,
            "an unscorable objective applies no pressure"
        );
    }

    #[test]
    fn public_planet_goal_raises_the_value_of_planets() {
        let player = PlayerId::new("a");
        let mut focused = table();
        focused.revealed_objectives.clear();
        focused
            .revealed_objectives
            .push(ti4_model::id::ObjectiveId::new("expand_borders"));
        let mut unrelated = table();
        unrelated.revealed_objectives.clear();
        unrelated
            .revealed_objectives
            .push(ti4_model::id::ObjectiveId::new("develop"));

        let planet = PlanetId::new("mr");
        let focused_factors = planet_multipliers(
            &watching(&focused),
            &player,
            &ti4_content::galaxy::all_planets(store(), POK)["mr"],
        );
        assert!(focused_factors.contains_key("public_planet_goal"));
        let focused_value = planet_value(&watching(&focused), &player, &planet);
        let unrelated_value = planet_value(&watching(&unrelated), &player, &planet);
        assert!(
            focused_value > unrelated_value,
            "focused {focused_value}, unrelated {unrelated_value}, factors {focused_factors:?}"
        );
    }

    #[test]
    fn a_defended_system_is_worth_less_than_an_empty_one() {
        let hub = ti4_engine::fixtures::plain_hub();
        let state = table();
        let player = PlayerId::new("a");
        let target = SystemId::new(hub.outer[0].clone());

        let empty = system_value(&watching_map(&state, &hub.galaxy), &player, &target);

        let mut defended = table();
        ti4_engine::fixtures::put(
            &mut defended,
            &target,
            "dreadnought",
            &PlayerId::new("b"),
            2,
        );
        let held = system_value(&watching_map(&defended, &hub.galaxy), &player, &target);

        assert!(
            held < empty,
            "defenders lower the prize: {held} against {empty}"
        );
    }

    #[test]
    fn a_system_whose_planets_you_already_hold_is_no_prize() {
        let hub = ti4_engine::fixtures::plain_hub();
        let mut state = table();
        let player = PlayerId::new("a");
        let target = SystemId::new(hub.outer[0].clone());

        let before = system_value(&watching_map(&state, &hub.galaxy), &player, &target);
        for record in ti4_content::galaxy::planets_in(store(), target.as_str(), POK) {
            state
                .system_mut(&target)
                .set_control(PlanetId::new(record.id()), player.clone());
        }
        let after = system_value(&watching_map(&state, &hub.galaxy), &player, &target);

        assert!(
            after < before,
            "taking what you hold is worth less: {after} against {before}"
        );
    }

    #[test]
    fn stranded_troops_counts_only_your_own_ground_forces() {
        let mut state = table();
        let (system, planet) = ti4_engine::fixtures::a_placed_planet();
        let mine = PlayerId::new("a");
        ti4_engine::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &mine, 3);
        ti4_engine::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("b"),
            2,
        );
        // A structure sits on the planet too, and is not a troop. There is deliberately no unit
        // in space: troops waiting only on a planet are exactly the stranded case this counts.
        ti4_engine::fixtures::put_on_planet(&mut state, &system, &planet, "pds", &mine, 1);

        assert_eq!(stranded_troops(&watching(&state), &mine), 3);
    }
}
