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
//! # What is not determinable here
//!
//! Two things are game data this corpus does not carry, and neither is invented:
//!
//! * **Adjacency between the seven Fracture systems.** The rules describe how to get *in*, not how
//!   the interior connects. The corpus gives the seven systems and no geometry, and no published
//!   source states it. [`interior_adjacency_known`] reports this.
//! * **The garrison.** Rule 6 says neutral units are placed but not which, or how many.
//!   [`enter_play`] therefore takes the garrison as an argument rather than choosing one.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlanetId, SystemId, UnitTypeId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

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
///
/// False: the corpus records the seven systems with no geometry, and the rules describe entry rather
/// than interior layout. Movement between two Fracture systems cannot be resolved until this is
/// supplied, and [`interior_adjacent`] refuses rather than guessing.
#[must_use]
pub const fn interior_adjacency_known() -> bool {
    false
}

/// Whether two Fracture systems are adjacent to each other.
///
/// # Errors
/// [`FractureError::InteriorLayoutUnknown`] always, until the layout is supplied.
pub const fn interior_adjacent(_left: &SystemId, _right: &SystemId) -> Result<bool, FractureError> {
    Err(FractureError::InteriorLayoutUnknown)
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
/// `garrison` is the neutral force placed on each Fracture planet and in the space area of its
/// system. Rule 6 says neutral units are placed and does not say which, so the composition is the
/// caller's to supply rather than this function's to invent.
///
/// # Errors
/// [`FractureError`] when the Fracture is already in play or neutral units are unavailable.
pub fn enter_play(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    ingresses: &[SystemId],
    garrison: &[UnitTypeId],
) -> Result<(), FractureError> {
    if state.fracture_in_play {
        return Err(FractureError::AlreadyInPlay);
    }
    crate::neutral_units::can_place(content, sources).map_err(|_| FractureError::NoNeutralUnits)?;

    let neutral = crate::neutral_units::owner();
    for system in systems(content, sources) {
        let Some(tile) = ti4_content::galaxy::system(content, system.as_str(), sources) else {
            continue;
        };
        let planets: Vec<String> = tile
            .planets()
            .into_iter()
            .map(std::borrow::ToOwned::to_owned)
            .collect();
        if planets.is_empty() {
            continue; // rule 6 places them on planets and in *those planets'* systems
        }
        for planet in planets {
            let record = state.system_mut(&system);
            let standing = record
                .planet_units
                .entry(PlanetId::new(planet))
                .or_default();
            for kind in garrison {
                standing.push(Unit::new(kind.clone(), neutral.clone()));
            }
        }
        for kind in garrison {
            state
                .system_mut(&system)
                .units
                .push(Unit::new(kind.clone(), neutral.clone()));
        }
    }

    // Rule 14: a set, so two tokens cannot land in one system.
    for system in ingresses {
        state.ingress_tokens.insert(system.clone());
    }
    // Rule 13: one more, into Thunder's Edge if it is on the board.
    if let Some(system) = state.thunders_edge_system.clone() {
        state.ingress_tokens.insert(system);
    }
    state.fracture_in_play = true;
    Ok(())
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
    /// The seven systems' adjacency to each other is not recorded anywhere.
    #[error(
        "adjacency within the Fracture is not in this corpus: the seven systems are recorded with \
         no geometry, and no published source states how they connect"
    )]
    InteriorLayoutUnknown,
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
    fn the_interior_layout_is_refused_rather_than_guessed() {
        assert!(!interior_adjacency_known());
        assert_eq!(
            interior_adjacent(&SystemId::new("fracture1"), &SystemId::new("fracture4")),
            Err(FractureError::InteriorLayoutUnknown)
        );
    }

    #[test]
    fn entering_play_garrisons_every_fracture_planet_and_its_space() {
        // Rule 6, with the garrison supplied by the caller.
        let mut state = crate::fixtures::game(&["a"]);
        let garrison = vec![UnitTypeId::new("neutral_infantry")];
        enter_play(&mut state, content(), ALL_SOURCES, &[], &garrison).expect("enters play");

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

        for system in planet_systems {
            let record = state.system_state(&system);
            assert!(
                !record.units_of(&neutral).is_empty(),
                "{system} space area is garrisoned"
            );
            assert!(
                record
                    .planet_units
                    .values()
                    .flatten()
                    .any(|unit| unit.owner == neutral),
                "{system} planets are garrisoned"
            );
        }
    }

    #[test]
    fn entering_play_twice_is_refused() {
        let mut state = crate::fixtures::game(&["a"]);
        let garrison = vec![UnitTypeId::new("neutral_infantry")];
        enter_play(&mut state, content(), ALL_SOURCES, &[], &garrison).expect("first");
        assert_eq!(
            enter_play(&mut state, content(), ALL_SOURCES, &[], &garrison),
            Err(FractureError::AlreadyInPlay)
        );
    }

    #[test]
    fn thunders_edge_takes_the_extra_ingress() {
        // Rule 13.
        let mut state = crate::fixtures::game(&["a"]);
        let here = SystemId::new("19");
        state.thunders_edge_system = Some(here.clone());
        enter_play(
            &mut state,
            content(),
            ALL_SOURCES,
            &[],
            &[UnitTypeId::new("neutral_infantry")],
        )
        .expect("enters play");
        assert!(state.ingress_tokens.contains(&here));
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
