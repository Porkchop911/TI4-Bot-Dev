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

    /// This player's home system, if their faction names one.
    fn home_system(&self) -> Option<String> {
        let seat = self.state.player(self.player)?;
        ti4_content::factions::get(self.content, seat.faction.as_str())
            .and_then(|faction| faction.home_system())
            .map(ToOwned::to_owned)
    }
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

/// The registered requirements, by objective alias.
///
/// Three tranches: planet control, technology and structures, and fleets/space. The oracle
/// registers 32; 19 are covered here. The rest stay unregistered and
/// therefore unscoreable, which is the designed behaviour for a coverage gap — see the module
/// documentation. [`unregistered_objectives`] reports which they are.
#[must_use]
pub fn requirement_for(alias: &ObjectiveId) -> Option<Requirement> {
    // Written as a match rather than a lazy map so the set is visible at a glance and adding
    // one is a one-line change with no initialisation order to think about.
    fn expand_borders(p: &Position<'_>) -> bool {
        non_home(6)(p)
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

    match alias.as_str() {
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
        _ => None,
    }
}

/// Every alias registered so far. Sorted, for stable reporting.
#[must_use]
pub fn registered_aliases() -> Vec<&'static str> {
    vec![
        "brain_trust",
        "build_defenses",
        "corner",
        "develop",
        "diversify",
        "expand_borders",
        "infrastructure",
        "massive_cities",
        "master_science",
        "ancient_monuments",
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
}

/// Objectives bought rather than achieved (61.10).
#[must_use]
pub fn bought_aliases() -> Vec<&'static str> {
    vec![
        "centralize_trade",
        "galvanize",
        "golden_age",
        "lead",
        "manipulate_law",
        "monument",
        "sway_council",
        "trade_routes",
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
        _ => return None,
    };
    Some(cost)
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

/// Revealed public objectives this player could score right now.
#[must_use]
pub fn scoreable(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<ObjectiveId> {
    let position = Position::new(state, content, sources, player);
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
                || requirement_for(alias).is_some_and(|check| check(&position)),
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
    let seat = state
        .player_mut(player)
        .ok_or_else(|| ScoreError::PlayerMissing(player.clone()))?;
    // 98.4a: a player cannot hold more than the target.
    seat.victory_points = (seat.victory_points + points).min(VICTORY_TARGET);
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
}

impl ScoringWindow {
    /// Open the window over `initiative`, which 81.1 requires be initiative order.
    #[must_use]
    pub fn new(initiative: &[PlayerId]) -> Self {
        let mut pending = initiative.to_vec();
        pending.reverse(); // so `pop` takes the earliest
        Self {
            pending,
            scored: Vec::new(),
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
            let available = scoreable(state, content, sources, player);
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
        award(state, content, sources, &player, &alias)?;
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
        state
            .revealed_objectives
            .push(ObjectiveId::new("make_history"));

        assert!(requirement_for(&ObjectiveId::new("make_history")).is_none());
        assert!(scoreable(&state, ContentStore::embedded(), POK, &PlayerId::new("a")).is_empty());
    }

    #[test]
    fn unregistered_revealed_objectives_are_reportable() {
        let players = ids(&["a"]);
        let mut state = game(&players);
        state.revealed_objectives = vec![
            ObjectiveId::new("expand_borders"),
            ObjectiveId::new("make_history"),
        ];

        assert_eq!(
            unregistered_objectives(&state),
            vec![ObjectiveId::new("make_history")]
        );
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
