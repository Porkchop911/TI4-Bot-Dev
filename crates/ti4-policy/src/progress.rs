//! What a game has produced for one seat so far, in rules facts only (M09-011, M09-012).
//!
//! Ported from the oracle's `learned_policy.opening_progress` and `horizon_progress`.
//!
//! This is what the two training stages climb. Stage 1 reads the first three fields, which are
//! dense, available after one round, and almost noise-free. Stage 2 reads the rest, where victory
//! points are the objective and the scoreable counts only shape the path to them.
//!
//! Every field is a fact the rules can check. `scoreable_public` and `scoreable_secret` come from
//! the engine's own predicates — the objectives this seat could score at this instant — and are
//! not an opinion about which objective is worth chasing.
//!
//! In the oracle these live in their own module rather than on the bot, for a reason worth keeping
//! in mind here: a tool derives the authored policy's parameter schema by harvesting every
//! identifier-shaped dictionary key inside the bot class, so writing this record there silently
//! turned `planets_gained`, `systems` and `units_gained` into tunable weights that no scorer read,
//! and an evolutionary search spent mutations on knobs wired to nothing.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use ti4_engine::choice::Observed;
use ti4_model::id::PlayerId;

/// One seat's position, as the trainer sees it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Planets taken since setup.
    pub planets_gained: i64,
    /// Distinct systems holding a controlled planet, counted absolutely.
    pub systems: i64,
    /// Units gained since setup, in space and on planets alike.
    pub units_gained: i64,
    /// Points scored.
    pub victory_points: i64,
    /// Revealed public objectives this seat could score right now.
    pub scoreable_public: i64,
    /// Secret objectives this seat could score right now.
    pub scoreable_secret: i64,
    /// Which round this snapshot was taken in.
    pub round_number: u32,
}

/// What a seat held at setup, so the gains above can be deltas.
///
/// Planets and units are measured against this. A seat with no baseline reports its absolute
/// holdings as gains, which is wrong in the direction that flatters it — so the baseline is a
/// required argument rather than an option with a default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Baseline {
    /// Planets controlled at setup.
    pub planets: usize,
    /// Units owned at setup.
    pub units: usize,
}

impl Baseline {
    /// What this seat holds now, for use as a later baseline.
    #[must_use]
    pub fn taken(seen: &Observed<'_>, player: &PlayerId) -> Self {
        Self {
            planets: seen.controlled_planets(player).len(),
            units: seen.units_held(player),
        }
    }
}

/// Measure one seat's progress against its setup baseline.
#[must_use]
pub fn measure(seen: &Observed<'_>, player: &PlayerId, baseline: Baseline) -> Progress {
    let controlled = seen.controlled_planets(player);
    let systems: BTreeSet<&ti4_model::id::SystemId> =
        controlled.iter().map(|(system, _)| *system).collect();
    let units = seen.units_held(player);

    let count = |value: usize| i64::try_from(value).unwrap_or(i64::MAX);
    Progress {
        // Saturating, not wrapping. A seat that ends a round worse off than it started has gained
        // nothing; an underflow here would read as an enormous gain and clear every bar at once.
        planets_gained: count(controlled.len().saturating_sub(baseline.planets)),
        systems: count(systems.len()),
        units_gained: count(units.saturating_sub(baseline.units)),
        victory_points: seen
            .seat(player)
            .map_or(0, |seat| i64::from(seat.victory_points)),
        scoreable_public: count(seen.scoreable_public(player)),
        scoreable_secret: count(seen.scoreable_secret(player)),
        round_number: seen.round(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::{PlanetId, SystemId};
    use ti4_model::state::GameState;

    fn watching(state: &GameState) -> Observed<'_> {
        Observed::new(state, ContentStore::embedded(), POK, None)
    }

    fn hold(state: &mut GameState, player: &PlayerId, system: &str, planet: &str) {
        state
            .system_mut(&SystemId::new(system))
            .set_control(PlanetId::new(planet), player.clone());
    }

    #[test]
    fn gains_are_measured_against_the_baseline_not_the_board() {
        // The distinction the whole Stage-1 signal rests on. Factions do not start level, so an
        // absolute count measures how a faction was set up rather than what it did.
        let mut state = ti4_engine::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        hold(&mut state, &player, "26", "arretze");
        hold(&mut state, &player, "26", "hercant");

        let already = Baseline::taken(&watching(&state), &player);
        assert_eq!(
            measure(&watching(&state), &player, already).planets_gained,
            0
        );

        let fresh = Baseline::default();
        assert_eq!(measure(&watching(&state), &player, fresh).planets_gained, 2);
    }

    #[test]
    fn systems_are_absolute_and_distinct() {
        let mut state = ti4_engine::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        hold(&mut state, &player, "26", "arretze");
        hold(&mut state, &player, "26", "hercant");
        assert_eq!(
            measure(&watching(&state), &player, Baseline::default()).systems,
            1
        );

        hold(&mut state, &player, "27", "wellon");
        assert_eq!(
            measure(&watching(&state), &player, Baseline::default()).systems,
            2
        );
    }

    #[test]
    fn a_unit_counts_wherever_it_stands() {
        let mut state = ti4_engine::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        let (system, planet) = ti4_engine::fixtures::a_placed_planet();
        ti4_engine::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &player, 2);
        ti4_engine::fixtures::put(&mut state, &system, "cruiser", &player, 1);

        assert_eq!(
            measure(&watching(&state), &player, Baseline::default()).units_gained,
            3
        );
    }

    #[test]
    fn losing_ground_reads_as_no_gain_rather_than_an_enormous_one() {
        // Unsigned arithmetic: without saturation this underflows and every bar clears at once.
        let state = ti4_engine::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        let rich = Baseline {
            planets: 9,
            units: 20,
        };
        let after = measure(&watching(&state), &player, rich);
        assert_eq!(after.planets_gained, 0);
        assert_eq!(after.units_gained, 0);
    }

    #[test]
    fn the_scoreable_counts_are_rules_predicates_and_start_at_none() {
        // They must come from the engine rather than from an opinion about which objective looks
        // promising. At setup nothing is met, which is what makes them something to climb.
        let state = ti4_engine::fixtures::game(&["a"]);
        let player = PlayerId::new("a");
        let progress = measure(&watching(&state), &player, Baseline::default());
        assert_eq!(progress.scoreable_public, 0);
        assert_eq!(progress.scoreable_secret, 0);
    }

    #[test]
    fn the_round_and_the_score_come_through() {
        let mut state = ti4_engine::fixtures::game(&["a"]);
        state.round = 4;
        let player = PlayerId::new("a");
        state.player_mut(&player).unwrap().victory_points = 3;

        let progress = measure(&watching(&state), &player, Baseline::default());
        assert_eq!(progress.round_number, 4);
        assert_eq!(progress.victory_points, 3);
    }

    #[test]
    fn one_seats_progress_is_not_anothers() {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        let mine = PlayerId::new("a");
        let theirs = PlayerId::new("b");
        hold(&mut state, &theirs, "26", "arretze");
        hold(&mut state, &theirs, "27", "wellon");

        let seen = watching(&state);
        assert_eq!(measure(&seen, &mine, Baseline::default()).planets_gained, 0);
        assert_eq!(
            measure(&seen, &theirs, Baseline::default()).planets_gained,
            2
        );
    }
}
