//! Secret objectives (LRR 45, 61.18).
//!
//! Ported from the oracle's `engine/secrets.py`: `scored_count`, `held_count`, `draw`,
//! `enforce_hand_limit`, `scoreable` and `award`, plus a first tranche of requirements.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{PlayerId, SecretObjectiveId};
use ti4_model::state::{Feat, FeatOccurrence, GameState};

use crate::choice::{Choice, ChoiceOption, IllegalChoice, Observed, Table};

/// 45.4: three in hand, counting scored ones.
pub const HAND_LIMIT: usize = 3;

/// The choice kind for returning a secret over the hand limit.
pub const RETURN_KIND: &str = "return";

/// When a secret may be scored.
///
/// The oracle is explicit that this matters: an action or agenda secret offered at status time
/// changes both its timing and whether the fact that satisfied it still exists by then.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Timing {
    Status,
    Action,
    Agenda,
}

/// When this secret may be scored, from its printed phase.
#[must_use]
pub fn timing(content: &ContentStore, alias: &SecretObjectiveId) -> Timing {
    let printed = content
        .get(ContentType::SecretObjectives, alias.as_str())
        .and_then(|record| record.text("phase"))
        .unwrap_or("status")
        .to_ascii_lowercase();
    match printed.as_str() {
        "action" => Timing::Action,
        "agenda" => Timing::Agenda,
        _ => Timing::Status,
    }
}

/// How many of this player's scored objectives were secrets (45.4 counts them).
#[must_use]
pub fn scored_count(state: &GameState, content: &ContentStore, player: &PlayerId) -> usize {
    state
        .scored_by(player)
        .iter()
        .filter(|alias| {
            content
                .get(ContentType::SecretObjectives, alias.as_str())
                .is_some()
        })
        .count()
}

/// Secrets in hand plus secrets already scored (45.4).
#[must_use]
pub fn held_count(state: &GameState, content: &ContentStore, player: &PlayerId) -> usize {
    state
        .player(player)
        .map_or(0, |seat| seat.secret_objectives.len())
        + scored_count(state, content, player)
}

/// Draw the top secret, then enforce the hand limit.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn draw(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
) -> Result<Option<SecretObjectiveId>, IllegalChoice> {
    if state.secret_deck.is_empty() {
        return Ok(None);
    }
    let top = state.secret_deck.remove(0);
    if let Some(seat) = state.player_mut(player) {
        seat.secret_objectives.push(top.clone());
    }
    enforce_hand_limit(state, content, table, player)?;
    Ok(Some(top))
}

/// 45.4: over three, return an *unscored* one to the deck.
///
/// Scored secrets count towards the limit but cannot be given back, so a player who has scored
/// two may only hold one more. A player whose every secret is scored has nothing to return and
/// simply stays over — which is the rules working, not a stuck state.
///
/// # Errors
/// [`IllegalChoice`] when a decider answers with something not offered.
pub fn enforce_hand_limit(
    state: &mut GameState,
    content: &ContentStore,
    table: &mut Table,
    player: &PlayerId,
) -> Result<(), IllegalChoice> {
    while held_count(state, content, player) > HAND_LIMIT {
        let held: Vec<SecretObjectiveId> = state
            .player(player)
            .map(|seat| seat.secret_objectives.clone())
            .unwrap_or_default();
        let Some(first) = held.first().cloned() else {
            return Ok(()); // every one is scored, so there is nothing to hand back
        };

        let chosen = if held.len() == 1 {
            first
        } else {
            let options: Vec<ChoiceOption> = held
                .iter()
                .map(|alias| {
                    ChoiceOption::labelled(
                        alias.to_string(),
                        RETURN_KIND,
                        format!("return {alias}"),
                    )
                })
                .collect();
            let choice = Choice::new(
                player.clone(),
                "return a secret objective to the deck",
                options,
            );
            SecretObjectiveId::new(
                table
                    .ask_seeing(
                        &choice,
                        &Observed::new(state, content, ti4_model::content_types::POK, None),
                    )?
                    .id,
            )
        };

        if let Some(seat) = state.player_mut(player) {
            seat.secret_objectives.retain(|alias| alias != &chosen);
        }
        state.secret_deck.push(chosen);
    }
    Ok(())
}

/// A registered requirement check.
type Requirement = fn(&Position<'_>) -> bool;

/// What a secret's requirement is evaluated against.
pub struct Position<'a> {
    pub state: &'a GameState,
    pub content: &'a ContentStore,
    pub sources: SourceSet,
    pub player: &'a PlayerId,
    /// The map, when the caller has one. `None` leaves board-shape requirements unmet.
    pub galaxy: Option<&'a ti4_content::galaxy::Galaxy>,
}

impl Position<'_> {
    /// Units of one base type this player has on the board, in space and on planets.
    fn units_on_board(&self, base_type: &str) -> usize {
        let types = ti4_content::units::catalogue(self.content, self.sources);
        let matches = |unit: &ti4_model::units::Unit| {
            &unit.owner == self.player
                && types
                    .get(unit.type_id.as_str())
                    .is_some_and(|kind| kind.base_type() == base_type)
        };
        self.state
            .board
            .values()
            .map(|board| {
                board.units.iter().filter(|u| matches(u)).count()
                    + board
                        .planet_units
                        .values()
                        .flatten()
                        .filter(|u| matches(u))
                        .count()
            })
            .sum()
    }
    /// Systems where this player has a ship.
    fn systems_with_ships(&self) -> usize {
        let types = ti4_content::units::catalogue(self.content, self.sources);
        self.state
            .board
            .values()
            .filter(|board| {
                board.units_of(self.player).into_iter().any(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
                })
            })
            .count()
    }

    /// Controlled planets of one trait.
    fn planets_of_trait(&self, trait_name: &str) -> usize {
        let catalogue = ti4_content::galaxy::all_planets(self.content, self.sources);
        self.state
            .controlled_planets(self.player)
            .into_iter()
            .filter(|(_, planet)| {
                catalogue
                    .get(planet.as_str())
                    .is_some_and(|record| record.has_trait(trait_name))
            })
            .count()
    }

    /// Combined resources or influence of controlled planets.
    fn combined(&self, kind: crate::production::Spend) -> i64 {
        self.state
            .controlled_planets(self.player)
            .into_iter()
            .map(|(_, planet)| {
                crate::production::planet_value(self.content, self.sources, planet, kind)
            })
            .sum()
    }

    /// Technologies owned of one colour.
    fn technologies_of_colour(&self, colour: &str) -> usize {
        self.state.player(self.player).map_or(0, |seat| {
            seat.technologies
                .iter()
                .filter(|alias| crate::technology::colour_type(self.content, alias) == Some(colour))
                .count()
        })
    }
}

/// Have `count` units of a base type on the board.
fn units_count(position: &Position<'_>, base_type: &str) -> usize {
    position.units_on_board(base_type)
}

/// Have ships in `count` systems.
fn ships_in_systems_count(position: &Position<'_>) -> usize {
    position.systems_with_ships()
}

/// Control `count` planets of one trait.
fn planets_of_trait_count(position: &Position<'_>, trait_name: &str) -> usize {
    position.planets_of_trait(trait_name)
}

/// Own `count` technologies of the same colour.
///
/// The same colour, not four technologies: a spread across four tracks is the opposite of what
/// this card asks for.
fn same_colour_technologies_count(position: &Position<'_>) -> usize {
    crate::technology::COLOURS
        .iter()
        .map(|colour| position.technologies_of_colour(colour))
        .max()
        .unwrap_or(0)
}

/// Have `count` mechs, each on a different planet.
///
/// Four mechs on one planet is not four planets, which is the whole shape of the card.
fn mechs_on_distinct_planets_count(position: &Position<'_>) -> usize {
    let types = ti4_content::units::catalogue(position.content, position.sources);
    let mut planets = std::collections::BTreeSet::new();
    for board in position.state.board.values() {
        for (planet, units) in &board.planet_units {
            if units.iter().any(|unit| {
                &unit.owner == position.player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "mech")
            }) {
                planets.insert(planet.clone());
            }
        }
    }
    planets.len()
}

/// Have `count` or more ground forces on one planet.
fn ground_forces_on_one_planet_count(position: &Position<'_>) -> usize {
    let types = ti4_content::units::catalogue(position.content, position.sources);
    position
        .state
        .board
        .values()
        .flat_map(|board| board.planet_units.values())
        .map(|units| {
            units
                .iter()
                .filter(|unit| {
                    &unit.owner == position.player
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_ground_force)
                })
                .count()
        })
        .max()
        .unwrap_or(0)
}

/// The systems where this player has a ship, by id.
///
/// `Position::systems_with_ships` counts them; several cards need to know *which*.
fn ship_systems<'a>(position: &Position<'a>) -> Vec<&'a ti4_model::id::SystemId> {
    let types = ti4_content::units::catalogue(position.content, position.sources);
    position
        .state
        .board
        .iter()
        .filter(|(_, board)| {
            board.units_of(position.player).into_iter().any(|unit| {
                types
                    .get(unit.type_id.as_str())
                    .is_some_and(ti4_content::units::UnitType::is_ship)
            })
        })
        .map(|(id, _)| id)
        .collect()
}

/// Relic fragments this player holds, of any trait.
fn fragment_count(state: &GameState, player: &PlayerId) -> i32 {
    state.player(player).map_or(0, |seat| {
        seat.relic_fragments
            .values()
            .filter(|held| **held > 0)
            .sum()
    })
}

fn legendary_planets_count(position: &Position<'_>) -> usize {
    let catalogue = ti4_content::galaxy::all_planets(position.content, position.sources);
    position
        .state
        .controlled_planets(position.player)
        .into_iter()
        .filter(|(_, planet)| {
            catalogue
                .get(planet.as_str())
                .is_some_and(ti4_content::galaxy::Planet::is_legendary)
        })
        .count()
}

fn mecatol_ships_while_controlling_count(position: &Position<'_>) -> usize {
    let mecatol = ti4_model::id::SystemId::new(crate::seating::MECATOL);
    let board = position.state.system_state(&mecatol);
    if !board.controls_a_planet(position.player) {
        return 0;
    }
    let types = ti4_content::units::catalogue(position.content, position.sources);
    board
        .units_of(position.player)
        .into_iter()
        .filter(|unit| {
            types
                .get(unit.type_id.as_str())
                .is_some_and(ti4_content::units::UnitType::is_ship)
        })
        .count()
}

fn rival_dock_systems_with_ships_count(position: &Position<'_>) -> usize {
    let types = ti4_content::units::catalogue(position.content, position.sources);
    ship_systems(position)
        .into_iter()
        .filter(|system| {
            position
                .state
                .system_state(system)
                .planet_units
                .values()
                .flatten()
                .any(|unit| {
                    &unit.owner != position.player
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(|kind| kind.base_type() == "spacedock")
                })
        })
        .count()
}

fn wormhole_kinds_count(position: &Position<'_>) -> usize {
    let systems = ti4_content::galaxy::all_systems(position.content, position.sources);
    let mut kinds = std::collections::BTreeSet::new();
    for system in ship_systems(position) {
        if let Some(record) = systems.get(system.as_str()) {
            kinds.extend(
                record
                    .wormholes()
                    .into_iter()
                    .filter(|wormhole| matches!(*wormhole, "ALPHA" | "BETA")),
            );
        }
    }
    kinds.len()
}

fn faction_technologies_count(position: &Position<'_>) -> usize {
    position.state.player(position.player).map_or(0, |seat| {
        seat.technologies
            .iter()
            .filter(|alias| {
                position
                    .content
                    .get(ContentType::Technologies, alias.as_str())
                    .and_then(|record| record.text("faction"))
                    .is_some()
            })
            .count()
    })
}

fn shared_planet_systems_count(position: &Position<'_>) -> usize {
    position
        .state
        .board
        .values()
        .filter(|board| {
            let mine = board
                .planet_control
                .values()
                .any(|owner| owner == position.player);
            let theirs = board
                .planet_control
                .values()
                .any(|owner| owner != position.player);
            mine && theirs
        })
        .count()
}

fn production_in_one_system_count(position: &Position<'_>) -> usize {
    position
        .state
        .board
        .keys()
        .map(|system| {
            crate::production::capacity(
                position.state,
                position.content,
                position.sources,
                position.player,
                system,
            )
        })
        .max()
        .and_then(|capacity| usize::try_from(capacity).ok())
        .unwrap_or(0)
}

fn ship_systems_beside_anomaly_count(position: &Position<'_>) -> Option<usize> {
    let galaxy = position.galaxy?;
    let systems = ti4_content::galaxy::all_systems(position.content, position.sources);
    Some(
        ship_systems(position)
            .into_iter()
            .filter(|here| {
                galaxy.adjacent(here.as_str()).into_iter().any(|other| {
                    systems
                        .get(other)
                        .is_some_and(ti4_content::galaxy::System::is_anomaly)
                })
            })
            .count(),
    )
}

fn ship_systems_beside_rival_home_count(position: &Position<'_>) -> Option<usize> {
    let galaxy = position.galaxy?;
    let mine = position
        .state
        .player(position.player)
        .map(|seat| seat.faction.to_string());
    let planets = ti4_content::galaxy::all_planets(position.content, position.sources);
    let systems = ti4_content::galaxy::all_systems(position.content, position.sources);
    Some(
        ship_systems(position)
            .into_iter()
            .filter(|here| {
                galaxy.adjacent(here.as_str()).into_iter().any(|other| {
                    systems.get(other).is_some_and(|system| {
                        system.planets().into_iter().any(|planet| {
                            planets
                                .get(planet)
                                .and_then(ti4_content::galaxy::Planet::homeworld_of)
                                .is_some_and(|faction| Some(faction.to_owned()) != mine)
                        })
                    })
                })
            })
            .count(),
    )
}

fn neighbour_progress(position: &Position<'_>) -> Option<(usize, usize)> {
    let galaxy = position.galaxy?;
    let threshold = position
        .state
        .players
        .iter()
        .filter(|seat| seat.id != *position.player)
        .count();
    if threshold == 0 {
        return None;
    }
    let reached = crate::transactions::neighbours(position.state, galaxy, position.player);
    let have = position
        .state
        .players
        .iter()
        .filter(|seat| seat.id != *position.player && reached.contains(&seat.id))
        .count();
    Some((have, threshold))
}

fn units_in_nexus_count(position: &Position<'_>) -> usize {
    ti4_content::galaxy::all_systems(position.content, position.sources)
        .iter()
        .filter(|(_, system)| {
            system
                .name()
                .is_some_and(|name| name.to_ascii_lowercase().contains("nexus"))
        })
        .map(|(id, _)| {
            position
                .state
                .system_state(&ti4_model::id::SystemId::new(*id))
                .units_of(position.player)
                .len()
        })
        .sum()
}

fn rival_note_issuers_count(position: &Position<'_>) -> usize {
    let mut issuers = std::collections::BTreeSet::new();
    for (owner, holder) in &position.state.support_holders {
        if holder == position.player && owner != position.player {
            issuers.insert(format!("player:{owner}"));
        }
    }
    let faction = position
        .state
        .player(position.player)
        .map(|seat| seat.faction.as_str());
    for (note, holder) in &position.state.promissory_notes {
        if holder == position.player
            && let Some(issuer) = position
                .content
                .get(
                    ContentType::PromissoryNotes,
                    crate::promissory::alias_of(note),
                )
                .and_then(|record| record.text("faction"))
            && Some(issuer) != faction
        {
            let issuer_key = position
                .state
                .players
                .iter()
                .find(|seat| seat.faction.as_str() == issuer)
                .map_or_else(
                    || format!("faction:{issuer}"),
                    |seat| format!("player:{}", seat.id),
                );
            issuers.insert(issuer_key);
        }
    }
    issuers.len()
}

/// What scoring a secret costs, beyond meeting its requirement.
///
/// Two secrets are *paid for* rather than achieved: the fragments are purged and the cards are
/// discarded as part of scoring. Charged in [`award`] and never in the requirement, so being
/// offered the objective costs nothing — a requirement that spent as a side effect would charge
/// a player for merely being asked.
fn pay_for(state: &mut GameState, player: &PlayerId, alias: &SecretObjectiveId) -> bool {
    match alias.as_str() {
        "dhw" => {
            if fragment_count(state, player) < 2 {
                return false;
            }
            let Some(seat) = state.player_mut(player) else {
                return false;
            };
            let mut owed = 2;
            for held in seat.relic_fragments.values_mut() {
                while owed > 0 && *held > 0 {
                    *held -= 1;
                    owed -= 1;
                }
            }
            seat.relic_fragments.retain(|_, held| *held > 0);
            owed == 0
        }
        "fsn" => {
            let Some(seat) = state.player_mut(player) else {
                return false;
            };
            if seat.action_cards.len() < 5 {
                return false;
            }
            seat.action_cards.drain(..5);
            true
        }
        _ => true,
    }
}

/// Exact progress for secret objectives backed by counting families.
#[must_use]
pub fn counting_progress(
    alias: &SecretObjectiveId,
    position: &Position<'_>,
) -> Option<crate::objectives::RequirementProgress> {
    use crate::objectives::{CountFamily, RequirementProgress};
    let (family, have, threshold) = match alias.as_str() {
        "eap" => (
            CountFamily::Units { base_type: "pds" },
            units_count(position, "pds"),
            4,
        ),
        "fwm" => (
            CountFamily::Units {
                base_type: "spacedock",
            },
            units_count(position, "spacedock"),
            3,
        ),
        "gamf" => (
            CountFamily::Units {
                base_type: "dreadnought",
            },
            units_count(position, "dreadnought"),
            5,
        ),
        "ctr" => (
            CountFamily::ShipsInSystems,
            ships_in_systems_count(position),
            6,
        ),
        "faa" => (
            CountFamily::PlanetsOfTrait {
                trait_name: "cultural",
            },
            planets_of_trait_count(position, "cultural"),
            4,
        ),
        "mrm" => (
            CountFamily::PlanetsOfTrait {
                trait_name: "hazardous",
            },
            planets_of_trait_count(position, "hazardous"),
            4,
        ),
        "mp" => (
            CountFamily::PlanetsOfTrait {
                trait_name: "industrial",
            },
            planets_of_trait_count(position, "industrial"),
            4,
        ),
        "mlp" => (
            CountFamily::SameColourTechnologies,
            same_colour_technologies_count(position),
            4,
        ),
        "mtm" => (
            CountFamily::MechsOnDistinctPlanets,
            mechs_on_distinct_planets_count(position),
            4,
        ),
        "otf" => (
            CountFamily::GroundForcesOnOnePlanet,
            ground_forces_on_one_planet_count(position),
            9,
        ),
        _ => return None,
    };
    Some(RequirementProgress {
        family,
        have,
        threshold,
    })
}

fn counting_satisfied(alias: &str, position: &Position<'_>) -> bool {
    counting_progress(&SecretObjectiveId::new(alias), position)
        .is_some_and(|progress| progress.satisfied())
}

/// Exact progress for every registered position secret not covered by M06-022.
#[must_use]
pub fn remaining_position_progress(
    alias: &SecretObjectiveId,
    position: &Position<'_>,
) -> Option<crate::objectives::RequirementProgress> {
    use crate::objectives::{CountFamily, RequirementProgress};
    let (family, have, threshold) = match alias.as_str() {
        "csl" => (
            CountFamily::RivalDockSystemsWithShips,
            rival_dock_systems_with_ships_count(position),
            1,
        ),
        "dhw" => (
            CountFamily::RelicFragments,
            usize::try_from(fragment_count(position.state, position.player)).unwrap_or(0),
            2,
        ),
        "fsn" => (
            CountFamily::ActionCards,
            position
                .state
                .player(position.player)
                .map_or(0, |seat| seat.action_cards.len()),
            5,
        ),
        "sb" => (
            CountFamily::RivalNoteIssuers,
            rival_note_issuers_count(position),
            1,
        ),
        "lsc" => (
            CountFamily::ShipSystemsBesideAnomaly,
            ship_systems_beside_anomaly_count(position)?,
            3,
        ),
        "te" => (
            CountFamily::ShipSystemsBesideRivalHome,
            ship_systems_beside_rival_home_count(position)?,
            1,
        ),
        "fc" => {
            let (have, threshold) = neighbour_progress(position)?;
            (CountFamily::Neighbours, have, threshold)
        }
        "dfat" => (CountFamily::UnitsInNexus, units_in_nexus_count(position), 1),
        "btgk" => (
            CountFamily::WormholeKinds,
            wormhole_kinds_count(position),
            2,
        ),
        "ans" => (
            CountFamily::FactionTechnologies,
            faction_technologies_count(position),
            2,
        ),
        "dp" => (CountFamily::Laws, position.state.laws.len(), 3),
        "syc" => (
            CountFamily::SharedPlanetSystems,
            shared_planet_systems_count(position),
            1,
        ),
        "pem" => (
            CountFamily::ProductionInOneSystem,
            production_in_one_system_count(position),
            8,
        ),
        "sai" => (
            CountFamily::LegendaryPlanets,
            legendary_planets_count(position),
            1,
        ),
        "ose" => (
            CountFamily::MecatolShipsWhileControlling,
            mecatol_ships_while_controlling_count(position),
            3,
        ),
        "eh" => (
            CountFamily::CombinedValue {
                kind: crate::production::Spend::Influence,
            },
            usize::try_from(position.combined(crate::production::Spend::Influence)).unwrap_or(0),
            12,
        ),
        "hrm" => (
            CountFamily::CombinedValue {
                kind: crate::production::Spend::Resources,
            },
            usize::try_from(position.combined(crate::production::Spend::Resources)).unwrap_or(0),
            12,
        ),
        _ => return None,
    };
    Some(RequirementProgress {
        family,
        have,
        threshold,
    })
}

fn remaining_position_satisfied(alias: &str, position: &Position<'_>) -> bool {
    remaining_position_progress(&SecretObjectiveId::new(alias), position)
        .is_some_and(|progress| progress.satisfied())
}

macro_rules! remaining_requirement {
    ($name:ident, $alias:literal) => {
        fn $name(position: &Position<'_>) -> bool {
            remaining_position_satisfied($alias, position)
        }
    };
}

remaining_requirement!(progress_csl, "csl");
remaining_requirement!(progress_dhw, "dhw");
remaining_requirement!(progress_fsn, "fsn");
remaining_requirement!(progress_sb, "sb");
remaining_requirement!(progress_lsc, "lsc");
remaining_requirement!(progress_te, "te");
remaining_requirement!(progress_fc, "fc");
remaining_requirement!(progress_dfat, "dfat");
remaining_requirement!(progress_btgk, "btgk");
remaining_requirement!(progress_ans, "ans");
remaining_requirement!(progress_dp, "dp");
remaining_requirement!(progress_syc, "syc");
remaining_requirement!(progress_pem, "pem");
remaining_requirement!(progress_sai, "sai");
remaining_requirement!(progress_ose, "ose");
remaining_requirement!(progress_eh, "eh");
remaining_requirement!(progress_hrm, "hrm");

/// The registered requirements.
///
/// Two tranches, and unregistered secrets are unscoreable — the same design the objective
/// registry documents, and for the same reason: a coverage gap must show as a card nobody can
/// take, never as a bot winning on a rule that was never written.
#[must_use]
pub fn requirement_for(alias: &SecretObjectiveId) -> Option<Requirement> {
    fn four_pds(p: &Position<'_>) -> bool {
        counting_satisfied("eap", p)
    }
    // Three, not four: the printed card says "Have 3 space docks on the game board", and
    // taking the count from memory rather than the corpus is how an objective ends up
    // unscoreable in practice while looking implemented.
    fn three_docks(p: &Position<'_>) -> bool {
        counting_satisfied("fwm", p)
    }
    fn five_dreadnoughts(p: &Position<'_>) -> bool {
        counting_satisfied("gamf", p)
    }
    fn ships_in_six(p: &Position<'_>) -> bool {
        counting_satisfied("ctr", p)
    }
    fn four_cultural(p: &Position<'_>) -> bool {
        counting_satisfied("faa", p)
    }
    fn four_hazardous(p: &Position<'_>) -> bool {
        counting_satisfied("mrm", p)
    }
    fn four_industrial(p: &Position<'_>) -> bool {
        counting_satisfied("mp", p)
    }
    fn four_of_a_colour(p: &Position<'_>) -> bool {
        counting_satisfied("mlp", p)
    }

    fn four_mechs(p: &Position<'_>) -> bool {
        counting_satisfied("mtm", p)
    }
    fn nine_ground_forces(p: &Position<'_>) -> bool {
        counting_satisfied("otf", p)
    }

    match alias.as_str() {
        "csl" => Some(progress_csl),
        "dhw" => Some(progress_dhw),
        "fsn" => Some(progress_fsn),
        "sb" => Some(progress_sb),
        "lsc" => Some(progress_lsc),
        "te" => Some(progress_te),
        "fc" => Some(progress_fc),
        "dfat" => Some(progress_dfat),
        "btgk" => Some(progress_btgk),
        "ans" => Some(progress_ans),
        "dp" => Some(progress_dp),
        "syc" => Some(progress_syc),
        "pem" => Some(progress_pem),
        "sai" => Some(progress_sai),
        "ose" => Some(progress_ose),
        "mtm" => Some(four_mechs),
        "otf" => Some(nine_ground_forces),
        "eap" => Some(four_pds),
        "fwm" => Some(three_docks),
        "gamf" => Some(five_dreadnoughts),
        "ctr" => Some(ships_in_six),
        "faa" => Some(four_cultural),
        "mrm" => Some(four_hazardous),
        "mp" => Some(four_industrial),
        "eh" => Some(progress_eh),
        "hrm" => Some(progress_hrm),
        "mlp" => Some(four_of_a_colour),
        _ => None,
    }
}

/// The feat each event-conditioned secret asks for.
///
/// Twelve cards are written against something that *happened*, not something that *is*: a war sun
/// that was destroyed, a combat that was won, a seat that passed last. A `Position` holds the
/// board and the board does not remember any of it, which is why these had no requirement at all
/// until the ledger existed. See [`ti4_model::state::Feat`].
#[must_use]
pub const fn feat_for(alias: &str) -> Option<Feat> {
    let feat = match alias.as_bytes() {
        b"dtgs" => Feat::DestroyedACapitalShip,
        b"mew" => Feat::BombardedOutTheLastGroundForces,
        b"ttfd" => Feat::SpaceCannonTookTheLastNonFighters,
        b"fwp" => Feat::BarrageTookTheLastFighters,
        b"sar" => Feat::WonAgainstThePointsLeader,
        b"uf" => Feat::WonBesideASurvivingFlagship,
        b"baf" => Feat::WonAgainstANoteHolder,
        b"btv" => Feat::WonInAnAnomaly,
        b"dts" => Feat::WonInARivalHome,
        b"dyp" => Feat::HeldThreeShipsAfterASpaceCombat,
        b"dtd" => Feat::ElectedByAnAgenda,
        b"pe" => Feat::LastToPass,
        b"bam" => Feat::LostAHomePlanet,
        _ => return None,
    };
    Some(feat)
}

/// Aliases with a registered requirement, whether a position or a feat.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "ans", "baf", "bam", "btgk", "btv", "csl", "ctr", "dfat", "dhw", "dp", "dtd", "dtgs",
        "dts", "dyp", "eap", "eh", "faa", "fc", "fsn", "fwm", "fwp", "gamf", "hrm", "lsc", "mew",
        "mlp", "mp", "mrm", "mtm", "ose", "otf", "pe", "pem", "sai", "sar", "sb", "syc", "te",
        "ttfd", "uf",
    ]
}

/// Secret objectives in the corpus that have no registered requirement.
#[must_use]
pub fn unimplemented(content: &ContentStore, sources: SourceSet) -> Vec<SecretObjectiveId> {
    let known = registered_aliases();
    content
        .from_sources(ContentType::SecretObjectives, sources)
        .filter_map(|record| record.text("alias"))
        .filter(|alias| !known.contains(alias))
        .map(SecretObjectiveId::new)
        .collect()
}

/// Status-phase secrets this player may score now.
///
/// Action and agenda secrets are deliberately excluded: they are offered at the event that
/// satisfies them, and treating them as status objectives changes both their timing and whether
/// the triggering fact still exists by the time status arrives.
#[must_use]
pub fn scoreable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<SecretObjectiveId> {
    scoreable_on(state, content, sources, player, None)
}

/// Status-phase secrets this player may score now, with the map available.
///
/// Requirements about the shape of the board report unmet without it, so a driver that has a
/// galaxy should pass it: the same position otherwise scores differently depending on who asked.
#[must_use]
pub fn scoreable_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Vec<SecretObjectiveId> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let already = state.scored_by(player);
    let position = Position {
        state,
        content,
        sources,
        player,
        galaxy,
    };
    seat.secret_objectives
        .iter()
        .filter(|alias| timing(content, alias) == Timing::Status)
        .filter(|alias| !already.contains(&ti4_model::id::ObjectiveId::new(alias.as_str())))
        .filter(|alias| requirement_for(alias).is_some_and(|check| check(&position)))
        .cloned()
        .collect()
}

/// Action- and agenda-timed secrets this player may score now.
///
/// The counterpart to [`scoreable_on`], which deliberately answers only for the status phase.
/// Until this existed nothing anywhere offered a secret with either other timing, so fourteen of
/// the forty cards in the deck could not be scored at all — twelve because they had no
/// requirement, and two (`dp`, `dtd`) because even a satisfied requirement was never asked.
///
/// A card qualifies on either footing: a feat recorded for this exact event scope, or a position
/// that is true right now. `dp` is the second kind — "there are 3 or more laws in play" is a fact
/// about the table, and it happens to be checked in the agenda phase.
#[must_use]
pub fn scoreable_event(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    when: Timing,
    occurrence: FeatOccurrence,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Vec<SecretObjectiveId> {
    if when == Timing::Status {
        // Status secrets have their own window, and offering them here would score them twice.
        return Vec::new();
    }
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let already = state.scored_by(player);
    let position = Position {
        state,
        content,
        sources,
        player,
        galaxy,
    };
    seat.secret_objectives
        .iter()
        .filter(|alias| timing(content, alias) == when)
        .filter(|alias| !already.contains(&ti4_model::id::ObjectiveId::new(alias.as_str())))
        .filter(|alias| {
            feat_for(alias.as_str())
                .is_some_and(|feat| state.did_at_occurrence(player, feat, occurrence))
                || requirement_for(alias).is_some_and(|check| check(&position))
        })
        .cloned()
        .collect()
}

/// 61.18: reveal it, take the points, and it leaves the hand.
pub fn award(
    state: &mut GameState,
    content: &ContentStore,
    player: &PlayerId,
    alias: &SecretObjectiveId,
) -> Option<i32> {
    let held = state
        .player(player)
        .is_some_and(|seat| seat.secret_objectives.contains(alias));
    if !held {
        return None;
    }
    // Paid for before it is recorded: a secret whose price cannot be met is not scored, and a
    // score recorded first would be kept even when the payment failed.
    if !pay_for(state, player, alias) {
        return None;
    }
    let points = content
        .get(ContentType::SecretObjectives, alias.as_str())
        .and_then(|record| record.int("points"))
        .and_then(|points| i32::try_from(points).ok())
        .unwrap_or(1);

    state.record_score(player, ti4_model::id::ObjectiveId::new(alias.as_str()));
    let seat = state.player_mut(player)?;
    seat.secret_objectives.retain(|held| held != alias);
    seat.victory_points = (seat.victory_points + points).min(crate::objectives::VICTORY_TARGET);
    Some(points)
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;

    use super::*;
    use crate::fixtures::{a_placed_planet, game, put};

    fn player() -> PlayerId {
        PlayerId::new("a")
    }

    fn hand(state: &GameState) -> Vec<SecretObjectiveId> {
        state.player(&player()).unwrap().secret_objectives.clone()
    }

    #[test]
    fn all_secret_counting_aliases_have_typed_progress_and_shared_legality() {
        use crate::objectives::CountFamily;

        let state = game(&["a"]);
        let before = state.clone();
        let seat = player();
        let position = Position {
            state: &state,
            content: ContentStore::embedded(),
            sources: POK,
            player: &seat,
            galaxy: None,
        };
        let expected = [
            ("eap", CountFamily::Units { base_type: "pds" }, 4),
            (
                "fwm",
                CountFamily::Units {
                    base_type: "spacedock",
                },
                3,
            ),
            (
                "gamf",
                CountFamily::Units {
                    base_type: "dreadnought",
                },
                5,
            ),
            ("ctr", CountFamily::ShipsInSystems, 6),
            (
                "faa",
                CountFamily::PlanetsOfTrait {
                    trait_name: "cultural",
                },
                4,
            ),
            (
                "mrm",
                CountFamily::PlanetsOfTrait {
                    trait_name: "hazardous",
                },
                4,
            ),
            (
                "mp",
                CountFamily::PlanetsOfTrait {
                    trait_name: "industrial",
                },
                4,
            ),
            ("mlp", CountFamily::SameColourTechnologies, 4),
            ("mtm", CountFamily::MechsOnDistinctPlanets, 4),
            ("otf", CountFamily::GroundForcesOnOnePlanet, 9),
        ];

        for (alias, family, threshold) in expected {
            let id = SecretObjectiveId::new(alias);
            let progress = counting_progress(&id, &position).expect(alias);
            assert_eq!(progress.family, family, "{alias}");
            assert_eq!(progress.threshold, threshold, "{alias}");
            assert_eq!(
                requirement_for(&id).unwrap()(&position),
                progress.satisfied(),
                "{alias} legality must derive from progress"
            );
        }
        assert!(counting_progress(&SecretObjectiveId::new("unknown"), &position).is_none());
        assert_eq!(state, before, "progress queries cannot mutate rules state");
    }

    #[test]
    fn all_remaining_secret_aliases_have_typed_progress_and_reconcile_the_deck() {
        use crate::objectives::CountFamily;

        let state = game(&["a", "b"]);
        let before = state.clone();
        let seat = player();
        let hub = crate::fixtures::hub_with_centre(crate::seating::MECATOL);
        let position = Position {
            state: &state,
            content: ContentStore::embedded(),
            sources: POK,
            player: &seat,
            galaxy: Some(&hub.galaxy),
        };
        let expected = [
            ("csl", CountFamily::RivalDockSystemsWithShips, 1),
            ("dhw", CountFamily::RelicFragments, 2),
            ("fsn", CountFamily::ActionCards, 5),
            ("sb", CountFamily::RivalNoteIssuers, 1),
            ("lsc", CountFamily::ShipSystemsBesideAnomaly, 3),
            ("te", CountFamily::ShipSystemsBesideRivalHome, 1),
            ("fc", CountFamily::Neighbours, 1),
            ("dfat", CountFamily::UnitsInNexus, 1),
            ("btgk", CountFamily::WormholeKinds, 2),
            ("ans", CountFamily::FactionTechnologies, 2),
            ("dp", CountFamily::Laws, 3),
            ("syc", CountFamily::SharedPlanetSystems, 1),
            ("pem", CountFamily::ProductionInOneSystem, 8),
            ("sai", CountFamily::LegendaryPlanets, 1),
            ("ose", CountFamily::MecatolShipsWhileControlling, 3),
            (
                "eh",
                CountFamily::CombinedValue {
                    kind: crate::production::Spend::Influence,
                },
                12,
            ),
            (
                "hrm",
                CountFamily::CombinedValue {
                    kind: crate::production::Spend::Resources,
                },
                12,
            ),
        ];
        for (alias, family, threshold) in expected {
            let id = SecretObjectiveId::new(alias);
            let progress = remaining_position_progress(&id, &position).expect(alias);
            assert_eq!(progress.family, family, "{alias}");
            assert_eq!(progress.threshold, threshold, "{alias}");
            assert_eq!(
                requirement_for(&id).unwrap()(&position),
                progress.satisfied(),
                "{alias} legality derives from progress"
            );
        }
        assert!(
            remaining_position_progress(&SecretObjectiveId::new("unknown"), &position).is_none()
        );

        let counting: std::collections::BTreeSet<_> = [
            "eap", "fwm", "gamf", "ctr", "faa", "mrm", "mp", "mlp", "mtm", "otf",
        ]
        .into_iter()
        .collect();
        let occurrence: std::collections::BTreeSet<_> = [
            "dtgs", "mew", "ttfd", "fwp", "sar", "uf", "baf", "btv", "dts", "dyp", "dtd", "pe",
            "bam",
        ]
        .into_iter()
        .collect();
        let remaining: std::collections::BTreeSet<_> = [
            "csl", "dhw", "fsn", "sb", "lsc", "te", "fc", "dfat", "btgk", "ans", "dp", "syc",
            "pem", "sai", "ose", "eh", "hrm",
        ]
        .into_iter()
        .collect();
        assert!(counting.is_disjoint(&occurrence));
        assert!(counting.is_disjoint(&remaining));
        assert!(occurrence.is_disjoint(&remaining));
        let all: std::collections::BTreeSet<_> = counting
            .into_iter()
            .chain(occurrence)
            .chain(remaining)
            .collect();
        assert_eq!(all.len(), 40);
        assert_eq!(all, registered_aliases().into_iter().collect());
        assert_eq!(state, before, "all secret progress queries are read-only");
    }

    #[test]
    fn remaining_map_secret_progress_is_unavailable_without_a_map_or_opponents() {
        let state = game(&["a"]);
        let seat = player();
        let position = Position {
            state: &state,
            content: ContentStore::embedded(),
            sources: POK,
            player: &seat,
            galaxy: None,
        };
        for alias in ["lsc", "te", "fc"] {
            assert!(
                remaining_position_progress(&SecretObjectiveId::new(alias), &position).is_none()
            );
        }
        let hub = crate::fixtures::hub_with_centre(crate::seating::MECATOL);
        let with_map = Position {
            galaxy: Some(&hub.galaxy),
            ..position
        };
        assert!(
            remaining_position_progress(&SecretObjectiveId::new("fc"), &with_map).is_none(),
            "a one-player table cannot manufacture a zero threshold"
        );
    }

    #[test]
    fn a_hand_over_three_returns_one_to_the_deck() {
        // 45.4.
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        state.secret_deck = (0..5)
            .map(|n| SecretObjectiveId::new(format!("s{n}")))
            .collect();
        let mut table = Table::new();

        for _ in 0..4 {
            draw(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap();
        }

        assert_eq!(hand(&state).len(), HAND_LIMIT, "the fourth was handed back");
    }

    #[test]
    fn a_scored_secret_counts_towards_the_limit_but_cannot_be_returned() {
        // A player who has scored two may only hold one more.
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("eap"));
        state.record_score(&player(), ti4_model::id::ObjectiveId::new("fwm"));

        assert_eq!(
            scored_count(&state, ContentStore::embedded(), &player()),
            2,
            "both are real secrets in the corpus"
        );

        state.secret_deck = (0..3)
            .map(|n| SecretObjectiveId::new(format!("s{n}")))
            .collect();
        let mut table = Table::new();
        for _ in 0..3 {
            draw(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap();
        }

        assert_eq!(
            hand(&state).len(),
            1,
            "two scored plus one held is the limit"
        );
    }

    #[test]
    fn a_player_whose_every_secret_is_scored_has_nothing_to_return() {
        // The rules working, not a stuck state.
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        for alias in ["eap", "fwm", "ctr", "dtd"] {
            state.record_score(&player(), ti4_model::id::ObjectiveId::new(alias));
        }
        let mut table = Table::new();

        enforce_hand_limit(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap();

        assert!(hand(&state).is_empty());
    }

    #[test]
    fn an_empty_deck_draws_nothing() {
        let mut state = game(&["a"]);
        state.secret_deck.clear();
        let mut table = Table::new();
        assert_eq!(
            draw(&mut state, ContentStore::embedded(), &mut table, &player()).unwrap(),
            None
        );
    }

    #[test]
    fn only_status_secrets_are_offered_at_status_time() {
        // Action and agenda secrets are offered at the event that satisfies them; treating
        // them as status objectives changes their timing and whether the fact still holds.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("eap")];
        // Satisfy it: four PDS on the board.
        let (system, planet) = a_placed_planet();
        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    player(),
                ));
        }

        let open = scoreable(&state, ContentStore::embedded(), POK, &player());
        for alias in &open {
            assert_eq!(timing(ContentStore::embedded(), alias), Timing::Status);
        }
    }

    #[test]
    fn an_unregistered_secret_is_never_scoreable() {
        // The same design the objective registry documents: a coverage gap shows as a card
        // nobody can take, never as a bot winning on a rule nobody wrote.
        let mut state = game(&["a"]);
        let unregistered = SecretObjectiveId::new("dtd");
        assert!(requirement_for(&unregistered).is_none());
        state.player_mut(&player()).unwrap().secret_objectives = vec![unregistered];

        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());
    }

    #[test]
    fn four_pds_satisfies_its_secret() {
        let mut state = game(&["a"]);
        let eap = SecretObjectiveId::new("eap");
        state.player_mut(&player()).unwrap().secret_objectives = vec![eap.clone()];
        let (system, planet) = a_placed_planet();

        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());

        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    player(),
                ));
        }

        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![eap]
        );
    }

    #[test]
    fn another_players_units_do_not_count() {
        let mut state = game(&["a", "b"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("eap")];
        let (system, planet) = a_placed_planet();
        for _ in 0..6 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    PlayerId::new("b"),
                ));
        }

        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());
    }

    /// Give this player one secret and nothing else in hand.
    fn holding(state: &mut GameState, alias: &str) {
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new(alias)];
    }

    fn can_score(state: &GameState, alias: &str) -> bool {
        scoreable(state, ContentStore::embedded(), POK, &player())
            .contains(&SecretObjectiveId::new(alias))
    }

    /// A position with the map attached.
    fn on_map<'a>(
        state: &'a GameState,
        seat: &'a PlayerId,
        galaxy: &'a ti4_content::galaxy::Galaxy,
    ) -> Position<'a> {
        Position {
            state,
            content: ContentStore::embedded(),
            sources: POK,
            player: seat,
            galaxy: Some(galaxy),
        }
    }

    #[test]
    fn the_map_shaped_secrets_are_unmet_without_a_map() {
        let state = game(&["a", "b"]);
        let seat = player();
        let blind = Position {
            state: &state,
            content: ContentStore::embedded(),
            sources: POK,
            player: &seat,
            galaxy: None,
        };

        for alias in ["lsc", "te", "fc"] {
            assert!(remaining_position_progress(&SecretObjectiveId::new(alias), &blind).is_none());
        }
    }

    #[test]
    fn learning_the_secrets_counts_systems_not_anomalies() {
        // A hub whose centre is an anomaly: every ring system is adjacent to it, so ships in
        // three ring systems satisfy the card and ships in two do not.
        let anomaly = ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| system.is_anomaly() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned());
        let Some(anomaly) = anomaly else {
            return; // no anomaly in this corpus
        };
        let hub = crate::fixtures::hub_with_centre(&anomaly);
        let mut state = game(&["a"]);
        let seat = player();

        for outer in hub.outer.iter().take(2) {
            crate::fixtures::put(
                &mut state,
                &ti4_model::id::SystemId::new(outer.clone()),
                "cruiser",
                &seat,
                3,
            );
        }
        assert_eq!(
            remaining_position_progress(
                &SecretObjectiveId::new("lsc"),
                &on_map(&state, &seat, &hub.galaxy),
            )
            .unwrap()
            .have,
            2,
            "two systems beside an anomaly are two, however many ships are in them"
        );

        crate::fixtures::put(
            &mut state,
            &ti4_model::id::SystemId::new(hub.outer[2].clone()),
            "cruiser",
            &seat,
            1,
        );
        assert!(
            remaining_position_progress(
                &SecretObjectiveId::new("lsc"),
                &on_map(&state, &seat, &hub.galaxy),
            )
            .unwrap()
            .satisfied()
        );
    }

    #[test]
    fn threatening_enemies_needs_a_rivals_home_not_your_own() {
        let content = ContentStore::embedded();
        let homes_in = |system: &str| -> Vec<String> {
            ti4_content::galaxy::planets_in(content, system, POK)
                .into_iter()
                .filter_map(|planet| planet.homeworld_of().map(ToOwned::to_owned))
                .collect()
        };

        // The ring must hold no homeworld at all, or a second home next door keeps the
        // requirement true after the player adopts the first faction and the test passes for
        // the wrong reason. Ordinary systems include homeworlds, so the ring has to be chosen.
        let (faction, home_system) = ti4_content::galaxy::all_planets(content, POK)
            .iter()
            .find_map(|(_, planet)| {
                planet
                    .homeworld_of()
                    .zip(planet.system_id())
                    .map(|(faction, system)| (faction.to_owned(), system.to_owned()))
            })
            .expect("the corpus has a homeworld");

        let mut ids = vec![home_system.clone()];
        ids.extend(
            ti4_content::galaxy::all_systems(content, POK)
                .iter()
                .filter(|(_, system)| !system.is_anomaly() && !system.is_hyperlane())
                .map(|(id, _)| (*id).to_owned())
                .filter(|id| id != &home_system && homes_in(id).is_empty())
                .take(6),
        );
        assert_eq!(ids.len(), 7, "a centre and a ring of six");
        let hub = crate::fixtures::hub_from(&ids);

        let mut state = game(&["a"]);
        let seat = player();
        crate::fixtures::put(
            &mut state,
            &ti4_model::id::SystemId::new(hub.outer[0].clone()),
            "cruiser",
            &seat,
            1,
        );

        assert!(
            remaining_position_progress(
                &SecretObjectiveId::new("te"),
                &on_map(&state, &seat, &hub.galaxy),
            )
            .unwrap()
            .satisfied(),
            "a ship next to somebody else's home"
        );

        // Playing that faction makes it your own home, and the threat is empty.
        state.player_mut(&seat).unwrap().faction = ti4_model::id::FactionId::new(faction);
        assert!(
            !remaining_position_progress(
                &SecretObjectiveId::new("te"),
                &on_map(&state, &seat, &hub.galaxy),
            )
            .unwrap()
            .satisfied(),
            "your own home is not an enemy to threaten"
        );
    }

    #[test]
    fn fostering_cohesion_needs_every_other_player() {
        let hub = crate::fixtures::plain_hub();
        let mut state = game(&["a", "b", "c"]);
        let seat = player();
        let centre = ti4_model::id::SystemId::new(hub.centre.clone());

        crate::fixtures::put(&mut state, &centre, "cruiser", &seat, 1);
        crate::fixtures::put(&mut state, &centre, "cruiser", &PlayerId::new("b"), 1);
        let one_of_two = remaining_position_progress(
            &SecretObjectiveId::new("fc"),
            &on_map(&state, &seat, &hub.galaxy),
        )
        .unwrap();
        assert_eq!((one_of_two.have, one_of_two.threshold), (1, 2));
        assert!(
            !one_of_two.satisfied(),
            "c is not a neighbour, so this is not cohesion"
        );

        crate::fixtures::put(&mut state, &centre, "cruiser", &PlayerId::new("c"), 1);
        assert!(
            remaining_position_progress(
                &SecretObjectiveId::new("fc"),
                &on_map(&state, &seat, &hub.galaxy),
            )
            .unwrap()
            .satisfied()
        );
    }

    #[test]
    fn defying_space_and_time_needs_the_nexus_itself() {
        let nexus = ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| {
                system
                    .name()
                    .is_some_and(|name| name.to_ascii_lowercase().contains("nexus"))
            })
            .map(|(id, _)| (*id).to_owned());
        let Some(nexus) = nexus else {
            return; // this corpus has no nexus
        };

        let mut state = game(&["a", "b"]);
        holding(&mut state, "dfat");
        let (elsewhere, _) = a_placed_planet();
        put(&mut state, &elsewhere, "cruiser", &player(), 1);
        assert!(
            !can_score(&state, "dfat"),
            "units elsewhere are not the nexus"
        );

        // Somebody else's fleet puts the nexus on the board without putting you in it, which is
        // the difference between "the nexus is in play" and "you have units there".
        let nexus = ti4_model::id::SystemId::new(nexus);
        put(&mut state, &nexus, "cruiser", &PlayerId::new("b"), 1);
        assert!(
            !can_score(&state, "dfat"),
            "another player's fleet in the nexus is not yours"
        );

        put(&mut state, &nexus, "cruiser", &player(), 1);
        assert!(can_score(&state, "dfat"));
    }

    #[test]
    fn seizing_an_icon_reports_each_controlled_legendary_planet() {
        let content = ContentStore::embedded();
        let legendary: Vec<ti4_model::id::PlanetId> =
            ti4_content::galaxy::all_planets(content, POK)
                .iter()
                .filter(|(_, planet)| planet.is_legendary())
                .take(2)
                .map(|(id, _)| ti4_model::id::PlanetId::new(*id))
                .collect();
        assert!(!legendary.is_empty(), "the corpus has a legendary planet");
        let mut state = game(&["a"]);
        let (system, _) = a_placed_planet();
        let seat = player();
        for (index, planet) in legendary.iter().enumerate() {
            state
                .system_mut(&system)
                .set_control(planet.clone(), seat.clone());
            let position = Position {
                state: &state,
                content,
                sources: POK,
                player: &seat,
                galaxy: None,
            };
            let progress =
                remaining_position_progress(&SecretObjectiveId::new("sai"), &position).unwrap();
            assert_eq!(progress.have, index + 1);
            assert!(progress.satisfied());
        }
    }

    #[test]
    fn destroying_heretical_works_purges_the_fragments_it_needs() {
        let mut state = game(&["a"]);
        holding(&mut state, "dhw");
        state.player_mut(&player()).unwrap().relic_fragments =
            [("CULTURAL".to_owned(), 1)].into_iter().collect();
        assert!(!can_score(&state, "dhw"), "one fragment is not two");

        state
            .player_mut(&player())
            .unwrap()
            .relic_fragments
            .insert("INDUSTRIAL".to_owned(), 1);
        assert!(can_score(&state, "dhw"), "two of any type");

        let points = award(
            &mut state,
            ContentStore::embedded(),
            &player(),
            &SecretObjectiveId::new("dhw"),
        );

        assert_eq!(points, Some(1));
        assert_eq!(
            fragment_count(&state, &player()),
            0,
            "both fragments were purged to pay for it"
        );
    }

    #[test]
    fn a_secret_whose_price_cannot_be_met_is_not_scored() {
        // The requirement and the price are checked at different moments, so a hand that
        // shrank in between must not score — and must not record a score it did not pay for.
        let mut state = game(&["a"]);
        holding(&mut state, "fsn");
        state.player_mut(&player()).unwrap().action_cards = (0..4)
            .map(|n| ti4_model::id::ActionCardId::new(format!("card{n}")))
            .collect();
        let before = state.player(&player()).unwrap().victory_points;

        let points = award(
            &mut state,
            ContentStore::embedded(),
            &player(),
            &SecretObjectiveId::new("fsn"),
        );

        assert_eq!(points, None);
        assert_eq!(state.player(&player()).unwrap().victory_points, before);
        assert_eq!(
            state.player(&player()).unwrap().action_cards.len(),
            4,
            "and nothing was taken"
        );
        assert_eq!(
            state.player(&player()).unwrap().secret_objectives.len(),
            1,
            "the card stays in hand"
        );
    }

    #[test]
    fn forming_a_spy_network_discards_five_cards_and_no_more() {
        let mut state = game(&["a"]);
        holding(&mut state, "fsn");
        state.player_mut(&player()).unwrap().action_cards = (0..7)
            .map(|n| ti4_model::id::ActionCardId::new(format!("card{n}")))
            .collect();
        assert!(can_score(&state, "fsn"));

        award(
            &mut state,
            ContentStore::embedded(),
            &player(),
            &SecretObjectiveId::new("fsn"),
        );

        assert_eq!(
            state.player(&player()).unwrap().action_cards.len(),
            2,
            "five discarded, the rest kept"
        );
    }

    #[test]
    fn strengthening_bonds_needs_somebody_elses_note() {
        let mut state = game(&["a", "b"]);
        holding(&mut state, "sb");
        assert!(!can_score(&state, "sb"));

        // Your own Support for the Throne, sitting in your own play area, is not a bond.
        state.support_holders.insert(player(), player());
        assert!(!can_score(&state, "sb"), "your own note is not another's");

        state.support_holders.insert(PlayerId::new("b"), player());
        assert!(can_score(&state, "sb"));
    }

    #[test]
    fn rival_note_progress_deduplicates_note_kinds_from_one_issuer() {
        let content = ContentStore::embedded();
        let (note, issuer) = content
            .from_sources(ContentType::PromissoryNotes, POK)
            .find_map(|record| {
                record
                    .text("alias")
                    .zip(record.text("faction"))
                    .map(|(alias, faction)| (alias.to_owned(), faction.to_owned()))
            })
            .expect("the corpus has a faction promissory note");
        let mut state = game(&["a", "b"]);
        let owner = PlayerId::new("b");
        let seat = player();
        state.player_mut(&owner).unwrap().faction = ti4_model::id::FactionId::new(issuer);
        state.support_holders.insert(owner.clone(), seat.clone());
        let real_note =
            crate::promissory::note_id(&note, state.player(&owner).unwrap().faction.as_str());
        state.promissory_notes.insert(real_note, seat.clone());
        let position = Position {
            state: &state,
            content,
            sources: POK,
            player: &seat,
            galaxy: None,
        };

        assert_eq!(
            remaining_position_progress(&SecretObjectiveId::new("sb"), &position)
                .unwrap()
                .have,
            1,
            "two note kinds from the same seated issuer are one rival issuer"
        );
    }

    #[test]
    fn cutting_supply_lines_needs_a_rival_dock_not_your_own() {
        let mut state = game(&["a", "b"]);
        holding(&mut state, "csl");
        let (system, planet) = a_placed_planet();
        put(&mut state, &system, "cruiser", &player(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "spacedock", &player(), 1);

        assert!(
            !can_score(&state, "csl"),
            "your own dock is not supply lines"
        );

        crate::fixtures::put_on_planet(
            &mut state,
            &system,
            &planet,
            "spacedock",
            &PlayerId::new("b"),
            1,
        );
        assert!(can_score(&state, "csl"));
    }

    #[test]
    fn the_gatekeeper_needs_both_wormhole_kinds() {
        // One kind twice is not both kinds, which is the only way to get this card wrong.
        let systems = ti4_content::galaxy::all_systems(ContentStore::embedded(), POK);
        let of_kind = |kind: &str, excluded: &str| -> Vec<String> {
            systems
                .iter()
                .filter(|(_, system)| {
                    system.wormholes().contains(kind) && !system.wormholes().contains(excluded)
                })
                .map(|(id, _)| (*id).to_owned())
                .collect()
        };
        let alphas = of_kind("ALPHA", "BETA");
        let betas = of_kind("BETA", "ALPHA");
        assert!(
            alphas.len() >= 2 && !betas.is_empty(),
            "the corpus has both"
        );

        let mut state = game(&["a"]);
        holding(&mut state, "btgk");
        for id in alphas.iter().take(2) {
            put(
                &mut state,
                &ti4_model::id::SystemId::new(id.clone()),
                "cruiser",
                &player(),
                1,
            );
        }
        assert!(!can_score(&state, "btgk"), "two alphas are still one kind");

        put(
            &mut state,
            &ti4_model::id::SystemId::new(betas[0].clone()),
            "cruiser",
            &player(),
            1,
        );
        assert!(can_score(&state, "btgk"));
    }

    #[test]
    fn adapting_new_strategies_counts_faction_technologies_only() {
        let content = ContentStore::embedded();
        let faction_techs: Vec<String> = content
            .from_sources(ContentType::Technologies, POK)
            .filter(|record| record.text("faction").is_some())
            .filter_map(|record| record.text("alias").map(ToOwned::to_owned))
            .take(2)
            .collect();
        let generic: Vec<String> = content
            .from_sources(ContentType::Technologies, POK)
            .filter(|record| record.text("faction").is_none())
            .filter_map(|record| record.text("alias").map(ToOwned::to_owned))
            .take(2)
            .collect();
        assert_eq!(faction_techs.len(), 2);

        let mut state = game(&["a"]);
        holding(&mut state, "ans");
        for alias in &generic {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
        assert!(
            !can_score(&state, "ans"),
            "two ordinary technologies are not two faction technologies"
        );

        for alias in &faction_techs {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
        assert!(can_score(&state, "ans"));
    }

    #[test]
    fn dictating_policy_reads_the_table_not_the_player() {
        // An agenda-phase secret, so scoreable() will not offer it at status time — the
        // requirement is still the thing under test.
        fn at<'a>(state: &'a GameState, seat: &'a PlayerId) -> Position<'a> {
            Position {
                state,
                content: ContentStore::embedded(),
                sources: POK,
                player: seat,
                galaxy: None,
            }
        }

        let mut state = game(&["a"]);
        let check = requirement_for(&SecretObjectiveId::new("dp")).expect("registered");
        let seat = player();
        assert!(!check(&at(&state, &seat)));
        for n in 0..3 {
            state.laws.insert(format!("law{n}"), String::new());
        }
        assert!(check(&at(&state, &seat)));
        assert_eq!(
            timing(ContentStore::embedded(), &SecretObjectiveId::new("dp")),
            Timing::Agenda,
            "it is scored in the agenda phase"
        );
    }

    #[test]
    fn staking_a_claim_needs_a_rival_in_the_same_system() {
        let mut state = game(&["a", "b"]);
        holding(&mut state, "syc");
        let (system, planet) = a_placed_planet();
        state
            .system_mut(&system)
            .set_control(planet.clone(), player());

        assert!(
            !can_score(&state, "syc"),
            "a system of your own is not a claim"
        );

        let other = ti4_content::galaxy::planets_in(ContentStore::embedded(), system.as_str(), POK)
            .into_iter()
            .map(|found| ti4_model::id::PlanetId::new(found.id()))
            .find(|found| found != &planet);
        let Some(other) = other else {
            return; // a one-planet system cannot show this
        };
        state
            .system_mut(&system)
            .set_control(other, PlayerId::new("b"));
        assert!(can_score(&state, "syc"));
    }

    #[test]
    fn producing_en_masse_counts_one_system_not_the_board() {
        // "in a single system" is the whole card: eight production spread over two systems is
        // not eight production.
        let content = ContentStore::embedded();
        let mut state = game(&["a", "b"]);
        holding(&mut state, "pem");

        // Find a system whose planets, all docked, reach the threshold.
        let big = ti4_content::galaxy::all_systems(content, POK)
            .iter()
            .filter(|(_, system)| system.planets().len() >= 2)
            .map(|(id, _)| (*id).to_owned())
            .find(|id| {
                let mut trial = game(&["a"]);
                let system = ti4_model::id::SystemId::new(id.clone());
                for planet in ti4_content::galaxy::planets_in(content, id, POK) {
                    let planet = ti4_model::id::PlanetId::new(planet.id());
                    trial
                        .system_mut(&system)
                        .set_control(planet.clone(), player());
                    crate::fixtures::put_on_planet(
                        &mut trial,
                        &system,
                        &planet,
                        "spacedock",
                        &player(),
                        1,
                    );
                }
                crate::production::capacity(&trial, content, POK, &player(), &system) >= 8
            });
        let Some(big) = big else {
            return; // no system in this corpus can reach eight on its own
        };

        let system = ti4_model::id::SystemId::new(big.clone());
        let planets: Vec<ti4_model::id::PlanetId> =
            ti4_content::galaxy::planets_in(content, &big, POK)
                .into_iter()
                .map(|planet| ti4_model::id::PlanetId::new(planet.id()))
                .collect();
        for planet in &planets {
            state
                .system_mut(&system)
                .set_control(planet.clone(), player());
        }
        // One dock short, so nothing on the board reaches eight yet.
        for planet in planets.iter().skip(1) {
            crate::fixtures::put_on_planet(&mut state, &system, planet, "spacedock", &player(), 1);
        }
        let (elsewhere, other_planet) = a_placed_planet();
        state
            .system_mut(&elsewhere)
            .set_control(other_planet.clone(), player());
        crate::fixtures::put_on_planet(
            &mut state,
            &elsewhere,
            &other_planet,
            "spacedock",
            &player(),
            1,
        );
        assert!(
            !can_score(&state, "pem"),
            "production spread over two systems is not production in one"
        );

        crate::fixtures::put_on_planet(&mut state, &system, &planets[0], "spacedock", &player(), 1);
        assert!(can_score(&state, "pem"));
    }

    #[test]
    fn four_mechs_on_one_planet_is_not_four_planets() {
        // The card counts planets, not mechs, which is its whole shape.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("mtm")];
        let (system, planet) = a_placed_planet();
        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("mech"),
                    player(),
                ));
        }

        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty(),
            "four mechs, one planet"
        );
        let seat = player();
        let position = Position {
            state: &state,
            content: ContentStore::embedded(),
            sources: POK,
            player: &seat,
            galaxy: None,
        };
        assert_eq!(
            counting_progress(&SecretObjectiveId::new("mtm"), &position)
                .unwrap()
                .have,
            1,
            "progress counts distinct occupied planets"
        );
    }

    #[test]
    fn holding_mecatol_needs_the_planet_and_the_ships() {
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("ose")];
        let mecatol = ti4_model::id::SystemId::new(crate::seating::MECATOL);

        // Ships alone are not control.
        for _ in 0..3 {
            state
                .system_mut(&mecatol)
                .units
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("cruiser"),
                    player(),
                ));
        }
        assert!(scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty());

        // Control without enough ships is not enough either.
        let rex =
            ti4_content::galaxy::planets_in(ContentStore::embedded(), crate::seating::MECATOL, POK)
                .first()
                .map(|planet| ti4_model::id::PlanetId::new(planet.id()));
        let Some(rex) = rex else { return };
        state.system_mut(&mecatol).set_control(rex, player());

        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![SecretObjectiveId::new("ose")],
            "control plus three ships scores it"
        );
    }

    #[test]
    fn four_technologies_of_one_colour_is_not_four_technologies() {
        // The card asks for four of the *same* colour; a spread across four tracks is the
        // opposite of what it wants.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("mlp")];

        let one_each: Vec<String> = crate::technology::COLOURS
            .iter()
            .filter_map(|colour| {
                ContentStore::embedded()
                    .records(ContentType::Technologies)
                    .iter()
                    .find(|record| record.strings("types").contains(colour))
                    .and_then(|record| record.text("alias"))
                    .map(ToOwned::to_owned)
            })
            .collect();
        for alias in &one_each {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &player()).is_empty(),
            "one of each colour is not four of one"
        );

        let four_biotic: Vec<String> = ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&"BIOTIC"))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(4)
            .collect();
        if four_biotic.len() < 4 {
            return;
        }
        for alias in &four_biotic {
            state
                .player_mut(&player())
                .unwrap()
                .technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![SecretObjectiveId::new("mlp")]
        );
    }

    #[test]
    fn a_combined_value_secret_adds_up_controlled_planets() {
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("hrm")];

        let mut total = 0;
        for (id, record) in &ti4_content::galaxy::all_planets(ContentStore::embedded(), POK) {
            if record.is_placed_during_play() || record.resources() == 0 {
                continue;
            }
            let system = ti4_model::id::SystemId::new(record.system_id().unwrap_or("18"));
            state
                .system_mut(&system)
                .set_control(ti4_model::id::PlanetId::new(*id), player());
            total += record.resources();
            if total >= 12 {
                break;
            }
        }

        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &player()),
            vec![SecretObjectiveId::new("hrm")],
            "twelve combined resources scores it"
        );
    }

    #[test]
    fn scoring_takes_it_out_of_the_hand_and_pays_points() {
        // 61.18.
        let mut state = game(&["a"]);
        let eap = SecretObjectiveId::new("eap");
        state.player_mut(&player()).unwrap().secret_objectives = vec![eap.clone()];

        let points = award(&mut state, ContentStore::embedded(), &player(), &eap).unwrap();

        assert!(points > 0);
        assert!(hand(&state).is_empty(), "it left the hand");
        assert_eq!(state.player(&player()).unwrap().victory_points, points);
        assert!(
            state
                .scored_by(&player())
                .contains(&ti4_model::id::ObjectiveId::new("eap"))
        );
    }

    #[test]
    fn a_secret_not_in_hand_cannot_be_scored() {
        let mut state = game(&["a"]);
        state
            .player_mut(&player())
            .unwrap()
            .secret_objectives
            .clear();
        assert_eq!(
            award(
                &mut state,
                ContentStore::embedded(),
                &player(),
                &SecretObjectiveId::new("eap")
            ),
            None
        );
    }

    #[test]
    fn every_registered_alias_is_a_real_secret() {
        for alias in registered_aliases() {
            assert!(
                ContentStore::embedded()
                    .get(ContentType::SecretObjectives, alias)
                    .is_some(),
                "{alias} is not a secret the corpus knows"
            );
            // Either footing counts: a position the board can answer, or a feat the ledger
            // records. Twelve of these have only the second, which is why they had no
            // requirement at all before the ledger existed.
            assert!(
                requirement_for(&SecretObjectiveId::new(alias)).is_some()
                    || feat_for(alias).is_some(),
                "{alias} is registered but nothing decides it"
            );
        }
        let _ = put;
    }

    #[test]
    fn every_secret_in_the_corpus_is_decidable() {
        // This asserted the opposite until the feat ledger landed: the deck shipped with 14 of
        // its 40 cards unscoreable, 12 with no requirement and 2 (`dp`, `dtd`) whose requirement
        // was never asked because nothing offered a non-status secret. A seat holding one of
        // those held a dead card, and no amount of play could score it.
        let missing = unimplemented(ContentStore::embedded(), POK);
        assert!(
            missing.is_empty(),
            "these secrets can never be scored: {missing:?}"
        );
    }

    #[test]
    fn an_action_secret_is_offered_only_to_the_seat_whose_feat_it_was() {
        let mut state = game(&["a", "b"]);
        state.turn_seq = 7;
        for seat in &mut state.players {
            seat.secret_objectives = vec![SecretObjectiveId::new("btv")];
        }
        let content = ContentStore::embedded();

        assert!(
            scoreable_event(
                &state,
                content,
                POK,
                &player(),
                Timing::Action,
                FeatOccurrence(7),
                None,
            )
            .is_empty(),
            "nobody has won anything yet"
        );

        state.record_event_feat(&player(), Feat::WonInAnAnomaly, FeatOccurrence(7));

        assert_eq!(
            scoreable_event(
                &state,
                content,
                POK,
                &player(),
                Timing::Action,
                FeatOccurrence(7),
                None,
            ),
            vec![SecretObjectiveId::new("btv")],
            "the seat that won the fight may score it"
        );
        assert!(
            scoreable_event(
                &state,
                content,
                POK,
                &PlayerId::new("b"),
                Timing::Action,
                FeatOccurrence(7),
                None
            )
            .is_empty(),
            "the other seat holds the same card and did not win the fight"
        );
        assert!(
            scoreable_event(
                &state,
                content,
                POK,
                &player(),
                Timing::Action,
                FeatOccurrence(8),
                None,
            )
            .is_empty(),
            "a fight won on turn 7 does not pay out on turn 8"
        );
    }

    #[test]
    fn an_occurrence_scopes_a_feat_to_that_event_and_its_owner() {
        let mut state = game(&["a", "b"]);
        let a = player();
        let b = PlayerId::new("b");
        state.player_mut(&a).unwrap().secret_objectives = vec![SecretObjectiveId::new("btv")];
        state.player_mut(&b).unwrap().secret_objectives = vec![SecretObjectiveId::new("btv")];
        let content = ContentStore::embedded();

        let combat = state.begin_feat_occurrence();
        state.record_event_feat(&a, Feat::WonInAnAnomaly, combat);
        let later = state.begin_feat_occurrence();

        assert_eq!(
            scoreable_event(&state, content, POK, &a, Timing::Action, combat, None,),
            vec![SecretObjectiveId::new("btv")],
        );
        assert!(
            scoreable_event(&state, content, POK, &b, Timing::Action, combat, None,).is_empty(),
            "another seat cannot read or claim the triggering feat"
        );
        assert!(
            scoreable_event(&state, content, POK, &a, Timing::Action, later, None,).is_empty(),
            "a feat from one occurrence cannot manufacture a later scoring window"
        );
    }

    #[test]
    fn become_a_martyr_is_offered_only_for_the_home_loss_occurrence() {
        let mut state = game(&["a", "b"]);
        let a = player();
        let b = PlayerId::new("b");
        let (home, planet) = a_placed_planet();
        {
            let seat = state.player_mut(&a).unwrap();
            seat.home_system = Some(home.clone());
            seat.home_planets.push(planet.clone());
            seat.secret_objectives = vec![SecretObjectiveId::new("bam")];
        }
        state.system_mut(&home).set_control(planet, b);

        let loss = state.begin_feat_occurrence();
        state.record_event_feat(&a, Feat::LostAHomePlanet, loss);
        let later = state.begin_feat_occurrence();
        let content = ContentStore::embedded();

        assert_eq!(feat_for("bam"), Some(Feat::LostAHomePlanet));
        assert!(requirement_for(&SecretObjectiveId::new("bam")).is_none());
        assert_eq!(
            scoreable_event(&state, content, POK, &a, Timing::Action, loss, None),
            vec![SecretObjectiveId::new("bam")],
        );
        assert!(
            scoreable_event(&state, content, POK, &a, Timing::Action, later, None).is_empty(),
            "the unchanged lost-home position must not leak into a later occurrence"
        );
    }

    #[test]
    fn a_status_secret_is_never_offered_in_an_event_window() {
        // The two windows would otherwise both offer it and score it twice.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives =
            vec![SecretObjectiveId::new("eap")];
        let content = ContentStore::embedded();
        for when in [Timing::Action, Timing::Agenda] {
            assert!(
                scoreable_event(
                    &state,
                    content,
                    POK,
                    &player(),
                    when,
                    FeatOccurrence(0),
                    None,
                )
                .is_empty(),
                "Establish a Perimeter is a status secret"
            );
        }
    }

    #[test]
    fn dictate_policy_needs_three_laws_and_the_agenda_window() {
        // Registered since the first tranche, and unscoreable the whole time: `scoreable_on`
        // returns status secrets only, and nothing else ever asked.
        let mut state = game(&["a"]);
        state.player_mut(&player()).unwrap().secret_objectives = vec![SecretObjectiveId::new("dp")];
        let content = ContentStore::embedded();

        assert!(
            scoreable_event(
                &state,
                content,
                POK,
                &player(),
                Timing::Agenda,
                FeatOccurrence(0),
                None,
            )
            .is_empty(),
            "no laws are in play"
        );
        for law in ["regulations", "censure", "articles"] {
            state.enact_law(law, "for");
        }
        assert_eq!(
            scoreable_event(
                &state,
                content,
                POK,
                &player(),
                Timing::Agenda,
                FeatOccurrence(0),
                None,
            ),
            vec![SecretObjectiveId::new("dp")],
            "three laws is what the card asks for"
        );
        assert!(
            scoreable_on(&state, content, POK, &player(), None).is_empty(),
            "and the status window still must not offer it"
        );
    }
}
