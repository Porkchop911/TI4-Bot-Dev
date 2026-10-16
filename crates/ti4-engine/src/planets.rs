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

/// The system a planet sits in: a placed planet's recorded tile, else its printed tile, else
/// whichever board system has it in `planet_units` / `planet_control`. `None` when none knows.
///
/// Used to locate a planet answer on the map (`payload.system`); it decides nothing.
#[must_use]
pub fn system_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &str,
) -> Option<SystemId> {
    let id = PlanetId::new(planet);
    if let Some(system) = state.placed_planets.get(&id) {
        return Some(system.clone());
    }
    if let Some(system) =
        ti4_content::galaxy::planet(content, planet, sources).and_then(|record| record.system_id())
    {
        return Some(SystemId::new(system));
    }
    state
        .board
        .iter()
        .find(|(_, board)| {
            board.planet_units.contains_key(&id) || board.planet_control.contains_key(&id)
        })
        .map(|(system, _)| system.clone())
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
    state.placed_planets.insert(planet.clone(), system.clone());
    state.board.entry(system.clone()).or_default();
    if let Some(here) = state.board.get_mut(system) {
        here.set_control(planet.clone(), player.clone());
    }
    state.exhausted_planets.remove(planet);
    true
}

/// Move a placed planet together with its controller, units and coexistence records.
/// Planet-keyed exhaustion, attachments and legendary-card state keep their identities.
///
/// # Panics
/// If the destination system disappears from the board between the check here and the move.
pub fn move_placed(state: &mut GameState, planet: &PlanetId, destination: &SystemId) -> bool {
    let Some(origin) = state.placed_planets.get(planet).cloned() else {
        return false;
    };
    if origin == *destination || !state.board.contains_key(destination) {
        return false;
    }
    let Some(here) = state.board.get_mut(&origin) else {
        return false;
    };
    if here.purged_planets.contains(planet) {
        return false;
    }
    let control = here.planet_control.remove(planet);
    let units = here.planet_units.remove(planet);
    let coexist = here.coexisting.remove(planet);
    let there = state
        .board
        .get_mut(destination)
        .expect("destination checked");
    if let Some(owner) = control {
        there.planet_control.insert(planet.clone(), owner);
    }
    if let Some(units) = units {
        there.planet_units.insert(planet.clone(), units);
    }
    if let Some(owners) = coexist {
        there.coexisting.insert(planet.clone(), owners);
    }
    state
        .placed_planets
        .insert(planet.clone(), destination.clone());
    true
}

/// A planet's technology specialties as they now stand: those printed on its card plus any an
/// attachment gave it (a research facility on a planet without one, LRR 35.8), lower-case.
///
/// Every rules question about specialties — prerequisites (90.8), specialty-exhausting abilities,
/// "control planets with specialties" — asks here, so an attached specialty is never invisible.
#[must_use]
pub fn tech_specialties_now(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
) -> Vec<String> {
    let mut found: Vec<String> = ti4_content::galaxy::planet(content, planet.as_str(), sources)
        .map(|record| {
            record
                .tech_specialties()
                .into_iter()
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();
    for id in state.planet_attachments.get(planet).into_iter().flatten() {
        let Some(record) = content.get(ti4_model::content_types::ContentType::Attachments, id)
        else {
            continue;
        };
        for specialty in record.strings("techSpeciality") {
            let specialty = specialty.to_ascii_lowercase();
            if !found.contains(&specialty) {
                found.push(specialty);
            }
        }
    }
    found
}

/// A planet's exploration traits as they now stand: those printed on its card plus any an
/// attachment gave it (Titans' Terraform: "treated as having all 3 planet traits"), upper-case,
/// printed ones first.
///
/// Readers that know the game state ask here; [`crate::exploration::traits_of`] is the printed
/// answer only.
#[must_use]
pub fn traits_now(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    planet: &PlanetId,
) -> Vec<String> {
    let mut found = crate::exploration::traits_of(content, sources, planet);
    for id in state.planet_attachments.get(planet).into_iter().flatten() {
        let Some(record) = content.get(ti4_model::content_types::ContentType::Attachments, id)
        else {
            continue;
        };
        for kind in record.strings("planetTypes") {
            let kind = kind.to_ascii_uppercase();
            if crate::deck::EXPLORATION_TRAITS.contains(&kind.as_str())
                && kind != crate::exploration::FRONTIER
                && !found.contains(&kind)
            {
                found.push(kind);
            }
        }
    }
    found
}

/// The SPACE CANNON an attachment on a planet gives it "as if it were a unit" (Titans' Geoform:
/// SPACE CANNON 5 (x3)): `(controller, hits on, dice)` for each such attachment on `planet`, which
/// must lie in `system`. Empty when the planet is uncontrolled or carries no such attachment.
///
/// Read by space cannon offense and, for the planet's own invasion, space cannon defense.
#[must_use]
pub fn attachment_cannons(
    state: &GameState,
    content: &ContentStore,
    system: &SystemId,
    planet: &PlanetId,
) -> Vec<(ti4_model::id::PlayerId, u32, usize)> {
    let Some(owner) = state
        .board
        .get(system)
        .and_then(|board| board.planet_control.get(planet))
    else {
        return Vec::new();
    };
    let mut found: Vec<(ti4_model::id::PlayerId, u32, usize)> = state
        .planet_attachments
        .get(planet)
        .into_iter()
        .flatten()
        .filter_map(|id| content.get(ti4_model::content_types::ContentType::Attachments, id))
        .filter_map(|record| {
            let hits_on = u32::try_from(record.int("spaceCannonHitsOn")?).ok()?;
            let dice = usize::try_from(record.int("spaceCannonDieCount")?).ok()?;
            (dice > 0).then(|| (owner.clone(), hits_on, dice))
        })
        .collect();
    // Custodian's Favour (Custodia Vigilia): Mecatol Rex gains SPACE CANNON 5 for its controller.
    found.extend(crate::factions::keleres::custodian_cannon(
        state, planet, owner,
    ));
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relocating_a_planet_keeps_every_planet_record_and_rejects_invalid_moves() {
        let mut state = crate::fixtures::game(&["a", "b"]);
        let (from, to) = (SystemId::new("19"), SystemId::new("20"));
        let planet = PlanetId::new("avernus");
        let owner = ti4_model::id::PlayerId::new("a");
        place(&mut state, &from, &planet, &owner);
        crate::fixtures::put_on_planet(&mut state, &from, &planet, "infantry", &owner, 1);
        state
            .system_mut(&from)
            .coexisting
            .entry(planet.clone())
            .or_default()
            .insert(ti4_model::id::PlayerId::new("b"));
        state.exhausted_planets.insert(planet.clone());
        state
            .player_mut(&owner)
            .unwrap()
            .exhausted_legendary
            .insert(planet.clone());
        assert!(!move_placed(&mut state, &planet, &to));
        state.board.entry(to.clone()).or_default();
        assert!(move_placed(&mut state, &planet, &to));
        assert_eq!(state.board[&to].planet_control.get(&planet), Some(&owner));
        assert_eq!(state.board[&to].on_planet(&planet).len(), 1);
        assert!(state.board[&to].coexisting[&planet].contains(&ti4_model::id::PlayerId::new("b")));
        assert!(state.exhausted_planets.contains(&planet));
        assert!(
            state
                .player(&owner)
                .unwrap()
                .exhausted_legendary
                .contains(&planet)
        );
        assert!(!move_placed(&mut state, &planet, &to));
    }

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

    /// `system_of`: placed planets by their recorded tile, printed ones by the corpus, and a
    /// planet nobody knows about by nothing at all.
    #[test]
    fn system_of_reads_placement_then_corpus_then_board() {
        let content = ContentStore::embedded();
        let sources = ti4_model::content_types::POK;
        let player = ti4_model::id::PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        assert_eq!(
            system_of(&state, content, sources, "lodor"),
            Some(SystemId::new("26"))
        );
        assert_eq!(system_of(&state, content, sources, "not_a_planet"), None);

        let (system, _) = crate::fixtures::a_placed_planet();
        assert!(place(
            &mut state,
            &system,
            &PlanetId::new("mirage"),
            &player
        ));
        assert_eq!(system_of(&state, content, sources, "mirage"), Some(system));

        let board_only = SystemId::new("board_only_system");
        state
            .system_mut(&board_only)
            .set_control(PlanetId::new("board_only_planet"), player);
        assert_eq!(
            system_of(&state, content, sources, "board_only_planet"),
            Some(board_only)
        );
    }
}
