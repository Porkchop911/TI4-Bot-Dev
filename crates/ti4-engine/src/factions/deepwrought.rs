//! The Deepwrought Scholarate (`deepwrought`, alias in `factions.json`). See
//! `plans/evidence/BF-deepwrought.md`.
//!
//! Split for size: this file holds the ocean-card model, Research Team, Oceanbound, Hydrothermal
//! Mining, Radical Advancement, the D.W.S. Luminous movement hook, the Eanautic and Aello's unlock;
//! `deepwrought_cards.rs` holds Doctor Carrina's placement, Share Knowledge, Visionaria Select and
//! Ta Zern.
//!
//! Card texts (latest printing, `crates/ti4-content/content/*.json`):
//!
//! * Research Team: "When ground forces are committed: if your units on that planet are not already
//!   coexisting, you may choose for your units to coexist."
//! * Oceanbound: "Any time you have more ocean cards than there are planets that contain your
//!   coexisting units, discard ocean cards until you do not." and "When your units begin
//!   coexisting on a planet: gain an ocean card and ready it." There are only 5 ocean cards.
//! * Hydrothermal Mining: "At the start of the status phase, gain 1 trade good for each ocean card
//!   in play."
//! * Radical Advancement: "At the start of the status phase, you may replace one of your non-unit
//!   upgrade technologies with a technology of the same color that has exactly 1 more
//!   prerequisite."
//! * D.W.S. Luminous: "This ship can move through systems that contain your units, even if other
//!   players' units are present; if it would, apply +1 to its move value for each of those
//!   systems." (Other players' units do not need to be present for the boost to apply.)
//! * Eanautic: "When another player activates this system, if this unit is coexisting, you may move
//!   it and any of your infantry on its planet to a planet you control in your home system."
//! * Aello (`deepwroughtcommander`): unlock "Have an ocean card in play." The research discount is
//!   `strategy_cards::deepwrought_commander`, reached by every Alliance/Yin/Mahact/Nekro route
//!   through `promissory::has_commander_ability`.
//!
//! # Ocean cards, modelled once
//!
//! The corpus prints the five oceans as planet records with no tile (`ocean1`..`ocean5`, 1
//! resource and 1 influence, "FAKE" and "FACTION" types). They are not on the map, so they are not
//! `placed_planets`: a card is a row `deepwrought:ocean:<id>` in `GameState::faction_marks` whose
//! value is its holder. A card's exhaustion is the planet-keyed `exhausted_planets` the rest of the
//! engine already reads, and the cards are spendable for resources and influence through
//! [`spendable_oceans`] (called by `production::spendable_planets`). Gaining one readies it.
//!
//! **Oceanbound's cap is derived, not stored.** [`oceans`] is the stored list cut to the number of
//! planets that contain the holder's coexisting units ([`coexisting_planets`]), lowest id first, so
//! "any time" holds at every read; [`gain_ocean`] also deletes the stale rows. Which ocean is
//! gained or discarded is not a decision (all five are 1/1 with no text): the lowest free id is
//! gained, the highest is lost.
//!
//! Coexistence itself is `crate::coexistence`; [`begin_coexisting`] is the one door Deepwrought
//! effects use, so every "begins coexisting" reaches Oceanbound.

use std::sync::Arc;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{LeaderId, PlanetId, PlayerId, SystemId, TechnologyId};
use ti4_model::state::{GameState, LeaderStatus};
use ti4_model::units::Unit;

use super::hooks_movement::{MovementHooks, PassSite};
use super::{FactionModule, Hooks};
use crate::choice::{Choice, ChoiceOption};
use crate::decision_context::{DecisionContext, DecisionSource, DecisionTarget};
use crate::timing::{Ability, Frequency, Relation, TimingContext, TimingError};

/// The faction alias; also the faction name in promissory note ids.
pub const FACTION: &str = "deepwrought";

const RESEARCH_TEAM: &str = "researchteam";
const OCEANBOUND: &str = "oceanbound";
const HYDROTHERMAL: &str = "hydrothermal";
const RADICAL: &str = "radical";
/// D.W.S. Luminous.
pub const FLAGSHIP: &str = "deepwrought_flagship";
/// Eanautic.
pub const MECH: &str = "deepwrought_mech";
/// Doctor Carrina.
pub const AGENT: &str = "deepwroughtagent";
/// Aello.
pub const COMMANDER: &str = "deepwroughtcommander";
/// Ta Zern.
pub const HERO: &str = "deepwroughthero";
/// Visionaria Select.
pub const BREAKTHROUGH: &str = "deepwroughtbt";
/// Share Knowledge.
pub const SHARE_KNOWLEDGE: &str = "shareknowledge";

/// The five ocean cards, in the order they are gained.
pub const OCEANS: [&str; 5] = ["ocean1", "ocean2", "ocean3", "ocean4", "ocean5"];

const OCEAN_PREFIX: &str = "deepwrought:ocean:";
const CARRINA_PREFIX: &str = "deepwrought:carrina:";
const PURGED_PREFIX: &str = "deepwrought:purged_tech:";

/// What this faction implements.
pub const MODULE: FactionModule = FactionModule {
    alias: FACTION,
    abilities: &[RESEARCH_TEAM, OCEANBOUND],
    technologies: &[HYDROTHERMAL, RADICAL],
    units: &[FLAGSHIP, MECH],
    promissory: &[SHARE_KNOWLEDGE],
    leaders: &[AGENT, COMMANDER, HERO],
    breakthroughs: &[BREAKTHROUGH],
    hooks: Hooks {
        timing_abilities: Some(timing_abilities),
        waived_prerequisites: Some(waived_prerequisites),
        commander_unlocked: Some(commander_unlocked),
        component_actions: Some(super::deepwrought_cards::component_actions),
        perform_component: Some(super::deepwrought_cards::perform_component),
        leader_action: Some(super::deepwrought_cards::leader_action),
        use_leader: Some(super::deepwrought_cards::use_leader),
        movement: MovementHooks {
            own_unit_passage: Some(own_unit_passage),
            ..MovementHooks::NONE
        },
        ..Hooks::NONE
    },
};

/// Whether `player` plays the Deepwrought Scholarate.
#[must_use]
pub fn is_deepwrought(state: &GameState, player: &PlayerId) -> bool {
    state
        .player(player)
        .is_some_and(|seat| seat.faction.as_str() == FACTION)
}

// -- the one place the faction asks ---------------------------------------------------------------

pub(crate) fn decision(
    state: &GameState,
    who: &PlayerId,
    card: &str,
    subtype: &str,
) -> DecisionContext {
    DecisionContext::new(
        who.clone(),
        DecisionSource::FactionAbility(card.to_owned()),
        subtype,
        state.phase,
        state.round,
    )
}

/// Put one question to `who`. Every Deepwrought choice made inside a timing window or a component
/// action is asked here (the research-window questions live in `strategy_cards.rs`, the commit
/// question in `invasion.rs`, each beside the flow that owns it).
pub(crate) fn ask(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    card: &str,
    subtype: &str,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options).contextualized(decision(
        context.state,
        who,
        card,
        subtype,
    ));
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

/// A question about one planet, so a viewer can point at it.
pub(crate) fn ask_about(
    context: &mut TimingContext<'_>,
    who: &PlayerId,
    prompt: String,
    card: &str,
    subtype: &str,
    target: DecisionTarget,
    options: Vec<ChoiceOption>,
) -> Result<ChoiceOption, TimingError> {
    let choice = Choice::new(who.clone(), prompt, options)
        .contextualized(decision(context.state, who, card, subtype).about(target));
    context
        .ask_seeing(&choice)
        .map_err(TimingError::IllegalChoice)
}

// -- ocean cards ----------------------------------------------------------------------------------

fn ocean_key(planet: &str) -> String {
    format!("{OCEAN_PREFIX}{planet}")
}

/// Ocean cards `player` holds in the state, lowest id first, before the Oceanbound cap.
fn stored_oceans(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    let holder = player.to_string();
    state
        .faction_marks
        .range(OCEAN_PREFIX.to_owned()..)
        .take_while(|(key, _)| key.starts_with(OCEAN_PREFIX))
        .filter(|(_, value)| **value == holder)
        .map(|(key, _)| PlanetId::new(&key[OCEAN_PREFIX.len()..]))
        .collect()
}

/// How many planets contain `player`'s coexisting units: the player is coexisting there and still
/// has a unit on it.
#[must_use]
pub fn coexisting_planets(state: &GameState, player: &PlayerId) -> usize {
    state
        .board
        .values()
        .map(|board| {
            board
                .coexisting
                .iter()
                .filter(|(planet, who)| {
                    who.contains(player) && !board.on_planet_of(planet, player).is_empty()
                })
                .count()
        })
        .sum()
}

/// The ocean cards `player` has in play: what they hold, cut to Oceanbound's cap.
#[must_use]
pub fn oceans(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    let mut held = stored_oceans(state, player);
    held.truncate(coexisting_planets(state, player));
    held
}

/// Ocean cards in play across the table ("each ocean card in play").
#[must_use]
pub fn oceans_in_play(state: &GameState) -> usize {
    state
        .players
        .iter()
        .map(|seat| oceans(state, &seat.id).len())
        .sum()
}

/// Oceanbound's discard, made real: delete every row beyond the cap.
fn prune_oceans(state: &mut GameState, player: &PlayerId) {
    let keep = coexisting_planets(state, player);
    for planet in stored_oceans(state, player).into_iter().skip(keep) {
        state.faction_marks.remove(&ocean_key(planet.as_str()));
        state.exhausted_planets.remove(&planet);
    }
}

/// Gain the lowest free ocean card, readied. `None` when all five are taken or the cap would
/// discard it at once.
pub fn gain_ocean(state: &mut GameState, player: &PlayerId) -> Option<PlanetId> {
    prune_oceans(state, player);
    if coexisting_planets(state, player) <= stored_oceans(state, player).len() {
        return None;
    }
    let next = OCEANS
        .iter()
        .find(|id| !state.faction_marks.contains_key(&ocean_key(id)))?;
    state
        .faction_marks
        .insert(ocean_key(next), player.to_string());
    let planet = PlanetId::new(*next);
    state.exhausted_planets.remove(&planet);
    Some(planet)
}

/// The holder's readied ocean cards, spendable like any planet (called by
/// `production::spendable_planets`). Empty for every other seat.
#[must_use]
pub fn spendable_oceans(state: &GameState, player: &PlayerId) -> Vec<PlanetId> {
    oceans(state, player)
        .into_iter()
        .filter(|planet| !state.exhausted_planets.contains(planet))
        .collect()
}

/// Oceanbound: "When your units begin coexisting on a planet: gain an ocean card and ready it."
/// For a site that records coexistence itself (Crash Landing). A no-op for every other faction.
pub fn note_coexistence_began(state: &mut GameState, player: &PlayerId) {
    if is_deepwrought(state, player) {
        gain_ocean(state, player);
    }
}

/// Put `player` into coexistence on a planet someone else holds (coexistence 3.1) and run
/// Oceanbound when their units began coexisting there. The units must already be on the planet.
/// Returns whether coexistence began (it had not already).
///
/// # Errors
/// [`crate::coexistence::CoexistError`] exactly as `coexistence::begin`; nothing has changed.
pub fn begin_coexisting(
    state: &mut GameState,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
    taker: Option<&PlayerId>,
) -> Result<bool, crate::coexistence::CoexistError> {
    let was = crate::coexistence::is_coexisting(state, system, planet, player);
    crate::coexistence::begin(state, system, planet, player, taker)?;
    if !was {
        note_coexistence_began(state, player);
    }
    Ok(!was)
}

// -- Research Team ---------------------------------------------------------------------------------

/// Whether `player` has the Research Team ability.
#[must_use]
pub fn has_research_team(state: &GameState, content: &ContentStore, player: &PlayerId) -> bool {
    crate::faction_abilities::has(state, content, player, RESEARCH_TEAM)
}

/// Whether Research Team's choice is open for `invader`'s ground forces committed to `planet`:
/// the ability, another player controlling the planet (the controller keeps it, 3.1), and not
/// already coexisting there. The caller adds the rival-ground-force test it already owns.
#[must_use]
pub fn research_team_open(
    state: &GameState,
    content: &ContentStore,
    invader: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    has_research_team(state, content, invader)
        && !crate::coexistence::is_coexisting(state, system, planet, invader)
        && state
            .system_state(system)
            .planet_control
            .get(planet)
            .is_some_and(|holder| holder != invader)
}

/// Whether Research Team's choice is open for `defender`, whose units stand on `planet` when
/// `invader` commits ground forces there (Dane's ruling: it works on defense too): the ability, a
/// seat other than the invader, units on the planet, and not already coexisting there. The
/// defender decides, not the invader.
#[must_use]
pub fn research_team_defender_open(
    state: &GameState,
    content: &ContentStore,
    defender: &PlayerId,
    invader: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    defender != invader
        && has_research_team(state, content, defender)
        && !crate::coexistence::is_coexisting(state, system, planet, defender)
        && state
            .system_state(system)
            .on_planet(planet)
            .iter()
            .any(|unit| &unit.owner == defender)
}

// -- technology purges (Ta Zern) ---------------------------------------------------------------------

/// Whether technology `alias` was purged from every deck by Ta Zern.
#[must_use]
pub fn technology_purged(state: &GameState, alias: &TechnologyId) -> bool {
    state
        .faction_marks
        .contains_key(&format!("{PURGED_PREFIX}{alias}"))
}

pub(crate) fn mark_technology_purged(state: &mut GameState, alias: &TechnologyId) {
    state
        .faction_marks
        .insert(format!("{PURGED_PREFIX}{alias}"), "1".to_owned());
}

/// Technologies in `player`'s deck: the active cards that are generic or their own faction's,
/// not purged and not marked "cannot be researched".
#[must_use]
pub fn deck(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<TechnologyId> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    crate::technology::active_aliases(content, sources)
        .into_iter()
        .filter(|alias| !technology_purged(state, alias))
        .filter(|alias| {
            crate::technology::faction_of(content, alias)
                .is_none_or(|faction| faction == seat.faction.as_str())
        })
        .filter(|alias| {
            content
                .get(ContentType::Technologies, alias.as_str())
                .and_then(|record| record.text("text"))
                .is_none_or(|text| !text.to_ascii_lowercase().contains("cannot be researched"))
        })
        .collect()
}

/// Total printed prerequisites of a technology.
#[must_use]
pub fn prerequisite_count(content: &ContentStore, alias: &TechnologyId) -> usize {
    crate::technology::prerequisites(content, alias)
        .values()
        .sum()
}

// -- Carrina's waiver --------------------------------------------------------------------------------

fn carrina_key(player: &PlayerId) -> String {
    format!("{CARRINA_PREFIX}{player}")
}

/// Doctor Carrina (held by `holder`) lets `researcher` ignore 1 prerequisite for the research now
/// under way. The mark names the holder, who is owed the placement that follows.
pub(crate) fn arm_waiver(state: &mut GameState, researcher: &PlayerId, holder: &PlayerId) {
    state
        .faction_marks
        .insert(carrina_key(researcher), holder.to_string());
}

/// The research is over: the waiver goes with it. Returns the holder who armed one.
pub(crate) fn disarm_waiver(state: &mut GameState, researcher: &PlayerId) -> Option<PlayerId> {
    state
        .faction_marks
        .remove(&carrina_key(researcher))
        .map(PlayerId::new)
}

fn waived_prerequisites(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    player: &PlayerId,
    _technology: &str,
) -> usize {
    usize::from(state.faction_marks.contains_key(&carrina_key(player)))
}

/// Seats other than `researcher` that hold a readied Doctor Carrina, in seating order.
#[must_use]
pub fn agent_holders(state: &GameState, researcher: &PlayerId) -> Vec<PlayerId> {
    state
        .seating_order
        .iter()
        .filter(|holder| *holder != researcher)
        .filter(|holder| {
            state
                .player(holder)
                .and_then(|seat| seat.leaders.get(&LeaderId::new(AGENT)).copied())
                == Some(LeaderStatus::Readied)
        })
        .cloned()
        .collect()
}

/// Non-home planets `controller` controls where `holder` could place an infantry into
/// coexistence: not a space station, not a home planet, infantry in `holder`'s reinforcements.
#[must_use]
pub fn infantry_targets(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    holder: &PlayerId,
    controller: &PlayerId,
) -> Vec<(SystemId, PlanetId)> {
    let Some(infantry) =
        crate::action_cards::placed_unit_id(state, content, sources, holder, "infantry")
    else {
        return Vec::new();
    };
    if crate::supply::allowed(state, content, sources, holder, &infantry, 1) == 0 {
        return Vec::new();
    }
    let mut found: Vec<(SystemId, PlanetId)> = state
        .controlled_planets(controller)
        .into_iter()
        .filter(|(_, planet)| {
            !ti4_content::galaxy::is_space_station(content, planet.as_str(), sources)
                && ti4_content::galaxy::planet(content, planet.as_str(), sources)
                    .is_none_or(|record| record.homeworld_of().is_none())
        })
        .map(|(system, planet)| (system.clone(), planet.clone()))
        .collect();
    found.sort();
    found
}

/// Doctor Carrina's second half: place 1 infantry from `holder`'s reinforcements on the planet and
/// put them into coexistence there. The infantry go down first so the coexistence is real
/// (`coexistence::reconcile` would otherwise end it). Returns whether it was done.
pub fn place_into_coexistence(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    holder: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) -> bool {
    let Some(infantry) =
        crate::action_cards::placed_unit_id(state, content, sources, holder, "infantry")
    else {
        return false;
    };
    if crate::supply::allowed(state, content, sources, holder, &infantry, 1) == 0 {
        return false;
    }
    let controller = state
        .system_state(system)
        .planet_control
        .get(planet)
        .cloned();
    if controller.is_none() || controller.as_ref() == Some(holder) {
        return false;
    }
    state
        .system_mut(system)
        .planet_units
        .entry(planet.clone())
        .or_default()
        .push(Unit::new(infantry, holder.clone()));
    // The holder keeps no control (3.1): `begin` cannot fail for a planet another player holds.
    begin_coexisting(state, holder, system, planet, None).is_ok()
}

// -- the timing abilities ------------------------------------------------------------------------------

fn timing_abilities(_state: &GameState, owner_name: &str, seat: &PlayerId) -> Vec<Ability> {
    let mut abilities = vec![
        hydrothermal(owner_name, seat),
        radical(owner_name, seat),
        eanautic(owner_name, seat),
    ];
    abilities.extend(super::deepwrought_cards::timing_abilities(owner_name, seat));
    abilities
}

/// Hydrothermal Mining: "At the start of the status phase, gain 1 trade good for each ocean card in
/// play." Every ocean card at the table counts, whoever holds it. Not optional.
fn hydrothermal(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{HYDROTHERMAL}:STATUS_PHASE_BEGAN:after"),
        seat.clone(),
        "STATUS_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let goods = i32::try_from(oceans_in_play(context.state)).unwrap_or(i32::MAX);
            crate::supply::gain_trade_goods_staged(context.state, &owner, goods, HYDROTHERMAL);
            Ok(())
        }),
    )
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        crate::technology::has_technology_text(context.state, &condition_owner, HYDROTHERMAL)
            && oceans_in_play(context.state) > 0
    }))
}

/// One replacement Radical Advancement could make.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Replacement {
    /// The technology given up (returned to the deck).
    pub old: TechnologyId,
    /// The technology gained in its place.
    pub new: TechnologyId,
}

impl Replacement {
    fn id(&self) -> String {
        format!("{}|{}", self.old, self.new)
    }
}

/// Every replacement of an owned non-unit-upgrade technology by a technology of the same color in
/// the player's deck with exactly 1 more prerequisite.
#[must_use]
pub fn replacements(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
) -> Vec<Replacement> {
    let Some(seat) = state.player(player) else {
        return Vec::new();
    };
    let deck = deck(state, content, sources, player);
    let mut found = Vec::new();
    for old in &seat.technologies {
        if crate::technology::is_unit_upgrade(content, old) {
            continue;
        }
        let Some(colour) = crate::technology::colour_type(content, old) else {
            continue;
        };
        let wanted = prerequisite_count(content, old) + 1;
        for new in &deck {
            if !seat.technologies.contains(new)
                && !crate::technology::is_unit_upgrade(content, new)
                && crate::technology::colour_type(content, new) == Some(colour)
                && prerequisite_count(content, new) == wanted
            {
                found.push(Replacement {
                    old: old.clone(),
                    new: new.clone(),
                });
            }
        }
    }
    found.sort();
    found
}

/// Radical Advancement: "At the start of the status phase, you may replace one of your non-unit
/// upgrade technologies with a technology of the same color that has exactly 1 more prerequisite."
fn radical(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("technology:{owner_name}:{RADICAL}:STATUS_PHASE_BEGAN:after"),
        seat.clone(),
        "STATUS_PHASE_BEGAN",
        Relation::After,
        Arc::new(move |_event, _resolver, context| {
            let options = replacements(context.state, context.content, context.sources, &owner);
            if options.is_empty() {
                return Ok(());
            }
            let answer = ask(
                context,
                &owner,
                "Radical Advancement: replace which technology with which".to_owned(),
                RADICAL,
                "radical_replace",
                options
                    .iter()
                    .map(|swap| {
                        ChoiceOption::labelled(
                            swap.id(),
                            "technology",
                            format!(
                                "replace {} with {}",
                                crate::technology::name(context.content, &swap.old),
                                crate::technology::name(context.content, &swap.new)
                            ),
                        )
                    })
                    .collect(),
            )?;
            let Some(swap) = options.into_iter().find(|swap| swap.id() == answer.id) else {
                return Ok(());
            };
            if let Some(seat) = context.state.player_mut(&owner) {
                seat.technologies.remove(&swap.old);
                seat.exhausted_technologies.remove(&swap.old);
            }
            crate::technology::gain(context.state, context.content, context.sources, &owner, &swap.new);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_stateful_condition(Arc::new(move |_event, _, context| {
        crate::technology::has_technology_text(context.state, &condition_owner, RADICAL)
            && !replacements(
                context.state,
                context.content,
                context.sources,
                &condition_owner,
            )
            .is_empty()
    }))
}

// -- Eanautic ------------------------------------------------------------------------------------------

/// Planets of `system` holding one of `owner`'s Eanautics while `owner` is coexisting there.
fn coexisting_mech_planets(
    state: &GameState,
    owner: &PlayerId,
    system: &SystemId,
) -> Vec<PlanetId> {
    state
        .system_state(system)
        .planet_units
        .iter()
        .filter(|(planet, units)| {
            units
                .iter()
                .any(|unit| unit.owner == *owner && unit.type_id.as_str() == MECH)
                && crate::coexistence::is_coexisting(state, system, planet, owner)
        })
        .map(|(planet, _)| planet.clone())
        .collect()
}

/// Planets `owner` controls in their own home system.
fn home_planets(state: &GameState, owner: &PlayerId) -> Vec<PlanetId> {
    let Some(home) = state
        .player(owner)
        .and_then(|seat| seat.home_system.clone())
    else {
        return Vec::new();
    };
    state
        .controlled_planets(owner)
        .into_iter()
        .filter(|(system, _)| **system == home)
        .map(|(_, planet)| planet.clone())
        .collect()
}

/// Eanautic: "When another player activates this system, if this unit is coexisting, you may move it
/// and any of your infantry on its planet to a planet you control in your home system."
///
/// Each Eanautic is its own use (the ability may resolve again while another coexisting one
/// remains). The planet, the destination and the number of infantry are each asked when there is
/// more than one answer; the infantry moved are the planet's own units, damage and all.
fn eanautic(owner_name: &str, seat: &PlayerId) -> Ability {
    let (owner, condition_owner) = (seat.clone(), seat.clone());
    Ability::stateful(
        format!("unit:{owner_name}:{MECH}:SYSTEM_ACTIVATED:when"),
        seat.clone(),
        "SYSTEM_ACTIVATED",
        Relation::When,
        Arc::new(move |event, _resolver, context| {
            let Some(system) = event.text("system").map(SystemId::new) else {
                return Ok(());
            };
            let from = coexisting_mech_planets(context.state, &owner, &system);
            let from = match from.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let answer = ask_about(
                        context,
                        &owner,
                        format!("Eanautic: move the mech on which planet of {system}"),
                        MECH,
                        "eanautic_planet",
                        DecisionTarget::System(system.clone()),
                        from.iter()
                            .map(|planet| {
                                ChoiceOption::labelled(
                                    planet.to_string(),
                                    "planet",
                                    format!("the Eanautic on {planet}"),
                                )
                            })
                            .collect(),
                    )?;
                    let Some(chosen) = from.iter().find(|planet| planet.as_str() == answer.id)
                    else {
                        return Ok(());
                    };
                    chosen.clone()
                }
            };
            let homes = home_planets(context.state, &owner);
            let to = match homes.as_slice() {
                [] => return Ok(()),
                [only] => only.clone(),
                _ => {
                    let answer = ask(
                        context,
                        &owner,
                        "Eanautic: move to which planet in your home system".to_owned(),
                        MECH,
                        "eanautic_destination",
                        homes
                            .iter()
                            .map(|planet| {
                                ChoiceOption::labelled(
                                    planet.to_string(),
                                    "planet",
                                    format!("move to {planet}"),
                                )
                            })
                            .collect(),
                    )?;
                    let Some(chosen) = homes.iter().find(|planet| planet.as_str() == answer.id)
                    else {
                        return Ok(());
                    };
                    chosen.clone()
                }
            };
            let types = ti4_content::units::catalogue(context.content, context.sources);
            let infantry: Vec<Unit> = context
                .state
                .system_state(&system)
                .on_planet_of(&from, &owner)
                .into_iter()
                .filter(|unit| {
                    types
                        .get(unit.type_id.as_str())
                        .is_some_and(|kind| kind.base_type() == "infantry")
                })
                .cloned()
                .collect();
            let take = if infantry.is_empty() {
                0
            } else {
                let answer = ask(
                    context,
                    &owner,
                    format!("Eanautic: how many infantry on {from} go with it"),
                    MECH,
                    "eanautic_infantry",
                    (0..=infantry.len())
                        .map(|count| {
                            ChoiceOption::labelled(
                                format!("infantry|{count}"),
                                "count",
                                format!("{count} infantry"),
                            )
                        })
                        .collect(),
                )?;
                answer
                    .id
                    .strip_prefix("infantry|")
                    .and_then(|count| count.parse::<usize>().ok())
                    .filter(|count| *count <= infantry.len())
                    .unwrap_or(0)
            };
            let mech = context
                .state
                .system_state(&system)
                .on_planet_of(&from, &owner)
                .into_iter()
                .find(|unit| unit.type_id.as_str() == MECH)
                .cloned();
            let Some(mech) = mech else {
                return Ok(());
            };
            let mut moving: Vec<Unit> = infantry.into_iter().take(take).collect();
            moving.push(mech);
            // The units leave the planet they coexist on and land on the home planet.
            context
                .state
                .system_mut(&system)
                .remove_from_planet(&from, &moving);
            let home = home_system_of(context.state, &owner).unwrap_or_else(|| system.clone());
            context
                .state
                .system_mut(&home)
                .planet_units
                .entry(to)
                .or_default()
                .extend(moving);
            crate::coexistence::reconcile(context.state, &system, &from);
            Ok(())
        }),
    )
    .with_optional(true)
    .with_frequency(Frequency::Unlimited)
    .with_repeatable_in_window(true)
    .with_stateful_condition(Arc::new(move |event, _, context| {
        event.text("player") != Some(condition_owner.as_str())
            && event.text("system").is_some_and(|system| {
                !coexisting_mech_planets(context.state, &condition_owner, &SystemId::new(system))
                    .is_empty()
            })
            && !home_planets(context.state, &condition_owner).is_empty()
    }))
}

fn home_system_of(state: &GameState, owner: &PlayerId) -> Option<SystemId> {
    state
        .player(owner)
        .and_then(|seat| seat.home_system.clone())
}

// -- Luminous --------------------------------------------------------------------------------------------

/// D.W.S. Luminous (and a Nekro flagship carrying its text): see
/// [`MovementHooks::own_unit_passage`].
fn own_unit_passage(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    site: &PassSite<'_>,
) -> bool {
    super::flagship_has_text(state, site.player, site.ship_type, FLAGSHIP)
}

// -- Aello -------------------------------------------------------------------------------------------------

/// "Have an ocean card in play."
fn commander_unlocked(
    state: &GameState,
    _content: &ContentStore,
    _sources: SourceSet,
    _galaxy: Option<&Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    (leader.as_str() == COMMANDER).then(|| !oceans(state, player).is_empty())
}

#[cfg(test)]
pub(crate) mod testkit {
    use std::cell::RefCell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    use ti4_content::ContentStore;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{PlanetId, PlayerId, SystemId};
    use ti4_model::state::GameState;

    use crate::choice::{Choice, ChoiceOption, Decider, IllegalChoice, Scripted, Table};

    pub(crate) const RADICAL_ID: &str = "technology:deepwrought:radical:STATUS_PHASE_BEGAN:after";
    pub(crate) const EANAUTIC_ID: &str = "unit:deepwrought:deepwrought_mech:SYSTEM_ACTIVATED:when";

    pub(crate) fn a() -> PlayerId {
        PlayerId::new("a")
    }
    pub(crate) fn b() -> PlayerId {
        PlayerId::new("b")
    }
    pub(crate) fn c() -> PlayerId {
        PlayerId::new("c")
    }
    pub(crate) fn content() -> &'static ContentStore {
        ContentStore::embedded()
    }
    /// `a` the Deepwrought, `b` Sol, `c` Hacan.
    pub(crate) fn game() -> GameState {
        crate::fixtures::seated_game(
            &[("a", "deepwrought"), ("b", "sol"), ("c", "hacan")],
            DEFAULT,
        )
    }
    pub(crate) fn scripted(answers: &[&str]) -> Table {
        Table::with_default(Box::new(Scripted::new(answers.iter().copied())))
    }

    /// A planet in no home system and not Mecatol Rex, with its system.
    pub(crate) fn plain_planet() -> (SystemId, PlanetId) {
        let content = content();
        ti4_content::galaxy::all_planets(content, DEFAULT)
            .iter()
            .find(|(_, planet)| {
                planet.system_id().is_some()
                    && !planet.is_placed_during_play()
                    && planet.homeworld_of().is_none()
                    && planet.name() != Some("Mecatol Rex")
            })
            .map(|(id, planet)| {
                (
                    SystemId::new(planet.system_id().expect("a system")),
                    PlanetId::new(*id),
                )
            })
            .expect("a plain planet")
    }

    /// `b` controls the planet with one infantry; `a`'s infantry stand there and coexist.
    pub(crate) fn coexisting_on(state: &mut GameState, system: &SystemId, planet: &PlanetId) {
        state.system_mut(system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(state, system, planet, "infantry", &b(), 1);
        crate::fixtures::put_on_planet(state, system, planet, "infantry", &a(), 1);
        assert!(super::begin_coexisting(state, &a(), system, planet, None).expect("begins"));
    }

    /// Several invented planets, each a coexistence of `a` on `b`'s planet.
    pub(crate) fn coexisting_everywhere(
        state: &mut GameState,
        count: usize,
    ) -> Vec<(SystemId, PlanetId)> {
        let spots: Vec<(SystemId, PlanetId)> = (0..count)
            .map(|n| {
                (
                    SystemId::new(format!("9{n}")),
                    PlanetId::new(format!("px{n}")),
                )
            })
            .collect();
        for (system, planet) in &spots {
            coexisting_on(state, system, planet);
        }
        spots
    }

    pub(crate) fn emit(
        state: &mut GameState,
        answers: &[&str],
        kind: &str,
        pairs: &[(&str, serde_json::Value)],
    ) {
        let mut table = scripted(answers);
        let mut resolver = crate::fixtures::armed_resolver(state);
        crate::fixtures::with_context(state, DEFAULT, None, &mut table, |context| {
            let payload: BTreeMap<String, serde_json::Value> = pairs
                .iter()
                .map(|(key, value)| ((*key).to_owned(), value.clone()))
                .collect();
            let event = context
                .event_sequence
                .next(kind, payload)
                .expect("an event id");
            resolver
                .emit_with_context(context, event, |_, _| {})
                .expect("emits");
        });
    }

    /// Takes the first of `prefer` that is offered, else the first option; records every prompt.
    #[derive(Debug)]
    pub(crate) struct Steer {
        pub(crate) prefer: Vec<String>,
        pub(crate) seen: Rc<RefCell<Vec<String>>>,
    }
    impl Decider for Steer {
        fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
            self.seen.borrow_mut().push(choice.prompt.clone());
            for want in &self.prefer {
                if let Some(option) = choice.option(want) {
                    return Ok(option.clone());
                }
            }
            choice
                .options
                .first()
                .cloned()
                .ok_or_else(|| IllegalChoice::NoOptions {
                    player: choice.player.clone(),
                    prompt: choice.prompt.clone(),
                })
        }
    }
    pub(crate) fn steer(prefer: &[&str]) -> (Table, Rc<RefCell<Vec<String>>>) {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let table = Table::with_default(Box::new(Steer {
            prefer: prefer.iter().map(|id| (*id).to_owned()).collect(),
            seen: seen.clone(),
        }));
        (table, seen)
    }

    pub(crate) fn infantry_on(
        state: &GameState,
        system: &SystemId,
        planet: &PlanetId,
        who: &PlayerId,
    ) -> usize {
        state
            .system_state(system)
            .on_planet_of(planet, who)
            .iter()
            .filter(|unit| unit.type_id.as_str().contains("infantry"))
            .count()
    }

    pub(crate) fn owned(state: &GameState, who: &PlayerId, tech: &str) -> bool {
        state.player(who).is_some_and(|seat| {
            seat.technologies
                .contains(&ti4_model::id::TechnologyId::new(tech))
        })
    }
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::DEFAULT;
    use ti4_model::id::{PlanetId, PlayerId, SystemId, TechnologyId};
    use ti4_model::state::GameState;

    use super::testkit::*;
    use super::*;

    // -- Oceanbound and the ocean cards ---------------------------------------------------------

    #[test]
    fn an_ocean_is_gained_readied_for_each_planet_where_units_begin_coexisting() {
        let mut state = game();
        let spots = coexisting_everywhere(&mut state, 1);
        assert_eq!(oceans(&state, &a()), vec![PlanetId::new("ocean1")]);
        assert_eq!(oceans_in_play(&state), 1);
        // Gaining readies it: exhaust it, then begin coexisting on a second planet.
        state.exhaust_planet(PlanetId::new("ocean1"));
        let (system, planet) = (SystemId::new("95a"), PlanetId::new("py"));
        coexisting_on(&mut state, &system, &planet);
        assert_eq!(
            oceans(&state, &a()),
            vec![PlanetId::new("ocean1"), PlanetId::new("ocean2")]
        );
        assert!(state.exhausted_planets.contains(&PlanetId::new("ocean1")));
        assert!(!state.exhausted_planets.contains(&PlanetId::new("ocean2")));
        // Units already coexisting on a planet do not begin again: no card, and no second answer.
        assert!(!begin_coexisting(&mut state, &a(), &spots[0].0, &spots[0].1, None).unwrap());
        assert_eq!(oceans(&state, &a()).len(), 2);
    }

    #[test]
    fn only_the_deepwrought_gain_oceans_by_coexisting() {
        let mut state = game();
        let (system, planet) = (SystemId::new("96"), PlanetId::new("pz"));
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        assert!(begin_coexisting(&mut state, &b(), &system, &planet, None).unwrap());
        assert!(oceans(&state, &b()).is_empty());
        assert_eq!(oceans_in_play(&state), 0);
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with(OCEAN_PREFIX))
        );
    }

    #[test]
    fn oceanbound_discards_oceans_beyond_the_planets_with_coexisting_units() {
        let mut state = game();
        let spots = coexisting_everywhere(&mut state, 2);
        assert_eq!(oceans(&state, &a()).len(), 2);
        // Lose the units on one planet: any time, the second ocean goes.
        let lost: Vec<_> = state
            .system_state(&spots[0].0)
            .on_planet_of(&spots[0].1, &a())
            .into_iter()
            .cloned()
            .collect();
        state
            .system_mut(&spots[0].0)
            .remove_from_planet(&spots[0].1, &lost);
        assert_eq!(coexisting_planets(&state, &a()), 1);
        assert_eq!(oceans(&state, &a()), vec![PlanetId::new("ocean1")]);
        assert_eq!(
            oceans_in_play(&state),
            1,
            "the discarded card is no longer in play"
        );
        // The stale row is deleted the next time a card is gained, and the card can be gained again.
        let (system, planet) = (SystemId::new("97"), PlanetId::new("pw"));
        coexisting_on(&mut state, &system, &planet);
        assert_eq!(
            oceans(&state, &a()),
            vec![PlanetId::new("ocean1"), PlanetId::new("ocean2")]
        );
    }

    #[test]
    fn there_are_only_five_oceans() {
        let mut state = game();
        coexisting_everywhere(&mut state, 6);
        assert_eq!(coexisting_planets(&state, &a()), 6);
        assert_eq!(
            oceans(&state, &a()).len(),
            5,
            "a sixth planet gains nothing"
        );
        assert_eq!(oceans_in_play(&state), 5);
    }

    #[test]
    fn ocean_cards_pay_for_things_like_planets_and_exhaust() {
        let mut state = game();
        coexisting_everywhere(&mut state, 1);
        // Nothing else to spend: every planet a controls is exhausted and the goods are gone.
        let held: Vec<PlanetId> = state
            .controlled_planets(&a())
            .into_iter()
            .map(|(_, planet)| planet.clone())
            .collect();
        for planet in held {
            state.exhaust_planet(planet);
        }
        state.player_mut(&a()).unwrap().trade_goods = 0;
        let plans = crate::payment::plans(
            &state,
            content(),
            DEFAULT,
            &a(),
            1,
            crate::production::Spend::Resources,
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].planets, vec![PlanetId::new("ocean1")]);
        assert!(crate::payment::apply(&mut state, &a(), &plans[0]));
        assert!(state.exhausted_planets.contains(&PlanetId::new("ocean1")));
        assert!(
            crate::payment::plans(
                &state,
                content(),
                DEFAULT,
                &a(),
                1,
                crate::production::Spend::Influence
            )
            .is_empty(),
            "a spent ocean pays nothing until it is readied"
        );
    }

    #[test]
    fn crash_landing_into_coexistence_is_the_beginning_oceanbound_waits_for() {
        let mut state = game();
        let (system, planet) = plain_planet();
        crate::fixtures::put(&mut state, &system, "infantry", &a(), 1);
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        state.last_ship_destroyed = Some((
            system.clone(),
            a(),
            ti4_model::id::UnitTypeId::new("cruiser"),
        ));
        let answer = format!("planet|{planet}");
        let mut table = scripted(&[answer.as_str()]);
        let effect =
            crate::action_cards::effect_for(&ti4_model::id::ActionCardId::new("crashlanding"))
                .expect("registered");
        crate::fixtures::with_context(&mut state, DEFAULT, None, &mut table, |context| {
            effect(context, &a());
        });
        assert!(crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert_eq!(oceans(&state, &a()), vec![PlanetId::new("ocean1")]);
    }

    // -- Research Team ---------------------------------------------------------------------------

    /// `a` (Deepwrought) holds 2 infantry in space; `b` controls the planet with 2 infantry.
    fn invasion_setup() -> (GameState, SystemId, PlanetId) {
        let mut state = game();
        let (system, planet) = plain_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 2);
        crate::fixtures::put(&mut state, &system, "infantry", &a(), 2);
        (state, system, planet)
    }

    fn invade(
        state: &mut GameState,
        who: &PlayerId,
        prefer: &[&str],
        system: &SystemId,
    ) -> (
        crate::invasion::InvasionReport,
        crate::dice::Dice,
        Vec<String>,
    ) {
        let (mut table, seen) = steer(prefer);
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(7);
        let report = crate::invasion::resolve(
            state,
            content(),
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            system,
            who,
        )
        .expect("the invasion resolves");
        let prompts = seen.borrow().clone();
        (report, dice, prompts)
    }

    #[test]
    fn research_team_lets_committed_ground_forces_coexist_instead_of_fighting() {
        let (mut state, system, planet) = invasion_setup();
        let commit = format!("commit|0|{planet}");
        let (report, dice, _) = invade(
            &mut state,
            &a(),
            &[commit.as_str(), "coexist", "done_committing"],
            &system,
        );
        assert!(dice.rolled("ground combat").is_empty(), "nobody fought");
        assert!(crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b()),
            "the controller keeps the planet (coexistence 3.1)"
        );
        assert!(
            report.captured.is_empty(),
            "a coexisting invader captures nothing"
        );
        assert_eq!(infantry_on(&state, &system, &planet, &b()), 2);
        assert_eq!(infantry_on(&state, &system, &planet, &a()), 2);
        assert_eq!(
            oceans(&state, &a()),
            vec![PlanetId::new("ocean1")],
            "Oceanbound answered"
        );
    }

    #[test]
    fn research_team_is_optional_and_declining_fights() {
        let (mut state, system, planet) = invasion_setup();
        let commit = format!("commit|0|{planet}");
        let (_, dice, _) = invade(
            &mut state,
            &a(),
            &[commit.as_str(), "fight", "done_committing"],
            &system,
        );
        assert!(
            !dice.rolled("ground combat").is_empty(),
            "the committed forces fought"
        );
        assert!(!crate::coexistence::in_coexistence(
            &state, &system, &planet
        ));
        assert!(oceans(&state, &a()).is_empty());
    }

    #[test]
    fn research_team_is_not_offered_without_a_rival_force_or_to_other_factions() {
        // No rival ground force: the planet is simply taken.
        let (mut state, system, planet) = invasion_setup();
        state.system_mut(&system).planet_units.remove(&planet);
        let commit = format!("commit|0|{planet}");
        let (report, _, prompts) = invade(
            &mut state,
            &a(),
            &[commit.as_str(), "coexist", "done_committing"],
            &system,
        );
        assert!(
            prompts
                .iter()
                .all(|prompt| !prompt.contains("Research Team")),
            "{prompts:?}"
        );
        assert_eq!(report.captured.len(), 1);
        // Another faction committing against the same defenders is never offered it.
        let (mut state, system, planet) = invasion_setup();
        crate::fixtures::put(&mut state, &system, "infantry", &c(), 2);
        let commit = format!("commit|0|{planet}");
        let (_, _, prompts) = invade(
            &mut state,
            &c(),
            &[commit.as_str(), "coexist", "done_committing"],
            &system,
        );
        assert!(
            prompts
                .iter()
                .all(|prompt| !prompt.contains("Research Team")),
            "{prompts:?}"
        );
        assert!(!crate::coexistence::in_coexistence(
            &state, &system, &planet
        ));
    }

    // -- Research Team on defense (Dane's ruling, operator 2026-10-07) -------------------------------

    /// `a` (Deepwrought) controls the planet with 2 infantry; `b` (Sol) has 2 infantry in space.
    fn defense_setup() -> (GameState, SystemId, PlanetId) {
        let mut state = game();
        let (system, planet) = plain_planet();
        state.system_mut(&system).set_control(planet.clone(), a());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);
        crate::fixtures::put(&mut state, &system, "infantry", &b(), 2);
        (state, system, planet)
    }

    #[test]
    fn a_defending_deepwrought_may_coexist_instead_of_fighting() {
        let (mut state, system, planet) = defense_setup();
        let commit = format!("commit|0|{planet}");
        let (report, dice, prompts) = invade(
            &mut state,
            &b(),
            &[commit.as_str(), "coexist", "done_committing"],
            &system,
        );
        assert!(
            prompts
                .iter()
                .any(|prompt| prompt.contains("Research Team")),
            "the defender was asked: {prompts:?}"
        );
        assert!(dice.rolled("ground combat").is_empty(), "nobody fought");
        assert!(
            prompts
                .iter()
                .all(|prompt| !prompt.contains("another ground combat")),
            "no rule-12 combat is offered either: {prompts:?}"
        );
        assert!(crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert!(
            report.captured.is_empty(),
            "nothing is captured: no combat decided the planet"
        );
        assert_eq!(report.coexisted, vec![planet.clone()]);
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b()),
            "a controller who coexists steps aside for the invader (coexistence 3.2)"
        );
        assert_eq!(infantry_on(&state, &system, &planet, &a()), 2);
        assert_eq!(infantry_on(&state, &system, &planet, &b()), 2);
        assert_eq!(
            oceans(&state, &a()),
            vec![PlanetId::new("ocean1")],
            "Oceanbound answered"
        );
    }

    #[test]
    fn the_defending_deepwrought_decides_and_declining_fights() {
        let (mut state, system, planet) = defense_setup();
        let commit = format!("commit|0|{planet}");
        // The defender is the one asked, not the invader.
        let (decider, seen) = crate::choice::Capturing::new(Box::new(Steer {
            prefer: vec![
                commit.clone(),
                "fight".to_owned(),
                "done_committing".to_owned(),
            ],
            seen: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }));
        let mut table = crate::choice::Table::with_default(Box::new(decider));
        let mut dice = crate::dice::Dice::new();
        let mut rng = crate::rng::GameRng::new(7);
        let _ = crate::invasion::resolve(
            &mut state,
            content(),
            DEFAULT,
            &mut table,
            &mut dice,
            &mut rng,
            &system,
            &b(),
        );
        let asked: Vec<(PlayerId, String)> = seen
            .borrow()
            .iter()
            .map(|choice| (choice.player.clone(), choice.prompt.clone()))
            .collect();
        assert!(
            asked
                .iter()
                .any(|(who, prompt)| who == &a() && prompt.contains("Research Team")),
            "Deepwrought is asked: {asked:?}"
        );
        assert!(
            asked
                .iter()
                .all(|(who, prompt)| who == &a() || !prompt.contains("Research Team")),
            "the invader is not"
        );
        assert!(
            !dice.rolled("ground combat").is_empty(),
            "declining means the ground combat is fought"
        );
        assert!(!crate::coexistence::is_coexisting(
            &state,
            &system,
            &planet,
            &a()
        ));
        assert!(oceans(&state, &a()).is_empty());
    }

    #[test]
    fn a_deepwrought_already_coexisting_is_not_asked_when_attacked_again() {
        let (mut state, system, planet) = defense_setup();
        let commit = format!("commit|0|{planet}");
        invade(
            &mut state,
            &b(),
            &[commit.as_str(), "coexist", "done_committing"],
            &system,
        );
        crate::fixtures::put(&mut state, &system, "infantry", &c(), 2);
        let (_, _, prompts) = invade(
            &mut state,
            &c(),
            &[commit.as_str(), "coexist", "done_committing"],
            &system,
        );
        assert!(
            prompts
                .iter()
                .all(|prompt| !prompt.contains("Research Team")),
            "{prompts:?}"
        );
    }

    #[test]
    fn only_a_deepwrought_defender_with_units_on_the_planet_is_offered_research_team_on_defense() {
        let (state, system, planet) = defense_setup();
        let content = content();
        assert!(research_team_defender_open(
            &state,
            content,
            &a(),
            &b(),
            &system,
            &planet
        ));
        assert!(
            !research_team_defender_open(&state, content, &b(), &a(), &system, &planet),
            "Sol has no Research Team"
        );
        assert!(
            !research_team_defender_open(&state, content, &a(), &a(), &system, &planet),
            "the invader is not a defender"
        );
        let mut empty = state.clone();
        empty.system_mut(&system).planet_units.remove(&planet);
        assert!(
            !research_team_defender_open(&empty, content, &a(), &b(), &system, &planet),
            "no units there: nothing to coexist"
        );
    }

    // -- Hydrothermal Mining and Radical Advancement ---------------------------------------------

    fn give(state: &mut GameState, who: &PlayerId, techs: &[&str]) {
        let seat = state.player_mut(who).expect("a seat");
        for tech in techs {
            seat.technologies.insert(TechnologyId::new(*tech));
        }
    }
    fn clear_techs(state: &mut GameState, who: &PlayerId) {
        state.player_mut(who).expect("a seat").technologies.clear();
    }
    fn goods(state: &GameState, who: &PlayerId) -> i32 {
        state.player(who).map_or(0, |seat| seat.trade_goods)
    }

    #[test]
    fn hydrothermal_mining_gains_a_trade_good_per_ocean_in_play_at_the_start_of_the_status_phase() {
        let mut state = game();
        give(&mut state, &a(), &["hydrothermal"]);
        coexisting_everywhere(&mut state, 3);
        let before = goods(&state, &a());
        emit(&mut state, &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(goods(&state, &a()), before + 3);
        // No oceans in play, no goods (and nothing offered).
        let mut bare = game();
        give(&mut bare, &a(), &["hydrothermal"]);
        let before = bare.clone();
        emit(&mut bare, &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(goods(&bare, &a()), goods(&before, &a()));
    }

    #[test]
    fn hydrothermal_mining_counts_every_ocean_in_play_whoever_holds_the_technology() {
        let mut state = game();
        coexisting_everywhere(&mut state, 2);
        give(&mut state, &c(), &["hydrothermal"]);
        let before = goods(&state, &c());
        emit(&mut state, &[], "STATUS_PHASE_BEGAN", &[]);
        assert_eq!(
            goods(&state, &c()),
            before + 2,
            "the Deepwrought's oceans are in play"
        );
        // The Deepwrought without the card gains nothing from it.
        assert_eq!(goods(&state, &a()), goods(&game(), &a()));
    }

    #[test]
    fn radical_advancement_replaces_a_technology_with_one_that_has_exactly_one_more_prerequisite() {
        let mut state = game();
        clear_techs(&mut state, &a());
        give(&mut state, &a(), &["radical", "nm"]);
        let swaps = replacements(&state, content(), DEFAULT, &a());
        let ids: Vec<String> = swaps.iter().map(Replacement::id).collect();
        assert!(ids.contains(&"nm|dxa".to_owned()), "{ids:?}");
        assert!(ids.contains(&"nm|bs".to_owned()), "{ids:?}");
        assert!(
            !ids.contains(&"nm|hm".to_owned()),
            "two more prerequisites: {ids:?}"
        );
        assert!(ids.iter().all(|id| !id.starts_with("radical|")
            || id.split('|').nth(1).is_some_and(|new| {
                crate::technology::colour_type(content(), &TechnologyId::new(new)) == Some("BIOTIC")
            })));
        emit(
            &mut state,
            &[RADICAL_ID, "nm|dxa"],
            "STATUS_PHASE_BEGAN",
            &[],
        );
        assert!(owned(&state, &a(), "dxa"));
        assert!(
            !owned(&state, &a(), "nm"),
            "the old card went back to the deck"
        );
        assert!(owned(&state, &a(), "radical"));
    }

    #[test]
    fn radical_advancement_is_optional() {
        let mut declined = game();
        clear_techs(&mut declined, &a());
        give(&mut declined, &a(), &["radical", "nm"]);
        emit(&mut declined, &["decline"], "STATUS_PHASE_BEGAN", &[]);
        assert!(owned(&declined, &a(), "nm"));
    }

    #[test]
    fn radical_advancement_never_replaces_a_unit_upgrade_or_offers_a_purged_card() {
        let mut state = game();
        clear_techs(&mut state, &a());
        give(&mut state, &a(), &["radical", "cv2", "dxa"]);
        let ids = |state: &GameState| -> Vec<String> {
            replacements(state, content(), DEFAULT, &a())
                .iter()
                .map(Replacement::id)
                .collect()
        };
        let open = ids(&state);
        assert!(open.contains(&"dxa|hm".to_owned()), "{open:?}");
        assert!(open.contains(&"radical|hm".to_owned()), "{open:?}");
        assert!(
            open.iter().all(|id| !id.starts_with("cv2|")),
            "a unit upgrade is never replaced: {open:?}"
        );
        mark_technology_purged(&mut state, &TechnologyId::new("hm"));
        let closed = ids(&state);
        assert!(
            closed.iter().all(|id| !id.ends_with("|hm")),
            "a purged card is out of every deck: {closed:?}"
        );
    }

    // -- D.W.S. Luminous --------------------------------------------------------------------------

    fn rules<'a>(
        hub: &'a crate::fixtures::Hub,
        state: &GameState,
        active: &str,
    ) -> crate::movement::MovementRules<'a> {
        crate::movement::MovementRules::with_laws(
            &hub.galaxy,
            content(),
            DEFAULT,
            active,
            crate::movement::Board::for_player(state, content(), DEFAULT, &a()),
            Some(state),
        )
    }

    #[test]
    fn the_luminous_moves_through_systems_holding_its_owners_units_past_blockades_and_gains_a_step()
    {
        let hub = crate::fixtures::plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let centre = SystemId::new(hub.centre.as_str());
        let mut state = game();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(near_a.as_str()),
            "deepwrought_flagship",
            &a(),
            1,
        );
        crate::fixtures::put(&mut state, &centre, "destroyer", &b(), 1);
        let flagship = Some("deepwrought_flagship");
        // A rival blockade stops everyone, flagship included, while no unit of the owner is there.
        assert!(!rules(&hub, &state, &near_b).can_reach_ship(&near_a, 2, flagship));
        // With one of the owner's units in the blockaded system the flagship passes, and the step
        // that system earns pays for the second step: move 1 is enough.
        crate::fixtures::put(&mut state, &centre, "cruiser", &a(), 1);
        assert!(rules(&hub, &state, &near_b).can_reach_ship(&near_a, 1, flagship));
        // Another ship type of the same player is still blocked, however far it moves.
        assert!(!rules(&hub, &state, &near_b).can_reach_ship(&near_a, 2, Some("cruiser")));
        // The boost does not need another player's units: remove the blockade.
        let mut quiet = game();
        crate::fixtures::put(
            &mut quiet,
            &SystemId::new(near_a.as_str()),
            "deepwrought_flagship",
            &a(),
            1,
        );
        crate::fixtures::put(&mut quiet, &centre, "cruiser", &a(), 1);
        assert!(rules(&hub, &quiet, &near_b).can_reach_ship(&near_a, 1, flagship));
        assert!(
            !rules(&hub, &quiet, &near_b).can_reach_ship(&near_a, 1, Some("cruiser")),
            "an ordinary ship of move 1 cannot make two steps"
        );
        // Without the owner's units there the flagship has no boost.
        let mut empty = game();
        crate::fixtures::put(
            &mut empty,
            &SystemId::new(near_a.as_str()),
            "deepwrought_flagship",
            &a(),
            1,
        );
        assert!(!rules(&hub, &empty, &near_b).can_reach_ship(&near_a, 1, flagship));
    }

    #[test]
    fn the_luminous_earns_one_step_for_each_system_it_moves_through_that_holds_its_units() {
        let hub = crate::fixtures::plain_hub();
        let (start, target) = (hub.outer[0].clone(), hub.outer[3].clone());
        let (one, two) = (hub.outer[1].clone(), hub.outer[2].clone());
        let centre = SystemId::new(hub.centre.as_str());
        let flagship = Some("deepwrought_flagship");
        let mut state = game();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(start.as_str()),
            "deepwrought_flagship",
            &a(),
            1,
        );
        // The short way (through the centre) is closed by a rival; the ring takes 3 steps.
        crate::fixtures::put(&mut state, &centre, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &SystemId::new(one.as_str()), "cruiser", &a(), 1);
        assert!(
            !rules(&hub, &state, &target).can_reach_ship(&start, 1, flagship),
            "move 1 + one own system is 2 steps; the ring takes 3"
        );
        crate::fixtures::put(&mut state, &SystemId::new(two.as_str()), "cruiser", &a(), 1);
        assert!(rules(&hub, &state, &target).can_reach_ship(&start, 1, flagship));
    }

    #[test]
    fn the_luminous_earns_each_systems_step_once_so_a_rift_loop_cannot_grow_its_move() {
        // A gravity rift holding the owner's units, beside another system holding them. Earning the
        // step on every pass made rift -> neighbour -> rift worth three steps for the two it cost,
        // so a search that found no destination never ended (BF-22 wide-roster trade teacher).
        let hub = crate::fixtures::hub_with_centre("41");
        let centre = SystemId::new(hub.centre.as_str());
        let start = hub.outer[0].clone();
        let beside = hub.outer[1].clone();
        let mut state = game();
        crate::fixtures::put(
            &mut state,
            &SystemId::new(start.as_str()),
            "deepwrought_flagship",
            &a(),
            1,
        );
        crate::fixtures::put(&mut state, &centre, "cruiser", &a(), 1);
        crate::fixtures::put(&mut state, &SystemId::new(beside.as_str()), "cruiser", &a(), 1);
        let flagship = Some("deepwrought_flagship");
        // No such destination: the search has to run out and say so.
        assert!(!rules(&hub, &state, "nowhere").can_reach_ship(&start, 1, flagship));
        // The honest reach is still there: through the rift (its own step plus the Luminous step)
        // to the far side of the ring on move 1.
        let across = hub.across(&start);
        assert!(rules(&hub, &state, &across).can_reach_ship(&start, 1, flagship));
    }

    #[test]
    fn the_tactical_action_offers_the_luminous_the_blockaded_move_and_no_other_ship() {
        let hub = crate::fixtures::plain_hub();
        let (near_a, near_b) = (hub.outer[0].clone(), hub.across(&hub.outer[0]));
        let centre = SystemId::new(hub.centre.as_str());
        let mut state = game();
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(a());
        let origin = SystemId::new(near_a.as_str());
        crate::fixtures::put(&mut state, &origin, "deepwrought_flagship", &a(), 1);
        crate::fixtures::put(&mut state, &origin, "destroyer", &a(), 1);
        crate::fixtures::put(&mut state, &centre, "destroyer", &b(), 1);
        crate::fixtures::put(&mut state, &centre, "infantry", &a(), 1);
        crate::tactical::activate(&mut state, &a(), &SystemId::new(near_b.as_str()))
            .expect("activates");
        let found = crate::tactical::movable(&state, content(), DEFAULT, &hub.galaxy, &a());
        let from_origin: Vec<&str> = found
            .iter()
            .filter(|ship| ship.origin == origin)
            .map(|ship| ship.unit.type_id.as_str())
            .collect();
        assert_eq!(from_origin, vec!["deepwrought_flagship"], "{found:?}");
    }

    #[test]
    fn ta_zerns_unlock_is_three_scored_objectives() {
        let mut state = game();
        let hero = LeaderId::new(HERO);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&hero),
            Some(&LeaderStatus::Locked)
        );
        for objective in ["o1", "o2", "o3"] {
            assert!(
                crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a())
                    .iter()
                    .all(|leader| leader != &hero)
            );
            state
                .scored_objectives
                .entry(a())
                .or_default()
                .insert(ti4_model::id::ObjectiveId::new(objective));
        }
        let unlocked = crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert!(unlocked.contains(&hero), "{unlocked:?}");
    }

    #[test]
    fn a_nekro_flagship_carrying_the_luminous_text_passes_too() {
        let state = crate::fixtures::nekro_with_z(
            &[("a", "nekro"), ("b", "deepwrought")],
            &["deepwrought"],
        );
        let system = SystemId::new("18");
        let site = PassSite {
            player: &a(),
            active: &system,
            ship_type: crate::factions::nekro::NEKRO_FLAGSHIP,
        };
        assert!(own_unit_passage(&state, content(), DEFAULT, &site));
        let plain = crate::fixtures::nekro_with_z(&[("a", "nekro"), ("b", "deepwrought")], &[]);
        assert!(!own_unit_passage(&plain, content(), DEFAULT, &site));
    }

    // -- Eanautic ---------------------------------------------------------------------------------

    fn mech_setup() -> (GameState, SystemId, PlanetId) {
        let mut state = game();
        let (system, planet) = plain_planet();
        state.system_mut(&system).set_control(planet.clone(), b());
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &b(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, MECH, &a(), 1);
        crate::fixtures::put_on_planet(&mut state, &system, &planet, "infantry", &a(), 2);
        assert!(begin_coexisting(&mut state, &a(), &system, &planet, None).unwrap());
        (state, system, planet)
    }

    fn home(state: &GameState) -> (SystemId, PlanetId) {
        let system = state
            .player(&a())
            .unwrap()
            .home_system
            .clone()
            .expect("a home");
        (system, PlanetId::new("ikatena"))
    }

    fn activated(system: &SystemId, by: &str) -> Vec<(&'static str, serde_json::Value)> {
        vec![("player", by.into()), ("system", system.to_string().into())]
    }

    #[test]
    fn the_eanautic_moves_itself_and_chosen_infantry_home_when_another_player_activates_its_system()
    {
        let (mut state, system, planet) = mech_setup();
        let (home_system, home_planet) = home(&state);
        let home_infantry = infantry_on(&state, &home_system, &home_planet, &a());
        emit(
            &mut state,
            &[EANAUTIC_ID, "infantry|1"],
            "SYSTEM_ACTIVATED",
            &activated(&system, "b"),
        );
        let mech_at = |state: &GameState, system: &SystemId, planet: &PlanetId| {
            state
                .system_state(system)
                .on_planet_of(planet, &a())
                .iter()
                .filter(|unit| unit.type_id.as_str() == MECH)
                .count()
        };
        assert_eq!(mech_at(&state, &system, &planet), 0);
        assert_eq!(mech_at(&state, &home_system, &home_planet), 1);
        assert_eq!(
            infantry_on(&state, &home_system, &home_planet, &a()),
            home_infantry + 1
        );
        assert_eq!(
            infantry_on(&state, &system, &planet, &a()),
            1,
            "one infantry stayed"
        );
        assert!(
            crate::coexistence::is_coexisting(&state, &system, &planet, &a()),
            "units remain, so the coexistence stands"
        );
    }

    #[test]
    fn the_eanautic_taking_every_unit_ends_the_coexistence() {
        let (mut state, system, planet) = mech_setup();
        emit(
            &mut state,
            &[EANAUTIC_ID, "infantry|2"],
            "SYSTEM_ACTIVATED",
            &activated(&system, "b"),
        );
        assert!(
            state
                .system_state(&system)
                .on_planet_of(&planet, &a())
                .is_empty()
        );
        assert!(
            !crate::coexistence::in_coexistence(&state, &system, &planet),
            "rule 6"
        );
        assert_eq!(
            state.system_state(&system).planet_control.get(&planet),
            Some(&b())
        );
    }

    #[test]
    fn the_eanautic_is_optional_and_needs_coexistence_and_another_players_activation() {
        let (state, system, _) = mech_setup();
        let mut plain = state.clone();
        emit(
            &mut plain,
            &["decline"],
            "SYSTEM_ACTIVATED",
            &activated(&system, "b"),
        );
        assert_eq!(plain, state, "declined: nothing moved");
        // The owner activating their own system is not "another player".
        let mut own = state.clone();
        emit(&mut own, &[], "SYSTEM_ACTIVATED", &activated(&system, "a"));
        assert_eq!(own, state);
        // A mech that is not coexisting is not offered.
        let mut alone = game();
        let (system, planet) = plain_planet();
        crate::fixtures::put_on_planet(&mut alone, &system, &planet, MECH, &a(), 1);
        alone.system_mut(&system).set_control(planet, a());
        let before = alone.clone();
        emit(
            &mut alone,
            &[],
            "SYSTEM_ACTIVATED",
            &activated(&system, "b"),
        );
        assert_eq!(alone, before);
    }

    // -- Aello --------------------------------------------------------------------------------------

    #[test]
    fn aello_unlocks_once_an_ocean_card_is_in_play() {
        let mut state = game();
        let leader = LeaderId::new(COMMANDER);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&leader),
            Some(&LeaderStatus::Locked)
        );
        assert!(
            crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a()).is_empty()
        );
        coexisting_everywhere(&mut state, 1);
        let unlocked = crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        assert_eq!(unlocked, vec![leader.clone()]);
        assert_eq!(
            state.player(&a()).unwrap().leaders.get(&leader),
            Some(&LeaderStatus::Unlocked)
        );
        assert!(crate::promissory::has_commander_ability(
            &state,
            &a(),
            COMMANDER
        ));
    }

    #[test]
    fn aello_takes_one_off_another_players_paid_research_and_pays_its_holder() {
        let mut state = game();
        coexisting_everywhere(&mut state, 1);
        crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        clear_techs(&mut state, &b());
        {
            let seat = state.player_mut(&b()).unwrap();
            seat.trade_goods = 4;
            seat.commodities = 0;
        }
        for (_, planet) in state
            .controlled_planets(&b())
            .iter()
            .map(|(s, p)| ((*s).clone(), (*p).clone()))
            .collect::<Vec<_>>()
        {
            state.exhaust_planet(planet);
        }
        state.player_mut(&a()).unwrap().commodities = 0;
        let mut table = scripted(&["reduce", "decline", "nm"]);
        let card =
            crate::strategy_cards::card_name(content(), "pok7technology").expect("Technology");
        assert_eq!(card, "Technology");
        crate::strategy_cards::secondary(
            &mut state,
            content(),
            DEFAULT,
            None,
            &mut table,
            &b(),
            "pok7technology",
        )
        .expect("resolves");
        assert!(owned(&state, &b(), "nm"), "b researched");
        assert_eq!(goods(&state, &b()), 1, "the bill was 3, not 4");
        assert_eq!(
            state.player(&a()).unwrap().commodities,
            1,
            "the holder gained a commodity"
        );
    }

    #[test]
    fn aello_is_reached_through_a_faceup_alliance() {
        let mut state = game();
        coexisting_everywhere(&mut state, 1);
        crate::leaders::check_unlocks(&mut state, content(), DEFAULT, None, &a());
        let note = crate::promissory::note_id("an", "deepwrought");
        state.promissory_notes.insert(note.clone(), c());
        assert!(
            !crate::promissory::has_commander_ability(&state, &c(), COMMANDER),
            "not faceup"
        );
        state.promissory_faceup.insert(note);
        assert!(crate::promissory::has_commander_ability(
            &state,
            &c(),
            COMMANDER
        ));
    }

    // -- neutrality ---------------------------------------------------------------------------------

    #[test]
    fn a_game_without_the_deepwrought_is_untouched_by_every_window() {
        let mut state =
            crate::fixtures::seated_game(&[("a", "sol"), ("b", "hacan"), ("c", "mentak")], DEFAULT);
        let (system, _) = plain_planet();
        let spendable = crate::production::spendable_planets(&state, &a());
        let researchable = crate::technology::researchable(&state, content(), DEFAULT, &a());
        let before = state.clone();
        emit(&mut state, &[], "STATUS_PHASE_BEGAN", &[]);
        emit(&mut state, &[], "STATUS_PHASE_ENDED", &[]);
        emit(
            &mut state,
            &[],
            "SYSTEM_ACTIVATED",
            &activated(&system, "b"),
        );
        assert_eq!(
            state
                .faction_marks
                .keys()
                .filter(|key| key.starts_with("deepwrought:"))
                .count(),
            0
        );
        assert_eq!(state.exhausted_planets, before.exhausted_planets);
        assert_eq!(
            crate::production::spendable_planets(&state, &a()),
            spendable
        );
        assert_eq!(
            crate::technology::researchable(&state, content(), DEFAULT, &a()),
            researchable
        );
        assert_eq!(oceans_in_play(&state), 0);
        // An ordinary research changes only what research changes.
        let mut table = scripted(&[]);
        crate::strategy_cards::primary(
            &mut state,
            content(),
            DEFAULT,
            None,
            &mut table,
            &a(),
            "pok7technology",
        )
        .expect("resolves");
        assert!(
            state
                .faction_marks
                .keys()
                .all(|key| !key.starts_with("deepwrought:"))
        );
        let _ = ContentStore::embedded();
        let _ = PlayerId::new("a");
    }
}
