//! Which planets are in a system, printed and placed.
//!
//! The corpus answers this for the 200-odd planets printed on tiles. Twelve are not: Mirage,
//! Custodia Vigilia and the ocean planets have a null `tileId` because they arrive from a deck
//! during play. `GameState::placed_planets` records where those went, and this module is the union
//! of the two — the one place a caller should ask, so a card that places a planet does not have to
//! find every reader and teach it about the overlay.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, SystemId};
use ti4_model::state::GameState;

/// Every planet in this system: printed on the tile, plus any placed there during play.
#[must_use]
pub fn in_system(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
) -> Vec<PlanetId> {
    let mut found: Vec<PlanetId> =
        ti4_content::galaxy::planets_in(content, system.as_str(), sources)
            .into_iter()
            .map(|planet| PlanetId::new(planet.id()))
            .collect();
    found.extend(
        state
            .placed_planets
            .iter()
            .filter(|(_, where_it_went)| *where_it_went == system)
            .map(|(planet, _)| planet.clone()),
    );
    found
}

/// Put a planet that has no printed tile onto one, and give its card to a player.
///
/// The planet arrives readied and controlled (LRR: a planet card gained this way is gained
/// readied). Returns `false` if it is already on the board, so a card cannot place it twice.
pub fn place(
    state: &mut GameState,
    system: &SystemId,
    planet: &PlanetId,
    player: &ti4_model::id::PlayerId,
) -> bool {
    if state.placed_planets.contains_key(planet) {
        return false;
    }
    state
        .placed_planets
        .insert(planet.clone(), system.clone());
    state.board.entry(system.clone()).or_default();
    if let Some(here) = state.board.get_mut(system) {
        here.set_control(planet.clone(), player.clone());
    }
    state.exhausted_planets.remove(planet);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A placed planet is in its system, and the printed ones are still there too.
    #[test]
    fn a_placed_planet_joins_the_printed_ones() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::DEFAULT;
        let player = ti4_model::id::PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);

        let (system, printed) = crate::fixtures::a_placed_planet();
        let before = in_system(&state, content, sources, &system);
        assert!(before.contains(&printed), "the printed planet is there");

        let mirage = PlanetId::new("mirage");
        assert!(
            !before.contains(&mirage),
            "and Mirage is not, until it is placed"
        );

        assert!(place(&mut state, &system, &mirage, &player));
        let after = in_system(&state, content, sources, &system);
        assert!(after.contains(&mirage), "now it is");
        assert!(after.contains(&printed), "and the printed one still is");
        assert!(
            !place(&mut state, &system, &mirage, &player),
            "and it cannot be placed twice"
        );
    }
}
