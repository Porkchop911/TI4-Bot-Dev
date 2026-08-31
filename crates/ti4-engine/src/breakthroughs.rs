//! Breakthrough abilities (Thunder's Edge), for the six trained factions.
//!
//! A breakthrough is a faction card gained through an expedition. `thunders_edge` grants it; this
//! is what having it *does*.
//!
//! > **4.** A breakthrough has 1 or more abilities, and a synergy.
//! >
//! > **4.1.** If a breakthrough has a passive ability and an exhaust ability, the passive ability
//! > always remains in effect, even when the breakthrough is exhausted.
//! >
//! > **4.2.** If a breakthrough has an exhaust ability, the synergy ability always remains in
//! > effect, even when the breakthrough is exhausted.
//! >
//! > **6.** A breakthrough itself is not a technology; it alone cannot be used to meet a
//! > prerequisite when researching a technology, nor can it alone be used to meet the technology
//! > requirement of an objective.
//!
//! Rule 6 holds by construction: a breakthrough lives in `Player::breakthrough`, and every count of
//! owned technologies reads `Player::technologies`. The test here exists to keep that true.
//!
//! The synergy half of every breakthrough is in [`crate::synergy`]. This module is the printed
//! abilities, registered per faction so [`unimplemented`] can say which are still missing — the same
//! pattern the rest of the engine uses for content.
//!
//! ## The six
//!
//! | faction | breakthrough | ability | here |
//! |---|---|---|---|
//! | letnev | Gravleash Maneuvers | move values levelled; +X in space combat | movement done, combat bonus outstanding |
//! | xxcha  | Archon's Gift | resources and influence are interchangeable | done |
//! | l1z1x  | Fealty Uplink | gaining a planet places infantry equal to its influence | done |
//! | sol    | Bellum Gloriosum | free ground/fighters with a capacity ship | done |
//! | hacan  | Auto-Factories | a fleet token for producing 3+ non-fighter ships | done |
//! | jolnar | Specialist Compounds | exhaust a specialty planet instead of paying to research | outstanding |

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{BreakthroughId, PlanetId, PlayerId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

/// The breakthrough aliases whose printed abilities this module implements.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec!["hacanbt", "letnevbt", "solbt", "xxchabt", "l1z1xbt"]
}

/// Breakthroughs belonging to the trained factions whose abilities are not implemented.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet, factions: &[&str]) -> Vec<BreakthroughId> {
    let known = registered_aliases();
    content
        .from_sources(ti4_model::content_types::ContentType::Breakthroughs, sources)
        .filter(|record| {
            record
                .text("faction")
                .is_some_and(|faction| factions.contains(&faction))
        })
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !known.contains(alias))
        .map(BreakthroughId::new)
        .collect()
}

/// Hacan, Auto-Factories: producing three or more non-fighter ships adds a fleet token.
///
/// > When you produce 3 or more non-fighter ships, place 1 command token from your reinforcements
/// > into your fleet pool.
///
/// Counted over the whole use of PRODUCTION rather than per placement: "when you produce 3 or more"
/// is one condition on one use, and checking per placement would pay three times for three ships.
/// The corpus note says the token arrives *before* fleet limits are resolved, which is why this is
/// called where the use finishes rather than after supply is enforced.
///
/// Returns whether a token was placed.
pub fn on_production_finished(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    produced: &[(UnitTypeId, String)],
) -> bool {
    if !holds(state, player, "hacanbt") {
        return false;
    }
    let types = ti4_content::units::catalogue(content, sources);
    let ships = produced
        .iter()
        .filter(|(kind, _)| {
            types.get(kind.as_str()).is_some_and(|unit| {
                unit.is_ship() && !unit.is_fighter()
            })
        })
        .count();
    if ships < 3 {
        return false;
    }
    if let Some(seat) = state.player_mut(player) {
        seat.gain_token(ti4_model::state::TokenPool::Fleet, 1);
    }
    true
}

/// Sol, Bellum Gloriosum: a capacity ship carries free ground forces and fighters.
///
/// > When you produce a ship that has capacity, you may also produce any combination of ground
/// > forces or fighters up to that ship's capacity; they do not count against your PRODUCTION
/// > limit.
///
/// Two separate things, and only the second is modelled here. *Being allowed* to produce them is
/// already true -- nothing stops a player buying fighters -- so what the card actually changes is
/// that they stop consuming the production limit. The allowance is therefore a budget the ship
/// opens and the small units spend, rather than a new purchase path.
///
/// The allowance is per ship and per use: a carrier with capacity four opens four, and a second
/// carrier opens four more.
#[must_use]
pub fn free_capacity_granted(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    produced: &UnitTypeId,
) -> i64 {
    if !holds(state, player, "solbt") {
        return 0;
    }
    ti4_content::units::unit_type(content, produced.as_str(), sources)
        .filter(ti4_content::UnitType::is_ship)
        .map_or(0, |kind| kind.capacity())
}

/// Whether a produced unit may be paid for out of that allowance.
///
/// "Any combination of ground forces or fighters" -- so a fighter or an infantry, and nothing else.
#[must_use]
pub fn spends_free_capacity(
    content: &ContentStore,
    sources: SourceSet,
    produced: &UnitTypeId,
) -> bool {
    ti4_content::units::unit_type(content, produced.as_str(), sources)
        .is_some_and(|kind| kind.is_fighter() || kind.is_ground_force())
}

/// Whether this player holds this breakthrough.
#[must_use]
pub fn holds(state: &GameState, player: &PlayerId, alias: &str) -> bool {
    state
        .player(player)
        .and_then(|seat| seat.breakthrough.as_ref())
        .is_some_and(|held| held.as_str() == alias)
}

/// L1Z1X, Fealty Uplink: gaining a planet places infantry equal to its influence.
///
/// > When you gain control of a planet, place infantry from your reinforcements equal to that
/// > planet's influence value on that planet.
///
/// Called wherever control is gained rather than only from an invasion, because "when you gain
/// control" does not say how. Space stations are excluded: they are not planets, and the corrected
/// control path can hand one over for merely being alone in the system, which would otherwise mint
/// infantry for flying past.
///
/// Infantry are not limited by plastic (LRR 31.4 and the fighter/infantry exemption in
/// [`crate::supply`]), so reinforcements never run out here.
///
/// Returns how many were placed.
pub fn on_gain_control(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> usize {
    if !holds(state, player, "l1z1xbt") {
        return 0;
    }
    if ti4_content::galaxy::is_space_station(content, planet.as_str(), sources) {
        return 0;
    }
    let Some(record) = ti4_content::galaxy::planet(content, planet.as_str(), sources) else {
        return 0;
    };
    let influence = usize::try_from(record.influence()).unwrap_or(0);
    if influence == 0 {
        return 0;
    }
    let standing = state.system_mut(system).planet_units.entry(planet.clone()).or_default();
    for _ in 0..influence {
        standing.push(Unit::new(UnitTypeId::new("infantry"), player.clone()));
    }
    influence
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;

    use super::*;

    fn seat_with(alias: &str) -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        let seat = state.player_mut(&player).expect("seated");
        seat.faction = ti4_model::id::FactionId::new("l1z1x");
        seat.breakthrough = Some(BreakthroughId::new(alias));
        (state, player)
    }

    #[test]
    fn fealty_uplink_places_infantry_equal_to_influence() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seat_with("l1z1xbt");
        // Bellatrix: a real planet with a known influence value.
        let system = SystemId::new("109");
        let planet = PlanetId::new("bellatrix");
        let influence = ti4_content::galaxy::planet(content, "bellatrix", ALL_SOURCES)
            .expect("in the corpus")
            .influence();
        assert!(influence > 0, "the fixture planet must carry influence");

        let placed = on_gain_control(
            &mut state, content, ALL_SOURCES, &player, &system, &planet,
        );
        assert_eq!(placed, usize::try_from(influence).unwrap());
        assert_eq!(
            state.system_state(&system).on_planet_of(&planet, &player).len(),
            usize::try_from(influence).unwrap()
        );
    }

    /// Auto-Factories pays once for a use of three or more non-fighter ships, and not below three.
    #[test]
    fn auto_factories_pays_once_for_three_non_fighter_ships() {
        use ti4_model::state::TokenPool;
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seat_with("hacanbt");

        let made = |kinds: &[&str]| -> Vec<(UnitTypeId, String)> {
            kinds
                .iter()
                .map(|k| (UnitTypeId::new(*k), "space".to_owned()))
                .collect()
        };

        let before = state.player(&player).expect("seated").tokens(TokenPool::Fleet);
        assert!(
            !on_production_finished(
                &mut state, content, ALL_SOURCES, &player,
                &made(&["cruiser", "cruiser"])
            ),
            "two is not three"
        );
        assert!(
            !on_production_finished(
                &mut state, content, ALL_SOURCES, &player,
                &made(&["cruiser", "cruiser", "fighter", "fighter"])
            ),
            "fighters are not non-fighter ships"
        );
        assert_eq!(
            state.player(&player).expect("seated").tokens(TokenPool::Fleet),
            before
        );

        assert!(on_production_finished(
            &mut state, content, ALL_SOURCES, &player,
            &made(&["cruiser", "destroyer", "carrier"])
        ));
        assert_eq!(
            state.player(&player).expect("seated").tokens(TokenPool::Fleet),
            before + 1,
            "one token for the use, however many ships above three"
        );
    }

    /// Bellum Gloriosum opens an allowance per capacity ship, spent only by fighters and ground.
    #[test]
    fn bellum_gloriosum_opens_capacity_for_small_units_only() {
        let content = ti4_content::ContentStore::embedded();
        let (state, player) = seat_with("solbt");

        let carrier = UnitTypeId::new("carrier");
        let opened = free_capacity_granted(&state, content, ALL_SOURCES, &player, &carrier);
        assert!(opened > 0, "a carrier carries something");

        // A ship with no capacity opens nothing.
        assert_eq!(
            free_capacity_granted(
                &state, content, ALL_SOURCES, &player, &UnitTypeId::new("destroyer")
            ),
            0
        );

        // And the allowance is spent by fighters and ground forces, not by ships.
        assert!(spends_free_capacity(content, ALL_SOURCES, &UnitTypeId::new("fighter")));
        assert!(spends_free_capacity(content, ALL_SOURCES, &UnitTypeId::new("infantry")));
        assert!(!spends_free_capacity(content, ALL_SOURCES, &UnitTypeId::new("cruiser")));

        // Without the breakthrough the carrier opens nothing at all.
        let (plain, other) = seat_with("l1z1xbt");
        assert_eq!(
            free_capacity_granted(&plain, content, ALL_SOURCES, &other, &carrier),
            0
        );
    }

    #[test]
    fn without_the_breakthrough_nothing_is_placed() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seat_with("solbt");
        assert_eq!(
            on_gain_control(
                &mut state,
                content,
                ALL_SOURCES,
                &player,
                &SystemId::new("109"),
                &PlanetId::new("bellatrix"),
            ),
            0
        );
    }

    #[test]
    fn a_space_station_mints_nothing() {
        // Stations are not planets, and sole occupancy can hand one over for flying past.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seat_with("l1z1xbt");
        assert_eq!(
            on_gain_control(
                &mut state,
                content,
                ALL_SOURCES,
                &player,
                &SystemId::new("117"),
                &PlanetId::new("thewatchtower"),
            ),
            0
        );
    }

    #[test]
    fn a_breakthrough_is_not_a_technology() {
        // Rule 6, kept true rather than assumed: holding one must not change any owned-colour count.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seat_with("l1z1xbt");
        let before = crate::technology::owned_colours(&state, content, &player);
        state.player_mut(&player).expect("seated").breakthrough =
            Some(BreakthroughId::new("l1z1xbt"));
        let after = crate::technology::owned_colours(&state, content, &player);
        assert_eq!(before, after, "a breakthrough is not a technology");
        assert!(after.is_empty(), "and this seat owns none");
    }

    #[test]
    fn the_outstanding_breakthroughs_are_reported() {
        let content = ti4_content::ContentStore::embedded();
        let missing = unimplemented(
            content,
            ALL_SOURCES,
            &["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"],
        );
        let mut names: Vec<&str> = missing.iter().map(BreakthroughId::as_str).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["jolnarbt"]);
    }
}
