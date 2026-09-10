//! The Fracture (Thunder's Edge).
//!
//! Seven systems outside the map, reachable only through paired ingress and egress tokens.
//!
//! > **1.** The Fracture consists of additional systems beyond the regular bounds of the game map.
//! >
//! > **2.** When a player gains their breakthrough, they roll a dice. On a result of 1 or 10, The
//! > Fracture enters play.
//! >
//! > **3.** The '0' side of the d10 represents a result of 10.
//! >
//! > **4.** When The Fracture enters play, it is placed against the edge of the regular game map.
//! >
//! > **5.** The placement location of The Fracture is arbitrary, and should have no mechanical
//! > effect on the game state.
//! >
//! > **6.** When The Fracture is brought into play, neutral units are placed on the planets in The
//! > Fracture as well as the space area of those planets' systems.
//! >
//! > **7.** When The Fracture is brought into play, several ingress tokens will be placed on the
//! > game board.
//! >
//! > **8.** With sufficient movement, a ship could move from a system containing an ingress in the
//! > regular game map, into a system containing an egress within The Fracture.
//! >
//! > **9.** If The Fracture was brought into play as a result of a player rolling a dice upon
//! > gaining their breakthrough, they will place ingress tokens according to the synergy on their
//! > breakthrough.
//! >
//! > **10.** If there are fewer than three planets with a technology specialty of a given color, as
//! > many planets as possible are chosen.
//! >
//! > **11.** If a different game effect places The Fracture into play, then the player that caused
//! > The Fracture to enter play chooses one planet with a technology specialty for each of the four
//! > colors.
//! >
//! > **12.** If one system contains two planets with a technology specialty, only one of those
//! > planets may be chosen when placing ingress tokens.
//! >
//! > **13.** After ingress tokens are placed, one additional ingress token is placed. If Thunder's
//! > Edge is on the game board, then an ingress token is placed into its system.
//! >
//! > **14.** A system cannot contain two or more ingress tokens.
//! >
//! > **15.** When a player gains control of a planet in The Fracture that is not already controlled
//! > by another player, they draw one relic card.
//!
//! # Rule 5 is why placement needs no geometry
//!
//! The Fracture is not laid out relative to the map, and the rules say so explicitly: where it goes
//! "should have no mechanical effect on the game state". Its systems are therefore not placed in the
//! galaxy's coordinate space at all. What connects them to the board is rule 8, and the adjacency
//! that implements it is published as a complete bipartite link: *each ingress is adjacent to each
//! egress, and vice versa; an ingress is not adjacent to an ingress, and an egress is not adjacent
//! to an egress*. [`ingress_egress_adjacent`] is exactly that, and it needs no coordinates.
//!
//! # Interior geometry
//!
//! The three printed Fracture pieces form one seven-system chain in their numbered corpus order:
//! Cocytus, left egress, left void, Styx, right void, right egress, Lethe/Phlegethon. Systems that
//! share a border in that chain are adjacent just like ordinary system tiles.
//!
//! The neutral garrison is printed on the backs of the three Fracture tiles. It is fixed setup
//! data, encoded by [`place_printed_garrison`] rather than supplied by a caller.

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};
use crate::decision_context::{DecisionContext, DecisionSource};

/// The tile back that marks a Fracture system in the corpus.
const FRACTURE_BACK: &str = "fracture";

/// Every Fracture system, in corpus order.
#[must_use]
pub fn systems(content: &ContentStore, sources: SourceSet) -> Vec<SystemId> {
    content
        .from_sources(ContentType::Systems, sources)
        .filter(|record| record.text("tileBack") == Some(FRACTURE_BACK))
        .filter_map(|record| record.id().map(SystemId::new))
        .collect()
}

/// Whether a system belongs to the Fracture rather than the regular map.
#[must_use]
pub fn is_fracture_system(content: &ContentStore, sources: SourceSet, system: &SystemId) -> bool {
    ti4_content::galaxy::system(content, system.as_str(), sources)
        .is_some_and(|tile| tile.record().text("tileBack") == Some(FRACTURE_BACK))
}

/// The Fracture systems carrying a printed egress.
///
/// Taken from the printed system names, which is where the corpus records it: two of the seven are
/// named "Fracture Egress Left" and "Fracture Egress Right". There is no separate flag to read.
#[must_use]
pub fn egress_systems(content: &ContentStore, sources: SourceSet) -> Vec<SystemId> {
    content
        .from_sources(ContentType::Systems, sources)
        .filter(|record| record.text("tileBack") == Some(FRACTURE_BACK))
        .filter(|record| {
            record
                .text("name")
                .is_some_and(|name| name.to_ascii_lowercase().contains("egress"))
        })
        .filter_map(|record| record.id().map(SystemId::new))
        .collect()
}

/// Rule 8's adjacency: an ingress system and an egress system are adjacent, and nothing else about
/// this pairing is.
///
/// Complete and bipartite. An ingress is never adjacent to another ingress, nor an egress to another
/// egress, so this asks only whether one side of the pair is each.
#[must_use]
pub fn ingress_egress_adjacent(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    from: &SystemId,
    to: &SystemId,
) -> bool {
    let egresses = egress_systems(content, sources);
    let ingress_then_egress = state.ingress_tokens.contains(from) && egresses.contains(to);
    let egress_then_ingress = egresses.contains(from) && state.ingress_tokens.contains(to);
    ingress_then_egress || egress_then_ingress
}

/// Whether adjacency *within* the Fracture is known.
#[must_use]
pub const fn interior_adjacency_known() -> bool {
    true
}

/// Whether two Fracture systems are adjacent to each other.
#[must_use]
pub fn interior_adjacent(left: &SystemId, right: &SystemId) -> bool {
    fn index(system: &SystemId) -> Option<i32> {
        system.as_str().strip_prefix("fracture")?.parse().ok()
    }
    matches!((index(left), index(right)), (Some(a), Some(b)) if (a - b).abs() == 1)
}

/// The breakthrough roll (rules 2, 3): a d10 where 1 or 10 brings the Fracture into play.
///
/// Rule 3 is about reading the physical die: its '0' face is printed for ten. `GameRng::die` yields
/// `1..=sides` already, so ten is simply ten here and the two triggering faces are 1 and 10.
#[must_use]
pub fn breakthrough_roll(rng: &mut crate::rng::GameRng) -> bool {
    let face = rng.die("fracture-breakthrough", 10);
    face == 1 || face == 10
}

fn specialty_candidates(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    colour: &str,
    already_chosen: &std::collections::BTreeSet<SystemId>,
) -> Vec<(SystemId, PlanetId)> {
    let planets = ti4_content::galaxy::all_planets(content, sources);
    state
        .board
        .keys()
        .filter(|system| !already_chosen.contains(*system))
        .filter(|system| !is_fracture_system(content, sources, system))
        .filter_map(|system| {
            let tile = ti4_content::galaxy::system(content, system.as_str(), sources)?;
            let planet = tile.planets().into_iter().find(|planet| {
                planets.get(*planet).is_some_and(|record| {
                    record
                        .tech_specialties()
                        .iter()
                        .any(|specialty| specialty.eq_ignore_ascii_case(colour))
                })
            })?;
            Some((system.clone(), PlanetId::new(planet)))
        })
        .collect()
}

/// Roll for and, when triggered, put the Fracture into play after a breakthrough is gained.
///
/// Ingress locations are selected by the player from the currently legal specialty systems.
/// Breakthroughs with two synergy colours place up to three for each colour; a breakthrough with
/// no synergy uses one system of each technology colour.
///
/// # Errors
/// Returns [`IllegalChoice`] when an ingress choice is not one the engine offered.
pub fn after_breakthrough_gained(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&Galaxy>,
    table: &mut Table,
    rng: &mut crate::rng::GameRng,
    player: &ti4_model::id::PlayerId,
) -> Result<bool, IllegalChoice> {
    if state.fracture_in_play {
        return Ok(false);
    }
    let Some(breakthrough) = state
        .player(player)
        .and_then(|seat| seat.breakthrough.clone())
    else {
        return Ok(false);
    };
    let Some(record) = content.get(ContentType::Breakthroughs, breakthrough.as_str()) else {
        return Ok(false);
    };
    if !breakthrough_roll(rng) {
        return Ok(false);
    }

    let synergy = record.strings("synergy");
    let quotas: Vec<(&str, usize)> = if synergy.len() >= 2 {
        synergy
            .into_iter()
            .take(2)
            .map(|colour| (colour, 3))
            .collect()
    } else {
        crate::technology::COLOURS
            .into_iter()
            .map(|colour| (colour, 1))
            .collect()
    };
    let mut chosen = std::collections::BTreeSet::new();
    for (colour, maximum) in quotas {
        for _ in 0..maximum {
            let candidates = specialty_candidates(state, content, sources, colour, &chosen);
            if candidates.is_empty() {
                break;
            }
            let options: Vec<ChoiceOption> = candidates
                .iter()
                .map(|(system, planet)| {
                    ChoiceOption::labelled(
                        system.to_string(),
                        "ingress_system",
                        format!("place a {colour} ingress in {system} ({planet})"),
                    )
                    .with("system", system.to_string())
                    .with("planet", planet.to_string())
                    .with("technology_colour", colour)
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                format!("The Fracture: choose a {colour} technology-specialty ingress system"),
                options,
            )
            .contextualized(DecisionContext::new(
                player.clone(),
                DecisionSource::Content(breakthrough.to_string()),
                "fracture_choose_ingress_system",
                state.phase,
                state.round,
            ));
            let answer =
                table.ask_seeing(&choice, &Observed::new(state, content, sources, galaxy))?;
            chosen.insert(SystemId::new(answer.id));
        }
    }

    enter_play(
        state,
        content,
        sources,
        &chosen.into_iter().collect::<Vec<_>>(),
    )
    .map_err(|error| IllegalChoice::DeciderFailed {
        player: player.clone(),
        prompt: "put The Fracture into play".to_owned(),
        reason: error.to_string(),
    })?;
    Ok(true)
}

/// Planets that may take an ingress token, one per technology-specialty colour (rules 9–12).
///
/// `colours` are the colours to place for: a breakthrough's synergy under rule 9, or all four under
/// rule 11. Rule 10 takes as many as exist when there are fewer than three of a colour, and rule 12
/// allows only one planet per system, so a system holding two specialty planets contributes once.
#[must_use]
pub fn ingress_candidates(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    colours: &[String],
) -> Vec<(SystemId, PlanetId)> {
    let catalogue = ti4_content::galaxy::all_planets(content, sources);
    let mut taken: std::collections::BTreeSet<SystemId> = std::collections::BTreeSet::new();
    let mut chosen = Vec::new();

    for colour in colours {
        let wanted = colour.to_ascii_uppercase();
        let mut found = 0;
        for (system, record) in &state.board {
            if found >= 3 {
                break; // rule 10's cap, read as "up to three"
            }
            if taken.contains(system) || state.ingress_tokens.contains(system) {
                continue; // rule 14, and rule 12's one-per-system
            }
            let Some(tile) = ti4_content::galaxy::system(content, system.as_str(), sources) else {
                continue;
            };
            let planet = tile.planets().into_iter().find(|planet| {
                catalogue.get(*planet).is_some_and(|record| {
                    record
                        .tech_specialties()
                        .iter()
                        .any(|specialty| specialty.to_ascii_uppercase() == wanted)
                })
            });
            let Some(planet) = planet else {
                continue;
            };
            let _ = record;
            taken.insert(system.clone());
            chosen.push((system.clone(), PlanetId::new(planet.to_owned())));
            found += 1;
        }
    }
    chosen
}

/// Bring the Fracture into play (rules 1, 6, 7, 13, 14).
///
/// # Errors
/// [`FractureError`] when the Fracture is already in play or neutral units are unavailable.
pub fn enter_play(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    ingresses: &[SystemId],
) -> Result<(), FractureError> {
    if state.fracture_in_play {
        return Err(FractureError::AlreadyInPlay);
    }
    crate::neutral_units::can_place(content, sources).map_err(|_| FractureError::NoNeutralUnits)?;

    for system in systems(content, sources) {
        state.board.entry(system).or_default();
    }
    place_printed_garrison(state);

    // Rule 14: a set, so two tokens cannot land in one system.
    for system in ingresses {
        state.ingress_tokens.insert(system.clone());
    }
    // Rule 13: one more, into Thunder's Edge if it is on the board.
    state.ingress_tokens.insert(
        state
            .thunders_edge_system
            .clone()
            .unwrap_or_else(|| SystemId::new(crate::seating::MECATOL)),
    );
    state.fracture_in_play = true;
    Ok(())
}

/// Place the neutral forces printed on the backs of the three Fracture tiles.
fn place_printed_garrison(state: &mut GameState) {
    let neutral = crate::neutral_units::owner();
    // The closure's block scope ends its borrow of `state` before the planet loop below.
    {
        let mut place_space = |system: &str, kind: &str, count: usize| {
            let standing = &mut state.system_mut(&SystemId::new(system)).units;
            standing.extend((0..count).map(|_| Unit::new(UnitTypeId::new(kind), neutral.clone())));
        };
        place_space("fracture1", "neutral_cruiser", 2);
        place_space("fracture4", "neutral_destroyer", 1);
        place_space("fracture4", "neutral_dreadnought", 2);
        place_space("fracture7", "neutral_carrier", 1);
        place_space("fracture7", "neutral_fighter", 4);
    }

    for (system, planet, count) in [
        ("fracture1", "cocytus", 2_usize),
        ("fracture4", "styx", 3),
        ("fracture7", "lethe", 1),
        ("fracture7", "phlegethon", 1),
    ] {
        state
            .system_mut(&SystemId::new(system))
            .planet_units
            .entry(PlanetId::new(planet))
            .or_default()
            .extend(
                (0..count).map(|_| Unit::new(UnitTypeId::new("neutral_infantry"), neutral.clone())),
            );
    }
}

/// Rule 15: taking an uncontrolled Fracture planet draws a relic.
///
/// "Not already controlled by another player" is the whole condition — a planet the neutral force is
/// standing on has no controller, so the first player to take it draws; a planet taken from a rival
/// does not.
#[must_use]
pub fn draws_a_relic(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    if !is_fracture_system(content, sources, system) {
        return false;
    }
    !state
        .system_state(system)
        .planet_control
        .contains_key(planet)
}

/// Something the Fracture cannot do.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FractureError {
    /// Rule 2 brings it into play once.
    #[error("the Fracture is already in play")]
    AlreadyInPlay,
    /// Rule 6 places neutral units, which need their reference card.
    #[error("the Fracture places neutral units, which this corpus cannot supply")]
    NoNeutralUnits,
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::DEFAULT as ALL_SOURCES;
    use ti4_model::id::PlayerId;

    use super::*;

    fn content() -> &'static ContentStore {
        ti4_content::ContentStore::embedded()
    }

    #[test]
    fn the_corpus_carries_seven_fracture_systems_two_of_them_egresses() {
        let all = systems(content(), ALL_SOURCES);
        assert_eq!(all.len(), 7, "the Fracture is seven systems: {all:?}");

        let egresses = egress_systems(content(), ALL_SOURCES);
        assert_eq!(egresses.len(), 2, "two printed egresses: {egresses:?}");
        for egress in &egresses {
            assert!(all.contains(egress));
        }
    }

    #[test]
    fn an_ingress_is_adjacent_to_every_egress_and_to_no_other_ingress() {
        // Rule 8, as published: complete and bipartite.
        let mut state = crate::fixtures::game(&["a"]);
        let egresses = egress_systems(content(), ALL_SOURCES);
        let one = SystemId::new("19");
        let two = SystemId::new("20");
        state.ingress_tokens.insert(one.clone());
        state.ingress_tokens.insert(two.clone());

        for egress in &egresses {
            assert!(ingress_egress_adjacent(
                &state,
                content(),
                ALL_SOURCES,
                &one,
                egress
            ));
            assert!(ingress_egress_adjacent(
                &state,
                content(),
                ALL_SOURCES,
                egress,
                &one
            ));
        }
        assert!(
            !ingress_egress_adjacent(&state, content(), ALL_SOURCES, &one, &two),
            "an ingress is not adjacent to an ingress"
        );
        assert!(
            !ingress_egress_adjacent(&state, content(), ALL_SOURCES, &egresses[0], &egresses[1]),
            "nor an egress to an egress"
        );
    }

    #[test]
    fn the_breakthrough_roll_triggers_on_one_and_on_the_zero_face() {
        // Rules 2 and 3. Over many rolls the trigger rate is two faces in ten.
        let mut rng = crate::rng::GameRng::new(20_260_829);
        let trials = 4_000;
        let hits = (0..trials).filter(|_| breakthrough_roll(&mut rng)).count();
        let rate = f64::from(u32::try_from(hits).unwrap_or(u32::MAX)) / f64::from(trials);
        assert!(
            (0.15..0.25).contains(&rate),
            "two faces of ten should fire about a fifth of the time, saw {rate}"
        );
    }

    #[test]
    fn the_interior_is_a_seven_system_chain() {
        assert!(interior_adjacency_known());
        for index in 1..7 {
            let left = SystemId::new(format!("fracture{index}"));
            let right = SystemId::new(format!("fracture{}", index + 1));
            assert!(interior_adjacent(&left, &right));
            assert!(interior_adjacent(&right, &left));
        }
        assert!(!interior_adjacent(
            &SystemId::new("fracture1"),
            &SystemId::new("fracture3")
        ));
        assert!(!interior_adjacent(
            &SystemId::new("fracture1"),
            &SystemId::new("19")
        ));
    }

    #[test]
    fn entering_play_places_the_garrison_printed_on_each_tile_back() {
        let mut state = crate::fixtures::game(&["a"]);
        enter_play(&mut state, content(), ALL_SOURCES, &[]).expect("enters play");

        assert!(state.fracture_in_play);
        let neutral = crate::neutral_units::owner();
        let planet_systems: Vec<SystemId> = systems(content(), ALL_SOURCES)
            .into_iter()
            .filter(|system| {
                ti4_content::galaxy::system(content(), system.as_str(), ALL_SOURCES)
                    .is_some_and(|tile| !tile.planets().is_empty())
            })
            .collect();
        assert_eq!(planet_systems.len(), 3, "Cocytus, Styx, Lethe/Phlegethon");

        let count_space = |system: &str, kind: &str| {
            state
                .system_state(&SystemId::new(system))
                .units_of(&neutral)
                .into_iter()
                .filter(|unit| unit.type_id.as_str() == kind)
                .count()
        };
        assert_eq!(count_space("fracture1", "neutral_cruiser"), 2);
        assert_eq!(count_space("fracture4", "neutral_destroyer"), 1);
        assert_eq!(count_space("fracture4", "neutral_dreadnought"), 2);
        assert_eq!(count_space("fracture7", "neutral_carrier"), 1);
        assert_eq!(count_space("fracture7", "neutral_fighter"), 4);
        for (system, planet, count) in [
            ("fracture1", "cocytus", 2_usize),
            ("fracture4", "styx", 3),
            ("fracture7", "lethe", 1),
            ("fracture7", "phlegethon", 1),
        ] {
            assert_eq!(
                state
                    .system_state(&SystemId::new(system))
                    .on_planet_of(&PlanetId::new(planet), &neutral)
                    .len(),
                count
            );
        }
    }

    #[test]
    fn entering_play_twice_is_refused() {
        let mut state = crate::fixtures::game(&["a"]);
        enter_play(&mut state, content(), ALL_SOURCES, &[]).expect("first");
        assert_eq!(
            enter_play(&mut state, content(), ALL_SOURCES, &[]),
            Err(FractureError::AlreadyInPlay)
        );
    }

    #[test]
    fn thunders_edge_takes_the_extra_ingress() {
        // Rule 13.
        let mut state = crate::fixtures::game(&["a"]);
        let here = SystemId::new("19");
        state.thunders_edge_system = Some(here.clone());
        enter_play(&mut state, content(), ALL_SOURCES, &[]).expect("enters play");
        assert!(state.ingress_tokens.contains(&here));
    }

    #[test]
    fn a_fracture_system_can_be_activated_once_the_fracture_is_in_play() {
        // The Fracture adds systems to the game after setup, and `movement` already treats an
        // ingress and each egress as adjacent -- so ships can reach them. Activation enumerated
        // the *galaxy*, which is the printed map and never learns about them, so the tiles were
        // reachable and unactivatable at the same time: no tactical action could ever target one.
        let mut state = crate::fixtures::game(&["a"]);
        let hub = crate::fixtures::plain_hub();
        enter_play(&mut state, content(), ALL_SOURCES, &[]).expect("enters play");

        let inside = systems(content(), ALL_SOURCES)
            .into_iter()
            .next()
            .expect("the corpus carries Fracture tiles");
        let offered = crate::tactical::activatable(&state, &hub.galaxy, &PlayerId::new("a"));
        assert!(
            offered.contains(&inside),
            "a Fracture system in play must be activatable: {inside:?} not among {} offered",
            offered.len()
        );
    }

    #[test]
    fn mecatol_takes_the_extra_ingress_when_thunders_edge_is_absent() {
        let mut state = crate::fixtures::game(&["a"]);
        enter_play(&mut state, content(), ALL_SOURCES, &[]).expect("enters play");
        assert!(
            state
                .ingress_tokens
                .contains(&SystemId::new(crate::seating::MECATOL))
        );
    }

    #[test]
    fn a_relic_is_drawn_only_for_a_planet_nobody_holds() {
        // Rule 15.
        let mut state = crate::fixtures::game(&["a", "b"]);
        let system = SystemId::new("fracture1");
        let planet = PlanetId::new("cocytus");
        assert!(draws_a_relic(
            &state,
            content(),
            ALL_SOURCES,
            &system,
            &planet
        ));

        state
            .system_mut(&system)
            .set_control(planet.clone(), PlayerId::new("b"));
        assert!(
            !draws_a_relic(&state, content(), ALL_SOURCES, &system, &planet),
            "a planet taken from another player draws nothing"
        );

        assert!(
            !draws_a_relic(
                &state,
                content(),
                ALL_SOURCES,
                &SystemId::new("19"),
                &PlanetId::new("x")
            ),
            "and this is a Fracture rule, not a general one"
        );
    }
}
