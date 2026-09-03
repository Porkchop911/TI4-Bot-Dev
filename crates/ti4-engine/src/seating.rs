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
/// **Faction scope and content scope are separate decisions.** Games are played with the whole
/// corpus enabled — Thunder's Edge, Prophecy of Kings, every codex, and the newest printing of
/// anything reprinted (`ti4_model::content_types::DEFAULT`) — while the seats stay these six. A
/// wider corpus means these factions meet more cards, systems and relics; it does not mean anybody
/// else sits down.
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

    // 51.1: a player begins with their faction's three leaders. `leaders::deploy` existed, was
    // tested, and had no caller outside a test -- so every seat in every simulated game held an
    // empty leader map, and the agent, commander and hero subsystems were unreachable no matter
    // how well they worked. The same shape as the custodians token: implemented, wired, never
    // reached.
    crate::leaders::deploy(state, content, sources, player);

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
    // One pass over the planet corpus for every homeworld system, rather than asking
    // `is_home_system` per candidate -- which rescans that corpus each time it is asked.
    let homes = ti4_content::galaxy::home_systems(content, sources);
    all_systems(content, sources)
        .into_iter()
        .filter(|(id, system)| {
            *id != MECATOL
                && !system.planets().is_empty()
                && !system.is_anomaly()
                && !system.is_hyperlane()
                && system.wormholes().is_empty()
                && !homes.contains(system.id())
        })
        .map(|(id, _)| SystemId::new(id))
        .take(count)
        .collect()
}

/// Filler tiles for one map, shuffled by seed.
///
/// [`neutral_systems`] returns the corpus in a stable order, which gives every game the same
/// board. That is right for a test — a board-dependent assertion needs a fixed board — and wrong
/// for training: a policy fitted on one map learns that map, and nothing in a batch report would
/// say so.
///
/// The shuffle is seeded and domain-separated, so a seed names a map and the same seed always
/// draws it. `count` tiles are taken *after* the shuffle rather than before, so the whole corpus
/// is in the draw rather than the first thirty entries of it.
#[must_use]
pub fn map_filler(
    content: &ContentStore,
    count: usize,
    sources: SourceSet,
    seed: u64,
) -> Vec<SystemId> {
    let mut pool = neutral_systems(content, usize::MAX, sources);
    let mut rng = crate::rng::GameRng::new(seed);
    rng.shuffle(crate::rng::domain::GALAXY, &mut pool);
    pool.truncate(count);
    pool
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
    fn l1z1x_deploys_its_capacity_two_super_dreadnought() {
        let state = seated(&[("a", "l1z1x")]);
        let player = PlayerId::new("a");
        let home = state
            .player(&player)
            .and_then(|seat| seat.home_system.clone())
            .expect("L1Z1X has a home system");
        let dreadnought = state
            .system_state(&home)
            .units
            .into_iter()
            .find(|unit| unit.type_id.as_str().contains("dreadnought"))
            .expect("L1Z1X starts with a dreadnought");
        assert_eq!(dreadnought.type_id.as_str(), "l1z1x_dreadnought");
        assert_eq!(
            ti4_content::units::unit_type(content(), dreadnought.type_id.as_str(), POK)
                .expect("the deployed hull exists")
                .capacity(),
            2
        );
    }

    #[test]
    fn saar_deploys_its_printed_production_unit() {
        let state = seated(&[("a", "saar")]);
        let player = PlayerId::new("a");
        let home = state
            .player(&player)
            .and_then(|seat| seat.home_system.clone())
            .expect("Saar has a home system");
        assert_eq!(
            crate::production::capacity(&state, content(), POK, &player, &home),
            5,
            "the starting Floating Factory supplies its printed PRODUCTION 5"
        );
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
    fn a_seed_names_a_map_and_different_seeds_name_different_ones() {
        // Every game in this project was played on one board until now: `neutral_systems` returns
        // the corpus in a stable order, so a batch of a thousand games was a thousand games on one
        // map. A policy fitted on it learns it, and no batch report would say so.
        let drawn: std::collections::BTreeSet<Vec<String>> = (0..8)
            .map(|seed| {
                map_filler(content(), 30, POK, seed)
                    .iter()
                    .map(ToString::to_string)
                    .collect()
            })
            .collect();
        assert!(drawn.len() > 1, "eight seeds drew one map");

        let once = map_filler(content(), 30, POK, 3);
        let twice = map_filler(content(), 30, POK, 3);
        assert_eq!(once, twice, "and a seed must always draw the same one");
    }

    #[test]
    fn a_drawn_map_is_made_of_tiles_that_belong_on_one() {
        // The shuffle must not reach past the filter: a home tile or an anomaly in the filler ring
        // would put two homes in one system or a hazard where expansion is meant to be.
        let drawn = map_filler(content(), 30, POK, 11);
        assert_eq!(drawn.len(), 30);

        let allowed: std::collections::BTreeSet<SystemId> =
            neutral_systems(content(), usize::MAX, POK)
                .into_iter()
                .collect();
        for tile in &drawn {
            assert!(
                allowed.contains(tile),
                "{tile} is not an ordinary filler tile"
            );
        }
        let distinct: std::collections::BTreeSet<&SystemId> = drawn.iter().collect();
        assert_eq!(distinct.len(), drawn.len(), "a tile was drawn twice");
    }

    #[test]
    fn the_whole_corpus_is_in_the_draw_not_just_its_first_entries() {
        // Taking `count` before the shuffle would draw from the same thirty tiles every time and
        // only reorder them, which looks like variety and is not.
        let mut seen: std::collections::BTreeSet<SystemId> = std::collections::BTreeSet::new();
        for seed in 0..24 {
            seen.extend(map_filler(content(), 30, POK, seed));
        }
        let first_thirty: std::collections::BTreeSet<SystemId> =
            neutral_systems(content(), 30, POK).into_iter().collect();
        assert!(
            seen.len() > first_thirty.len(),
            "twenty-four maps used only {} tiles, the same {} the stable order returns",
            seen.len(),
            first_thirty.len()
        );
    }

    #[test]
    fn every_drawn_map_still_seats_the_homes_properly() {
        // The board fix is not allowed to depend on which tiles were drawn. Homes three apart,
        // everyone the same distance from Mecatol — on every map, not just the fixed one.
        let seats = six_seats();
        for seed in 0..6 {
            let filler = map_filler(content(), 30, POK, seed);
            let refs: Vec<&str> = filler.iter().map(SystemId::as_str).collect();
            let galaxy = build_board(content(), &seats, &refs, POK)
                .unwrap_or_else(|error| panic!("map {seed} could not be built: {error}"));

            let homes = home_systems(content(), &seats).unwrap();
            let mut closest = i32::MAX;
            for (index, one) in homes.iter().enumerate() {
                for other in homes.iter().skip(index + 1) {
                    closest = closest.min(
                        galaxy
                            .distance(one.as_str(), other.as_str())
                            .expect("both homes are placed"),
                    );
                }
            }
            assert!(closest >= 3, "map {seed} seated two homes {closest} apart");

            let reach: std::collections::BTreeSet<i32> = homes
                .iter()
                .map(|home| galaxy.distance(home.as_str(), MECATOL).expect("placed"))
                .collect();
            assert_eq!(
                reach.len(),
                1,
                "map {seed} gave someone a shorter run at Mecatol"
            );
        }
    }

    #[test]
    fn a_wider_corpus_does_not_widen_the_table() {
        // Content scope and faction scope are separate decisions, and enabling Thunder's Edge
        // widened the corpus from 195 systems to 231 and from 83 leaders to 103. None of that is
        // an invitation for a thirty-fourth faction to sit down.
        let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("p{i}"))).collect();
        let seated = seat_in_scope(&players);

        for faction in seated.values() {
            assert!(
                IN_SCOPE_FACTIONS.contains(&faction.as_str()),
                "{faction} was seated under the wider corpus and is not in scope"
            );
        }
        // The wider corpus really is wider, or the check above proves nothing.
        let narrow = ti4_content::factions::catalogue(content(), POK).len();
        let wide =
            ti4_content::factions::catalogue(content(), ti4_model::content_types::DEFAULT).len();
        assert!(
            wide >= narrow,
            "full scope offers at least as many factions: {wide} against {narrow}"
        );
        assert!(wide > IN_SCOPE_FACTIONS.len(), "and far more than we seat");
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
