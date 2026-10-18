//! Per-faction modules (BF-00a, `plans/BASE_FACTIONS_PLAN_2026-10-02.md`).
//!
//! Each faction being brought into scope lives in its own file here and declares one
//! [`FactionModule`]: what it implements, by printed id, and the [`Hooks`] the shared engine
//! calls. The shared modules (`faction_abilities`, `leaders`, `breakthroughs`, ...) dispatch to
//! every registered module from the hook functions they already expose, so a faction's code never
//! needs a branch at a call site and two factions never edit the same file.
//!
//! The six factions in scope before this plan (Sol, Hacan, Letnev, Xxcha, Jol-Nar, L1Z1X) stay
//! where they are: moving them would be behaviour-identical churn, and nothing new depends on it.
//!
//! **Ownership.** A hook is called for *every* player and every module, not only for the seat
//! playing that faction: a promissory note, a copied ability or a captured unit belongs to
//! whoever holds the card. Each hook checks its own condition and returns the neutral value
//! otherwise. Modules are visited in [`MODULES`] order, which is fixed, so dispatch never depends
//! on anything but that list.
//!
//! **Coverage.** The id lists in a module are claims that the printed text is implemented and
//! tested. [`ledger`] compares them, plus the pre-existing registries, against the content corpus
//! so a faction's gap is a list rather than an impression.

use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, SourceSet};
use ti4_model::id::{LeaderId, PlanetId, PlayerId, StrategyCardId, SystemId};
use ti4_model::state::GameState;

use crate::choice::{Choice, ChoiceOption};
use crate::timing::{Ability, TimingContext};

/// One unit about to roll combat dice, for the per-unit combat hooks.
#[derive(Debug, Clone, Copy)]
pub struct CombatUnit<'a> {
    /// Its owner.
    pub player: &'a PlayerId,
    /// The system the combat is in (the active system for space combat).
    pub system: Option<&'a SystemId>,
    /// The planet, for ground combat.
    pub planet: Option<&'a PlanetId>,
    /// Its unit type id, e.g. `sardakk_flagship`.
    pub unit_type: &'a str,
    /// "space" or "ground".
    pub context: &'a str,
}

pub mod arborec;
pub mod argent;
pub mod bastion;
pub mod bastion_units;
mod borrowed_commanders;
pub(crate) mod borrowed_commanders_b;
pub mod borrowed_round_agents;
pub mod cabal;
pub mod crimson;
pub(crate) mod crimson_cards;
pub mod deepwrought;
mod deepwrought_cards;
pub(crate) mod deepwrought_research;
pub mod empyrean;
pub mod empyrean_units;
pub mod firmament;
mod firmament_flip;
pub mod firmament_plots;
pub mod ghost;
pub mod hooks_cards;
pub mod hooks_combat;
pub mod hooks_economy;
pub mod hooks_ground;
pub mod hooks_movement;
pub mod hooks_strategy;
pub mod keleres;
pub mod keleres_units;
pub mod mahact;
pub mod mahact_units;
pub mod mentak;
pub mod muaat;
pub mod naalu;
pub mod naaz;
pub mod nekro;
pub mod nekro_units;
pub mod nomad;
pub mod nomad_agents;
pub mod obsidian;
pub mod ralnel;
pub(crate) mod ralnel_cards;
pub mod saar;
pub mod sardakk;
pub mod titans;
pub mod titans_leaders;
pub mod winnu;
pub mod yin;
pub mod yssaril;

/// Every per-faction module, in dispatch order.
pub const MODULES: [&FactionModule; 27] = [
    &arborec::MODULE,
    &argent::MODULE,
    &bastion::MODULE,
    &cabal::MODULE,
    &crimson::MODULE,
    &deepwrought::MODULE,
    &empyrean::MODULE,
    &firmament::MODULE,
    &ghost::MODULE,
    &keleres::MODULE_M,
    &keleres::MODULE_X,
    &keleres::MODULE_A,
    &mahact::MODULE,
    &mentak::MODULE,
    &muaat::MODULE,
    &naalu::MODULE,
    &naaz::MODULE,
    &nekro::MODULE,
    &nomad::MODULE,
    &obsidian::MODULE,
    &ralnel::MODULE,
    &saar::MODULE,
    &sardakk::MODULE,
    &titans::MODULE,
    &winnu::MODULE,
    &yin::MODULE,
    &yssaril::MODULE,
];

/// What one faction implements and how the engine reaches it.
#[derive(Debug)]
pub struct FactionModule {
    /// The faction alias in `factions.json`.
    pub alias: &'static str,
    /// Printed abilities (`abilities.json` ids) implemented here.
    pub abilities: &'static [&'static str],
    /// Faction technologies implemented (including unit upgrades verified data-driven).
    pub technologies: &'static [&'static str],
    /// Faction units whose printed ability text is implemented (flagship, mech, upgrades).
    pub units: &'static [&'static str],
    /// Promissory notes implemented, both for the owner and for whoever receives them.
    pub promissory: &'static [&'static str],
    /// Leaders (agent, commander, hero) implemented, by leader id.
    pub leaders: &'static [&'static str],
    /// Thunder's Edge breakthroughs implemented, by alias.
    pub breakthroughs: &'static [&'static str],
    /// The engine's entry points into this faction.
    pub hooks: Hooks,
}

impl FactionModule {
    /// A module that implements nothing yet.
    #[must_use]
    pub const fn empty(alias: &'static str) -> Self {
        Self {
            alias,
            abilities: &[],
            technologies: &[],
            units: &[],
            promissory: &[],
            leaders: &[],
            breakthroughs: &[],
            hooks: Hooks::NONE,
        }
    }
}

/// Whether a unit of `owner` with type `unit_type` has the printed text of flagship
/// `flagship_id`: it is that flagship, or it is a Nekro flagship whose owner has placed the Z
/// token (Valefar Assimilator Z) on that faction. Only text abilities follow; stats never do.
#[must_use]
pub fn flagship_has_text(
    state: &GameState,
    owner: &PlayerId,
    unit_type: &str,
    flagship_id: &str,
) -> bool {
    unit_type == flagship_id
        || (unit_type == nekro::NEKRO_FLAGSHIP && nekro::z_lends(state, owner, flagship_id))
}

/// Whether `owner` has, in `system`'s space area, a unit with the text of flagship `flagship_id`.
#[must_use]
pub fn has_flagship_text_in(
    state: &GameState,
    owner: &PlayerId,
    system: &ti4_model::id::SystemId,
    flagship_id: &str,
) -> bool {
    state.system_state(system).units.iter().any(|unit| {
        &unit.owner == owner && flagship_has_text(state, owner, unit.type_id.as_str(), flagship_id)
    })
}

/// Engine entry points. Each mirrors the shared hook of the same name and is optional; a module
/// sets only the ones it needs (`..Hooks::NONE`). New hook points are added here by the
/// coordinator, never by a faction package.
///
/// **Atomicity.** A hook that reports "not performed" (`false`, `None`, `Some(false)`) must leave
/// the state exactly as it found it: callers do not roll back. Decide every choice and check every
/// cost before the first mutation, as the shared hooks do.
///
/// **Order.** Module hooks run *before* the shared faction code in `control_gained`,
/// `perform_component` (modules claim first), `strategy_resolved`, `ground_combat_round_ended`
/// (so module dice are rolled before Harrow's) and `space_combat_round_started`; they run *after*
/// it in the value hooks (`combat_modifier`, `fleet_supply`, `status_tokens`, ...). Among modules,
/// [`MODULES`] order. This order is part of the contract, since it fixes the dice stream.
#[derive(Debug, Clone, Copy)]
#[allow(
    clippy::type_complexity,
    reason = "plain fn-pointer table; aliases would only move the signatures elsewhere"
)]
pub struct Hooks {
    /// Shift to each combat die; `context` is "space" or "ground".
    pub combat_modifier: Option<fn(&GameState, &ContentStore, &PlayerId, &str) -> i64>,
    /// Adjust the fleet-supply limit.
    pub fleet_supply: Option<fn(&GameState, &ContentStore, &PlayerId, i32) -> i32>,
    /// Adjust command tokens gained in the status phase.
    pub status_tokens: Option<fn(&GameState, &ContentStore, &PlayerId, i32) -> i32>,
    /// Prerequisites waived when researching this technology.
    pub waived_prerequisites:
        Option<fn(&GameState, &ContentStore, SourceSet, &PlayerId, &str) -> usize>,
    /// Whether this strategy card's secondary is resolved as its primary.
    pub substitutes_primary: Option<fn(&GameState, &ContentStore, &PlayerId, &str) -> bool>,
    /// Whether this strategy card's secondary costs no token.
    pub secondary_is_free: Option<fn(&GameState, &ContentStore, &PlayerId, &str) -> bool>,
    /// Whether this player may trade action cards.
    pub trades_action_cards: Option<fn(&GameState, &ContentStore, &PlayerId) -> bool>,
    /// Whether this player may transact with non-neighbours.
    pub ignores_neighbours: Option<fn(&GameState, &ContentStore, &PlayerId) -> bool>,
    /// After this player gains control of a planet.
    pub control_gained:
        Option<fn(&mut GameState, &ContentStore, SourceSet, &PlayerId, &SystemId, &PlanetId)>,
    /// Component actions offered on this player's turn. Option ids must start
    /// `faction|<alias>|` so [`Hooks::perform_component`] can claim them.
    pub component_actions: Option<fn(&GameState, &ContentStore, &PlayerId) -> Vec<ChoiceOption>>,
    /// Map-dependent actions, generated only when their legal destinations exist.
    pub mapped_component_actions: Option<
        fn(
            &GameState,
            &ContentStore,
            SourceSet,
            &ti4_content::galaxy::Galaxy,
            &PlayerId,
        ) -> Vec<ChoiceOption>,
    >,
    /// Perform a component action; `true` if this module claimed and performed it.
    pub perform_component: Option<fn(&mut TimingContext<'_>, &PlayerId, &ChoiceOption) -> bool>,
    /// After a strategy card (primary or secondary) resolves for this player.
    pub strategy_resolved: Option<fn(&mut TimingContext<'_>, &PlayerId, &str)>,
    /// Extra hits at the end of a ground-combat round in `system`.
    pub ground_combat_round_ended: Option<
        fn(
            &GameState,
            &ContentStore,
            SourceSet,
            &mut crate::dice::Dice,
            &mut crate::rng::GameRng,
            &PlayerId,
            &SystemId,
        ) -> usize,
    >,
    /// At the start of a space-combat round, for each participant.
    pub space_combat_round_started:
        Option<fn(&mut GameState, &ContentStore, SourceSet, &mut crate::choice::Table, &PlayerId)>,
    /// Whether a commander's unlock condition is met; `None` if not this module's commander.
    pub commander_unlocked: Option<
        fn(
            &GameState,
            &ContentStore,
            SourceSet,
            Option<&ti4_content::galaxy::Galaxy>,
            &PlayerId,
            &LeaderId,
        ) -> Option<bool>,
    >,
    /// For an action-phase leader this module delivers: whether it can resolve now. `None` if
    /// not this module's leader (it is then not offered as a component action).
    pub leader_action: Option<fn(&GameState, &ContentStore, &PlayerId, &LeaderId) -> Option<bool>>,
    /// Use a leader; `None` if not this module's leader, else whether it resolved.
    pub use_leader: Option<fn(&mut TimingContext<'_>, &PlayerId, &LeaderId) -> Option<bool>>,
    /// Action leaders whose effects open nested timing windows.
    pub use_leader_timed: Option<
        fn(
            &mut TimingContext<'_>,
            &mut crate::timing::Resolver,
            &PlayerId,
            &LeaderId,
        ) -> Result<Option<bool>, crate::timing::TimingError>,
    >,
    /// A leader that performs a strategy primary and continues through Game-owned windows.
    pub use_leader_strategy_primary: Option<
        fn(
            &mut TimingContext<'_>,
            &PlayerId,
            &LeaderId,
        ) -> Result<
            Option<(StrategyCardId, crate::strategy_cards::Ability)>,
            crate::timing::TimingError,
        >,
    >,
    /// Legal follower choices after a leader's primary, delivered by Game's continuation.
    pub leader_strategy_followers:
        Option<fn(&GameState, &PlayerId, &LeaderId, &StrategyCardId) -> Option<Vec<Choice>>>,
    /// Extra votes this player casts.
    pub vote_bonus: Option<fn(&GameState, &PlayerId) -> i64>,
    /// Timing abilities to register for one seat when the game is seated (`reactions::arm`).
    ///
    /// The general route to every window the engine emits as a typed event
    /// (`reactions::EMITTED_EVENTS` and every other `emit_typed`/`Resolving::emit` call): turn
    /// start, activation, movement, combat rounds, hits to assign, sustain, production, agenda
    /// reveal, votes, transactions, action-card play/discard, planet control. Called for **every**
    /// seat with `(state, owner_name, seat)`, since a card can change hands or be gained later and
    /// the resolver has no late registration; the ability's condition limits it to whoever holds
    /// the card. Ability ids must be unique per seat: `"<kind>:<owner_name>:<card>:<EVENT>:<rel>"`.
    ///
    /// The state passed is the one the game was constructed from: build the same abilities
    /// whatever it says, and decide everything in the ability's condition and effect. The
    /// resolver's frequency bookkeeping (`OncePerTurn`, `OncePerRound`) is **not** saved with the
    /// game and resets when a game is restored or branched, so gate "once" on state (an exhausted
    /// card, a flag on `GameState`), not on `Frequency`. Some emit sites ignore a WHEN cancel and
    /// swallow errors (`let _ = ...emit(...)`): an effect must be complete or not happen.
    pub timing_abilities: Option<fn(&GameState, &str, &PlayerId) -> Vec<Ability>>,
    /// Shift to one unit's combat roll (positive = better), on top of `combat_modifier`.
    pub unit_roll_modifier:
        Option<fn(&GameState, &ContentStore, SourceSet, &CombatUnit<'_>) -> i64>,
    /// One unit's combat dice, given the count so far. Adjust the count (add, multiply); an
    /// effect that sets an absolute number must say why it overrides earlier modules.
    pub unit_dice: Option<fn(&GameState, &ContentStore, SourceSet, &CombatUnit<'_>, i64) -> i64>,
    /// After the movement step of `player`'s tactical action into `system`: whether to skip directly
    /// to the "Commit Ground Forces" step (no space cannon offense, space combat or bombardment).
    /// The module asks its own optional question and returns `Some(true)` when the player takes it;
    /// `None` when it has nothing to offer. What follows the commitment (a hero's purge) is the
    /// module's, on `GROUND_COMMITMENT_FINISHED`.
    ///
    /// Sardakk N'orr hero Sh'val, Harbinger: "After you move ships into the active system: You may
    /// skip directly to the 'Commit Ground Forces' step."
    pub skip_to_commit: Option<fn(&mut TimingContext<'_>, &PlayerId, &SystemId) -> Option<bool>>,
    /// Space-combat hooks (`hooks_combat.rs`).
    pub combat: hooks_combat::CombatHooks,
    /// Invasion and ground-combat hooks (`hooks_ground.rs`).
    pub ground: hooks_ground::GroundHooks,
    /// Production, placement, payment and action-card hooks (`hooks_economy.rs`).
    pub economy: hooks_economy::EconomyHooks,
    /// Movement and adjacency hooks (`hooks_movement.rs`).
    pub movement: hooks_movement::MovementHooks,
    /// Hidden-hand and card-window hooks (`hooks_cards.rs`).
    pub cards: hooks_cards::CardHooks,
    /// Strategy, initiative and unit-form hooks (`hooks_strategy.rs`).
    pub strategy: hooks_strategy::StrategyHooks,
}

impl Hooks {
    /// No hooks.
    pub const NONE: Self = Self {
        combat_modifier: None,
        fleet_supply: None,
        status_tokens: None,
        waived_prerequisites: None,
        substitutes_primary: None,
        secondary_is_free: None,
        trades_action_cards: None,
        ignores_neighbours: None,
        control_gained: None,
        component_actions: None,
        mapped_component_actions: None,
        perform_component: None,
        strategy_resolved: None,
        ground_combat_round_ended: None,
        space_combat_round_started: None,
        commander_unlocked: None,
        leader_action: None,
        use_leader: None,
        use_leader_timed: None,
        use_leader_strategy_primary: None,
        leader_strategy_followers: None,
        vote_bonus: None,
        timing_abilities: None,
        unit_roll_modifier: None,
        unit_dice: None,
        skip_to_commit: None,
        combat: hooks_combat::CombatHooks::NONE,
        ground: hooks_ground::GroundHooks::NONE,
        economy: hooks_economy::EconomyHooks::NONE,
        movement: hooks_movement::MovementHooks::NONE,
        cards: hooks_cards::CardHooks::NONE,
        strategy: hooks_strategy::StrategyHooks::NONE,
    };
}

fn hooks() -> impl Iterator<Item = &'static Hooks> {
    MODULES.iter().map(|module| &module.hooks)
}

// -- dispatch, called from the shared hook functions --------------------------------------------

pub(crate) fn combat_modifier(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    context: &str,
) -> i64 {
    hooks()
        .filter_map(|h| h.combat_modifier)
        .map(|f| f(state, content, player, context))
        .sum()
}

pub(crate) fn fleet_supply(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    limit: i32,
) -> i32 {
    hooks()
        .filter_map(|h| h.fleet_supply)
        .fold(limit, |limit, f| f(state, content, player, limit))
}

pub(crate) fn status_tokens(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    count: i32,
) -> i32 {
    hooks()
        .filter_map(|h| h.status_tokens)
        .fold(count, |count, f| f(state, content, player, count))
}

pub(crate) fn waived_prerequisites(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    technology: &str,
) -> usize {
    hooks()
        .filter_map(|h| h.waived_prerequisites)
        .map(|f| f(state, content, sources, player, technology))
        .sum()
}

pub(crate) fn substitutes_primary(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &str,
) -> bool {
    hooks()
        .filter_map(|h| h.substitutes_primary)
        .any(|f| f(state, content, player, card))
}

pub(crate) fn secondary_is_free(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    card: &str,
) -> bool {
    hooks()
        .filter_map(|h| h.secondary_is_free)
        .any(|f| f(state, content, player, card))
}

pub(crate) fn trades_action_cards(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> bool {
    hooks()
        .filter_map(|h| h.trades_action_cards)
        .any(|f| f(state, content, player))
}

pub(crate) fn ignores_neighbours(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> bool {
    hooks()
        .filter_map(|h| h.ignores_neighbours)
        .any(|f| f(state, content, player))
}

pub(crate) fn control_gained(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    system: &SystemId,
    planet: &PlanetId,
) {
    for f in hooks().filter_map(|h| h.control_gained) {
        f(state, content, sources, player, system, planet);
    }
}

pub(crate) fn component_actions(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    hooks()
        .filter_map(|h| h.component_actions)
        .flat_map(|f| f(state, content, player))
        .collect()
}

pub(crate) fn mapped_component_actions(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &ti4_content::galaxy::Galaxy,
    player: &PlayerId,
) -> Vec<ChoiceOption> {
    hooks()
        .filter_map(|h| h.mapped_component_actions)
        .flat_map(|f| f(state, content, sources, galaxy, player))
        .collect()
}

pub(crate) fn perform_component(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    option: &ChoiceOption,
) -> bool {
    hooks()
        .filter_map(|h| h.perform_component)
        .any(|f| f(context, player, option))
}

pub(crate) fn strategy_resolved(context: &mut TimingContext<'_>, player: &PlayerId, card: &str) {
    for f in hooks().filter_map(|h| h.strategy_resolved) {
        f(context, player, card);
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "mirrors faction_abilities::ground_combat_round_ended"
)]
pub(crate) fn ground_combat_round_ended(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    dice: &mut crate::dice::Dice,
    rng: &mut crate::rng::GameRng,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    hooks()
        .filter_map(|h| h.ground_combat_round_ended)
        .map(|f| f(state, content, sources, dice, rng, player, system))
        .sum()
}

pub(crate) fn space_combat_round_started(
    state: &mut GameState,
    content: &ContentStore,
    sources: SourceSet,
    table: &mut crate::choice::Table,
    player: &PlayerId,
) {
    for f in hooks().filter_map(|h| h.space_combat_round_started) {
        f(state, content, sources, table, player);
    }
}

pub(crate) fn commander_unlocked(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: Option<&ti4_content::galaxy::Galaxy>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    hooks()
        .filter_map(|h| h.commander_unlocked)
        .find_map(|f| f(state, content, sources, galaxy, player, leader))
}

pub(crate) fn leader_action(
    state: &GameState,
    content: &ContentStore,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    hooks()
        .filter_map(|h| h.leader_action)
        .find_map(|f| f(state, content, player, leader))
}

pub(crate) fn use_leader(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Option<bool> {
    hooks()
        .filter_map(|h| h.use_leader)
        .find_map(|f| f(context, player, leader))
}

pub(crate) fn use_leader_timed(
    context: &mut TimingContext<'_>,
    resolver: &mut crate::timing::Resolver,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<Option<bool>, crate::timing::TimingError> {
    for f in hooks().filter_map(|h| h.use_leader_timed) {
        if let Some(done) = f(context, resolver, player, leader)? {
            return Ok(Some(done));
        }
    }
    Ok(None)
}

pub(crate) fn use_leader_strategy_primary(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    leader: &LeaderId,
) -> Result<Option<(StrategyCardId, crate::strategy_cards::Ability)>, crate::timing::TimingError> {
    for f in hooks().filter_map(|h| h.use_leader_strategy_primary) {
        if let Some(outcome) = f(context, player, leader)? {
            return Ok(Some(outcome));
        }
    }
    Ok(None)
}

pub(crate) fn leader_strategy_followers(
    state: &GameState,
    player: &PlayerId,
    leader: &LeaderId,
    card: &StrategyCardId,
) -> Vec<Choice> {
    hooks()
        .filter_map(|h| h.leader_strategy_followers)
        .find_map(|f| f(state, player, leader, card))
        .unwrap_or_default()
}

pub(crate) fn timing_abilities(
    state: &GameState,
    owner_name: &str,
    seat: &PlayerId,
) -> Vec<Ability> {
    hooks()
        .filter_map(|h| h.timing_abilities)
        .flat_map(|f| f(state, owner_name, seat))
        .chain(crate::technology::timing_abilities(owner_name, seat))
        .chain(borrowed_commanders::timing_abilities(owner_name, seat))
        .chain(borrowed_commanders_b::timing_abilities(owner_name, seat))
        .collect()
}

pub(crate) fn unit_roll_modifier(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &CombatUnit<'_>,
) -> i64 {
    hooks()
        .filter_map(|h| h.unit_roll_modifier)
        .map(|f| f(state, content, sources, unit))
        .sum::<i64>()
        + borrowed_commanders::unit_roll_modifier(state, content, sources, unit)
}

pub(crate) fn unit_dice(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    unit: &CombatUnit<'_>,
    dice: i64,
) -> i64 {
    hooks()
        .filter_map(|h| h.unit_dice)
        .fold(dice, |dice, f| f(state, content, sources, unit, dice))
}

/// Ships that arrived in the active system during the current activation's movement step, as
/// counted by the game (also `MOVEMENT_FINISHED`'s `ships_moved`). Readable until the movement
/// step's windows (incl. [`Hooks::skip_to_commit`]) have closed. "After you move ships into the
/// active system" holds only when this is above zero.
#[must_use]
pub fn ships_moved_this_activation(state: &GameState) -> u32 {
    state
        .faction_marks
        .get(&format!("private:#moved:{}", state.activation_seq))
        .and_then(|count| count.parse::<u32>().ok())
        .unwrap_or(0)
}

pub(crate) fn skip_to_commit(
    context: &mut TimingContext<'_>,
    player: &PlayerId,
    system: &SystemId,
) -> bool {
    hooks()
        .filter_map(|h| h.skip_to_commit)
        .any(|f| f(context, player, system) == Some(true))
}

pub(crate) fn vote_bonus(state: &GameState, player: &PlayerId) -> i64 {
    hooks()
        .filter_map(|h| h.vote_bonus)
        .map(|f| f(state, player))
        .sum()
}

// -- registries, unioned into the shared ledgers -------------------------------------------------

/// Ability ids every module claims.
#[must_use]
pub fn registered_abilities() -> Vec<&'static str> {
    MODULES
        .iter()
        .flat_map(|m| m.abilities.iter().copied())
        .collect()
}

/// Unit ids every module claims.
#[must_use]
pub fn registered_units() -> Vec<&'static str> {
    MODULES
        .iter()
        .flat_map(|m| m.units.iter().copied())
        .collect()
}

/// Leader ids every module claims.
#[must_use]
pub fn registered_leaders() -> Vec<&'static str> {
    MODULES
        .iter()
        .flat_map(|m| m.leaders.iter().copied())
        .collect()
}

/// Breakthrough aliases every module claims.
#[must_use]
pub fn registered_breakthroughs() -> Vec<&'static str> {
    MODULES
        .iter()
        .flat_map(|m| m.breakthroughs.iter().copied())
        .collect()
}

/// The module for a faction alias, if it has one.
#[must_use]
pub fn module(alias: &str) -> Option<&'static FactionModule> {
    MODULES.iter().copied().find(|m| m.alias == alias)
}

// -- the per-faction asset ledger ----------------------------------------------------------------

/// The kind of a faction-specific asset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AssetKind {
    /// A printed faction ability.
    Ability,
    /// A faction technology.
    Technology,
    /// A faction unit with printed ability text.
    Unit,
    /// A promissory note.
    Promissory,
    /// An agent, commander or hero.
    Leader,
    /// A Thunder's Edge breakthrough.
    Breakthrough,
}

/// One faction-specific card or component.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Asset {
    /// What it is.
    pub kind: AssetKind,
    /// Its content id.
    pub id: String,
}

/// Every faction-specific asset a faction has in `sources`, from the corpus.
///
/// Units are the faction's own records among those its sheet lists, their upgrade chain, and
/// extra upgrades a later source adds. Every one appears whether or not it prints ability text:
/// a unit that is purely statistics (or whose ability is a stat, like PRODUCTION) is claimed by a
/// module once a test shows the data-driven stats deliver it.
/// Leaders follow [`crate::leaders::for_faction`], so a reprint replaces its original.
#[must_use]
pub fn assets(content: &ContentStore, sources: SourceSet, alias: &str) -> Vec<Asset> {
    let mut found = std::collections::BTreeSet::new();
    let Some(faction) = ti4_content::factions::get(content, alias) else {
        return Vec::new();
    };
    let mut add = |kind, id: &str| {
        found.insert(Asset {
            kind,
            id: id.to_owned(),
        });
    };
    for id in faction.abilities() {
        add(AssetKind::Ability, id);
    }
    // Operator ruling 2026-10-07: only the technologies the faction sheet lists (`factionTech`,
    // which includes the unit upgrades) count, plus official Thunder's Edge reprints. Cards that
    // merely carry the faction's tag from an obscure variant (Nekro's `nekroc4y`, `nekroc4r`) are
    // not part of the game and are not ledger assets.
    let sheet_technologies: std::collections::BTreeSet<&str> = faction
        .record()
        .strings("factionTech")
        .into_iter()
        .collect();
    for record in content.from_sources(ContentType::Technologies, sources) {
        if record
            .text("faction")
            .is_some_and(|f| keleres::tag_belongs_to(f, alias, false))
            && let Some(id) = record.text("alias")
            && (sheet_technologies.contains(id) || record.text("source") == Some("thunders_edge"))
        {
            add(AssetKind::Technology, id);
        }
    }
    // Units: the sheet's own, their upgrade chain, and extra upgrades a later source adds (Thunder's
    // Edge's Corsair, Eidolon Maximum). Unlisted alternates of the same unit (Naalu's three mech
    // printings, a community variant filed under an official source) are not this faction's.
    let units: Vec<&ti4_content::Record> = content
        .from_sources(ContentType::Units, sources)
        .filter(|record| {
            record
                .text("faction")
                .is_some_and(|f| keleres::tag_belongs_to(f, alias, true))
        })
        .collect();
    let id_of = |record: &ti4_content::Record| {
        record
            .text("id")
            .or_else(|| record.text("alias"))
            .map(ToOwned::to_owned)
    };
    let mut sheet: std::collections::BTreeSet<String> = faction
        .record()
        .strings("units")
        .into_iter()
        .map(ToOwned::to_owned)
        .collect();
    loop {
        let before = sheet.len();
        for record in &units {
            let Some(id) = id_of(record) else { continue };
            let upgrades_listed = record
                .text("upgradesFromUnitId")
                .is_some_and(|from| sheet.contains(from));
            let extra = record.flag("isUpgrade")
                && record.text("upgradesFromUnitId").is_none()
                && record.text("homebrewReplacesID").is_none();
            if upgrades_listed || extra {
                sheet.insert(id);
            }
        }
        if sheet.len() == before {
            break;
        }
    }
    for record in &units {
        if let Some(id) = id_of(record)
            && sheet.contains(&id)
        {
            add(AssetKind::Unit, &id);
        }
    }
    for record in content.from_sources(ContentType::PromissoryNotes, sources) {
        if record
            .text("faction")
            .is_some_and(|f| f.eq_ignore_ascii_case(alias))
            && let Some(id) = record.text("alias").or_else(|| record.text("id"))
        {
            add(AssetKind::Promissory, id);
        }
    }
    for leader in crate::leaders::for_faction(content, sources, alias) {
        add(AssetKind::Leader, leader.as_str());
    }
    for record in content.from_sources(ContentType::Breakthroughs, sources) {
        if record
            .text("faction")
            .is_some_and(|f| keleres::tag_belongs_to(f, alias, false))
            && let Some(id) = record.text("alias")
        {
            add(AssetKind::Breakthrough, id);
        }
    }
    found.into_iter().collect()
}

use AssetKind as K;

/// Assets of the original six factions (the ones in [`crate::seating::IN_SCOPE_FACTIONS`]) that
/// are audited complete, by faction alias.
///
/// These factions are deliberately *not* in [`MODULES`]: `supply::staging_enabled` and other paths
/// switch on "a module faction is seated", so a module for them would change the behaviour of
/// every game that seats one of the six, and every training checkpoint plays them. Their rules
/// live in the shared engine instead, and this table only records, row by row, which of their
/// ledger assets were verified (BF-ORIGINAL-SIX). A row is added only when its text is fully
/// implemented and tested; [`implemented`] and [`missing`] consult it next to the modules.
pub const LEGACY_CLAIMS: &[(&str, &[(AssetKind, &str)])] = &[
    (
        "sol",
        &[
            (K::Technology, "ac2"),
            (K::Technology, "so2"),
            (K::Unit, "sol_carrier"),
            (K::Unit, "sol_carrier2"),
            (K::Unit, "sol_flagship"),
            (K::Unit, "sol_infantry"),
            (K::Unit, "sol_infantry2"),
            (K::Promissory, "ms"),
        ],
    ),
    (
        "hacan",
        &[
            (K::Technology, "pm"),
            (K::Technology, "qdn"),
            (K::Unit, "hacan_flagship"),
            (K::Promissory, "convoys"),
        ],
    ),
    (
        "letnev",
        &[
            (K::Technology, "l4"),
            (K::Technology, "nes"),
            (K::Unit, "letnev_flagship"),
            (K::Promissory, "war_funding"),
        ],
    ),
    (
        "xxcha",
        &[
            (K::Ability, "quash"),
            (K::Technology, "it"),
            (K::Technology, "nf"),
            (K::Unit, "xxcha_flagship"),
            (K::Promissory, "favor"),
        ],
    ),
    (
        "jolnar",
        &[
            (K::Technology, "ers"),
            (K::Technology, "scc"),
            (K::Unit, "jolnar_flagship"),
            (K::Promissory, "ra"),
        ],
    ),
    (
        "l1z1x",
        &[
            (K::Technology, "is"),
            (K::Technology, "sdn2"),
            (K::Unit, "l1z1x_dreadnought"),
            (K::Unit, "l1z1x_dreadnought2"),
            (K::Unit, "l1z1x_flagship"),
            (K::Promissory, "ce"),
        ],
    ),
];

/// Whether [`LEGACY_CLAIMS`] claims this asset.
fn legacy_claimed(asset: &Asset) -> bool {
    LEGACY_CLAIMS.iter().any(|(_, claims)| {
        claims
            .iter()
            .any(|(kind, id)| *kind == asset.kind && *id == asset.id)
    })
}

/// Whether the engine claims this asset, through a module, a legacy claim or a pre-existing
/// registry.
#[must_use]
pub fn implemented(asset: &Asset) -> bool {
    if legacy_claimed(asset) {
        return true;
    }
    let id = asset.id.as_str();
    let claimed = |pick: fn(&FactionModule) -> &'static [&'static str]| {
        MODULES.iter().any(|m| pick(m).contains(&id))
    };
    match asset.kind {
        AssetKind::Ability => {
            claimed(|m| m.abilities) || crate::faction_abilities::registered().contains(&id)
        }
        AssetKind::Technology => claimed(|m| m.technologies),
        AssetKind::Unit => {
            claimed(|m| m.units)
                || crate::faction_abilities::registered_mech_abilities().contains(&id)
        }
        AssetKind::Promissory => claimed(|m| m.promissory),
        AssetKind::Leader => {
            claimed(|m| m.leaders)
                || crate::leaders::registered_abilities().contains(&id)
                || crate::leaders::modifiers().contains_key(id)
        }
        AssetKind::Breakthrough => {
            claimed(|m| m.breakthroughs) || crate::breakthroughs::registered_aliases().contains(&id)
        }
    }
}

/// A faction's assets nothing claims yet.
#[must_use]
pub fn missing(content: &ContentStore, sources: SourceSet, alias: &str) -> Vec<Asset> {
    assets(content, sources, alias)
        .into_iter()
        .filter(|asset| !implemented(asset))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::content_types::DEFAULT;

    #[test]
    fn every_planned_faction_has_one_module_in_the_corpus() {
        let content = ContentStore::embedded();
        let mut seen = std::collections::BTreeSet::new();
        for module in MODULES {
            assert!(seen.insert(module.alias), "{} twice", module.alias);
            assert!(
                ti4_content::factions::get(content, module.alias).is_some(),
                "{} is not a corpus faction",
                module.alias
            );
            assert!(
                !crate::seating::IN_SCOPE_FACTIONS.contains(&module.alias),
                "{} is already in scope through the shared modules",
                module.alias
            );
        }
    }

    /// Three leaders per sheet, except the Nomad: The Company gives it three agents.
    fn leader_count(alias: &str) -> usize {
        if alias == "nomad" { 5 } else { 3 }
    }

    #[test]
    fn every_planned_faction_has_a_full_asset_sheet() {
        // The ledger is only useful if it finds the sheet: abilities, two faction technologies,
        // a flagship and mech, a promissory note, three leaders and a breakthrough.
        let content = ContentStore::embedded();
        for module in MODULES {
            let assets = assets(content, DEFAULT, module.alias);
            let count = |kind| assets.iter().filter(|a| a.kind == kind).count();
            assert!(count(AssetKind::Ability) >= 1, "{}", module.alias);
            assert!(count(AssetKind::Technology) >= 2, "{}", module.alias);
            assert!(count(AssetKind::Unit) >= 2, "{}", module.alias);
            // Empyrean's Dark Whispers: "you have 2 faction promissory notes".
            let notes = if module.alias == "empyrean" { 2 } else { 1 };
            assert_eq!(count(AssetKind::Promissory), notes, "{}", module.alias);
            assert_eq!(
                count(AssetKind::Leader),
                leader_count(module.alias),
                "{}",
                module.alias
            );
            assert_eq!(count(AssetKind::Breakthrough), 1, "{}", module.alias);
        }
    }

    #[test]
    fn leaders_follow_the_sheet_and_leave_the_six_unchanged() {
        // BF-00a narrowed `leaders::for_faction` to the faction sheet. For the six factions
        // already in scope that must deal exactly what it dealt before: every corpus leader of
        // the faction, minus those a reprint replaces.
        let content = ContentStore::embedded();
        for sources in [DEFAULT, ti4_model::content_types::POK] {
            for alias in crate::seating::IN_SCOPE_FACTIONS {
                let records: Vec<_> = content
                    .from_sources(ContentType::Leaders, sources)
                    .filter(|r| {
                        r.text("faction")
                            .is_some_and(|f| f.eq_ignore_ascii_case(alias))
                    })
                    .collect();
                let replaced: Vec<&str> = records
                    .iter()
                    .filter_map(|r| r.text("homebrewReplacesID"))
                    .collect();
                let before: Vec<String> = records
                    .iter()
                    .filter_map(|r| r.text("id").or_else(|| r.text("alias")))
                    .filter(|id| !replaced.contains(id))
                    .map(ToOwned::to_owned)
                    .collect();
                let now: Vec<String> = crate::leaders::for_faction(content, sources, alias)
                    .into_iter()
                    .map(|l| l.as_str().to_owned())
                    .collect();
                assert_eq!(now, before, "{alias}");
            }
        }
        let ids = |alias, sources| -> Vec<String> {
            crate::leaders::for_faction(content, sources, alias)
                .into_iter()
                .map(|l| l.as_str().to_owned())
                .collect()
        };
        assert_eq!(
            ids("naalu", DEFAULT),
            ["naalucommander", "naaluhero", "naaluagent-te"] // corpus order
        );
        // Without Thunder's Edge the listed agent is out of reach, and its predecessor stands in.
        assert_eq!(
            ids("naalu", ti4_model::content_types::POK),
            ["naaluagent", "naalucommander", "naaluhero"]
        );
        assert_eq!(
            ids("ghost", DEFAULT),
            ["ghostagent", "ghostcommander", "ghosthero"]
        );
    }

    #[test]
    fn every_planned_faction_seats_with_the_test_fixture() {
        // The agents' tests start from `fixtures::seated_game`; it must work for every faction.
        for module in MODULES {
            let state = crate::fixtures::seated_game(&[("a", module.alias), ("b", "sol")], DEFAULT);
            let seat = state.player(&PlayerId::new("a")).expect("seated");
            assert_eq!(seat.faction.as_str(), module.alias);
            assert!(seat.home_system.is_some(), "{}", module.alias);
            assert_eq!(
                seat.leaders.len(),
                leader_count(module.alias),
                "{} leaders",
                module.alias
            );
        }
    }

    #[test]
    fn legacy_claims_name_real_assets_of_their_own_original_faction() {
        let content = ContentStore::embedded();
        let mut seen = std::collections::BTreeSet::new();
        for (alias, claims) in LEGACY_CLAIMS {
            assert!(
                crate::seating::IN_SCOPE_FACTIONS.contains(alias),
                "{alias} is not one of the original six"
            );
            assert!(
                MODULES.iter().all(|module| module.alias != *alias),
                "{alias} must stay out of MODULES"
            );
            assert!(seen.insert(*alias), "{alias} twice");
            let sheet = assets(content, DEFAULT, alias);
            for (kind, id) in *claims {
                assert!(
                    sheet.iter().any(|a| a.kind == *kind && a.id == *id),
                    "{alias} claims {kind:?} {id}"
                );
            }
        }
    }

    #[test]
    fn claims_name_real_assets_of_their_own_faction() {
        // A module may only claim what its own sheet prints: a typo or a neighbour's card would
        // otherwise read as coverage.
        let content = ContentStore::embedded();
        for module in MODULES {
            let sheet = assets(content, DEFAULT, module.alias);
            let has = |kind, id: &str| sheet.iter().any(|a| a.kind == kind && a.id == id);
            let lists: [(AssetKind, &[&str]); 6] = [
                (AssetKind::Ability, module.abilities),
                (AssetKind::Technology, module.technologies),
                (AssetKind::Unit, module.units),
                (AssetKind::Promissory, module.promissory),
                (AssetKind::Leader, module.leaders),
                (AssetKind::Breakthrough, module.breakthroughs),
            ];
            for (kind, ids) in lists {
                for id in ids {
                    assert!(has(kind, id), "{} claims {kind:?} {id}", module.alias);
                }
            }
        }
    }

    #[test]
    #[ignore = "report: cargo test -p ti4-engine print_faction_ledger -- --ignored --nocapture"]
    fn print_faction_ledger() {
        let content = ContentStore::embedded();
        let aliases = crate::seating::IN_SCOPE_FACTIONS
            .iter()
            .copied()
            .chain(MODULES.iter().map(|m| m.alias));
        for alias in aliases {
            let all = assets(content, DEFAULT, alias);
            let gaps = missing(content, DEFAULT, alias);
            println!(
                "{alias:<9} {:>2}/{:<2} implemented",
                all.len() - gaps.len(),
                all.len()
            );
            for gap in gaps {
                println!("    {:?} {}", gap.kind, gap.id);
            }
        }
    }
}
