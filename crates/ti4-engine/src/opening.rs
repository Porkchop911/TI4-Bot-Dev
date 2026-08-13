//! What a faction achieved in its opening round, and whether that clears a bar (M09-011).
//!
//! Ported from the oracle's `engine/opening.py`.
//!
//! Round-4 victory points have a standard deviation of about 1.4 per player-game and are mostly
//! interaction, so from zero weights they are very nearly pure noise — a search run against them
//! selects on luck for a long time before it selects on play. The three quantities here are dense,
//! available after one round instead of four, and almost noise-free, which is what makes them
//! usable as a training signal before victory points are.
//!
//! They are also a direct measurement of defects seen on a live table: a faction capping at two
//! planets in five of six openings, another blocked on more than half its ground-force loads. A
//! faction that never delivers troops cannot take a planet, and both show up here as a flat
//! planets-gained of zero.
//!
//! Nothing in this module scores or judges play beyond the bar it is given. It reports what is on
//! the board.

use std::collections::{BTreeMap, BTreeSet};

use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

/// The bar one faction must clear in round one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirement {
    /// Planets taken during the round, over and above those held at setup.
    ///
    /// A delta rather than an absolute count, because the factions do not start level: Hacan
    /// begins on three planets and Letnev and Jol-Nar on two, so an absolute "control three
    /// planets" is already met at setup by one of them and measures nothing. Measured over 240
    /// six-player games, an absolute bar of three planets and two systems was cleared 100% of the
    /// time by all six factions — a gate that is always open is not a gate.
    pub planets_gained: usize,
    /// Distinct systems containing a controlled planet, counted absolutely.
    ///
    /// Absolute rather than a delta because every faction starts at one of these, and three means
    /// genuinely spreading out.
    pub systems: usize,
    /// Strictly more units than the faction deployed with. One is enough; the check is that the
    /// seat built something, not how much.
    ///
    /// "Unit" is the LRR sense — ships, ground forces and structures alike — so a space dock or
    /// PDS counts. Those are *constructed* rather than produced, so in principle this bar could be
    /// cleared without producing anything. Measured over 60 games it is not: every faction gains
    /// at least two ships on its own.
    pub units_gained: usize,
}

impl Default for Requirement {
    fn default() -> Self {
        DEFAULT_REQUIREMENT
    }
}

/// The stated shape, applied to every faction unless a per-faction bar is given.
pub const DEFAULT_REQUIREMENT: Requirement = Requirement {
    planets_gained: 3,
    systems: 3,
    units_gained: 1,
};

/// One seat's round-one position, and whether it cleared its bar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opening {
    /// Whose position this is.
    pub player: PlayerId,
    /// The faction in that seat.
    pub faction: String,
    /// Planets controlled now.
    pub planets: usize,
    /// Distinct systems holding one of those planets.
    pub systems: usize,
    /// Units owned now, in space and on planets.
    pub units: usize,
    /// Planets gained, measured against the same seat at setup.
    pub planets_gained: usize,
    /// Units gained, likewise.
    pub units_gained: usize,
    /// The bar this seat was measured against.
    pub requirement: Requirement,
}

impl Opening {
    /// Whether enough planets were taken.
    #[must_use]
    pub const fn planets_ok(&self) -> bool {
        self.planets_gained >= self.requirement.planets_gained
    }

    /// Whether the seat spread across enough systems.
    #[must_use]
    pub const fn systems_ok(&self) -> bool {
        self.systems >= self.requirement.systems
    }

    /// Whether the seat built anything.
    #[must_use]
    pub const fn units_ok(&self) -> bool {
        self.units_gained >= self.requirement.units_gained
    }

    /// Whether all three parts are met.
    #[must_use]
    pub const fn cleared(&self) -> bool {
        self.planets_ok() && self.systems_ok() && self.units_ok()
    }

    /// Planets short of the bar.
    #[must_use]
    pub const fn planet_shortfall(&self) -> usize {
        self.requirement
            .planets_gained
            .saturating_sub(self.planets_gained)
    }

    /// Systems short of the bar.
    #[must_use]
    pub const fn system_shortfall(&self) -> usize {
        self.requirement.systems.saturating_sub(self.systems)
    }

    /// Units short of the bar.
    #[must_use]
    pub const fn unit_shortfall(&self) -> usize {
        self.requirement
            .units_gained
            .saturating_sub(self.units_gained)
    }

    /// How far off the bar, summed over the three parts. Zero when cleared.
    ///
    /// A graded distance rather than a pass/fail bit, because a search needs to see that two
    /// planets is closer than one. Pass/fail alone is flat everywhere below the bar and gives a
    /// from-zero policy nothing to climb.
    #[must_use]
    pub const fn shortfall(&self) -> usize {
        self.planet_shortfall() + self.system_shortfall() + self.unit_shortfall()
    }

    /// Shortfall with the three parts priced separately.
    ///
    /// The unweighted sum treats a missing planet, a missing system and a missing unit as equally
    /// far from the bar. Measured, they are nothing like equally hard: every seat clears the unit
    /// requirement trivially — one faction gains 6.90 units against a bar of 1 — while the planet
    /// and system parts are what actually fail.
    ///
    /// Pricing expansion above units puts the gradient where the failure is. It does not change
    /// [`Opening::cleared`], which is still all three parts met.
    #[must_use]
    pub fn weighted_shortfall(&self, expansion: f64, unit: f64) -> f64 {
        #[expect(clippy::cast_precision_loss, reason = "shortfalls are single digits")]
        let expansion_part = (self.planet_shortfall() + self.system_shortfall()) as f64;
        #[expect(clippy::cast_precision_loss, reason = "shortfalls are single digits")]
        let unit_part = self.unit_shortfall() as f64;
        expansion * expansion_part + unit * unit_part
    }
}

/// Controlled planets, and how many distinct systems they lie in.
fn planets_of(state: &GameState, player: &PlayerId) -> (usize, usize) {
    let controlled = state.controlled_planets(player);
    let systems: BTreeSet<&ti4_model::id::SystemId> =
        controlled.iter().map(|(system, _)| *system).collect();
    (controlled.len(), systems.len())
}

/// Every unit the player owns, in space **and** on planets.
///
/// Ground forces and structures live on planets rather than in the space area. Counting only the
/// space area counted ships and nothing else, which made "the seat built something" mean "the seat
/// built a ship" — so an opening that produced two infantry and took two planets with them scored
/// as having built nothing at all.
fn units_of(state: &GameState, player: &PlayerId) -> usize {
    state
        .board
        .values()
        .map(|system| {
            system.units_of(player).len()
                + system
                    .planet_units
                    .values()
                    .flatten()
                    .filter(|unit| &unit.owner == player)
                    .count()
        })
        .sum()
}

/// Planets held and units on the board per player, for use as a later baseline.
///
/// Taken before the game runs. Both deltas are measured against this, so a caller that forgets it
/// gets no deltas rather than wrong ones.
#[must_use]
pub fn snapshot(state: &GameState) -> BTreeMap<PlayerId, (usize, usize)> {
    state
        .players
        .iter()
        .map(|seat| {
            (
                seat.id.clone(),
                (planets_of(state, &seat.id).0, units_of(state, &seat.id)),
            )
        })
        .collect()
}

/// Every seat's opening position, keyed by player id.
///
/// `start` is a [`snapshot`] of the setup state. `requirements` is keyed by faction alias; any
/// faction not named there takes [`DEFAULT_REQUIREMENT`].
#[must_use]
pub fn measure(
    state: &GameState,
    start: &BTreeMap<PlayerId, (usize, usize)>,
    requirements: &BTreeMap<String, Requirement>,
) -> BTreeMap<PlayerId, Opening> {
    state
        .players
        .iter()
        .map(|seat| {
            let (planets, systems) = planets_of(state, &seat.id);
            let units = units_of(state, &seat.id);
            let (began_planets, began_units) = start.get(&seat.id).copied().unwrap_or((0, 0));
            let faction = seat.faction.to_string();
            let opening = Opening {
                player: seat.id.clone(),
                faction: faction.clone(),
                planets,
                systems,
                units,
                planets_gained: planets.saturating_sub(began_planets),
                units_gained: units.saturating_sub(began_units),
                requirement: requirements
                    .get(&faction)
                    .copied()
                    .unwrap_or(DEFAULT_REQUIREMENT),
            };
            (seat.id.clone(), opening)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{FactionId, PlanetId, SystemId};

    fn seat(state: &mut GameState, player: &PlayerId, faction: &str) {
        state.player_mut(player).unwrap().faction = FactionId::new(faction);
    }

    fn hold(state: &mut GameState, player: &PlayerId, system: &str, planet: &str) {
        state
            .system_mut(&SystemId::new(system))
            .set_control(PlanetId::new(planet), player.clone());
    }

    fn opening_of(state: &GameState, start: &BTreeMap<PlayerId, (usize, usize)>) -> Opening {
        measure(state, start, &BTreeMap::new())
            .remove(&PlayerId::new("a"))
            .expect("a is seated")
    }

    #[test]
    fn planets_are_counted_as_a_gain_not_as_a_total() {
        // The distinction the bar rests on. Factions do not start level — one begins on three
        // planets and others on two — so an absolute "control three planets" is already met at
        // setup by one of them, and a gate that is always open is not a gate.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        seat(&mut state, &player, "hacan");
        hold(&mut state, &player, "26", "arretze");
        hold(&mut state, &player, "26", "hercant");
        hold(&mut state, &player, "26", "kamdorn");

        // Measured against a setup that already held all three: nothing was gained.
        let start = snapshot(&state);
        let held = opening_of(&state, &start);
        assert_eq!(held.planets, 3, "three planets are held");
        assert_eq!(held.planets_gained, 0, "and none of them were taken");
        assert!(!held.planets_ok(), "so the planet bar is not met");

        // The same board, measured against a setup that held none.
        let empty = BTreeMap::new();
        let gained = opening_of(&state, &empty);
        assert_eq!(gained.planets_gained, 3);
        assert!(gained.planets_ok());
    }

    #[test]
    fn systems_are_counted_absolutely_and_distinctly() {
        // Three planets in one system is not spreading out. `systems` is what says so.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        hold(&mut state, &player, "26", "arretze");
        hold(&mut state, &player, "26", "hercant");
        hold(&mut state, &player, "26", "kamdorn");

        let huddled = opening_of(&state, &BTreeMap::new());
        assert_eq!(huddled.planets, 3);
        assert_eq!(huddled.systems, 1, "all in one system");
        assert!(huddled.planets_ok(), "the planet bar is met");
        assert!(!huddled.systems_ok(), "the system bar is not");
        assert!(!huddled.cleared());
    }

    #[test]
    fn a_unit_is_a_unit_wherever_it_stands() {
        // Ground forces and structures live on planets, not in the space area. Counting only
        // space made "the seat built something" mean "the seat built a ship", so an opening that
        // produced two infantry and took two planets with them scored as having built nothing.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player, 2);

        let ground_only = opening_of(&state, &BTreeMap::new());
        assert_eq!(ground_only.units, 2, "troops on a planet are units");
        assert!(ground_only.units_ok());

        crate::fixtures::put(&mut state, &system, "cruiser", &player, 1);
        assert_eq!(opening_of(&state, &BTreeMap::new()).units, 3);
    }

    #[test]
    fn only_your_own_units_count() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let mine = PlayerId::new("a");
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &mine, 1);
        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "infantry",
            &PlayerId::new("b"),
            4,
        );
        crate::fixtures::put(&mut state, &system, "cruiser", &PlayerId::new("b"), 3);

        assert_eq!(opening_of(&state, &BTreeMap::new()).units, 1);
    }

    #[test]
    fn losing_ground_shows_as_a_gain_of_none_rather_than_a_negative() {
        // A seat that ends round one worse off than it started has gained nothing. Saturating
        // rather than wrapping: an underflow here would read as an enormous gain and clear every
        // bar at once.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        let start: BTreeMap<PlayerId, (usize, usize)> =
            [(player.clone(), (5, 9))].into_iter().collect();

        let lost = opening_of(&state, &start);
        assert_eq!(lost.planets_gained, 0);
        assert_eq!(lost.units_gained, 0);
        assert!(!lost.cleared());
        let _ = &mut state;
    }

    #[test]
    fn the_shortfall_is_graded_rather_than_pass_or_fail() {
        // A search needs to see that two planets is closer than one. Pass/fail alone is flat
        // everywhere below the bar and gives a from-zero policy nothing to climb.
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");

        let nothing = opening_of(&state, &BTreeMap::new());
        assert_eq!(nothing.shortfall(), 3 + 3 + 1);

        hold(&mut state, &player, "26", "arretze");
        let some = opening_of(&state, &BTreeMap::new());
        assert!(
            some.shortfall() < nothing.shortfall(),
            "one planet is closer than none: {} against {}",
            some.shortfall(),
            nothing.shortfall()
        );
    }

    #[test]
    fn a_cleared_opening_has_no_shortfall() {
        let mut state = crate::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        for (system, planet) in [("26", "arretze"), ("27", "wellon"), ("28", "vefutii")] {
            hold(&mut state, &player, system, planet);
        }
        let (system, planet) = crate::fixtures::a_placed_planet();
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player, 1);

        let cleared = opening_of(&state, &BTreeMap::new());
        assert!(cleared.cleared(), "{cleared:?}");
        assert_eq!(cleared.shortfall(), 0);
    }

    #[test]
    fn expansion_is_priced_above_units_where_the_failure_actually_is() {
        // Every seat clears the unit bar trivially; the planet and system parts are what fail.
        // A weighting that treated them alike would put the gradient where nothing is wrong.
        let state = crate::fixtures::game(&["a"]);
        let missing_everything = opening_of(&state, &BTreeMap::new());

        let priced = missing_everything.weighted_shortfall(2.0, 1.0);
        let flat = missing_everything.weighted_shortfall(1.0, 1.0);
        assert!(priced > flat, "{priced} against {flat}");

        #[expect(clippy::cast_precision_loss, reason = "single-digit shortfalls")]
        let unweighted = missing_everything.shortfall() as f64;
        assert!(
            (flat - unweighted).abs() < f64::EPSILON,
            "weights of one reproduce the plain sum"
        );
    }

    #[test]
    fn a_per_faction_bar_overrides_the_default_and_nothing_else_moves() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        seat(&mut state, &PlayerId::new("a"), "sol");
        seat(&mut state, &PlayerId::new("b"), "hacan");
        let easier = Requirement {
            planets_gained: 0,
            systems: 0,
            units_gained: 0,
        };
        let bars: BTreeMap<String, Requirement> =
            [("sol".to_owned(), easier)].into_iter().collect();

        let measured = measure(&state, &BTreeMap::new(), &bars);
        assert!(measured[&PlayerId::new("a")].cleared(), "sol's bar is met");
        assert_eq!(
            measured[&PlayerId::new("b")].requirement,
            DEFAULT_REQUIREMENT,
            "hacan keeps the default"
        );
        assert!(!measured[&PlayerId::new("b")].cleared());
    }

    #[test]
    fn a_seated_game_starts_below_the_bar() {
        // The property that makes this usable as a signal at all: if setup already cleared it,
        // there would be nothing for round one to achieve. Measured against a real deployment
        // rather than an invented one.
        let content = ContentStore::embedded();
        let players = [PlayerId::new("a")];
        let mut state = crate::setup::start_game(content, &players, POK, None).unwrap();
        let faction = FactionId::new("sol");
        crate::seating::deploy(&mut state, content, &players[0], &faction, POK).unwrap();

        let start = snapshot(&state);
        let opening = measure(&state, &start, &BTreeMap::new())
            .remove(&players[0])
            .unwrap();

        assert!(opening.units > 0, "a deployed seat owns units");
        assert!(
            !opening.cleared(),
            "setup alone must not clear the bar: {opening:?}"
        );
    }
}
