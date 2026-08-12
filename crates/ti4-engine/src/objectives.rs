//! Objectives and scoring (LRR 61, 81.1, 98).
//!
//! Ported from the oracle's `engine/objectives.py`.
//!
//! Objective cards carry their requirement as English prose — "Control 6 planets in non-home
//! systems" — so there is nothing to evaluate mechanically. Each therefore needs a predicate,
//! registered here against the card's alias. The *cards* stay data; only the requirement
//! checks are code.
//!
//! **An objective with no registered predicate cannot be scored.** That is the oracle's design
//! and it is deliberate: an unimplemented requirement must make the objective unavailable,
//! never silently scoreable, so coverage gaps show up as an objective nobody can take rather
//! than as a bot quietly winning on a rule that was never written.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::galaxy::{Planet, all_planets};
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{ObjectiveId, PlayerId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption, IllegalChoice, validate};

/// Ten victory points wins (LRR 98).
pub const VICTORY_TARGET: i32 = 10;

/// What the engine needs to evaluate a requirement.
///
/// The controlled-planet records are resolved once, at construction. They were previously
/// looked up per predicate, which rebuilt an index over the whole planet corpus for every
/// requirement of every player on every step — correct, but quadratic enough to dominate a
/// hundred-seed campaign.
pub struct Position<'a> {
    pub state: &'a GameState,
    pub content: &'a ContentStore,
    pub sources: SourceSet,
    pub player: &'a PlayerId,
    /// The map, when the caller has one.
    ///
    /// Several objectives ask about the *shape* of the board — its edge, what is adjacent to
    /// Mecatol Rex — which no amount of state can answer. Without a galaxy those requirements
    /// report unmet rather than guessing, exactly as the oracle does.
    pub galaxy: Option<&'a ti4_content::galaxy::Galaxy>,
    controlled: Vec<Planet<'a>>,
}

/// A registered requirement check.
type Requirement = fn(&Position<'_>) -> bool;

impl<'a> Position<'a> {
    /// Resolve a player's position once, ready for any number of requirement checks.
    ///
    /// Planets the corpus does not know are dropped rather than counted, matching the
    /// oracle's `_controlled`, which indexes into `all_planets` and skips misses.
    #[must_use]
    pub fn new(
        state: &'a GameState,
        content: &'a ContentStore,
        sources: SourceSet,
        player: &'a PlayerId,
    ) -> Self {
        let catalogue = all_planets(content, sources);
        let controlled = state
            .controlled_planets(player)
            .into_iter()
            .filter_map(|(_, planet)| catalogue.get(planet.as_str()).copied())
            .collect();
        Self {
            galaxy: None,
            state,
            content,
            sources,
            player,
            controlled,
        }
    }

    /// The content records for every planet this player controls.
    fn controlled(&self) -> &[Planet<'a>] {
        &self.controlled
    }

    /// How many technologies this player owns carrying a given corpus type.
    fn technology_types(&self, wanted: &str) -> usize {
        let Some(seat) = self.state.player(self.player) else {
            return 0;
        };
        seat.technologies
            .iter()
            .filter(|alias| {
                self.content
                    .get(ContentType::Technologies, alias.as_str())
                    .is_some_and(|record| record.strings("types").contains(&wanted))
            })
            .count()
    }

    /// Every structure this player has, as (system, planet).
    fn structures(&self) -> Vec<(String, String)> {
        let types = ti4_content::units::catalogue(self.content, self.sources);
        let mut found = Vec::new();
        for (system_id, system) in &self.state.board {
            for (planet, units) in &system.planet_units {
                for unit in units {
                    if &unit.owner == self.player
                        && types
                            .get(unit.type_id.as_str())
                            .is_some_and(ti4_content::units::UnitType::is_structure)
                    {
                        found.push((system_id.to_string(), planet.to_string()));
                    }
                }
            }
        }
        found
    }

    /// Attach the map, so requirements about the board's shape can be answered.
    #[must_use]
    pub const fn with_galaxy(mut self, galaxy: &'a ti4_content::galaxy::Galaxy) -> Self {
        self.galaxy = Some(galaxy);
        self
    }

    /// This player's home system, if their faction names one.
    fn home_system(&self) -> Option<String> {
        let seat = self.state.player(self.player)?;
        // The seat's own record wins over its faction's. A game may seat a player at a home
        // that is not their faction's printed one — a tournament replica, or a setup that
        // placed them elsewhere — and reading only the faction would call that home a foreign
        // system, which flips every requirement phrased "other than your home system".
        if let Some(home) = &seat.home_system {
            return Some(home.to_string());
        }
        ti4_content::factions::get(self.content, seat.faction.as_str())
            .and_then(|faction| faction.home_system())
            .map(ToOwned::to_owned)
    }
}

/// Systems where this player has a flagship or a war sun.
fn flagship_or_war_sun(position: &Position<'_>) -> Vec<String> {
    let types = ti4_content::units::catalogue(position.content, position.sources);
    position
        .state
        .board
        .iter()
        .filter(|(_, board)| {
            board.units.iter().any(|unit| {
                &unit.owner == position.player
                    && types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| matches!(kind.base_type(), "flagship" | "warsun"))
            })
        })
        .map(|(id, _)| id.to_string())
        .collect()
}

/// The home planets of everyone except this player.
///
/// A seat's own record wins over its faction's, so a tournament replica home is the one that
/// counts — the same order the oracle resolves them in.
fn rival_home_planets(position: &Position<'_>) -> std::collections::BTreeSet<String> {
    let mut planets = std::collections::BTreeSet::new();
    for seat in &position.state.players {
        if &seat.id == position.player {
            continue;
        }
        if !seat.home_planets.is_empty() {
            planets.extend(seat.home_planets.iter().map(ToString::to_string));
            continue;
        }
        if let Some(faction) = ti4_content::factions::get(position.content, seat.faction.as_str()) {
            planets.extend(faction.home_planets().iter().map(|&id| id.to_owned()));
        }
    }
    planets
}

/// The home systems of everyone except this player.
fn rival_home_systems(position: &Position<'_>) -> std::collections::BTreeSet<String> {
    let mut systems = std::collections::BTreeSet::new();
    for seat in &position.state.players {
        if &seat.id == position.player {
            continue;
        }
        if let Some(home) = &seat.home_system {
            systems.insert(home.to_string());
            continue;
        }
        if let Some(home) = ti4_content::factions::get(position.content, seat.faction.as_str())
            .and_then(|faction| faction.home_system())
        {
            systems.insert(home.to_owned());
        }
    }
    systems
}

/// Control one planet in another player's home system.
fn conquer_the_weak(position: &Position<'_>) -> bool {
    let rivals = rival_home_planets(position);
    position
        .controlled()
        .iter()
        .any(|planet| rivals.contains(planet.id()))
}

/// Have your flagship or a war sun on the game board.
fn engineer_a_marvel(position: &Position<'_>) -> bool {
    !flagship_or_war_sun(position).is_empty()
}

/// Have your flagship or war sun in another player's home system, or Mecatol Rex's.
fn achieve_supremacy(position: &Position<'_>) -> bool {
    let mut theirs = rival_home_systems(position);
    theirs.insert(crate::seating::MECATOL.to_owned());
    flagship_or_war_sun(position)
        .iter()
        .any(|system| theirs.contains(system))
}

/// Systems on the edge of the board: those with a neighbouring hex that holds no tile.
///
/// Derived, never listed. A board is built from whatever tiles a game was set up with, so its
/// edge is a property of that arrangement and a fixed list would be right for exactly one map.
fn edge_systems(galaxy: &ti4_content::galaxy::Galaxy) -> std::collections::BTreeSet<String> {
    galaxy
        .system_ids()
        .into_iter()
        .filter(|id| {
            galaxy.coord_of(id).is_some_and(|here| {
                here.neighbours()
                    .into_iter()
                    .any(|next| galaxy.system_at(next).is_none())
            })
        })
        .map(ToOwned::to_owned)
        .collect()
}

impl Position<'_> {
    /// Systems where this player has any unit, in space or on a planet.
    fn systems_holding_units(&self) -> Vec<String> {
        self.state
            .board
            .iter()
            .filter(|(_, board)| {
                board.units.iter().any(|unit| &unit.owner == self.player)
                    || board
                        .planet_units
                        .values()
                        .flatten()
                        .any(|unit| &unit.owner == self.player)
            })
            .map(|(id, _)| id.to_string())
            .collect()
    }

    /// Systems where this player has a ship.
    fn systems_with_ships(&self) -> Vec<String> {
        let types = ti4_content::units::catalogue(self.content, self.sources);
        self.state
            .board
            .iter()
            .filter(|(_, board)| {
                board.units_of(self.player).into_iter().any(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(ti4_content::units::UnitType::is_ship)
                })
            })
            .map(|(id, _)| id.to_string())
            .collect()
    }
}

/// Have units in `count` edge systems other than your home system.
fn on_the_rim(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        let Some(galaxy) = position.galaxy else {
            return false;
        };
        let edge = edge_systems(galaxy);
        let home = position.home_system();
        position
            .systems_holding_units()
            .into_iter()
            .filter(|system| edge.contains(system) && Some(system) != home.as_ref())
            .count()
            >= count
    }
}

/// Have ships in two systems adjacent to Mecatol Rex's.
fn intimidate_council(position: &Position<'_>) -> bool {
    let Some(galaxy) = position.galaxy else {
        return false;
    };
    let beside: std::collections::BTreeSet<&str> = galaxy.adjacent(crate::seating::MECATOL);
    if beside.is_empty() {
        return false; // Mecatol is not on this map, so nothing is adjacent to it
    }
    position
        .systems_with_ships()
        .into_iter()
        .filter(|system| beside.contains(system.as_str()))
        .count()
        >= 2
}

/// Control more planets than each of two of your neighbours.
///
/// "More than each of two" is the difficulty: beating one neighbour twice over is not beating
/// two neighbours.
fn push_boundaries(position: &Position<'_>) -> bool {
    let Some(galaxy) = position.galaxy else {
        return false;
    };
    let mine = position.state.controlled_planets(position.player).len();
    crate::transactions::neighbours(position.state, galaxy, position.player)
        .into_iter()
        .filter(|other| position.state.controlled_planets(other).len() < mine)
        .count()
        >= 2
}

/// Control two planets each in or adjacent to a *different* other player's home system.
///
/// "Different" is the whole difficulty: two planets around one opponent's home are one distant
/// land, not two.
fn rule_distant_lands(position: &Position<'_>) -> bool {
    let Some(galaxy) = position.galaxy else {
        return false;
    };
    let mut homes: Vec<(PlayerId, std::collections::BTreeSet<String>)> = Vec::new();
    for seat in &position.state.players {
        if &seat.id == position.player {
            continue;
        }
        let home = seat
            .home_system
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| {
                ti4_content::factions::get(position.content, seat.faction.as_str())
                    .and_then(|faction| faction.home_system())
                    .map(ToOwned::to_owned)
            });
        let Some(home) = home else {
            continue;
        };
        let mut reach: std::collections::BTreeSet<String> = galaxy
            .adjacent(&home)
            .into_iter()
            .map(ToOwned::to_owned)
            .collect();
        reach.insert(home);
        homes.push((seat.id.clone(), reach));
    }

    // One planet may only speak for one opponent, so count the opponents reached, not the
    // planets held.
    let held: Vec<String> = position
        .state
        .controlled_planets(position.player)
        .into_iter()
        .map(|(system, _)| system.to_string())
        .collect();
    homes
        .iter()
        .filter(|(_, reach)| held.iter().any(|system| reach.contains(system)))
        .count()
        >= 2
}

/// Control `count` planets in non-home systems.
fn non_home(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        position
            .controlled()
            .iter()
            .filter(|planet| planet.homeworld_of().is_none())
            .count()
            >= count
    }
}

/// Control `count` planets that each have the same planet trait.
fn same_trait(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
        for planet in position.controlled() {
            if let Some(trait_name) = planet.planet_type() {
                *counts.entry(trait_name).or_default() += 1;
            }
        }
        counts.values().any(|n| *n >= count)
    }
}

/// Control `count` planets that have technology specialties.
fn tech_specialties(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        position
            .controlled()
            .iter()
            .filter(|planet| !planet.tech_specialties().is_empty())
            .count()
            >= count
    }
}

/// Own `count` unit-upgrade technologies.
fn unit_upgrades(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| position.technology_types("UNITUPGRADE") >= count
}

/// Own `per_colour` technologies in each of `colours` colours.
///
/// Unit upgrades have no colour (90.7b), which is why they are counted separately above rather
/// than being one more entry in this tally.
fn colours(per_colour: usize, colours: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        COLOURS
            .iter()
            .filter(|colour| position.technology_types(colour) >= per_colour)
            .count()
            >= colours
    }
}

/// The four technology colours. `UNITUPGRADE` and `NONE` are deliberately absent.
const COLOURS: [&str; 4] = ["BIOTIC", "CYBERNETIC", "PROPULSION", "WARFARE"];

/// Have `count` or more structures.
fn structure_count(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| position.structures().len() >= count
}

/// Have structures on `planets` planets outside your home system.
fn structures_away(planets: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        let home = position.home_system();
        position
            .structures()
            .into_iter()
            .filter(|(system, _)| Some(system.as_str()) != home.as_deref())
            .map(|(_, planet)| planet)
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            >= planets
    }
}

/// Have `ships` or more non-fighter ships in a single system.
///
/// One system, not a total: a fleet spread across the board is not an armada, which is the
/// whole point of the card.
fn fleet_in_one_system(ships: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        let types = ti4_content::units::catalogue(position.content, position.sources);
        position.state.board.values().any(|system| {
            system
                .units_of(position.player)
                .into_iter()
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.is_ship() && !kind.is_fighter())
                })
                .count()
                >= ships
        })
    }
}

/// Have units in `count` systems that contain no planets.
fn planetless_systems(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        let systems = ti4_content::galaxy::all_systems(position.content, position.sources);
        position
            .state
            .board
            .iter()
            .filter(|(_, board)| !board.units_of(position.player).is_empty())
            .filter(|(id, _)| {
                systems
                    .get(id.as_str())
                    .is_some_and(|system| system.planets().is_empty())
            })
            .count()
            >= count
    }
}

/// Control `count` planets that have an exploration attachment.
fn attached_planets(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        position
            .state
            .controlled_planets(position.player)
            .into_iter()
            .filter(|(_, planet)| {
                position
                    .state
                    .planet_attachments
                    .get(*planet)
                    .is_some_and(|attached| !attached.is_empty())
            })
            .count()
            >= count
    }
}

/// Have units in `count` systems holding a legendary planet, Mecatol Rex, or an anomaly.
///
/// "Notable" is the card's word for places worth contesting, and it is read from the corpus
/// rather than from a hand-written tile list — a list would go stale the moment the corpus does.
fn in_notable_systems(count: usize) -> impl Fn(&Position<'_>) -> bool {
    move |position| {
        let systems = ti4_content::galaxy::all_systems(position.content, position.sources);
        let planets = all_planets(position.content, position.sources);
        position
            .state
            .board
            .iter()
            .filter(|(_, board)| !board.units_of(position.player).is_empty())
            .filter(|(id, _)| {
                if id.as_str() == crate::seating::MECATOL {
                    return true;
                }
                let Some(system) = systems.get(id.as_str()) else {
                    return false;
                };
                system.is_anomaly()
                    || system.planets().iter().any(|planet| {
                        planets
                            .get(planet)
                            .is_some_and(ti4_content::galaxy::Planet::is_legendary)
                    })
            })
            .count()
            >= count
    }
}

/// The registered requirements, by objective alias.
///
/// Three tranches: planet control, technology and structures, and fleets/space. The oracle
/// registers 32; 22 are covered here, plus the eight bought ones. The rest stay unregistered and
/// therefore unscoreable, which is the designed behaviour for a coverage gap — see the module
/// documentation. [`unregistered_objectives`] reports which they are.
#[must_use]
#[allow(
    clippy::too_many_lines,
    reason = "one arm per objective: the list is the point, and splitting it hides the set"
)]
pub fn requirement_for(alias: &ObjectiveId) -> Option<Requirement> {
    // Written as a match rather than a lazy map so the set is visible at a glance and adding
    // one is a one-line change with no initialisation order to think about.
    fn expand_borders(p: &Position<'_>) -> bool {
        non_home(6)(p)
    }
    fn outer_rim(p: &Position<'_>) -> bool {
        on_the_rim(3)(p)
    }
    fn control_borderlands(p: &Position<'_>) -> bool {
        on_the_rim(5)(p)
    }
    fn subdue(p: &Position<'_>) -> bool {
        non_home(11)(p)
    }
    fn corner(p: &Position<'_>) -> bool {
        same_trait(4)(p)
    }
    fn unify_colonies(p: &Position<'_>) -> bool {
        same_trait(6)(p)
    }
    fn research_outposts(p: &Position<'_>) -> bool {
        tech_specialties(3)(p)
    }
    fn brain_trust(p: &Position<'_>) -> bool {
        tech_specialties(5)(p)
    }
    fn develop(p: &Position<'_>) -> bool {
        unit_upgrades(2)(p)
    }
    fn revolutionize(p: &Position<'_>) -> bool {
        unit_upgrades(3)(p)
    }
    fn diversify(p: &Position<'_>) -> bool {
        colours(2, 2)(p)
    }
    fn master_science(p: &Position<'_>) -> bool {
        colours(2, 4)(p)
    }
    fn build_defenses(p: &Position<'_>) -> bool {
        structure_count(4)(p)
    }
    fn massive_cities(p: &Position<'_>) -> bool {
        structure_count(7)(p)
    }
    fn infrastructure(p: &Position<'_>) -> bool {
        structures_away(3)(p)
    }
    fn protect_border(p: &Position<'_>) -> bool {
        structures_away(5)(p)
    }
    fn raise_fleet(p: &Position<'_>) -> bool {
        fleet_in_one_system(5)(p)
    }
    fn command_armada(p: &Position<'_>) -> bool {
        fleet_in_one_system(8)(p)
    }
    fn deep_space(p: &Position<'_>) -> bool {
        planetless_systems(3)(p)
    }
    fn vast_territories(p: &Position<'_>) -> bool {
        planetless_systems(5)(p)
    }
    fn ancient_monuments(p: &Position<'_>) -> bool {
        attached_planets(3)(p)
    }
    fn lost_outposts(p: &Position<'_>) -> bool {
        attached_planets(2)(p)
    }
    fn make_history(p: &Position<'_>) -> bool {
        in_notable_systems(2)(p)
    }
    fn become_legend(p: &Position<'_>) -> bool {
        in_notable_systems(4)(p)
    }

    match alias.as_str() {
        "conquer" => Some(conquer_the_weak),
        "intimidate" => Some(intimidate_council),
        "outer_rim" => Some(outer_rim),
        "control_borderlands" => Some(control_borderlands),
        "push_boundaries" => Some(push_boundaries),
        "distant_lands" => Some(rule_distant_lands),
        "engineer_marvel" => Some(engineer_a_marvel),
        "supremacy" => Some(achieve_supremacy),
        "expand_borders" => Some(expand_borders),
        "subdue" => Some(subdue),
        "corner" => Some(corner),
        "unify_colonies" => Some(unify_colonies),
        "research_outposts" => Some(research_outposts),
        "brain_trust" => Some(brain_trust),
        "develop" => Some(develop),
        "revolutionize" => Some(revolutionize),
        "diversify" => Some(diversify),
        "master_science" => Some(master_science),
        "build_defenses" => Some(build_defenses),
        "massive_cities" => Some(massive_cities),
        "infrastructure" => Some(infrastructure),
        "protect_border" => Some(protect_border),
        "raise_fleet" => Some(raise_fleet),
        "command_armada" => Some(command_armada),
        "deep_space" => Some(deep_space),
        "vast_territories" => Some(vast_territories),
        "ancient_monuments" => Some(ancient_monuments),
        "lost_outposts" => Some(lost_outposts),
        "make_history" => Some(make_history),
        "become_legend" => Some(become_legend),
        _ => None,
    }
}

/// Every alias registered so far. Sorted, for stable reporting.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "brain_trust",
        "build_defenses",
        "conquer",
        "control_borderlands",
        "distant_lands",
        "engineer_marvel",
        "intimidate",
        "outer_rim",
        "push_boundaries",
        "supremacy",
        "corner",
        "develop",
        "diversify",
        "expand_borders",
        "infrastructure",
        "lost_outposts",
        "make_history",
        "massive_cities",
        "master_science",
        "ancient_monuments",
        "become_legend",
        "command_armada",
        "deep_space",
        "protect_border",
        "raise_fleet",
        "research_outposts",
        "revolutionize",
        "subdue",
        "unify_colonies",
        "vast_territories",
    ]
}

/// Revealed objectives that no predicate covers, and so cannot currently be scored.
///
/// Exposed rather than hidden: this is the honest measure of how far scoring has been ported,
/// and a caller that wants to know why a game is not progressing should be able to ask.
#[must_use]
pub fn unregistered_objectives(state: &GameState) -> Vec<ObjectiveId> {
    state
        .revealed_objectives
        .iter()
        .filter(|alias| requirement_for(alias).is_none())
        .cloned()
        .collect()
}

/// 61.16: every planet in the player's home system must be theirs.
///
/// Players may have no faction, in which case there is no home system to lose and the
/// requirement is vacuously met — the oracle says the same, and notes it will start biting
/// once factions are set up.
#[must_use]
pub fn controls_home_system(position: &Position<'_>) -> bool {
    let Some(player) = position.state.player(position.player) else {
        return false;
    };
    // No faction record, or a faction with no listed homeworlds (the neutral placeholder),
    // means there is no home system to lose.
    let Some(faction) = ti4_content::factions::get(position.content, player.faction.as_str())
    else {
        return true;
    };
    let home_planets = faction.home_planets();
    if home_planets.is_empty() {
        return true;
    }
    let controlled = position.state.controlled_planets(position.player);
    home_planets
        .iter()
        .all(|planet| controlled.iter().any(|(_, held)| held.as_str() == *planet))
}

/// What an objective scored by spending costs (61.10).
///
/// These are the objectives you *buy* rather than achieve. They are affordable to **offer** and
/// paid to **take**, which is why the cost is a separate lookup from the requirement: a
/// predicate that spent as a side effect would charge a player for merely being asked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cost {
    /// Exhaust planets and spend trade goods for resources or influence.
    Spend {
        amount: i64,
        kind: crate::production::Spend,
    },
    /// Spend trade goods alone.
    TradeGoods(i32),
    /// Spend command tokens from any pools.
    Tokens(i32),
    /// Spend this much influence, this many resources **and** this many trade goods.
    ///
    /// All three, not any one of them, and the planets exhausted for resources cannot also pay
    /// the influence: a planet is exhausted once. Paying it twice is the mistake this variant
    /// exists to make impossible.
    AllThree(i64),
}

/// An objective's stage, derived from its printed points (61.13).
///
/// The corpus carries no stage field — a stage I is worth one point and a stage II two — so it
/// is read from the points rather than from a field that does not exist.
#[must_use]
pub fn stage_of(content: &ContentStore, alias: &ObjectiveId) -> Option<u8> {
    match points_for(content, alias)? {
        1 => Some(1),
        2 => Some(2),
        _ => None,
    }
}

/// Reveal the first facedown objective of a given stage (61.13, 61.14a).
///
/// **Not simply the top card.** The deck is stage I then stage II in order, so taking the top
/// would reveal the wrong stage whenever any stage I remains — and an agenda that names the
/// stage would then quietly do the opposite of what it says.
pub fn reveal_stage(
    state: &mut GameState,
    content: &ContentStore,
    stage: u8,
) -> Option<ObjectiveId> {
    let index = state
        .objective_deck
        .iter()
        .position(|alias| stage_of(content, alias) == Some(stage))?;
    let alias = state.objective_deck.remove(index);
    state.revealed_objectives.push(alias.clone());
    Some(alias)
}

/// Objectives bought rather than achieved (61.10).
#[must_use]
pub fn bought_aliases() -> Vec<&'static str> {
    vec![
        "amass_wealth",
        "centralize_trade",
        "galvanize",
        "golden_age",
        "lead",
        "manipulate_law",
        "monument",
        "sway_council",
        "trade_routes",
        "vast_reserves",
    ]
}

/// The price of an objective, if it is bought rather than achieved.
#[must_use]
pub fn cost_of(alias: &ObjectiveId) -> Option<Cost> {
    use crate::production::Spend;
    let cost = match alias.as_str() {
        "monument" => Cost::Spend {
            amount: 8,
            kind: Spend::Resources,
        },
        "golden_age" => Cost::Spend {
            amount: 16,
            kind: Spend::Resources,
        },
        "sway_council" => Cost::Spend {
            amount: 8,
            kind: Spend::Influence,
        },
        "manipulate_law" => Cost::Spend {
            amount: 16,
            kind: Spend::Influence,
        },
        "trade_routes" => Cost::TradeGoods(5),
        "centralize_trade" => Cost::TradeGoods(10),
        "lead" => Cost::Tokens(3),
        "galvanize" => Cost::Tokens(6),
        "amass_wealth" => Cost::AllThree(3),
        "vast_reserves" => Cost::AllThree(6),
        _ => return None,
    };
    Some(cost)
}

/// A disjoint pair of plans paying `amount` resources and `amount` influence, plus the trade
/// goods both plans and the printed cost need.
///
/// Returned rather than checked so affording and paying cannot disagree: paying re-plans against
/// the same state and takes the same first answer.
fn all_three_plan(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    amount: i64,
) -> Option<(crate::payment::Plan, crate::payment::Plan)> {
    use crate::production::Spend;
    let held = state.player(player)?.trade_goods;
    let resources =
        crate::payment::plans(state, content, sources, player, amount, Spend::Resources);
    let influence =
        crate::payment::plans(state, content, sources, player, amount, Spend::Influence);

    for paying_resources in &resources {
        for paying_influence in &influence {
            // A planet exhausted for resources is exhausted; it cannot also pay the influence.
            if paying_resources
                .planets
                .iter()
                .any(|planet| paying_influence.planets.contains(planet))
            {
                continue;
            }
            let goods = paying_resources.trade_goods
                + paying_influence.trade_goods
                + i32::try_from(amount).unwrap_or(i32::MAX);
            if goods <= held {
                return Some((paying_resources.clone(), paying_influence.clone()));
            }
        }
    }
    None
}

/// Whether this player could pay for a bought objective right now.
#[must_use]
pub fn can_afford(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    cost: Cost,
) -> bool {
    match cost {
        Cost::Spend { amount, kind } => {
            crate::payment::affordable(state, content, sources, player, amount, kind)
        }
        Cost::TradeGoods(amount) => state
            .player(player)
            .is_some_and(|seat| seat.trade_goods >= amount),
        Cost::AllThree(amount) => all_three_plan(state, content, sources, player, amount).is_some(),
        Cost::Tokens(amount) => state
            .player(player)
            .is_some_and(|seat| seat.total_tokens() >= amount),
    }
}

/// Pay for a bought objective. `false` without spending anything if it cannot be met.
///
/// Token costs are taken from the strategy pool first, then fleet, then tactic. The oracle
/// leaves the split to the player; taking a fixed order here is a simplification, and it is
/// recorded rather than hidden because a player who wanted to keep strategy tokens has had that
/// choice made for them.
pub fn pay_for(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    cost: Cost,
) -> bool {
    if !can_afford(state, content, sources, player, cost) {
        return false;
    }
    match cost {
        Cost::Spend { amount, kind } => {
            let Some(plan) = crate::payment::plans(state, content, sources, player, amount, kind)
                .into_iter()
                .next()
            else {
                return false;
            };
            crate::payment::apply(state, player, &plan)
        }
        Cost::TradeGoods(amount) => {
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods -= amount;
            }
            true
        }
        Cost::AllThree(amount) => {
            let Some((resources, influence)) =
                all_three_plan(state, content, sources, player, amount)
            else {
                return false;
            };
            // Both halves and the printed trade goods, or none of it: a half-paid objective
            // takes planets off the table and gives nothing back.
            if !crate::payment::apply(state, player, &resources) {
                return false;
            }
            if !crate::payment::apply(state, player, &influence) {
                return false;
            }
            if let Some(seat) = state.player_mut(player) {
                seat.trade_goods -= i32::try_from(amount).unwrap_or(i32::MAX);
            }
            true
        }
        Cost::Tokens(amount) => {
            let mut owed = amount;
            for pool in [
                ti4_model::state::TokenPool::Strategic,
                ti4_model::state::TokenPool::Fleet,
                ti4_model::state::TokenPool::Tactic,
            ] {
                if owed == 0 {
                    break;
                }
                let Some(seat) = state.player_mut(player) else {
                    return false;
                };
                let held = seat.tokens(pool);
                let take = held.min(owed);
                seat.gain_token(pool, -take);
                owed -= take;
            }
            owed == 0
        }
    }
}

/// Whether a revealed objective's requirement is met, whichever deck it came from.
///
/// Classified Document Leaks moves a *secret* objective into the public area, where anyone may
/// score it. Its requirement stays registered in `secrets`, so a public-only lookup would leave
/// the leaked objective sitting on the table worth nothing to anybody — which is the whole card.
fn satisfied(position: &Position<'_>, alias: &ObjectiveId) -> bool {
    if let Some(check) = requirement_for(alias) {
        return check(position);
    }
    let secret = ti4_model::id::SecretObjectiveId::new(alias.as_str());
    crate::secrets::requirement_for(&secret).is_some_and(|check| {
        check(&crate::secrets::Position {
            state: position.state,
            content: position.content,
            sources: position.sources,
            player: position.player,
            galaxy: position.galaxy,
        })
    })
}

/// Revealed public objectives this player could score right now.
///
/// # Example
///
/// ```
/// use ti4_content::ContentStore;
/// use ti4_model::content_types::POK;
/// use ti4_model::id::{ObjectiveId, PlayerId};
///
/// let players = [PlayerId::new("a"), PlayerId::new("b")];
/// let mut state =
///     ti4_engine::setup::start_game(ContentStore::embedded(), &players, POK, None).unwrap();
///
/// // Nothing is revealed, so nothing can be scored.
/// assert!(ti4_engine::objectives::scoreable(&state, ContentStore::embedded(), POK, &players[0])
///     .is_empty());
///
/// // Engineer a Marvel: have your flagship or a war sun on the board.
/// state.revealed_objectives.push(ObjectiveId::new("engineer_marvel"));
/// assert!(ti4_engine::objectives::scoreable(&state, ContentStore::embedded(), POK, &players[0])
///     .is_empty(), "revealed, but not yet met");
/// ```
pub fn scoreable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<ObjectiveId> {
    scoreable_on(state, content, sources, player, None)
}

/// Revealed public objectives this player could score right now, with the map available.
///
/// Objectives that ask about the shape of the board report unmet without it, so a driver holding
/// a galaxy should pass it — otherwise the same position scores differently depending on who
/// asked.
#[must_use]
pub fn scoreable_on(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
) -> Vec<ObjectiveId> {
    let mut position = Position::new(state, content, sources, player);
    position.galaxy = galaxy;
    if !controls_home_system(&position) {
        return Vec::new(); // 61.16
    }
    let already = state.scored_by(player);
    state
        .revealed_objectives
        .iter()
        .filter(|alias| !already.contains(*alias))
        .filter(|alias| {
            // 61.10: a bought objective is offered when it can be afforded. Its price is
            // checked here and charged in `award`, so being asked costs nothing.
            cost_of(alias).map_or_else(
                || satisfied(&position, alias),
                |cost| can_afford(state, content, sources, player, cost),
            )
        })
        .cloned()
        .collect()
}

/// What a revealed objective is worth, or `None` if the corpus does not know it.
#[must_use]
pub fn points_for(content: &ContentStore, alias: &ObjectiveId) -> Option<i32> {
    // Both decks: Classified Document Leaks moves a *secret* objective into the public area,
    // where anyone may score it, and a public-only lookup would silently value it at nothing.
    [ContentType::PublicObjectives, ContentType::SecretObjectives]
        .into_iter()
        .find_map(|category| content.get(category, alias.as_str()))
        .and_then(|record| record.int("points"))
        .and_then(|points| i32::try_from(points).ok())
}

/// An objective could not be scored.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScoreError {
    #[error("objective {0} is worth nothing the corpus knows about")]
    UnknownObjective(ObjectiveId),
    #[error("player {0} is not seated")]
    PlayerMissing(PlayerId),
    #[error("objective {0} could not be paid for")]
    Unaffordable(ObjectiveId),
}

/// Score an objective, capping victory points at the target (98.4a).
///
/// # Errors
/// [`ScoreError::UnknownObjective`] when the corpus has no points for it, and
/// [`ScoreError::PlayerMissing`] when the player has left the table.
pub fn award(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    alias: &ObjectiveId,
) -> Result<i32, ScoreError> {
    let points =
        points_for(content, alias).ok_or_else(|| ScoreError::UnknownObjective(alias.clone()))?;

    // 61.10: paid on taking, not on being offered. Nothing is scored if the price cannot be
    // met, so a bought objective can never be taken for free.
    if let Some(cost) = cost_of(alias)
        && !pay_for(state, content, sources, player, cost)
    {
        return Err(ScoreError::Unaffordable(alias.clone()));
    }

    state.record_score(player, alias.clone());
    {
        let seat = state
            .player_mut(player)
            .ok_or_else(|| ScoreError::PlayerMissing(player.clone()))?;
        // 98.4a: a player cannot hold more than the target.
        seat.victory_points = (seat.victory_points + points).min(VICTORY_TARGET);
    }
    // 51.7: leaders unlock the moment their condition is met, not at end of phase. A hero
    // unlocked by a third objective must not wait for a status phase the game may never reach.
    crate::leaders::check_unlocks(state, content, player);
    Ok(points)
}

/// 98.8, 61.15a: most victory points, ties broken by initiative order.
///
/// A game that ends with nobody having scored is still a tie, not a null result — everyone
/// level on zero is tied, so the first player in initiative order takes it.
#[must_use]
pub fn leader(state: &GameState) -> Option<PlayerId> {
    let best = state.players.iter().map(|p| p.victory_points).max()?;
    first_in_initiative(
        state,
        state
            .players
            .iter()
            .filter(|p| p.victory_points == best)
            .map(|p| p.id.clone()),
    )
}

/// A winner only exists once somebody reaches the target (98).
#[must_use]
pub fn winner(state: &GameState) -> Option<PlayerId> {
    first_in_initiative(
        state,
        state
            .players
            .iter()
            .filter(|p| p.victory_points >= VICTORY_TARGET)
            .map(|p| p.id.clone()),
    )
}

/// The earliest of `candidates` in initiative order; unseated players sort last.
fn first_in_initiative(
    state: &GameState,
    candidates: impl Iterator<Item = PlayerId>,
) -> Option<PlayerId> {
    let order = state.initiative_order();
    candidates.min_by_key(|id| {
        order
            .iter()
            .position(|seated| seated == id)
            .unwrap_or(usize::MAX)
    })
}

/// The choice kind for scoring an objective at status step 81.1.
pub const SCORE_KIND: &str = "score";

/// The ordered 81.1 window: in initiative order, each player may score one public objective.
///
/// Players with nothing scoreable are skipped rather than asked. The oracle offers them their
/// secret objectives instead; secrets are not implemented, so there is nothing to ask and a
/// forced "decline" would be a question with one answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScoringWindow {
    pending: Vec<PlayerId>,
    scored: Vec<(PlayerId, ObjectiveId)>,
    /// The map, when the driver has one.
    galaxy: Option<ti4_content::galaxy::Galaxy>,
}

impl ScoringWindow {
    /// Attach the map for the duration of the window.
    ///
    /// Owned rather than borrowed because the window outlives any one call and the driver holds
    /// the galaxy alongside it. Nothing places a tile during the status phase, so a snapshot
    /// taken when the window opens is the same map it closes on.
    #[must_use]
    pub fn with_galaxy(mut self, galaxy: ti4_content::galaxy::Galaxy) -> Self {
        self.galaxy = Some(galaxy);
        self
    }

    /// Open the window over `initiative`, which 81.1 requires be initiative order.
    #[must_use]
    pub fn new(initiative: &[PlayerId]) -> Self {
        let mut pending = initiative.to_vec();
        pending.reverse(); // so `pop` takes the earliest
        Self {
            pending,
            scored: Vec::new(),
            galaxy: None,
        }
    }

    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.pending.is_empty()
    }

    /// What was scored, in resolution order.
    #[must_use]
    pub fn scored(&self) -> &[(PlayerId, ObjectiveId)] {
        &self.scored
    }

    /// The next player with something to score, and their options.
    ///
    /// Looks past players with nothing scoreable rather than mutating, so this stays callable
    /// from an immutable inspection of the game.
    #[must_use]
    pub fn pending_choice(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<Choice> {
        let (_, player, available) = self.next_askable(state, content, sources)?;
        let mut options: Vec<ChoiceOption> = available
            .into_iter()
            .map(|alias| ChoiceOption::labelled(alias.as_str(), SCORE_KIND, alias.as_str()))
            .collect();
        options.push(ChoiceOption::decline());
        Some(Choice::new(player, "score an objective", options))
    }

    /// The first pending player who can score, with how many entries to drop to reach them.
    fn next_askable(
        &self,
        state: &GameState,
        content: &ContentStore,
        sources: SourceSet,
    ) -> Option<(usize, PlayerId, Vec<ObjectiveId>)> {
        for (offset, player) in self.pending.iter().rev().enumerate() {
            // 61.6: one public and one secret at most per status phase, and the oracle offers
            // both in the same window. A player with no public objective in reach may still
            // have a secret in reach, so this must not stop at the public list.
            let mut available = scoreable_on(state, content, sources, player, self.galaxy.as_ref());
            available.extend(
                crate::secrets::scoreable_on(state, content, sources, player, self.galaxy.as_ref())
                    .into_iter()
                    .map(|secret| ObjectiveId::new(secret.as_str())),
            );
            if !available.is_empty() {
                return Some((offset, player.clone(), available));
            }
        }
        None
    }

    /// Apply one player's decision, advancing past everyone skipped to reach them.
    ///
    /// # Errors
    /// [`ScoreError`] when the chosen objective cannot be awarded, and
    /// [`IllegalChoice`] via [`ScoringError`] when the answer was not offered.
    pub fn resolve(
        &mut self,
        state: &mut GameState,
        content: &ContentStore,
        sources: SourceSet,
        answer: ChoiceOption,
    ) -> Result<Option<ObjectiveId>, ScoringError> {
        let choice = self
            .pending_choice(state, content, sources)
            .ok_or(ScoringError::Complete)?;
        let option = validate(&choice, answer)?;
        let (offset, player, _) = self
            .next_askable(state, content, sources)
            .ok_or(ScoringError::Complete)?;

        // Everyone ahead of this player had nothing to score and is now past.
        let keep = self.pending.len() - offset - 1;
        self.pending.truncate(keep);

        if option.is_decline() {
            return Ok(None);
        }
        let alias = ObjectiveId::new(option.id);
        // A secret leaves its owner's hand when scored (61.18), which a public award does not
        // do — so which module owns the card decides which path it takes.
        let secret = ti4_model::id::SecretObjectiveId::new(alias.as_str());
        if crate::secrets::award(state, content, &player, &secret).is_none() {
            award(state, content, sources, &player, &alias)?;
        }
        if winner(state).is_some() {
            state.finished = true;
        }
        self.scored.push((player, alias.clone()));
        Ok(Some(alias))
    }
}

/// A failure while resolving the 81.1 window.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ScoringError {
    #[error("the scoring window is complete")]
    Complete,
    #[error(transparent)]
    Score(#[from] ScoreError),
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
}

#[cfg(test)]
mod tests {
    use ti4_model::content_types::POK;
    use ti4_model::id::{PlanetId, SystemId};

    use super::*;
    use crate::setup::start_game;

    fn game(players: &[PlayerId]) -> GameState {
        start_game(ContentStore::embedded(), players, POK, None).unwrap()
    }

    fn ids(names: &[&str]) -> Vec<PlayerId> {
        names.iter().map(|n| PlayerId::new(*n)).collect()
    }

    /// Control is recorded on the planet's own system, so a test cannot set it globally.
    fn give(state: &mut GameState, planet: &str, player: &str) {
        let catalogue = all_planets(ContentStore::embedded(), POK);
        let system = catalogue
            .get(planet)
            .and_then(ti4_content::galaxy::Planet::system_id)
            .unwrap_or("18");
        state
            .system_mut(&SystemId::new(system))
            .set_control(PlanetId::new(planet), PlayerId::new(player));
    }

    /// The ids of `count` planets in no faction's home system.
    fn non_home_planets(count: usize) -> Vec<String> {
        all_planets(ContentStore::embedded(), POK)
            .iter()
            .filter(|(_, planet)| {
                planet.homeworld_of().is_none() && !planet.is_placed_during_play()
            })
            .map(|(id, _)| (*id).to_owned())
            .take(count)
            .collect()
    }

    #[test]
    fn an_objective_with_no_registered_predicate_is_never_scoreable() {
        // The design the oracle documents: a coverage gap shows up as an objective nobody can
        // take, never as a bot winning on a rule that was never written.
        let players = ids(&["a"]);
        let mut state = game(&players);
        // Every printed objective is registered now, so this uses one that does not exist:
        // the point is the *design* — an unknown card is unscoreable, never freely scoreable.
        state
            .revealed_objectives
            .push(ObjectiveId::new("no_such_objective"));

        assert!(requirement_for(&ObjectiveId::new("no_such_objective")).is_none());
        assert!(scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty());
    }

    #[test]
    fn unregistered_revealed_objectives_are_reportable() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![
            ObjectiveId::new("expand_borders"),
            ObjectiveId::new("no_such_objective"),
        ];

        assert_eq!(
            unregistered_objectives(&state),
            vec![ObjectiveId::new("no_such_objective")]
        );
    }

    #[test]
    fn conquering_the_weak_needs_a_rivals_home_not_your_own() {
        let players = ids(&["a", "b"]);
        let mut state = game(&players);
        let (system, planet) = crate::fixtures::a_placed_planet();

        // b's home is recorded on the seat, which wins over their faction's record.
        state.player_mut(&PlayerId::new("b")).unwrap().home_planets = vec![planet.clone()];
        state.player_mut(&PlayerId::new("a")).unwrap().home_planets = vec![planet.clone()];

        let position = |state: &GameState| {
            conquer_the_weak(&Position::new(
                state,
                ContentStore::embedded(),
                POK,
                &PlayerId::new("a"),
            ))
        };
        assert!(!position(&state), "controlling nothing conquers nothing");

        state
            .system_mut(&system)
            .set_control(planet.clone(), PlayerId::new("a"));
        assert!(
            position(&state),
            "the planet is b's home as well, which is what the card asks for"
        );

        // And a planet that is only *your* home is not a conquest.
        state.player_mut(&PlayerId::new("b")).unwrap().home_planets = Vec::new();
        assert!(!position(&state));
    }

    #[test]
    fn a_marvel_is_a_flagship_or_a_war_sun_and_nothing_else() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        let (system, _) = crate::fixtures::a_placed_planet();
        let seat = PlayerId::new("a");
        let marvel = |state: &GameState| {
            engineer_a_marvel(&Position::new(state, ContentStore::embedded(), POK, &seat))
        };

        crate::fixtures::put(&mut state, &system, "dreadnought", &seat, 3);
        assert!(!marvel(&state), "three dreadnoughts are not a marvel");

        crate::fixtures::put(&mut state, &system, "warsun", &seat, 1);
        assert!(marvel(&state));
    }

    #[test]
    fn supremacy_needs_the_war_sun_where_it_hurts() {
        let players = ids(&["a", "b"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");
        let (elsewhere, _) = crate::fixtures::a_placed_planet();
        let home = SystemId::new("some_home_system");
        state.player_mut(&PlayerId::new("b")).unwrap().home_system = Some(home.clone());

        let supreme = |state: &GameState| {
            achieve_supremacy(&Position::new(state, ContentStore::embedded(), POK, &seat))
        };

        crate::fixtures::put(&mut state, &elsewhere, "warsun", &seat, 1);
        assert!(!supreme(&state), "a war sun at home is not supremacy");

        crate::fixtures::put(&mut state, &home, "warsun", &seat, 1);
        assert!(supreme(&state));
    }

    #[test]
    fn supremacy_counts_mecatol_too() {
        let players = ids(&["a", "b"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");
        crate::fixtures::put(
            &mut state,
            &SystemId::new(crate::seating::MECATOL),
            "flagship",
            &seat,
            1,
        );

        assert!(achieve_supremacy(&Position::new(
            &state,
            ContentStore::embedded(),
            POK,
            &seat
        )));
    }

    #[test]
    fn amassing_wealth_cannot_exhaust_one_planet_for_two_costs() {
        // The trap this cost exists for: a planet with 3 resources and 3 influence looks like it
        // pays both halves of Amass Wealth on its own. It pays one of them.
        let content = ContentStore::embedded();
        let players = ids(&["a"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");

        let dual = ti4_content::galaxy::all_planets(content, POK)
            .into_iter()
            .find(|(_, planet)| planet.resources() >= 3 && planet.influence() >= 3)
            .map(|(id, _)| PlanetId::new(id));
        let Some(dual) = dual else {
            return; // no such planet in this corpus
        };

        let (system, _) = crate::fixtures::a_placed_planet();
        state.system_mut(&system).set_control(dual, seat.clone());
        state.player_mut(&seat).unwrap().trade_goods = 3;

        assert!(
            !can_afford(&state, content, POK, &seat, Cost::AllThree(3)),
            "one planet cannot pay both the resources and the influence"
        );
    }

    #[test]
    fn amassing_wealth_spends_all_three_when_it_can() {
        let content = ContentStore::embedded();
        let players = ids(&["a"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");

        // Two planets each worth 3 or more of one thing, plus the trade goods.
        let mut rich: Vec<PlanetId> = ti4_content::galaxy::all_planets(content, POK)
            .into_iter()
            .filter(|(_, planet)| planet.resources() >= 3 || planet.influence() >= 3)
            .map(|(id, _)| PlanetId::new(id))
            .take(6)
            .collect();
        rich.sort();
        let (system, _) = crate::fixtures::a_placed_planet();
        for planet in &rich {
            state
                .system_mut(&system)
                .set_control(planet.clone(), seat.clone());
        }
        state.player_mut(&seat).unwrap().trade_goods = 10;

        if !can_afford(&state, content, POK, &seat, Cost::AllThree(3)) {
            return; // this corpus cannot make the position; the trap test above still holds
        }
        assert!(pay_for(&mut state, content, POK, &seat, Cost::AllThree(3)));

        let after = state.player(&seat).unwrap();
        assert!(
            after.trade_goods <= 7,
            "the three printed trade goods were spent: {} left",
            after.trade_goods
        );
        assert!(
            !state.exhausted_planets.is_empty(),
            "planets were exhausted to pay for it"
        );
    }

    #[test]
    fn every_bought_objective_has_a_price() {
        // A bought objective with no cost is free, and one with a cost but no registration is
        // unscoreable. Both are silent.
        for alias in bought_aliases() {
            assert!(
                cost_of(&ObjectiveId::new(alias)).is_some(),
                "{alias} is bought but has no price"
            );
            assert!(
                ContentStore::embedded()
                    .get(ContentType::PublicObjectives, alias)
                    .is_some(),
                "{alias} is not an objective the corpus knows"
            );
        }
    }

    /// A position with the map attached.
    fn on_map<'a>(
        state: &'a GameState,
        seat: &'a PlayerId,
        galaxy: &'a ti4_content::galaxy::Galaxy,
    ) -> Position<'a> {
        Position::new(state, ContentStore::embedded(), POK, seat).with_galaxy(galaxy)
    }

    #[test]
    fn a_map_shaped_objective_is_unmet_without_a_map() {
        // Not "true by default" and not a panic: the requirement reports unmet, so a driver
        // with no galaxy leaves it unscoreable instead of giving it away.
        let players = ids(&["a", "b"]);
        let state = game(&players);
        let seat = PlayerId::new("a");
        let position = Position::new(&state, ContentStore::embedded(), POK, &seat);

        assert!(!intimidate_council(&position));
        assert!(!push_boundaries(&position));
        assert!(!rule_distant_lands(&position));
        assert!(!on_the_rim(3)(&position));
    }

    #[test]
    fn the_edge_of_the_board_is_derived_from_the_tiles_that_are_there() {
        // A hub is a centre ringed by six systems: every ring system has an empty neighbour and
        // the centre does not. A hard-coded edge list would be right for one map and wrong here.
        let hub = crate::fixtures::plain_hub();
        let edge = edge_systems(&hub.galaxy);

        assert!(
            !edge.contains(&hub.centre),
            "the centre is enclosed by the ring"
        );
        for outer in &hub.outer {
            assert!(edge.contains(outer), "{outer} is on the rim");
        }
    }

    #[test]
    fn populating_the_outer_rim_does_not_count_your_home() {
        let hub = crate::fixtures::plain_hub();
        let players = ids(&["a"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");

        for outer in hub.outer.iter().take(3) {
            crate::fixtures::put(
                &mut state,
                &SystemId::new(outer.clone()),
                "cruiser",
                &seat,
                1,
            );
        }
        assert!(
            on_the_rim(3)(&on_map(&state, &seat, &hub.galaxy)),
            "three rim systems"
        );

        // Declaring one of them home takes it out of the count, leaving two.
        state.player_mut(&seat).unwrap().home_system = Some(SystemId::new(hub.outer[0].clone()));
        assert!(
            !on_the_rim(3)(&on_map(&state, &seat, &hub.galaxy)),
            "your own home does not populate the rim"
        );
    }

    #[test]
    fn intimidating_the_council_needs_two_systems_not_two_ships() {
        // A hub centred on Mecatol: the ring is exactly what is adjacent to it.
        let hub = crate::fixtures::hub_with_centre(crate::seating::MECATOL);
        let players = ids(&["a"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");

        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[0].clone()),
            "cruiser",
            &seat,
            5,
        );
        assert!(
            !intimidate_council(&on_map(&state, &seat, &hub.galaxy)),
            "five ships in one system is one system"
        );

        crate::fixtures::put(
            &mut state,
            &SystemId::new(hub.outer[1].clone()),
            "cruiser",
            &seat,
            1,
        );
        assert!(intimidate_council(&on_map(&state, &seat, &hub.galaxy)));
    }

    #[test]
    fn intimidating_the_council_ignores_ground_forces() {
        let hub = crate::fixtures::hub_with_centre(crate::seating::MECATOL);
        let players = ids(&["a"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");

        for outer in hub.outer.iter().take(2) {
            crate::fixtures::put(
                &mut state,
                &SystemId::new(outer.clone()),
                "infantry",
                &seat,
                1,
            );
        }
        assert!(
            !intimidate_council(&on_map(&state, &seat, &hub.galaxy)),
            "the card asks for ships"
        );
    }

    #[test]
    fn pushing_boundaries_needs_two_neighbours_beaten_not_one_beaten_twice() {
        let hub = crate::fixtures::plain_hub();
        let players = ids(&["a", "b", "c"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");
        let centre = SystemId::new(hub.centre.clone());

        // All three share the centre system, so all three are neighbours.
        for player in &players {
            crate::fixtures::put(&mut state, &centre, "cruiser", player, 1);
        }
        let planets: Vec<PlanetId> =
            ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
                .into_keys()
                .map(PlanetId::new)
                .take(4)
                .collect();
        let (system, _) = crate::fixtures::a_placed_planet();
        for planet in planets.iter().take(3) {
            state
                .system_mut(&system)
                .set_control(planet.clone(), seat.clone());
        }
        // b holds one, c holds three: only one neighbour is behind.
        state
            .system_mut(&system)
            .set_control(planets[3].clone(), PlayerId::new("b"));
        for planet in ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
            .into_keys()
            .map(PlanetId::new)
            .filter(|planet| !planets.contains(planet))
            .take(3)
        {
            state
                .system_mut(&system)
                .set_control(planet, PlayerId::new("c"));
        }

        assert!(
            !push_boundaries(&on_map(&state, &seat, &hub.galaxy)),
            "beating one neighbour is not beating two"
        );

        // Take c's planets away and both are behind.
        state
            .system_mut(&system)
            .planet_control
            .retain(|_, owner| owner == &seat || owner == &PlayerId::new("b"));
        assert!(push_boundaries(&on_map(&state, &seat, &hub.galaxy)));
    }

    #[test]
    fn distant_lands_must_be_two_different_opponents() {
        let hub = crate::fixtures::plain_hub();
        let players = ids(&["a", "b", "c"]);
        let mut state = game(&players);
        let seat = PlayerId::new("a");

        // b's home is the centre; both of a's planets sit in its ring, so both speak for b.
        state.player_mut(&PlayerId::new("b")).unwrap().home_system =
            Some(SystemId::new(hub.centre.clone()));
        state.player_mut(&PlayerId::new("c")).unwrap().home_system =
            Some(SystemId::new(hub.outer[3].clone()));

        let planets: Vec<PlanetId> =
            ti4_content::galaxy::all_planets(ContentStore::embedded(), POK)
                .into_keys()
                .map(PlanetId::new)
                .take(2)
                .collect();
        state
            .system_mut(&SystemId::new(hub.outer[0].clone()))
            .set_control(planets[0].clone(), seat.clone());
        state
            .system_mut(&SystemId::new(hub.outer[1].clone()))
            .set_control(planets[1].clone(), seat.clone());

        assert!(
            !rule_distant_lands(&on_map(&state, &seat, &hub.galaxy)),
            "two planets around one opponent's home are one distant land"
        );

        // c's home is outer[3]; a planet in it reaches a second opponent.
        state
            .system_mut(&SystemId::new(hub.outer[3].clone()))
            .set_control(planets[1].clone(), seat.clone());
        state
            .system_mut(&SystemId::new(hub.outer[1].clone()))
            .planet_control
            .clear();
        assert!(rule_distant_lands(&on_map(&state, &seat, &hub.galaxy)));
    }

    #[test]
    fn expand_borders_needs_six_non_home_planets() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("expand_borders")];

        let planets = non_home_planets(6);
        assert_eq!(planets.len(), 6, "the corpus should have six to give");

        for planet in &planets[..5] {
            give(&mut state, planet, "a");
        }
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "five non-home planets is not six"
        );

        give(&mut state, &planets[5], "a");
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("expand_borders")]
        );
    }

    /// Aliases of technologies carrying a given corpus type.
    fn technologies_of_type(kind: &str, count: usize) -> Vec<String> {
        ContentStore::embedded()
            .records(ContentType::Technologies)
            .iter()
            .filter(|record| record.strings("types").contains(&kind))
            .filter_map(|record| record.text("alias"))
            .map(ToOwned::to_owned)
            .take(count)
            .collect()
    }

    fn give_technologies(state: &mut GameState, player: &str, aliases: &[String]) {
        let seat = state.player_mut(&PlayerId::new(player)).unwrap();
        for alias in aliases {
            seat.technologies
                .insert(ti4_model::id::TechnologyId::new(alias.clone()));
        }
    }

    #[test]
    fn develop_counts_unit_upgrade_technologies() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("develop")];

        let upgrades = technologies_of_type("UNITUPGRADE", 2);
        assert_eq!(upgrades.len(), 2, "the corpus has unit upgrades");

        give_technologies(&mut state, "a", &upgrades[..1]);
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "one upgrade is not two"
        );

        give_technologies(&mut state, "a", &upgrades);
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("develop")]
        );
    }

    #[test]
    fn unit_upgrades_are_not_counted_as_a_colour() {
        // 90.7b: unit upgrades have no colour. Counting them as one would make Diversify
        // scoreable off a stack of upgrades that share no research track at all.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("diversify")];
        give_technologies(&mut state, "a", &technologies_of_type("UNITUPGRADE", 6));

        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "six upgrades are still no colours"
        );
    }

    #[test]
    fn diversify_needs_two_technologies_in_each_of_two_colours() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("diversify")];

        give_technologies(&mut state, "a", &technologies_of_type("BIOTIC", 2));
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "one colour is not two"
        );

        give_technologies(&mut state, "a", &technologies_of_type("WARFARE", 2));
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("diversify")]
        );
    }

    #[test]
    fn build_defenses_counts_structures_on_planets() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("build_defenses")];

        let planets = non_home_planets(4);
        for (index, planet) in planets.iter().enumerate() {
            let catalogue = all_planets(ContentStore::embedded(), POK);
            let system = catalogue
                .get(planet.as_str())
                .and_then(ti4_content::galaxy::Planet::system_id)
                .unwrap_or("18");
            state
                .system_mut(&SystemId::new(system))
                .planet_units
                .entry(PlanetId::new(planet.clone()))
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("spacedock"),
                    PlayerId::new("a"),
                ));
            if index == 2 {
                assert!(
                    scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a"))
                        .is_empty(),
                    "three structures is not four"
                );
            }
        }

        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("build_defenses")]
        );
    }

    #[test]
    fn a_ship_in_space_is_not_a_structure() {
        // Structures sit on planets. Counting hulls would make Build Defenses scoreable from
        // a fleet, which is the opposite of what the card asks for.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("build_defenses")];
        for _ in 0..8 {
            state
                .system_mut(&SystemId::new("18"))
                .units
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("dreadnought"),
                    PlayerId::new("a"),
                ));
        }

        assert!(scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty());
    }

    #[test]
    fn an_armada_must_be_in_one_system() {
        // A fleet spread across the board is not an armada, which is the whole point of the
        // card — so this counts per system rather than in total.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("raise_fleet")];
        let systems = crate::fixtures::plain_systems(2);

        // Five cruisers, split three and two: no single system has five.
        for (index, count) in [(0, 3), (1, 2)] {
            for _ in 0..count {
                state
                    .system_mut(&SystemId::new(systems[index].clone()))
                    .units
                    .push(ti4_model::units::Unit::new(
                        ti4_model::id::UnitTypeId::new("cruiser"),
                        PlayerId::new("a"),
                    ));
            }
        }
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "three plus two is not five in one place"
        );

        for _ in 0..2 {
            state
                .system_mut(&SystemId::new(systems[0].clone()))
                .units
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("cruiser"),
                    PlayerId::new("a"),
                ));
        }
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("raise_fleet")]
        );
    }

    #[test]
    fn fighters_do_not_make_an_armada() {
        // The card counts non-fighter ships.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("raise_fleet")];
        let system = SystemId::new(crate::fixtures::plain_systems(1)[0].clone());
        for _ in 0..9 {
            state
                .system_mut(&system)
                .units
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("fighter"),
                    PlayerId::new("a"),
                ));
        }

        assert!(scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty());
    }

    #[test]
    fn deep_space_counts_systems_without_planets() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("deep_space")];

        let empty: Vec<String> = ti4_content::galaxy::all_systems(ContentStore::embedded(), POK)
            .iter()
            .filter(|(_, system)| system.planets().is_empty() && !system.is_hyperlane())
            .map(|(id, _)| (*id).to_owned())
            .take(3)
            .collect();
        if empty.len() < 3 {
            return;
        }

        for id in &empty[..2] {
            state
                .system_mut(&SystemId::new(id.clone()))
                .units
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("cruiser"),
                    PlayerId::new("a"),
                ));
        }
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "two is not three"
        );

        state
            .system_mut(&SystemId::new(empty[2].clone()))
            .units
            .push(ti4_model::units::Unit::new(
                ti4_model::id::UnitTypeId::new("cruiser"),
                PlayerId::new("a"),
            ));
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("deep_space")]
        );
    }

    #[test]
    fn ancient_monuments_needs_planets_with_attachments() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("ancient_monuments")];
        let planets = non_home_planets(3);
        for planet in &planets {
            give(&mut state, planet, "a");
        }
        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "controlling them is not enough without attachments"
        );

        for planet in &planets {
            state
                .planet_attachments
                .entry(PlanetId::new(planet.clone()))
                .or_default()
                .push("some_attachment".to_owned());
        }
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("ancient_monuments")]
        );
    }

    #[test]
    fn the_scoring_window_offers_a_secret_too() {
        // 61.6 lets a player score one public and one secret. A window that only looked at
        // public objectives left a satisfied secret unscoreable all game.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives.clear();
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("eap")];

        // Four PDS satisfies it.
        let (system, planet) = crate::fixtures::a_placed_planet();
        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    PlayerId::new("a"),
                ));
        }

        let window = ScoringWindow::new(&[PlayerId::new("a")]);
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .expect("the secret is offered");
        assert!(choice.ids().contains(&"eap"));
    }

    #[test]
    fn scoring_a_secret_takes_it_out_of_hand() {
        // A secret leaves its owner's hand when scored (61.18); a public objective does not.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives.clear();
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .secret_objectives = vec![ti4_model::id::SecretObjectiveId::new("eap")];
        let (system, planet) = crate::fixtures::a_placed_planet();
        for _ in 0..4 {
            state
                .system_mut(&system)
                .planet_units
                .entry(planet.clone())
                .or_default()
                .push(ti4_model::units::Unit::new(
                    ti4_model::id::UnitTypeId::new("pds"),
                    PlayerId::new("a"),
                ));
        }

        let mut window = ScoringWindow::new(&[PlayerId::new("a")]);
        let choice = window
            .pending_choice(&state, ContentStore::embedded(), POK)
            .unwrap();
        let pick = choice.option("eap").unwrap().clone();
        window
            .resolve(&mut state, ContentStore::embedded(), POK, pick)
            .unwrap();

        assert!(
            state
                .player(&PlayerId::new("a"))
                .unwrap()
                .secret_objectives
                .is_empty(),
            "it left the hand"
        );
        assert!(state.player(&PlayerId::new("a")).unwrap().victory_points > 0);
    }

    #[test]
    fn revealing_a_stage_skips_past_the_wrong_stage() {
        // The deck is stage I then stage II in order, so taking the top card reveals the wrong
        // stage while any stage I remains. An agenda naming the stage would then do the
        // opposite of what it says.
        let players = ids(&["a"]);
        let mut state = game(&players);

        let by_stage = |stage: u8| -> Vec<ObjectiveId> {
            ContentStore::embedded()
                .records(ContentType::PublicObjectives)
                .iter()
                .filter_map(|record| record.text("alias"))
                .map(ObjectiveId::new)
                .filter(|alias| stage_of(ContentStore::embedded(), alias) == Some(stage))
                .take(2)
                .collect()
        };
        let stage_one = by_stage(1);
        let stage_two = by_stage(2);
        assert!(!stage_one.is_empty() && !stage_two.is_empty());

        // Stage I sits on top, as in a real deck.
        state.objective_deck = stage_one.iter().chain(&stage_two).cloned().collect();
        state.revealed_objectives.clear();

        let revealed = reveal_stage(&mut state, ContentStore::embedded(), 2).unwrap();

        assert_eq!(
            stage_of(ContentStore::embedded(), &revealed),
            Some(2),
            "it reached past the stage I cards"
        );
        assert!(
            state.objective_deck.contains(&stage_one[0]),
            "and left them in the deck"
        );
    }

    #[test]
    fn a_bought_objective_is_offered_when_affordable_and_charged_when_taken() {
        // 61.10. Being asked costs nothing; taking it charges. A predicate that spent as a
        // side effect would bill a player for merely being offered the card.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("trade_routes")];

        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "no trade goods, so it is not offered"
        );

        state.player_mut(&PlayerId::new("a")).unwrap().trade_goods = 5;
        assert_eq!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")),
            vec![ObjectiveId::new("trade_routes")]
        );
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            5,
            "being offered spent nothing"
        );

        award(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
            &ObjectiveId::new("trade_routes"),
        )
        .unwrap();
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().trade_goods,
            0,
            "taking it charged the five"
        );
    }

    #[test]
    fn an_unaffordable_purchase_scores_nothing() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("centralize_trade")];
        let before = state.clone();

        assert_eq!(
            award(
                &mut state,
                ContentStore::embedded(),
                POK,
                &PlayerId::new("a"),
                &ObjectiveId::new("centralize_trade")
            ),
            Err(ScoreError::Unaffordable(ObjectiveId::new(
                "centralize_trade"
            )))
        );
        assert!(state.identical(&before), "nothing was spent or scored");
    }

    #[test]
    fn a_token_purchase_spends_command_tokens() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("lead")];
        let before = state.player(&PlayerId::new("a")).unwrap().total_tokens();
        assert!(before >= 3, "a fresh player has tokens");

        award(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
            &ObjectiveId::new("lead"),
        )
        .unwrap();

        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().total_tokens(),
            before - 3
        );
    }

    #[test]
    fn every_bought_objective_is_a_real_card() {
        for alias in [
            "monument",
            "golden_age",
            "sway_council",
            "manipulate_law",
            "trade_routes",
            "centralize_trade",
            "lead",
            "galvanize",
        ] {
            let alias = ObjectiveId::new(alias);
            assert!(cost_of(&alias).is_some());
            assert!(
                points_for(ContentStore::embedded(), &alias).is_some(),
                "{alias} is not an objective the corpus knows"
            );
        }
    }

    #[test]
    fn an_already_scored_objective_is_not_offered_again() {
        // 61.8: each objective scores once per game, per player.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("expand_borders")];
        for planet in non_home_planets(6) {
            give(&mut state, &planet, "a");
        }
        assert!(!scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty());

        state.record_score(&PlayerId::new("a"), ObjectiveId::new("expand_borders"));

        assert!(scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty());
    }

    #[test]
    fn awarding_adds_the_cards_points_and_records_it() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        let alias = ObjectiveId::new("expand_borders");

        let points = award(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
            &alias,
        )
        .unwrap();

        assert!(points > 0, "a stage I objective is worth something");
        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().victory_points,
            points
        );
        assert!(state.scored_by(&PlayerId::new("a")).contains(&alias));
    }

    #[test]
    fn scoring_a_third_objective_unlocks_a_hero_at_once() {
        // 51.7: leaders unlock the moment their condition is met. A hero waiting for the end
        // of the phase might wait for a status phase the game never reaches.
        let players = ids(&["a"]);
        let mut state = game(&players);
        let faction = ti4_content::factions::catalogue(ContentStore::embedded(), POK)
            .iter()
            .find(|(alias, _)| {
                crate::leaders::for_faction(ContentStore::embedded(), POK, alias)
                    .iter()
                    .any(|leader| {
                        crate::leaders::kind_of(ContentStore::embedded(), leader).as_deref()
                            == Some(crate::leaders::HERO)
                    })
            })
            .map(|(alias, _)| (*alias).to_owned());
        let Some(faction) = faction else { return };
        state.player_mut(&PlayerId::new("a")).unwrap().faction =
            ti4_model::id::FactionId::new(faction);
        crate::leaders::deploy(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
        );
        let hero = crate::leaders::of_kind(
            &state,
            ContentStore::embedded(),
            &PlayerId::new("a"),
            crate::leaders::HERO,
        )
        .first()
        .cloned()
        .unwrap();

        state.record_score(&PlayerId::new("a"), ObjectiveId::new("o1"));
        state.record_score(&PlayerId::new("a"), ObjectiveId::new("o2"));
        assert_eq!(
            crate::leaders::status(&state, &PlayerId::new("a"), &hero),
            Some(ti4_model::state::LeaderStatus::Locked)
        );

        award(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
            &ObjectiveId::new("expand_borders"),
        )
        .unwrap();

        assert_eq!(
            crate::leaders::status(&state, &PlayerId::new("a"), &hero),
            Some(ti4_model::state::LeaderStatus::Unlocked),
            "the third objective unlocked it there and then"
        );
    }

    #[test]
    fn victory_points_are_capped_at_the_target() {
        // 98.4a. Without the cap a final objective could push a player past ten and any
        // check written as `== VICTORY_TARGET` would miss the win entirely.
        let players = ids(&["a"]);
        let mut state = game(&players);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .victory_points = VICTORY_TARGET - 1;

        award(
            &mut state,
            ContentStore::embedded(),
            POK,
            &PlayerId::new("a"),
            &ObjectiveId::new("expand_borders"),
        )
        .unwrap();

        assert_eq!(
            state.player(&PlayerId::new("a")).unwrap().victory_points,
            VICTORY_TARGET
        );
    }

    #[test]
    fn an_unknown_objective_scores_nothing_and_is_refused() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        let before = state.clone();
        let alias = ObjectiveId::new("not_an_objective");

        assert_eq!(
            award(
                &mut state,
                ContentStore::embedded(),
                POK,
                &PlayerId::new("a"),
                &alias
            ),
            Err(ScoreError::UnknownObjective(alias))
        );
        assert!(state.identical(&before));
    }

    #[test]
    fn there_is_no_winner_until_somebody_reaches_the_target() {
        let players = ids(&["a", "b"]);
        let mut state = game(&players);
        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .victory_points = VICTORY_TARGET - 1;

        assert_eq!(winner(&state), None);

        state
            .player_mut(&PlayerId::new("a"))
            .unwrap()
            .victory_points = VICTORY_TARGET;
        assert_eq!(winner(&state), Some(PlayerId::new("a")));
    }

    #[test]
    fn the_leader_of_a_scoreless_game_is_the_first_in_initiative() {
        // Everyone level on zero is tied, not a null result.
        let players = ids(&["a", "b", "c"]);
        let state = game(&players);

        let expected = state.initiative_order().first().cloned();
        assert_eq!(leader(&state), expected);
    }

    #[test]
    fn ties_break_by_initiative_not_by_seating() {
        let players = ids(&["a", "b"]);
        let mut state = game(&players);
        // b takes Leadership (initiative 1); a takes Imperial (8). Seating still says a first.
        state.deal_strategy_card(
            &PlayerId::new("b"),
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        state.deal_strategy_card(
            &PlayerId::new("a"),
            ti4_model::id::StrategyCardId::new("pok8imperial"),
        );
        for id in ["a", "b"] {
            state.player_mut(&PlayerId::new(id)).unwrap().victory_points = 4;
        }

        assert_eq!(leader(&state), Some(PlayerId::new("b")));
    }

    #[test]
    fn a_winner_is_also_chosen_by_initiative_when_two_arrive_together() {
        let players = ids(&["a", "b"]);
        let mut state = game(&players);
        state.deal_strategy_card(
            &PlayerId::new("b"),
            ti4_model::id::StrategyCardId::new("pok1leadership"),
        );
        for id in ["a", "b"] {
            state.player_mut(&PlayerId::new(id)).unwrap().victory_points = VICTORY_TARGET;
        }

        assert_eq!(winner(&state), Some(PlayerId::new("b")));
    }

    #[test]
    fn a_faction_with_no_listed_homeworlds_vacuously_controls_it() {
        let players = ids(&["a"]);
        let state = game(&players);
        let player = PlayerId::new("a");
        let position = Position::new(&state, ContentStore::embedded(), POK, &player);

        assert!(controls_home_system(&position));
    }

    #[test]
    fn every_registered_alias_resolves_to_a_predicate() {
        for alias in registered_aliases() {
            assert!(
                requirement_for(&ObjectiveId::new(alias)).is_some(),
                "{alias} is listed but not registered"
            );
        }
    }

    #[test]
    fn every_registered_alias_is_a_real_objective_in_the_corpus() {
        // A predicate registered against a misspelled alias would never fire, and nothing
        // else would ever say so.
        for alias in registered_aliases() {
            assert!(
                points_for(ContentStore::embedded(), &ObjectiveId::new(alias)).is_some(),
                "{alias} is not an objective the corpus knows"
            );
        }
    }

    #[test]
    fn scoring_ignores_planets_the_corpus_does_not_know() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![ObjectiveId::new("expand_borders")];
        for index in 0..8 {
            state.system_mut(&SystemId::new("18")).set_control(
                PlanetId::new(format!("invented_{index}")),
                PlayerId::new("a"),
            );
        }

        assert!(
            scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty(),
            "invented planets must not satisfy a requirement"
        );
    }
}
