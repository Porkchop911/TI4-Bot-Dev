//! Entropic scars (Thunder's Edge).
//!
//! An anomaly that switches off unit abilities and hands out faction technology.
//!
//! > **1.** An entropic scar is an anomaly that affects unit abilities and faction technologies.
//! >
//! > **2.** A unit in an entropic scar cannot use unit abilities.
//! >
//! > **2.1.** Only unit abilities are removed; text abilities are unaffected.
//! >
//! > **2.2.** Text abilities that rely on unit abilities … will have no effect.
//! >
//! > **2.3.** If a unit loses its Deploy ability, then the text that describes how that unit's
//! > Deploy ability is used will have no effect.
//! >
//! > **4.** A unit outside of an entropic scar cannot use unit abilities against units that are in
//! > an entropic scar.
//! >
//! > **5.** Wormhole tokens that would be placed in an entropic scar are returned to the supply or a
//! > player's reinforcements as appropriate.
//! >
//! > **6.** At the start of the status phase, a player with ships in an entropic scar may spend a
//! > command token from their strategy pool to gain one of their faction-specific technologies.
//! >
//! > **6.1.** A player need not meet the prerequisites for the technology they gain this way.
//! >
//! > **6.2.** Ground forces in an entropic scar do not allow a player to gain a faction technology.
//! >
//! > **6.3.** If a player has ships in two (or more) entropic scars, they may spend two command
//! > tokens … to gain both of their faction-specific technologies.
//!
//! # Rules 2 and 4 are one predicate
//!
//! Rule 2 blocks an ability used *from* a scar; rule 4 blocks one used *into* one. Both are
//! [`abilities_usable`], which takes the acting unit's system and the system being acted on. Writing
//! them as two checks in each of the seven places a unit ability fires would mean seven chances to
//! implement half the rule.
//!
//! # Rules 2.1 to 2.3 need no code here
//!
//! They say what is *not* suppressed: printed card text keeps working, and text that merely
//! describes how to use a now-suppressed ability is inert because the ability it describes is gone.
//! Both fall out of suppressing the ability rather than the card.

use ti4_content::ContentStore;
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlayerId, SystemId, TechnologyId};
use ti4_model::state::{GameState, TokenPool};

/// Whether this system is an entropic scar.
#[must_use]
pub fn is_scar(content: &ContentStore, sources: SourceSet, system: &SystemId) -> bool {
    ti4_content::galaxy::system(content, system.as_str(), sources)
        .is_some_and(|tile| tile.is_scar())
}

/// Whether a unit ability may be used, given where it is used from and what it is used against
/// (rules 2 and 4).
///
/// `against` is `None` for an ability with no target outside its own system — production, for
/// instance — in which case only rule 2 applies.
#[must_use]
pub fn abilities_usable(
    content: &ContentStore,
    sources: SourceSet,
    from: &SystemId,
    against: Option<&SystemId>,
) -> bool {
    if is_scar(content, sources, from) {
        return false; // rule 2
    }
    match against {
        Some(target) => !is_scar(content, sources, target), // rule 4
        None => true,
    }
}

/// Every entropic scar this player has ships in (rules 6, 6.2).
///
/// Ships, not units: 6.2 says ground forces do not qualify, and a scar with only your infantry in it
/// grants nothing.
#[must_use]
pub fn scars_with_ships(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SystemId> {
    let types = ti4_content::units::catalogue(content, sources);
    state
        .board
        .iter()
        .filter(|(system, _)| is_scar(content, sources, system))
        .filter(|(_, record)| {
            record.units_of(player).into_iter().any(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        })
        .map(|(system, _)| system.clone())
        .collect()
}

/// This player's faction technologies that they do not already own.
#[must_use]
pub fn unowned_faction_technologies(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let faction = seat.faction.as_str();
    content
        .from_sources(ti4_model::content_types::ContentType::Technologies, sources)
        .filter(|record| record.text("faction") == Some(faction))
        .filter_map(|record| record.text("alias"))
        .map(TechnologyId::new)
        .filter(|alias| !seat.technologies.contains(alias))
        .collect()
}

/// How many faction technologies this player may take at the start of the status phase.
///
/// One per scar they have ships in (6.3), capped by strategy-pool tokens to spend and by how many
/// faction technologies they are still missing.
#[must_use]
pub fn grants_available(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> usize {
    let scars = scars_with_ships(state, content, sources, player).len();
    if scars == 0 {
        return 0;
    }
    let tokens = state.player(player).map_or(0, |seat| {
        usize::try_from(seat.tokens(TokenPool::Strategic)).unwrap_or(0)
    });
    let wanted = unowned_faction_technologies(state, content, sources, player).len();
    scars.min(tokens).min(wanted)
}

/// Take one faction technology, paying a strategy-pool token (rules 6, 6.1).
///
/// Prerequisites are not checked: 6.1 waives them outright, which is why this does not route through
/// `technology::can_research`.
///
/// # Errors
/// [`ScarError`] when nothing is available to grant.
pub fn grant(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    wanted: &TechnologyId,
) -> Result<(), ScarError> {
    if grants_available(state, content, sources, player) == 0 {
        return Err(ScarError::NothingToGrant);
    }
    if !unowned_faction_technologies(state, content, sources, player).contains(wanted) {
        return Err(ScarError::NotAFactionTechnology(wanted.clone()));
    }
    let Some(seat) = state.player_mut(player) else {
        return Err(ScarError::NothingToGrant);
    };
    if !seat.spend_token(TokenPool::Strategic) {
        return Err(ScarError::NothingToGrant);
    }
    seat.technologies.insert(wanted.clone());
    Ok(())
}

/// A scar grant that cannot be made.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScarError {
    /// No scar with ships, no strategy token, or nothing left to gain.
    #[error("no faction technology may be gained from an entropic scar right now")]
    NothingToGrant,
    /// Rule 6 grants a *faction-specific* technology, not any technology.
    #[error("{0} is not one of this player's faction technologies")]
    NotAFactionTechnology(TechnologyId),
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;
    use ti4_model::units::Unit;

    use super::*;

    const SCAR: &str = "114";
    const OTHER_SCAR: &str = "116";
    const PLAIN: &str = "19";

    fn ship(owner: &PlayerId, kind: &str) -> Unit {
        Unit::new(ti4_model::id::UnitTypeId::new(kind), owner.clone())
    }

    fn seated() -> (GameState, PlayerId) {
        let player = PlayerId::new("a");
        let mut state = crate::fixtures::game(&["a"]);
        state.player_mut(&player).expect("seated").faction = ti4_model::id::FactionId::new("sol");
        (state, player)
    }

    #[test]
    fn the_corpus_marks_the_scars() {
        let content = ti4_content::ContentStore::embedded();
        assert!(is_scar(content, ALL_SOURCES, &SystemId::new(SCAR)));
        assert!(is_scar(content, ALL_SOURCES, &SystemId::new(OTHER_SCAR)));
        assert!(!is_scar(content, ALL_SOURCES, &SystemId::new(PLAIN)));
    }

    #[test]
    fn abilities_are_suppressed_both_ways() {
        let content = ti4_content::ContentStore::embedded();
        let scar = SystemId::new(SCAR);
        let plain = SystemId::new(PLAIN);

        assert!(
            !abilities_usable(content, ALL_SOURCES, &scar, None),
            "rule 2: a unit in a scar cannot use unit abilities"
        );
        assert!(
            !abilities_usable(content, ALL_SOURCES, &plain, Some(&scar)),
            "rule 4: nor may one outside use them against units inside"
        );
        assert!(
            abilities_usable(content, ALL_SOURCES, &plain, Some(&plain)),
            "and everywhere else is unaffected"
        );
    }

    #[test]
    fn only_ships_earn_the_grant() {
        // Rule 6.2: ground forces in a scar do not allow the technology.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seated();
        let scar = SystemId::new(SCAR);
        state.system_mut(&scar).planet_units.insert(
            ti4_model::id::PlanetId::new("x"),
            vec![ship(&player, "infantry")],
        );
        assert!(scars_with_ships(&state, content, ALL_SOURCES, &player).is_empty());

        state.system_mut(&scar).units.push(ship(&player, "carrier"));
        assert_eq!(
            scars_with_ships(&state, content, ALL_SOURCES, &player),
            vec![scar]
        );
    }

    #[test]
    fn two_scars_grant_two_technologies() {
        // Rule 6.3, and the caps that bound it.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seated();
        for id in [SCAR, OTHER_SCAR] {
            state
                .system_mut(&SystemId::new(id))
                .units
                .push(ship(&player, "carrier"));
        }
        assert_eq!(grants_available(&state, content, ALL_SOURCES, &player), 2);

        // Capped by strategy tokens.
        state.player_mut(&player).expect("seated").strategic_tokens = 1;
        assert_eq!(grants_available(&state, content, ALL_SOURCES, &player), 1);
    }

    #[test]
    fn a_granted_technology_ignores_prerequisites_and_costs_a_token() {
        // Rule 6.1.
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seated();
        state
            .system_mut(&SystemId::new(SCAR))
            .units
            .push(ship(&player, "carrier"));

        let wanted = unowned_faction_technologies(&state, content, ALL_SOURCES, &player)
            .into_iter()
            .next()
            .expect("sol has faction technologies");
        let before = state
            .player(&player)
            .expect("seated")
            .tokens(TokenPool::Strategic);

        grant(&mut state, content, ALL_SOURCES, &player, &wanted).expect("granted");

        let seat = state.player(&player).expect("seated");
        assert!(seat.technologies.contains(&wanted));
        assert_eq!(seat.tokens(TokenPool::Strategic), before - 1);
    }

    #[test]
    fn only_your_own_faction_technologies_may_be_taken() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seated();
        state
            .system_mut(&SystemId::new(SCAR))
            .units
            .push(ship(&player, "carrier"));

        let alien = TechnologyId::new("td"); // generic, not a Sol faction technology
        assert_eq!(
            grant(&mut state, content, ALL_SOURCES, &player, &alien),
            Err(ScarError::NotAFactionTechnology(alien))
        );
    }

    #[test]
    fn no_ships_in_a_scar_grants_nothing() {
        let content = ti4_content::ContentStore::embedded();
        let (mut state, player) = seated();
        state
            .system_mut(&SystemId::new(PLAIN))
            .units
            .push(ship(&player, "carrier"));
        assert_eq!(grants_available(&state, content, ALL_SOURCES, &player), 0);
        let wanted = unowned_faction_technologies(&state, content, ALL_SOURCES, &player)
            .into_iter()
            .next()
            .expect("sol has faction technologies");
        assert_eq!(
            grant(&mut state, content, ALL_SOURCES, &player, &wanted),
            Err(ScarError::NothingToGrant)
        );
    }
}
