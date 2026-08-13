//! Seating players as factions and deploying their opening positions.
//!
//! Ported from the oracle's `engine/factions.py` `deploy` and `home_systems`, and the
//! galaxy-building half of `engine/game.py` `seated_game`.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::factions::{self, FleetError, Placement};
use ti4_content::galaxy::{Galaxy, GalaxyError, all_systems};
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{FactionId, PlanetId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

/// Mecatol Rex, which sits at the centre of the board.
pub const MECATOL: &str = "18";

/// Something went wrong seating a game.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SeatingError {
    #[error("no faction {0:?} in the corpus")]
    UnknownFaction(String),
    #[error("faction {0:?} has no home system")]
    NoHomeSystem(String),
    #[error("no player {0:?} in this game")]
    UnknownPlayer(String),
    #[error("the board needs {wanted} filler tiles to space the homes, but was given {given}")]
    NotEnoughFiller { wanted: usize, given: usize },
    #[error(transparent)]
    Fleet(#[from] FleetError),
    #[error(transparent)]
    Galaxy(#[from] GalaxyError),
}

/// The factions this project plays.
///
/// Six, by owner decision: Sol, Hacan, Letnev, Xxcha, Jol-Nar and L1Z1X. The Firmament is
/// explicitly out of scope, and the rest of the corpus's thirty-four factions have no leaders,
/// abilities or units ported.
///
/// Named here rather than left to "whatever the catalogue lists first", which is how six seats
/// came to be playing Arborec, Argent and the Vuil'raith Cabal — factions with no implemented
/// abilities at all — in every rollout a trainer would have learned from. Alphabetical order is
/// not a scope decision, and a scope decision should not be alphabetical order.
pub const IN_SCOPE_FACTIONS: [&str; 6] = ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"];

/// Faction assignments for a table, taking the in-scope factions in order.
///
/// Seats beyond the sixth reuse the list from the start, so a larger table is still playing
/// factions this engine implements rather than falling off the end into unported ones.
#[must_use]
pub fn seat_in_scope(players: &[PlayerId]) -> BTreeMap<PlayerId, FactionId> {
    players
        .iter()
        .enumerate()
        .map(|(index, player)| {
            (
                player.clone(),
                FactionId::new(IN_SCOPE_FACTIONS[index % IN_SCOPE_FACTIONS.len()]),
            )
        })
        .collect()
}

/// Home system tile ids for a player-to-faction assignment, in assignment order.
///
/// # Errors
/// [`SeatingError::UnknownFaction`] or [`SeatingError::NoHomeSystem`].
pub fn home_systems(
    content: &ContentStore,
    assignments: &BTreeMap<PlayerId, FactionId>,
) -> Result<Vec<SystemId>, SeatingError> {
    assignments
        .values()
        .map(|alias| {
            let faction = factions::get(content, alias.as_str())
                .ok_or_else(|| SeatingError::UnknownFaction(alias.to_string()))?;
            faction
                .home_system()
                .map(SystemId::new)
                .ok_or_else(|| SeatingError::NoHomeSystem(alias.to_string()))
        })
        .collect()
}

/// Seat one player as a faction and place their opening position.
///
/// Sets control of every home planet, deploys the starting fleet with `mech` and `flagship`
/// resolved to the faction's own versions, and grants the faction's starting technology.
///
/// # Errors
/// Any [`SeatingError`].
pub fn deploy(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    alias: &FactionId,
    sources: SourceSet,
) -> Result<(), SeatingError> {
    if state.player(player).is_none() {
        return Err(SeatingError::UnknownPlayer(player.to_string()));
    }
    let faction = factions::get(content, alias.as_str())
        .ok_or_else(|| SeatingError::UnknownFaction(alias.to_string()))?;
    let home = faction
        .home_system()
        .ok_or_else(|| SeatingError::NoHomeSystem(alias.to_string()))?;
    let system_id = SystemId::new(home);
    let home_planets = faction.home_planets();
    let deployments = faction.deployments(content)?;

    // Resolve everything against the corpus before touching the state, so a faction that
    // fails to deploy leaves no half-seated player behind.
    let mut placements = Vec::new();
    for deployment in deployments {
        let unit_id = factions::resolve_unit(content, alias.as_str(), &deployment.unit_id, sources);
        let unit = Unit::new(unit_id, player.clone());
        placements.push((deployment.count, deployment.placement, unit));
    }

    let system = state.system_mut(&system_id);
    for planet in &home_planets {
        system.set_control(PlanetId::new(*planet), player.clone());
    }
    for (count, placement, unit) in placements {
        let units: Vec<Unit> = std::iter::repeat_n(unit, count as usize).collect();
        match placement {
            Placement::Space => system.add(&units),
            Placement::Planet(planet) => {
                system.planet_units.entry(planet).or_default().extend(units);
            }
        }
    }

    let starting_tech: Vec<TechnologyId> = faction
        .starting_tech()
        .iter()
        .filter_map(|t| content.resolve_id(ContentType::Technologies, t, sources))
        .map(TechnologyId::new)
        .collect();
    let seat = state
        .player_mut(player)
        .ok_or_else(|| SeatingError::UnknownPlayer(player.to_string()))?;
    seat.faction = alias.clone();
    seat.home_system = Some(system_id);
    seat.home_planets = home_planets.iter().map(|p| PlanetId::new(*p)).collect();
    seat.technologies.extend(starting_tech);
    // Commodities are deliberately not set. LRR 21: the faction record's `commodities` is
    // the *capacity* a player refreshes to, not an opening balance, and a player starts
    // with none. The oracle sets trade_goods to 0 here for the same reason.
    Ok(())
}

/// How many tiles each ring of a three-ring board holds, from the centre out.
const RING_SIZES: [usize; 4] = [1, 6, 12, 18];

/// A board with Mecatol at the centre, filler between, and the home systems **spaced** around
/// the outer ring.
///
/// Not real map setup — there is no draft — but a legal board with somewhere to expand into.
///
/// The spacing is the part that took a measurement to get right. [`Galaxy::build`] fills a spiral
/// positionally, so appending the homes to the end of the id list dropped all six into
/// *consecutive* outer-ring slots: every neighbouring pair of players started one tile apart, in
/// a huddle occupying a third of the ring. The consequences reached everything downstream —
/// home systems were being invaded in round one, a fifth of all scoring windows were refused by
/// 61.16, and no seat ever developed an economy because every seat was under immediate attack.
/// None of that is TI4; it was a board nobody had looked at.
///
/// Homes now sit at every third outer slot, which is where a real six-player board puts them, and
/// filler occupies the rest. That needs enough filler to complete both the inner rings and the
/// gaps — [`SeatingError::NotEnoughFiller`] says so rather than silently huddling them again.
///
/// The filler matters more than it looks for a second reason: with only Mecatol and home systems
/// on the board there is nothing explorable, so exploration could never fire and conquest would
/// have nowhere to go.
///
/// # Errors
/// [`SeatingError::NotEnoughFiller`] when the filler cannot fill the inner rings and the gaps
/// between homes, or any other [`SeatingError`].
pub fn build_board(
    content: &ContentStore,
    assignments: &BTreeMap<PlayerId, FactionId>,
    filler: &[&str],
    sources: SourceSet,
) -> Result<Galaxy, SeatingError> {
    let homes = home_systems(content, assignments)?;
    let outer = RING_SIZES[3];
    // Evenly spaced: with six homes on an eighteen-tile ring that is every third slot. With
    // fewer players the stride grows, which is what keeps them apart rather than clustered at
    // the start of the ring.
    let stride = if homes.is_empty() {
        outer
    } else {
        outer / homes.len()
    };

    let inner: usize = RING_SIZES[..3].iter().sum::<usize>() - 1; // less Mecatol at the centre
    // Only up to the last home: outer slots beyond it can stay empty, because `Galaxy::build`
    // stops where the id list stops. Slots *before* it cannot — the spiral is filled
    // positionally, so a missing tile would slide every home one place round the ring.
    let outer_used = homes.len().saturating_sub(1) * stride + usize::from(!homes.is_empty());
    let wanted = inner + outer_used.saturating_sub(homes.len());
    if filler.len() < wanted {
        return Err(SeatingError::NotEnoughFiller {
            wanted,
            given: filler.len(),
        });
    }

    let mut ids: Vec<&str> = vec![MECATOL];
    let mut filler = filler.iter().copied();
    for _ in 0..inner {
        if let Some(tile) = filler.next() {
            ids.push(tile);
        }
    }
    let mut placed = 0usize;
    for slot in 0..outer_used {
        if slot % stride == 0 && placed < homes.len() {
            ids.push(homes[placed].as_str());
            placed += 1;
        } else if let Some(tile) = filler.next() {
            ids.push(tile);
        }
    }

    // Three rings hold 37 tiles, enough for Mecatol plus six homes and filler.
    let rings = 3;
    Ok(Galaxy::build(content, &ids, sources, rings)?)
}

/// Ordinary planet-bearing tiles to fill a map with.
///
/// Excludes Mecatol, home systems, anomalies, wormholes, and hyperlanes, so the filler is
/// somewhere to expand into rather than a hazard course.
///
/// Returned in corpus order rather than shuffled from a seed as the oracle does. The oracle
/// needs a seed because its map is one of its variables; here a deterministic filler ring
/// keeps board-dependent tests stable. Seeded selection belongs with the simulation harness.
#[must_use]
pub fn neutral_systems(content: &ContentStore, count: usize, sources: SourceSet) -> Vec<SystemId> {
    all_systems(content, sources)
        .into_iter()
        .filter(|(id, system)| {
            *id != MECATOL
                && !system.planets().is_empty()
                && !system.is_anomaly()
                && !system.is_hyperlane()
                && system.wormholes().is_empty()
                && !is_home_tile(content, system.id(), sources)
        })
        .map(|(id, _)| SystemId::new(id))
        .take(count)
        .collect()
}

fn is_home_tile(content: &ContentStore, system_id: &str, sources: SourceSet) -> bool {
    ti4_content::galaxy::planets_in(content, system_id, sources)
        .iter()
        .any(|p| p.homeworld_of().is_some())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::start_game;
    use ti4_model::content_types::POK;

    fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }

    fn assignments(pairs: &[(&str, &str)]) -> BTreeMap<PlayerId, FactionId> {
        pairs
            .iter()
            .map(|(p, f)| (PlayerId::new(*p), FactionId::new(*f)))
            .collect()
    }

    fn seated(pairs: &[(&str, &str)]) -> GameState {
        let ids: Vec<PlayerId> = pairs.iter().map(|(p, _)| PlayerId::new(*p)).collect();
        let mut state = start_game(content(), &ids, POK, None).unwrap();
        for (player, alias) in assignments(pairs) {
            deploy(&mut state, content(), &player, &alias, POK).unwrap();
        }
        state
    }

    #[test]
    fn deploying_seats_the_faction_and_takes_home_control() {
        let state = seated(&[("a", "sol")]);
        let player = state.player(&PlayerId::new("a")).unwrap();
        assert_eq!(player.faction, FactionId::new("sol"));
        assert_eq!(player.home_system, Some(SystemId::new("01")));
        assert_eq!(player.home_planets, vec![PlanetId::new("jord")]);

        let home = state.system_state(&SystemId::new("01"));
        assert!(home.controls_a_planet(&PlayerId::new("a")));
    }

    #[test]
    fn sol_opens_with_two_carriers_in_space_and_five_infantry_on_jord() {
        let state = seated(&[("a", "sol")]);
        let home = state.system_state(&SystemId::new("01"));

        let carriers = home
            .units
            .iter()
            .filter(|u| u.type_id.as_str().contains("carrier"))
            .count();
        assert_eq!(carriers, 2);

        let jord = home.on_planet(&PlanetId::new("jord"));
        let infantry = jord
            .iter()
            .filter(|u| u.type_id.as_str().contains("infantry"))
            .count();
        assert_eq!(infantry, 5);
    }

    #[test]
    fn a_faction_gets_its_own_mech_and_flagship() {
        let state = seated(&[("a", "sol")]);
        let home = state.system_state(&SystemId::new("01"));
        let all: Vec<&str> = home
            .units
            .iter()
            .chain(home.planet_units.values().flatten())
            .map(|u| u.type_id.as_str())
            .collect();
        assert!(
            all.iter().any(|u| *u == "sol_infantry" || *u == "infantry"),
            "got {all:?}"
        );
        assert!(
            !all.contains(&"mech"),
            "a generic mech means resolution failed: {all:?}"
        );
    }

    #[test]
    fn every_deployed_unit_belongs_to_the_seated_player() {
        let state = seated(&[("a", "sol"), ("b", "hacan")]);
        for (system, player) in [("01", "a"), ("13", "b")] {
            let home = state.system_state(&SystemId::new(system));
            for unit in home
                .units
                .iter()
                .chain(home.planet_units.values().flatten())
            {
                assert_eq!(unit.owner, PlayerId::new(player), "in system {system}");
            }
        }
    }

    #[test]
    fn a_seated_player_holds_their_factions_starting_technology() {
        let state = seated(&[("a", "sol")]);
        let player = state.player(&PlayerId::new("a")).unwrap();
        assert!(!player.technologies.is_empty());
        assert!(player.technologies.contains(&TechnologyId::new("amd")));
    }

    #[test]
    fn every_base_faction_deploys_onto_a_board() {
        for (alias, faction) in ti4_content::factions::catalogue(content(), POK) {
            if faction.starting_fleet().is_empty() {
                continue;
            }
            let player = PlayerId::new("a");
            let mut state =
                start_game(content(), std::slice::from_ref(&player), POK, None).unwrap();
            deploy(&mut state, content(), &player, &FactionId::new(alias), POK)
                .unwrap_or_else(|e| panic!("{alias}: {e}"));

            let home = state.player(&player).unwrap().home_system.clone().unwrap();
            let system = state.system_state(&home);
            let placed =
                system.units.len() + system.planet_units.values().map(Vec::len).sum::<usize>();
            assert!(placed > 0, "{alias} deployed nothing");
        }
    }

    #[test]
    fn seating_an_unknown_faction_fails_rather_than_seating_nothing() {
        let player = PlayerId::new("a");
        let mut state = start_game(content(), std::slice::from_ref(&player), POK, None).unwrap();
        let err = deploy(
            &mut state,
            content(),
            &player,
            &FactionId::new("nonesuch"),
            POK,
        )
        .unwrap_err();
        assert!(matches!(err, SeatingError::UnknownFaction(_)), "{err}");
        assert!(state.board.is_empty(), "nothing may be placed on a failure");
    }

    #[test]
    fn seating_an_unseated_player_is_refused() {
        let mut state = start_game(content(), &[PlayerId::new("a")], POK, None).unwrap();
        let err = deploy(
            &mut state,
            content(),
            &PlayerId::new("ghost"),
            &FactionId::new("sol"),
            POK,
        )
        .unwrap_err();
        assert!(matches!(err, SeatingError::UnknownPlayer(_)), "{err}");
    }

    // -- the board ------------------------------------------------------------------

    /// Enough filler for the inner rings and the gaps between six homes.
    fn full_filler() -> Vec<SystemId> {
        neutral_systems(content(), 30, POK)
    }

    fn six_seats() -> BTreeMap<PlayerId, FactionId> {
        assignments(&[
            ("a", "sol"),
            ("b", "hacan"),
            ("c", "letnev"),
            ("d", "xxcha"),
            ("e", "jolnar"),
            ("f", "l1z1x"),
        ])
    }

    #[test]
    fn homes_are_spaced_around_the_outer_ring_not_huddled_at_the_start_of_it() {
        // This is the check nobody had. `Galaxy::build` fills a spiral positionally, so appending
        // the homes to the end of the id list put all six in *consecutive* outer slots: every
        // neighbouring pair started one tile apart. Nothing failed, because nothing looked — the
        // board was legal, connected, and wrong.
        //
        // What it cost, measured over twelve games before the fix: twenty of seventy-two seats
        // lost a home planet, six of them in round one, and a fifth of every scoring window was
        // refused by 61.16. After it, one seat of seventy-two, in round six.
        let seats = six_seats();
        let filler = full_filler();
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &seats, &refs, POK).unwrap();

        let homes = home_systems(content(), &seats).unwrap();
        let mut closest = i32::MAX;
        for (index, one) in homes.iter().enumerate() {
            for other in homes.iter().skip(index + 1) {
                let apart = galaxy
                    .distance(one.as_str(), other.as_str())
                    .expect("both homes are on the board");
                closest = closest.min(apart);
            }
        }
        assert!(
            closest >= 3,
            "the closest pair of homes is {closest} tiles apart; a six-player board seats them 3"
        );
    }

    #[test]
    fn every_home_is_the_same_distance_from_mecatol() {
        // The other half of a fair board. Homes evenly spaced but at different radii would give
        // one seat a shorter run at the centre, which decides games on its own.
        let seats = six_seats();
        let filler = full_filler();
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &seats, &refs, POK).unwrap();

        let reach: std::collections::BTreeSet<i32> = home_systems(content(), &seats)
            .unwrap()
            .iter()
            .map(|home| {
                galaxy
                    .distance(home.as_str(), MECATOL)
                    .expect("Mecatol is on the board")
            })
            .collect();
        assert_eq!(
            reach.len(),
            1,
            "seats sit at different distances from Mecatol: {reach:?}"
        );
    }

    #[test]
    fn a_board_without_the_filler_to_space_the_homes_is_refused() {
        // Refused rather than huddled. Silently falling back to consecutive slots is exactly the
        // failure this whole test group exists for, and it would be invisible again.
        let seats = six_seats();
        let filler = neutral_systems(content(), 18, POK);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();

        let err = build_board(content(), &seats, &refs, POK).unwrap_err();
        assert!(matches!(err, SeatingError::NotEnoughFiller { .. }), "{err}");
    }

    // -- scope ----------------------------------------------------------------------

    #[test]
    fn the_six_in_scope_factions_are_the_six_named() {
        // A list, asserted as a list. Changing who this project plays should be a deliberate edit
        // to a test, not a side effect of a catalogue reordering.
        assert_eq!(
            IN_SCOPE_FACTIONS,
            ["sol", "hacan", "letnev", "xxcha", "jolnar", "l1z1x"]
        );
    }

    #[test]
    fn every_in_scope_faction_exists_in_the_corpus() {
        // A typo here seats a faction that does not exist, and `deploy` fails at setup with an
        // error about an unknown faction rather than about a misspelled constant.
        for alias in IN_SCOPE_FACTIONS {
            assert!(
                ti4_content::factions::get(content(), alias).is_some(),
                "{alias} is not a faction in the corpus"
            );
        }
    }

    #[test]
    fn every_in_scope_faction_is_one_this_engine_has_actually_ported() {
        // The point of a scope. A faction in this list with no leaders registered would be seated
        // in every game and every training rollout while contributing nothing but its starting
        // fleet — which is exactly what Arborec, Argent and the Cabal were doing when the seating
        // took whatever the catalogue listed first.
        for alias in IN_SCOPE_FACTIONS {
            let leaders = crate::leaders::for_faction(content(), POK, alias);
            assert!(
                !leaders.is_empty(),
                "{alias} is in scope but has no leaders in the corpus"
            );
            let known = crate::leaders::registered_abilities();
            let standing = crate::leaders::modifiers();
            let ported = leaders.iter().filter(|leader| {
                known.contains(&leader.as_str()) || standing.contains_key(leader.as_str())
            });
            assert!(
                ported.count() > 0,
                "{alias} is in scope but not one of its leaders is implemented"
            );
        }
    }

    #[test]
    fn a_table_is_seated_from_the_scope_and_from_nothing_else() {
        let players = [
            PlayerId::new("a"),
            PlayerId::new("b"),
            PlayerId::new("c"),
            PlayerId::new("d"),
            PlayerId::new("e"),
            PlayerId::new("f"),
        ];
        let seated = seat_in_scope(&players);

        assert_eq!(seated.len(), 6);
        for faction in seated.values() {
            assert!(
                IN_SCOPE_FACTIONS.contains(&faction.as_str()),
                "{faction} was seated and is not in scope"
            );
        }
        let distinct: std::collections::BTreeSet<&str> =
            seated.values().map(FactionId::as_str).collect();
        assert_eq!(distinct.len(), 6, "six seats, six factions: {distinct:?}");
    }

    #[test]
    fn a_table_larger_than_the_scope_still_plays_in_scope_factions() {
        // Falling off the end of the list is how an unported faction gets seated. Reusing it is
        // wrong as a matchup and right as a scope, and a duplicated faction is visible where a
        // silently unported one is not.
        let players: Vec<PlayerId> = (0..8)
            .map(|index| PlayerId::new(format!("p{index}")))
            .collect();
        let seated = seat_in_scope(&players);

        assert_eq!(seated.len(), 8);
        for faction in seated.values() {
            assert!(
                IN_SCOPE_FACTIONS.contains(&faction.as_str()),
                "{faction} was seated and is not in scope"
            );
        }
    }

    #[test]
    fn the_firmament_is_not_in_scope() {
        // Out by owner decision, and it is in the corpus, so nothing else would stop it being
        // seated.
        assert!(
            ti4_content::factions::get(content(), "firmament").is_some(),
            "the corpus does carry it"
        );
        assert!(!IN_SCOPE_FACTIONS.contains(&"firmament"));
    }

    #[test]
    fn every_in_scope_faction_can_actually_be_deployed() {
        // A faction in scope that cannot be seated would fail every game at setup rather than at
        // the point somebody chose it.
        for alias in IN_SCOPE_FACTIONS {
            let player = PlayerId::new("a");
            let mut state =
                start_game(content(), std::slice::from_ref(&player), POK, None).unwrap();
            deploy(&mut state, content(), &player, &FactionId::new(alias), POK)
                .unwrap_or_else(|error| panic!("{alias} could not be deployed: {error}"));
            assert!(
                !state.board.is_empty(),
                "{alias} deployed nothing onto the board"
            );
        }
    }

    #[test]
    fn home_systems_are_read_from_the_faction_records() {
        let homes = home_systems(content(), &assignments(&[("a", "sol"), ("b", "hacan")])).unwrap();
        assert!(homes.contains(&SystemId::new("01")), "Sol's home is 01");
        assert_eq!(homes.len(), 2);
    }

    #[test]
    fn filler_systems_carry_explorable_planets_and_no_special_cases() {
        let filler = neutral_systems(content(), 6, POK);
        assert_eq!(filler.len(), 6);
        let systems = all_systems(content(), POK);
        assert!(
            !filler.contains(&SystemId::new(MECATOL)),
            "Mecatol is not filler"
        );
        for id in &filler {
            let system = systems[id.as_str()];
            assert!(!system.planets().is_empty(), "{id} has no planet to take");
            assert!(!system.is_anomaly(), "{id} is an anomaly");
            assert!(system.wormholes().is_empty(), "{id} has a wormhole");
        }
    }

    #[test]
    fn a_seated_game_places_mecatol_at_the_centre() {
        let pairs = [("a", "sol"), ("b", "hacan")];
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &assignments(&pairs), &filler_refs, POK).unwrap();

        assert_eq!(galaxy.coord_of(MECATOL), Some(ti4_model::Hex::ORIGIN));
        assert_eq!(galaxy.adjacent(MECATOL).len(), 6, "a full first ring");
    }

    #[test]
    fn neutral_systems_separate_the_homes_from_mecatol() {
        let pairs = [("a", "sol"), ("b", "hacan")];
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let filler_refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        let galaxy = build_board(content(), &assignments(&pairs), &filler_refs, POK).unwrap();

        // Homes are placed after the filler ring, so nobody starts next to Mecatol.
        for home in home_systems(content(), &assignments(&pairs)).unwrap() {
            assert!(
                !galaxy.are_adjacent(MECATOL, home.as_str()),
                "{home} starts adjacent to Mecatol"
            );
        }
    }

    #[test]
    fn a_board_is_deterministic() {
        let pairs = assignments(&[("a", "sol"), ("b", "hacan")]);
        let filler: Vec<SystemId> = neutral_systems(content(), 30, POK);
        let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
        assert_eq!(
            build_board(content(), &pairs, &refs, POK).unwrap(),
            build_board(content(), &pairs, &refs, POK).unwrap()
        );
        assert_eq!(filler, neutral_systems(content(), 30, POK));
    }
}
