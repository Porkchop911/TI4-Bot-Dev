//! Technology: prerequisites and research (LRR 90).
//!
//! Ported from the oracle's `engine/technology.py`: `owned_colours`, `can_research`,
//! `researchable`, `research` and `grant`.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlayerId, TechnologyId};
use ti4_model::state::GameState;

/// The four research tracks. Unit upgrades have no colour (90.7b), which is why
/// `UNITUPGRADE` is deliberately absent.
pub const COLOURS: [&str; 4] = ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"];

/// The letter each colour is written as in a technology's `requirements` string.
///
/// The corpus spells prerequisites as e.g. `RRRY` — three warfare and one cybernetic — rather
/// than as counts per named track.
#[must_use]
pub fn colour_of(letter: char) -> Option<&'static str> {
    match letter.to_ascii_uppercase() {
        'G' => Some("BIOTIC"),
        'Y' => Some("CYBERNETIC"),
        'B' => Some("PROPULSION"),
        'R' => Some("WARFARE"),
        _ => None,
    }
}

/// What a technology needs, as counts per colour.
#[must_use]
pub fn prerequisites(
    content: &ContentStore,
    alias: &TechnologyId,
) -> BTreeMap<&'static str, usize> {
    let mut needs = BTreeMap::new();
    let Some(record) = content.get(ContentType::Technologies, alias.as_str()) else {
        return needs;
    };
    let printed = record.text("requirements").unwrap_or("").trim();
    // The corpus writes "no prerequisites" as the literal strings "null" and "None" as well as
    // an absent field. They are spelled out here rather than being caught by the
    // is-not-a-colour-letter fallback, because that fallback would make any future typo a free
    // technology instead of an error.
    if printed.is_empty()
        || printed.eq_ignore_ascii_case("null")
        || printed.eq_ignore_ascii_case("none")
    {
        return needs;
    }
    for letter in printed.chars() {
        if let Some(colour) = colour_of(letter) {
            *needs.entry(colour).or_insert(0) += 1;
        }
    }
    needs
}

/// The colour a technology itself counts as, if any.
#[must_use]
pub fn colour_type(content: &ContentStore, alias: &TechnologyId) -> Option<&'static str> {
    let record = content.get(ContentType::Technologies, alias.as_str())?;
    let types = record.strings("types");
    COLOURS
        .iter()
        .find(|colour| types.contains(colour))
        .copied()
}

/// Whether this is a unit upgrade, which has no colour (90.7b).
#[must_use]
pub fn is_unit_upgrade(content: &ContentStore, alias: &TechnologyId) -> bool {
    content
        .get(ContentType::Technologies, alias.as_str())
        .is_some_and(|record| record.strings("types").contains(&"UNITUPGRADE"))
}

/// The faction a technology belongs to, if it is faction-specific (90.11).
#[must_use]
pub fn faction_of<'a>(content: &'a ContentStore, alias: &TechnologyId) -> Option<&'a str> {
    content
        .get(ContentType::Technologies, alias.as_str())
        .and_then(|record| record.text("faction"))
        .filter(|faction| !faction.is_empty())
}

/// How many technologies of each colour this player owns (90.7a).
#[must_use]
pub fn owned_colours(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> BTreeMap<&'static str, usize> {
    let mut held = BTreeMap::new();
    let Some(seat) = state.player(player) else {
        return held;
    };
    for alias in &seat.technologies {
        if let Some(colour) = colour_type(content, alias) {
            *held.entry(colour).or_insert(0) += 1;
        }
    }
    held
}

/// Technology specialties on the planets this player controls, by colour.
///
/// A specialty stands in for one prerequisite of its colour (90.8), which is most of why a
/// planet with one is worth taking.
#[must_use]
pub fn specialties(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> BTreeMap<&'static str, usize> {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let mut found = BTreeMap::new();
    for (_, planet) in state.controlled_planets(player) {
        let Some(record) = catalogue.get(planet.as_str()) else {
            continue;
        };
        for specialty in record.tech_specialties() {
            let upper = specialty.to_ascii_uppercase();
            if let Some(colour) = COLOURS.iter().find(|c| **c == upper) {
                *found.entry(*colour).or_insert(0) += 1;
            }
        }
    }
    found
}

/// Whether this player may research a technology now.
#[must_use]
pub fn can_research(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
    let Some(record) = content.get(ContentType::Technologies, alias.as_str()) else {
        return false;
    };
    let Some(seat) = state.player(player) else {
        return false;
    };
    if seat.technologies.contains(alias) {
        return false;
    }
    // Some cards say so of themselves.
    if record.text("text").is_some_and(|printed| {
        printed
            .to_ascii_lowercase()
            .contains("cannot be researched")
    }) {
        return false;
    }
    // 90.11: a faction technology belongs to that faction alone.
    if let Some(faction) = faction_of(content, alias)
        && faction != seat.faction.as_str()
    {
        return false;
    }

    let held = owned_colours(state, content, player);
    let specialties = specialties(state, content, sources, player);
    prerequisites(content, alias)
        .into_iter()
        .all(|(colour, need)| {
            held.get(colour).copied().unwrap_or(0) + specialties.get(colour).copied().unwrap_or(0)
                >= need
        })
}

/// Everything this player could research now, in a stable order.
#[must_use]
pub fn researchable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    content
        .records(ContentType::Technologies)
        .iter()
        .filter_map(|record| record.text("alias"))
        .map(TechnologyId::new)
        .filter(|alias| can_research(state, content, sources, player, alias))
        .collect()
}

/// Gain a technology outright (90.5), without checking prerequisites.
///
/// Separate from [`research`] because gaining is not researching: several effects grant a
/// technology directly, and the rules that fire on *research* must not fire for those.
pub fn grant(state: &mut GameState, player: &PlayerId, alias: &TechnologyId) {
    if let Some(seat) = state.player_mut(player) {
        seat.technologies.insert(alias.clone());
    }
}

/// Research a technology, having satisfied its prerequisites. `false` if it could not be.
pub fn research(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &TechnologyId,
) -> bool {
    if !can_research(state, content, sources, player, alias) {
        return false;
    }
    grant(state, player, alias);
    true
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::game;

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn give(state: &mut GameState, aliases: &[&str]) {
        for alias in aliases {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(TechnologyId::new(*alias));
        }
    }

    #[test]
    fn every_requirement_letter_names_a_track() {
        // If the corpus ever spells a prerequisite with a letter this does not know, the
        // technology silently becomes free rather than unresearchable.
        let mut unknown: Vec<char> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter_map(|record| record.text("requirements"))
            .map(str::trim)
            .filter(|printed| {
                !printed.is_empty()
                    && !printed.eq_ignore_ascii_case("null")
                    && !printed.eq_ignore_ascii_case("none")
            })
            .flat_map(str::chars)
            .filter(|letter| colour_of(*letter).is_none())
            .collect();
        unknown.sort_unstable();
        unknown.dedup();
        assert!(
            unknown.is_empty(),
            "unmapped requirement letters: {unknown:?}"
        );
    }

    #[test]
    fn prerequisites_are_read_off_the_requirement_string() {
        // Gravity Drive needs one propulsion; a war sun needs three warfare and a cybernetic.
        assert_eq!(
            prerequisites(ContentStore::embedded(), &TechnologyId::new("gd")),
            BTreeMap::from([("PROPULSION", 1)])
        );
        assert_eq!(
            prerequisites(ContentStore::embedded(), &TechnologyId::new("ws")),
            BTreeMap::from([("WARFARE", 3), ("CYBERNETIC", 1)])
        );
    }

    #[test]
    fn a_unit_upgrade_has_no_colour() {
        // 90.7b, and the reason unit upgrades are counted separately from the four tracks.
        let ws = TechnologyId::new("ws");
        assert!(is_unit_upgrade(ContentStore::embedded(), &ws));
        assert_eq!(colour_type(ContentStore::embedded(), &ws), None);
    }

    #[test]
    fn a_technology_with_unmet_prerequisites_cannot_be_researched() {
        let state = game(&["a"]);
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &TechnologyId::new("ws")
        ));
    }

    #[test]
    fn owning_the_prerequisites_unlocks_it() {
        let mut state = game(&["a"]);
        // Three warfare and one cybernetic.
        let warfare: Vec<String> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&"WARFARE"))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(3)
            .collect();
        let cybernetic: Vec<String> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&"CYBERNETIC"))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(1)
            .collect();
        let held: Vec<&str> = warfare
            .iter()
            .chain(&cybernetic)
            .map(String::as_str)
            .collect();
        give(&mut state, &held);

        assert!(can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &TechnologyId::new("ws")
        ));
    }

    #[test]
    fn a_technology_already_owned_is_not_researchable_again() {
        let mut state = game(&["a"]);
        give(&mut state, &["gd"]);
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &TechnologyId::new("gd")
        ));
    }

    #[test]
    fn a_faction_technology_belongs_to_its_faction_alone() {
        // 90.11.
        let state = game(&["a"]);
        let foreign = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .find(|record| {
                record.text("faction").is_some_and(|f| {
                    !f.is_empty() && f != state.player(&player()).unwrap().faction.as_str()
                })
            })
            .and_then(|record| record.text("alias"))
            .map(TechnologyId::new);

        if let Some(foreign) = foreign {
            assert!(!can_research(
                &state,
                ContentStore::embedded(),
                POK,
                &player(),
                &foreign
            ));
        }
    }

    #[test]
    fn a_planet_specialty_stands_in_for_a_prerequisite() {
        // 90.8, and most of why a planet with one is worth taking.
        let mut state = game(&["a"]);
        let target = TechnologyId::new("gd"); // one propulsion
        assert!(!can_research(
            &state,
            ContentStore::embedded(),
            POK,
            &player(),
            &target
        ));

        let planet = ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, record)| {
                record
                    .tech_specialties()
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case("propulsion"))
            })
            .map(|(id, record)| {
                (
                    ti4_model::id::SystemId::new(record.system_id().unwrap_or("18")),
                    ti4_model::id::PlanetId::new(*id),
                )
            });

        let Some((system, planet)) = planet else {
            return; // no propulsion specialty in this scope
        };
        state.system_mut(&system).set_control(planet, player());

        assert!(
            can_research(&state, ContentStore::embedded(), POK, &player(), &target),
            "the specialty covers the prerequisite"
        );
    }

    #[test]
    fn researching_grants_it_and_gaining_does_not_need_prerequisites() {
        // 90.5: several effects grant a technology outright, and that is not researching.
        let mut state = game(&["a"]);
        let ws = TechnologyId::new("ws");

        assert!(!research(
            &mut state,
            ContentStore::embedded(),
            POK,
            &player(),
            &ws
        ));
        assert!(!state.player(&player()).unwrap().technologies.contains(&ws));

        grant(&mut state, &player(), &ws);
        assert!(state.player(&player()).unwrap().technologies.contains(&ws));
    }

    #[test]
    fn researchable_lists_only_what_is_reachable() {
        let state = game(&["a"]);
        let open = researchable(&state, ContentStore::embedded(), POK, &player());

        assert!(!open.is_empty(), "some technologies need nothing");
        assert!(
            !open.contains(&TechnologyId::new("ws")),
            "a war sun needs four prerequisites"
        );
        for alias in &open {
            assert!(prerequisites(ContentStore::embedded(), alias).is_empty());
        }
    }
}
