//! What a client needs to stage a tactical action and the pending choice does not say: the ships
//! in range of a system, for the movement the path of each ship, the rifts on it, and what the
//! ship can load, and for the production what a whole build costs and what can pay for it.
//!
//! Nothing here decides a rule. Every fact is the answer of a public function of the engine, asked
//! the way the engine asks it when it applies the move (`Game::begin_one_move`).

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_content::units::catalogue;
use ti4_engine::choice::Choice;
use ti4_engine::decision_context::{ConstraintKind, DecisionTarget};
use ti4_engine::invasion::COMMIT_KIND;
use ti4_engine::movement::{Board, MovementRules};
use ti4_engine::production::{PRODUCE_KIND, Spend, WITHHELD_KEY, Withheld, WithheldUnit};
use ti4_engine::tactical::{ACTIVATE_KIND, Movable, effective_move_value_for_ship, movable_into};
pub use ti4_engine::transit::Unloadable;
use ti4_engine::transit::{CargoSource, CargoWindow, LOAD_KIND, Route, rifts_exited};
use ti4_model::content_types::SourceSet;
use ti4_model::id::{PlanetId, PlayerId, SystemId};
use ti4_model::state::GameState;
use ti4_model::units::Unit;

/// The subtype of the choice of a system to activate.
pub const ACTIVATION_SUBTYPE: &str = "activate_system";
/// The subtype of the choice of a system for a tactical action that places no command token
/// (Thunder's Edge Warfare).
pub const FREE_ACTIVATION_SUBTYPE: &str = "warfare_free_tactical";
/// The subtype of the choice of a ship to move.
pub const MOVEMENT_SUBTYPE: &str = "movement_step";
/// The subtype of the choice of a ground force to land.
pub const INVASION_SUBTYPE: &str = "commit_ground_forces";
/// The subtype of the choice of a unit to produce.
pub const PRODUCTION_SUBTYPE: &str = "produce_unit";

/// How many ships of the seat can move to one system, if it is activated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ReachFact {
    pub system: String,
    pub ships: usize,
    /// The number of systems that these ships are in.
    pub origins: usize,
    /// The production value of the units of the seat in the system: with more than 0 the action
    /// has a production step there.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub production: i64,
    /// How many guns of the seat roll SPACE CANNON at the activation: another player has ships in
    /// the system, and the guns are there or reach it.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub cannon: usize,
}

fn is_zero<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// How a ship gets to the active system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MoveFact {
    /// The move uses Gravity Drive: the ship cannot arrive without it.
    pub gravity_drive: bool,
    /// The move exhausts the Ionian Fuel Refinery.
    pub ionian: bool,
    /// The systems that the ship enters, in order. The last one is the active system.
    pub path: Vec<String>,
    /// The gravity rifts that the ship leaves on this path. It rolls once for each.
    pub rifts: Vec<String>,
    /// The wormholes that the path goes through, by kind ("ALPHA", "BETA"): a step between two
    /// systems that are not neighbours.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wormholes: Vec<String>,
}

/// Why a ship cannot move to the active system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Blocked {
    /// The system of the ship has a command token of the seat.
    CommandToken,
    /// No path of the ship reaches the active system.
    Range,
}

/// One ship of the seat outside the active system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ShipFact {
    pub origin: String,
    /// The place of the ship among the units of the seat in the space area of `origin`.
    pub index: usize,
    pub unit: String,
    pub damaged: bool,
    pub capacity: i64,
    /// The ship counts against the fleet pool.
    pub fleet: bool,
    /// The move value of the ship in this activation, without Gravity Drive and the Ionian Fuel
    /// Refinery: a move that uses one of them says so, and has 1 more for each.
    pub move_value: i32,
    /// The ship starts in a nebula: it moves with a value of 1, whatever `move_value` is.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub nebula: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub r#move: Option<MoveFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<Blocked>,
    /// What the ship can load: places in [`MovementFacts::cargo`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub loads: Vec<usize>,
}

/// Units of one kind in one place that a ship can load.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CargoPool {
    pub system: String,
    /// The planet, or `None` for the space area.
    pub source: Option<String>,
    pub unit: String,
    pub damaged: bool,
    pub galvanized: bool,
    pub count: usize,
}

/// Units of one kind in one place that use capacity, in a system that a ship of the movement
/// starts in or passes, that no ship can load: they stay where they are
/// (`ti4_engine::transit::unloadable`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct UnloadableCargo {
    pub system: String,
    /// The planet, or `None` for the space area.
    pub source: Option<String>,
    pub unit: String,
    pub damaged: bool,
    pub count: usize,
    pub reason: Unloadable,
}

/// What the seat may keep in the active system (fleet supply).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FleetSupplyFact {
    pub limit: i64,
    pub charged: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ActivationFacts {
    pub tactic_tokens: i32,
    /// The activation places a command token of the tactic pool. False for a tactical action
    /// that places none, in any system, also one with the seat's token (Thunder's Edge Warfare).
    pub costs_token: bool,
    pub systems: Vec<ReachFact>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MovementFacts {
    pub active: String,
    pub ships: Vec<ShipFact>,
    pub cargo: Vec<CargoPool>,
    /// Gravity Drive can still give one ship of this action its move.
    pub gravity_drive: bool,
    /// The Ionian Fuel Refinery can still give one ship its move.
    pub ionian: bool,
    pub fleet_supply: FleetSupplyFact,
    /// How many ships already moved in this activation.
    pub moved: u32,
    /// The units that use capacity that no ship can load, in the systems that the movement
    /// starts in or passes, outside the active system.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unloadable: Vec<UnloadableCargo>,
    /// The space combat that ships moving in start: set only when another seat has ships in
    /// the active system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub battle: Option<BattleFacts>,
    /// The guns that roll in the space cannon offense after the movement, of every seat, in the
    /// order in which they roll, as the state is before a ship moves. Empty: no gun fires.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cannon: Vec<CannonFact>,
}

/// One gun of the space cannon offense of the active system: a unit in the system or one whose
/// SPACE CANNON reaches it, or the SPACE CANNON of an attachment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CannonFact {
    pub seat: String,
    /// The unit, by type id; `None` for an attachment, which names its `planet`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unit: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planet: Option<String>,
    pub dice: usize,
    pub hits_on: u32,
    /// The seat whose ships the gun fires at: the seat that moves, or for its own guns the seat
    /// with ships in the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// One side of a space combat in the active system as the state has it, before a ship moves
/// in: the ships there by unit id, how many of them are damaged, and the units of the side that
/// fire SPACE CANNON in that system before the combat. The shape of a side of `ti4_calc`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CombatSide {
    pub faction: String,
    /// The ships of the side in the active system.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub units: BTreeMap<String, usize>,
    /// How many of them are damaged.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub damaged: BTreeMap<String, usize>,
    /// The units of the side that fire SPACE CANNON in the active system before the combat.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub guns: BTreeMap<String, usize>,
}

/// The space combat in the active system: the seat that defends, and both sides. The attacker
/// is the seat that moves; the ships it moves in are added by the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BattleFacts {
    /// The seat that defends.
    pub defender: String,
    /// The seat that moves, with what it has there already.
    pub attacker: CombatSide,
    pub defender_side: CombatSide,
}

/// Ground forces of one kind in one place that the seat can land.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LandingPool {
    pub unit: String,
    pub damaged: bool,
    /// Where the units are, when it is not the space area of the active system: a faction lets
    /// them land from a planet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_system: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_planet: Option<String>,
    pub count: usize,
}

/// One planet of the active system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LandingPlanet {
    pub planet: String,
    /// What can land here: places in [`InvasionFacts::forces`]. Empty when nothing can.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lands: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InvasionFacts {
    pub active: String,
    pub forces: Vec<LandingPool>,
    /// Every planet of the active system, in the order of the system.
    pub planets: Vec<LandingPlanet>,
}

/// One kind of unit of a production, also one that the seat cannot produce now (`unavailable`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BuildFact {
    pub unit: String,
    /// The printed cost of one purchase, before a discount.
    pub cost: i64,
    /// How many units one purchase makes: 2 for fighters and infantry.
    pub per: i64,
    /// How many units of this kind are left in the reinforcements. `None`: there is no limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supply: Option<i64>,
    /// The unit is a ship that counts against the fleet pool.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fleet: bool,
    /// The unit uses capacity while it is in a space area: a fighter, a ground force.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub cargo: bool,
    /// The unit is a fighter: in the space area it counts as a fighter of
    /// [`ti4_engine::fleet::FleetCounts`], else as a ground force that is carried.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub fighter: bool,
    /// Where the unit can be placed: `space`, or a planet of the system.
    pub places: Vec<String>,
    /// What the unit hits on in a combat, and with how many dice. `None`: it does not fight.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatFact>,
    /// The move value. 0: the unit does not move by itself.
    #[serde(default)]
    pub move_value: i64,
    /// What the unit carries.
    #[serde(default)]
    pub capacity: i64,
    /// The keyword abilities of the unit, as printed: "SUSTAIN DAMAGE", "BOMBARDMENT 5+",
    /// "PRODUCTION +2".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abilities: Vec<String>,
    /// The special rule that is printed on the unit. `None`: it has none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<String>,
    /// Why the seat cannot produce the unit here now. `None`: it can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable: Option<Unavailable>,
}

/// The combat value of a unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CombatFact {
    pub hits_on: i64,
    pub dice: i64,
}

/// Why a unit cannot be produced: the reason of the production window
/// (`ti4_engine::production::ProductionWindow::withheld`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Unavailable {
    /// The seat does not have the technology that the unit needs.
    Technology,
    /// None is left in the reinforcements.
    Supply,
    /// Ships of another player are in the system.
    Blockade,
    /// The system has no place for the unit.
    NoPlace,
    /// This use of PRODUCTION makes another unit only (Minister of Industry, Freelancers).
    OnlyAnother,
    /// This use of PRODUCTION makes units up to a cost that this unit is above.
    TooCostly,
    /// An ability of the seat bars the unit from a production by an ability.
    Banned,
    /// An ability of the seat bars the producers in the system from making this unit (Mitosis).
    Barred,
    /// The game does not offer the unit, and the question does not say why: it is not one of a
    /// production window.
    NotOffered,
}

/// A planet that can pay resources, with what it is worth to the seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PayingPlanet {
    pub planet: String,
    pub worth: i64,
    /// The kind of value that the planet pays with, when it is not the kind of the bill: its
    /// influence for resources (Archon's Gift, Freelancers). The payment option of the planet is
    /// then `exhaust|<planet>|<face>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub face: Option<String>,
}

/// A limit and what already counts against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct LimitFact {
    pub limit: i64,
    pub used: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ProductionFacts {
    /// The system that produces.
    pub system: String,
    /// The production limit of this use, and the units that it already made.
    pub capacity: LimitFact,
    pub units: Vec<BuildFact>,
    /// What the combined cost of this use is still reduced by (Sarween Tools).
    pub discount: i64,
    /// Resources that are paid and not used yet.
    pub credit: i64,
    /// Every unit of this use costs nothing.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub free: bool,
    /// The ready planets of the seat that pay resources, each with its best face.
    pub planets: Vec<PayingPlanet>,
    pub trade_goods: i64,
    /// Commodities that the seat may spend as trade goods in this payment (the Keleres agent).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub commodities: i64,
    /// What one trade good pays.
    pub trade_good_worth: i64,
    pub fleet_supply: FleetSupplyFact,
    /// The capacity of the ships of the seat in the system, and what uses it.
    pub transport: LimitFact,
    /// What the fleet supply and the capacity in the system are read from, before the build.
    /// A fighter that the seat produces counts as its own kind (Fighter II).
    pub fleet: ti4_engine::fleet::FleetCounts,
}

/// The facts of the pending choice of a tactical action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum TacticalFacts {
    Activation(ActivationFacts),
    Movement(MovementFacts),
    Invasion(InvasionFacts),
    Production(ProductionFacts),
}

/// The facts for `choice`, when it is the choice of a system to activate, of a ship to move, of
/// a ground force to land or of a unit to produce.
#[must_use]
pub fn project_tactical_facts(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    choice: &Choice,
) -> Option<TacticalFacts> {
    let player = &choice.player;
    match choice.context.as_ref()?.subtype.as_str() {
        ACTIVATION_SUBTYPE | FREE_ACTIVATION_SUBTYPE => Some(TacticalFacts::Activation(
            activation(state, content, sources, galaxy, player, choice),
        )),
        MOVEMENT_SUBTYPE => {
            movement(state, content, sources, galaxy, player).map(TacticalFacts::Movement)
        }
        INVASION_SUBTYPE => {
            invasion(state, content, sources, player, choice).map(TacticalFacts::Invasion)
        }
        PRODUCTION_SUBTYPE => {
            production(state, content, sources, player, choice).map(TacticalFacts::Production)
        }
        _ => None,
    }
}

fn activation(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
    choice: &Choice,
) -> ActivationFacts {
    let systems = choice
        .options
        .iter()
        .filter(|option| option.kind == ACTIVATE_KIND)
        .map(|option| {
            let system = SystemId::new(option.id.clone());
            let ships = movable_into(state, content, sources, galaxy, player, &system);
            let origins: BTreeSet<&SystemId> = ships.iter().map(|ship| &ship.origin).collect();
            ReachFact {
                system: option.id.clone(),
                ships: ships.len(),
                origins: origins.len(),
                production: ti4_engine::production::capacity(
                    state, content, sources, player, &system,
                ),
                cannon: cannon(state, content, sources, galaxy, player, &system),
            }
        })
        .collect();
    ActivationFacts {
        tactic_tokens: state.player(player).map_or(0, |seat| seat.tactic_tokens),
        costs_token: choice
            .context
            .as_ref()
            .is_none_or(|context| context.subtype != FREE_ACTIVATION_SUBTYPE),
        systems,
    }
}

/// The guns of `player` that roll in the space cannon offense of `system`, if the seat activates it.
fn cannon(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
    system: &SystemId,
) -> usize {
    let types = catalogue(content, sources);
    let guns = ti4_engine::combat::space_cannon_guns(
        state,
        content,
        sources,
        system,
        player,
        Some(galaxy),
    );
    let units = guns.units.iter().filter(|unit| {
        &unit.owner == player
            && types.get(unit.type_id.as_str()).is_some_and(|kind| {
                kind.space_cannon_hits_on().is_some() && kind.space_cannon_dice() > 0
            })
    });
    let attachments = guns
        .attachments
        .iter()
        .filter(|(owner, _, _, dice)| owner == player && *dice > 0);
    units.count() + attachments.count()
}

/// The key of a pool: where the units are and what makes two of them the same for the game.
type PoolKey = (String, Option<String>, String, bool, bool);

fn pool_count(state: &GameState, player: &PlayerId, key: &PoolKey) -> usize {
    let (system, source, unit, damaged, galvanized) = key;
    let Some(system) = state.board.get(&SystemId::new(system.clone())) else {
        return 0;
    };
    let same = |candidate: &&Unit| {
        &candidate.owner == player
            && candidate.type_id.as_str() == unit
            && candidate.sustained_damage == *damaged
            && candidate.galvanized == *galvanized
    };
    match source {
        None => system.units.iter().filter(same).count(),
        Some(planet) => system
            .planet_units
            .iter()
            .filter(|(id, _)| id.as_str() == planet)
            .flat_map(|(_, units)| units)
            .filter(same)
            .count(),
    }
}

/// What the hold of `ship` is offered on `path`: the `load` options of the engine's own window.
fn pools_of(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    origin: &SystemId,
    ship: &Unit,
    path: &[String],
) -> Vec<PoolKey> {
    let hold = CargoWindow::for_ship(state, content, sources, player, origin, ship, path);
    let Some(choice) = hold.pending_choice() else {
        return Vec::new();
    };
    choice
        .options
        .iter()
        .filter(|option| option.kind == LOAD_KIND)
        .filter_map(|option| {
            let text = |key: &str| option.payload.get(key).and_then(serde_json::Value::as_str);
            let flag = |key: &str| {
                option
                    .payload
                    .get(key)
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
            };
            Some((
                text("pickup_system")?.to_owned(),
                text("source").map(str::to_owned),
                text("unit")?.to_owned(),
                flag("damaged"),
                flag("galvanized"),
            ))
        })
        .collect()
}

fn movement(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
) -> Option<MovementFacts> {
    let active = state.active_system.clone()?;
    let types = catalogue(content, sources);
    let mut rules = MovementRules::with_laws(
        galaxy,
        content,
        sources,
        active.as_str(),
        Board::for_player(state, content, sources, player),
        Some(state),
    );
    ti4_engine::action_cards::apply_movement_effects(&mut rules, state, player);
    let rolls = !rules.anomalies_ignored && !rules.rifts_ignored;

    let movable: BTreeMap<(SystemId, usize), Movable> =
        movable_into(state, content, sources, galaxy, player, &active)
            .into_iter()
            .map(|ship| ((ship.origin.clone(), ship.index), ship))
            .collect();

    let mut pools: Vec<PoolKey> = Vec::new();
    let mut ships = Vec::new();
    let mut routes: Vec<(SystemId, Unit, Vec<String>)> = Vec::new();
    for origin in state.systems_with_units_of(player) {
        if *origin == active {
            continue;
        }
        for (index, hull) in state.ships_of(player, origin).into_iter().enumerate() {
            let Some(kind) = types.get(hull.type_id.as_str()) else {
                continue;
            };
            if !kind.moves_as_ship() {
                continue;
            }
            let found = movable.get(&(origin.clone(), index));
            // A fighter that does not move on its own is cargo, not a ship that stays behind.
            if found.is_none() && kind.consumes_capacity() {
                continue;
            }
            // The route of the offer: the same value and the same type as `begin_one_move`.
            let path = found.and_then(|found| {
                let value = effective_move_value_for_ship(
                    state,
                    kind,
                    player,
                    origin,
                    Some(index),
                    found.gravity_drive,
                    found.ionian,
                );
                rules
                    .path_from_ship(origin.as_str(), value, Some(hull.type_id.as_str()))
                    .map(|path| (found, path))
            });
            let mut loads = Vec::new();
            let r#move = path.map(|(found, path)| {
                routes.push((origin.clone(), hull.clone(), path.clone()));
                for key in pools_of(state, content, sources, player, origin, hull, &path) {
                    let place = pools.iter().position(|known| *known == key);
                    loads.push(place.unwrap_or_else(|| {
                        pools.push(key);
                        pools.len() - 1
                    }));
                }
                let mut wormholes: Vec<String> = Vec::new();
                for step in path.windows(2) {
                    for kind in rules.wormholes_between(&step[0], &step[1]) {
                        if !wormholes.contains(&kind) {
                            wormholes.push(kind);
                        }
                    }
                }
                MoveFact {
                    gravity_drive: found.gravity_drive,
                    ionian: found.ionian,
                    rifts: if rolls {
                        rifts_exited(&rules, &path)
                    } else {
                        Vec::new()
                    },
                    wormholes,
                    path,
                }
            });
            let blocked = r#move.is_none().then(|| {
                if rules.may_depart(origin.as_str()) {
                    Blocked::Range
                } else {
                    Blocked::CommandToken
                }
            });
            ships.push(ShipFact {
                origin: origin.to_string(),
                index,
                unit: hull.type_id.to_string(),
                damaged: hull.sustained_damage,
                capacity: kind.capacity(),
                fleet: !kind.is_fighter(),
                move_value: effective_move_value_for_ship(
                    state,
                    kind,
                    player,
                    origin,
                    Some(index),
                    false,
                    false,
                ),
                nebula: rules.nebula_caps(origin.as_str()),
                r#move,
                blocked,
                loads,
            });
        }
    }

    let routes: Vec<Route<'_>> = routes
        .iter()
        .map(|(origin, ship, path)| Route { origin, ship, path })
        .collect();
    let mut unloadable: Vec<UnloadableCargo> = Vec::new();
    for cargo in ti4_engine::transit::unloadable(state, content, sources, player, &active, &routes)
    {
        let fact = UnloadableCargo {
            system: cargo.system.to_string(),
            source: match cargo.source {
                CargoSource::Space => None,
                CargoSource::Planet(planet) => Some(planet.to_string()),
            },
            unit: cargo.unit.type_id.to_string(),
            damaged: cargo.unit.sustained_damage,
            count: cargo.count,
            reason: cargo.reason,
        };
        // The facts do not tell a galvanized unit from another.
        match unloadable.iter_mut().find(|known| {
            known.system == fact.system
                && known.source == fact.source
                && known.unit == fact.unit
                && known.damaged == fact.damaged
        }) {
            Some(known) => {
                known.count += fact.count;
                known.reason = known.reason.min(fact.reason);
            }
            None => unloadable.push(fact),
        }
    }
    let standing = ti4_engine::fleet::standing(state, content, sources, player, &active, None);
    let battle = battle(state, content, sources, galaxy, player, &active);
    Some(MovementFacts {
        active: active.to_string(),
        ships,
        cargo: pools
            .into_iter()
            .map(|key| CargoPool {
                count: pool_count(state, player, &key),
                system: key.0,
                source: key.1,
                unit: key.2,
                damaged: key.3,
                galvanized: key.4,
            })
            .collect(),
        gravity_drive: ti4_engine::technology::gravity_drive_available(state, player),
        ionian: ti4_engine::legendary::ionian_available(state, player),
        fleet_supply: FleetSupplyFact {
            limit: standing.fleet_limit,
            charged: standing.fleet_charged,
        },
        moved: state
            .faction_marks
            .get(&format!("private:#moved:{}", state.activation_seq))
            .and_then(|count| count.parse().ok())
            .unwrap_or(0),
        unloadable,
        battle,
        cannon: ti4_engine::combat::space_cannon_shots(
            state,
            content,
            sources,
            &active,
            player,
            Some(galaxy),
        )
        .into_iter()
        .map(|shot| CannonFact {
            seat: shot.owner.to_string(),
            unit: shot.unit.map(|unit| unit.type_id.to_string()),
            planet: shot.planet.map(|planet| planet.to_string()),
            dice: shot.dice,
            hits_on: shot.hits_on,
            target: shot.target.map(|seat| seat.to_string()),
        })
        .collect(),
    })
}

/// The space combat in `active` if `player` moves in: the defender is the seat that the space
/// cannon offense of the engine fires at.
fn battle(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    galaxy: &Galaxy,
    player: &PlayerId,
    active: &SystemId,
) -> Option<BattleFacts> {
    let defender =
        ti4_engine::combat::opponent_with_ships(state, content, sources, player, active)?;
    let side = |seat: &PlayerId| {
        let gathered =
            ti4_calc::odds::gather_side(content, sources, state, Some(active), Some(galaxy), seat)
                .ok()?;
        let tally = |counts: Option<ti4_calc::odds::UnitCounts>| {
            let mut tally: BTreeMap<String, usize> = BTreeMap::new();
            for (unit, count) in counts
                .map(ti4_calc::odds::UnitCounts::into_entries)
                .into_iter()
                .flatten()
            {
                if count > 0 {
                    *tally.entry(unit).or_default() += count;
                }
            }
            tally
        };
        Some(CombatSide {
            faction: gathered.faction?,
            units: tally(gathered.units),
            damaged: tally(gathered.damaged),
            guns: tally(gathered.guns),
        })
    };
    Some(BattleFacts {
        attacker: side(player)?,
        defender_side: side(&defender)?,
        defender: defender.to_string(),
    })
}

/// What the seat can land in the active system: the `commit` options of the choice, by the kind
/// of unit and the place where it is, with how many units of the seat are there.
fn invasion(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    choice: &Choice,
) -> Option<InvasionFacts> {
    let active = state.active_system.clone()?;
    let mut forces: Vec<LandingPool> = Vec::new();
    let mut planets: Vec<LandingPlanet> =
        ti4_engine::planets::in_system(state, content, sources, &active)
            .into_iter()
            .map(|planet| LandingPlanet {
                planet: planet.to_string(),
                lands: Vec::new(),
            })
            .collect();
    for option in &choice.options {
        if option.kind != COMMIT_KIND {
            continue;
        }
        let text = |key: &str| option.payload.get(key).and_then(serde_json::Value::as_str);
        let (Some(planet), Some(unit)) = (text("planet"), text("unit")) else {
            continue;
        };
        let damaged = option
            .payload
            .get("damaged")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let from = text("from_system").zip(text("from_planet"));
        let same = |candidate: &&Unit| {
            &candidate.owner == player
                && candidate.type_id.as_str() == unit
                && candidate.sustained_damage == damaged
        };
        let pool = LandingPool {
            unit: unit.to_owned(),
            damaged,
            from_system: from.map(|(system, _)| system.to_owned()),
            from_planet: from.map(|(_, planet)| planet.to_owned()),
            count: match from {
                None => state
                    .system_state(&active)
                    .units
                    .iter()
                    .filter(same)
                    .count(),
                Some((system, planet)) => state
                    .system_state(&SystemId::new(system))
                    .on_planet(&PlanetId::new(planet))
                    .iter()
                    .filter(same)
                    .count(),
            },
        };
        let place = forces
            .iter()
            .position(|known| {
                (
                    &known.unit,
                    known.damaged,
                    &known.from_system,
                    &known.from_planet,
                ) == (
                    &pool.unit,
                    pool.damaged,
                    &pool.from_system,
                    &pool.from_planet,
                )
            })
            .unwrap_or_else(|| {
                forces.push(pool);
                forces.len() - 1
            });
        // A planet that was placed during play may be missing from the list of the system.
        let at = planets
            .iter()
            .position(|known| known.planet == planet)
            .unwrap_or_else(|| {
                planets.push(LandingPlanet {
                    planet: planet.to_owned(),
                    lands: Vec::new(),
                });
                planets.len() - 1
            });
        planets[at].lands.push(place);
    }
    Some(InvasionFacts {
        active: active.to_string(),
        forces,
        planets,
    })
}

/// The order of the units of a production on a sheet.
const SHEET_ORDER: [&str; 9] = [
    "carrier",
    "cruiser",
    "destroyer",
    "dreadnought",
    "flagship",
    "warsun",
    "fighter",
    "infantry",
    "mech",
];

/// One unit of a production: its price, its places and what is printed on it.
#[allow(
    clippy::too_many_arguments,
    reason = "the game, the seat, and what the caller already knows of the unit"
)]
fn build_fact(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    kind: &ti4_content::units::UnitType<'_>,
    (cost, per): (i64, i64),
    places: Vec<String>,
    unavailable: Option<Unavailable>,
) -> BuildFact {
    // "BOMBARDMENT 5+", and "BOMBARDMENT 3+ (x3)" with more than one die: as the dice are read.
    let rolls = |name: &str, hits_on: Option<i64>, dice: i64| {
        hits_on.map(|value| {
            if dice > 1 {
                format!("{name} {value}+ (x{dice})")
            } else {
                format!("{name} {value}+")
            }
        })
    };
    let abilities = [
        kind.sustain_damage().then(|| "SUSTAIN DAMAGE".to_owned()),
        rolls("ANTI-FIGHTER BARRAGE", kind.afb_hits_on(), kind.afb_dice()),
        rolls("BOMBARDMENT", kind.bombard_hits_on(), kind.bombard_dice()),
        rolls(
            "SPACE CANNON",
            kind.space_cannon_hits_on(),
            kind.space_cannon_dice(),
        ),
        kind.planetary_shield()
            .then(|| "PLANETARY SHIELD".to_owned()),
        kind.production_text()
            .map(|value| format!("PRODUCTION {value}")),
    ];
    BuildFact {
        unit: kind.id().to_owned(),
        cost,
        per,
        supply: ti4_engine::supply::plastic(kind.base_type()).map(|_| {
            ti4_engine::supply::remaining(
                state,
                content,
                sources,
                player,
                &ti4_model::id::UnitTypeId::new(kind.id()),
            )
        }),
        fleet: kind.is_ship() && !kind.is_fighter(),
        cargo: kind.consumes_capacity(),
        fighter: kind.is_fighter(),
        places,
        combat: kind.combat_hits_on().map(|hits_on| CombatFact {
            hits_on,
            dice: kind.combat_dice().max(1),
        }),
        move_value: kind.move_value(),
        capacity: kind.capacity(),
        abilities: abilities.into_iter().flatten().collect(),
        rules: kind.ability_text().map(str::to_owned),
        unavailable,
    }
}

/// What the seat can produce in the system of the choice: the `produce` options, with what a
/// client needs to stage several of them and their payment before it sends any. `None` when a
/// unit has places that this function does not know (a production by an ability).
fn production(
    state: &GameState,
    content: &ContentStore,
    sources: SourceSet,
    player: &PlayerId,
    choice: &Choice,
) -> Option<ProductionFacts> {
    let context = choice.context.as_ref()?;
    let Some(DecisionTarget::System(system)) = &context.target else {
        return None;
    };
    let limit = context
        .outstanding
        .iter()
        .find(|owed| owed.kind == ConstraintKind::ProductionCapacity)?;
    let types = catalogue(content, sources);
    let builds: Vec<_> = choice
        .options
        .iter()
        .filter(|option| option.kind == PRODUCE_KIND && option.payload.get("exchange").is_none())
        .collect();
    let number = |option: &ti4_engine::choice::ChoiceOption, key: &str| {
        option.payload.get(key).and_then(serde_json::Value::as_i64)
    };
    let mut units = Vec::new();
    for option in &builds {
        let unit = option.payload.get("unit")?.as_str()?;
        let kind = types.get(unit)?;
        let places = match option.payload.get("destination") {
            Some(only) => vec![only.as_str()?.to_owned()],
            None => {
                let places = ti4_engine::production::placements(
                    state, content, sources, player, system, kind,
                );
                let pending = usize::try_from(number(option, "placement_pending")?).ok()?;
                if places.len() != pending {
                    return None;
                }
                places
            }
        };
        units.push(build_fact(
            state,
            content,
            sources,
            player,
            kind,
            (number(option, "printed_cost")?, number(option, "yield")?),
            places,
            None,
        ));
    }
    // Before the first unit the discount of this use is what the game keeps for it. After that
    // the options say what is left of it for one more purchase.
    let discount = if limit.paid == 0 {
        i64::from(state.production_discount_remaining)
            + ti4_engine::factions::hooks_economy::production_cost_reduction(state, player, system)
    } else {
        builds
            .iter()
            .filter_map(|option| number(option, "discount"))
            .max()
            .unwrap_or(0)
    };
    let credit = builds
        .first()
        .and_then(|option| number(option, "credit"))
        .unwrap_or(0);
    // Every other unit that a production makes, with why the game does not offer it
    // (`ProductionWindow::withheld`). A unit that the seat only cannot afford now, or that the
    // limit of this use leaves out, has no reason: these are limits that the client shows.
    // A kind that the window does not name was never considered.
    let withheld: Vec<WithheldUnit> = choice.hint(WITHHELD_KEY).unwrap_or_default();
    let buildable = ti4_engine::production::buildable_for(state, content, sources, player);
    for base in ti4_engine::production::BUILDABLE {
        let of_base = |unit: &str| types.get(unit).is_some_and(|kind| kind.base_type() == base);
        if units.iter().any(|unit| of_base(&unit.unit)) {
            continue;
        }
        let (kind, unavailable) = match withheld.iter().find(|known| of_base(&known.unit)) {
            Some(known) => (
                types.get(known.unit.as_str()),
                match known.reason {
                    Withheld::Technology => Some(Unavailable::Technology),
                    Withheld::OnlyAnother => Some(Unavailable::OnlyAnother),
                    Withheld::TooCostly => Some(Unavailable::TooCostly),
                    Withheld::Banned => Some(Unavailable::Banned),
                    Withheld::Blockade => Some(Unavailable::Blockade),
                    Withheld::NoPlace => Some(Unavailable::NoPlace),
                    Withheld::Barred => Some(Unavailable::Barred),
                    Withheld::Supply => Some(Unavailable::Supply),
                    Withheld::Unaffordable | Withheld::Limit => None,
                },
            ),
            // A unit behind a technology is shown as its printed generic form.
            None => (
                buildable
                    .iter()
                    .filter_map(|id| types.get(id.as_str()))
                    .find(|kind| kind.base_type() == base)
                    .or_else(|| types.get(base)),
                Some(Unavailable::NotOffered),
            ),
        };
        let Some(kind) = kind else {
            continue;
        };
        let (cost, per) = ti4_engine::production::price_of_under(Some(state), kind);
        let places =
            ti4_engine::production::placements(state, content, sources, player, system, kind);
        units.push(build_fact(
            state,
            content,
            sources,
            player,
            kind,
            (cost, i64::try_from(per).unwrap_or(1)),
            places,
            unavailable,
        ));
    }
    // The ships, then the fighters, then the ground forces.
    let rank = |unit: &BuildFact| {
        let base = types
            .get(unit.unit.as_str())
            .map_or("", ti4_content::units::UnitType::base_type);
        SHEET_ORDER
            .iter()
            .position(|kind| *kind == base)
            .unwrap_or(SHEET_ORDER.len())
    };
    units.sort_by_key(rank);
    let standing = ti4_engine::fleet::standing(state, content, sources, player, system, None);
    let mut fleet = ti4_engine::fleet::counts(state, content, sources, player, system);
    // A fighter that the seat produces brings its own rule (Fighter II) to the fighters there.
    for unit in &units {
        if let Some(kind) = types
            .get(unit.unit.as_str())
            .filter(|kind| kind.is_fighter())
        {
            let (to_pool, half) = ti4_engine::fleet::fighter_rules(state, content, player, kind);
            fleet.fighters_to_fleet_pool |= to_pool;
            fleet.half_fighters |= half;
        }
    }
    Some(ProductionFacts {
        system: system.to_string(),
        capacity: LimitFact {
            limit: limit.amount,
            used: limit.paid,
        },
        units,
        discount,
        credit,
        free: builds.iter().any(|option| {
            option
                .payload
                .get("free_this_use")
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        }),
        planets: ti4_engine::production::best_payment_faces(
            state,
            content,
            sources,
            player,
            Spend::Resources,
        )
        .into_iter()
        .map(|(planet, worth, face)| PayingPlanet {
            planet: planet.to_string(),
            worth,
            face: (face != Spend::Resources)
                .then(|| ti4_engine::production::spend_name(face).to_owned()),
        })
        .collect(),
        commodities: ti4_engine::factions::keleres::spendable_commodities(state, player),
        trade_goods: state
            .player(player)
            .map_or(0, |seat| i64::from(seat.trade_goods)),
        trade_good_worth: ti4_engine::production::trade_good_worth(state, player),
        fleet_supply: FleetSupplyFact {
            limit: standing.fleet_limit,
            charged: standing.fleet_charged,
        },
        transport: LimitFact {
            limit: standing.transport,
            used: standing.consumed,
        },
        fleet,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::ChoiceOption;
    use ti4_engine::decision_context::{DecisionContext, DecisionSource};
    use ti4_engine::tactical::activation_options_with;
    use ti4_model::content_types::POK;
    use ti4_model::id::UnitTypeId;
    use ti4_model::state::Phase;

    fn seat() -> PlayerId {
        PlayerId::new("a")
    }

    /// A game of six on the recommended map, before the first action.
    fn game() -> (GameState, Galaxy) {
        let content = ContentStore::embedded();
        let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
            .iter()
            .map(|name| PlayerId::new(*name))
            .collect();
        let loader = crate::maps::TemplateLoader::load().expect("the templates load");
        let template = crate::maps::default_template_for(content, &loader, 6, POK)
            .expect("a template for six");
        let (mut state, galaxy) =
            crate::map::create_game_with_template(content, &ids, 3, Some(&template))
                .expect("a game");
        state.phase = Phase::Action;
        (state, galaxy)
    }

    fn movement_choice() -> Choice {
        Choice::new(seat(), "movement", vec![ChoiceOption::decline()]).contextualized(
            DecisionContext::new(
                seat(),
                DecisionSource::Rule("89.2".to_owned()),
                MOVEMENT_SUBTYPE,
                Phase::Action,
                1,
            ),
        )
    }

    fn facts_into(state: &mut GameState, galaxy: &Galaxy, active: &str) -> MovementFacts {
        state.active_system = Some(SystemId::new(active));
        match project_tactical_facts(
            state,
            ContentStore::embedded(),
            POK,
            galaxy,
            &movement_choice(),
        ) {
            Some(TacticalFacts::Movement(facts)) => facts,
            other => panic!("no movement facts: {other:?}"),
        }
    }

    /// The system where the seat has its ships at the start.
    fn home(state: &GameState) -> SystemId {
        state
            .systems_with_units_of(&seat())
            .into_iter()
            .next()
            .expect("a home system")
            .clone()
    }

    #[test]
    fn only_the_choices_of_a_tactical_action_have_facts() {
        let (state, galaxy) = game();
        let content = ContentStore::embedded();
        let other = Choice::new(seat(), "action phase", vec![ChoiceOption::decline()])
            .contextualized(DecisionContext::new(
                seat(),
                DecisionSource::Rule("22".to_owned()),
                "action_menu",
                Phase::Action,
                1,
            ));
        assert_eq!(
            project_tactical_facts(&state, content, POK, &galaxy, &other),
            None
        );
        let bare = Choice::new(seat(), "movement", vec![ChoiceOption::decline()]);
        assert_eq!(
            project_tactical_facts(&state, content, POK, &galaxy, &bare),
            None
        );
    }

    #[test]
    fn activation_counts_the_ships_that_the_movement_then_lists() {
        let (mut state, galaxy) = game();
        let content = ContentStore::embedded();
        let choice =
            activation_options_with(&state, content, POK, &galaxy, &seat()).expect("offered");
        let Some(TacticalFacts::Activation(facts)) =
            project_tactical_facts(&state, content, POK, &galaxy, &choice)
        else {
            panic!("no activation facts");
        };
        assert_eq!(facts.systems.len(), choice.options.len());
        assert!(facts.tactic_tokens > 0);
        assert!(facts.systems.iter().any(|reach| reach.ships > 0));
        for reach in &facts.systems {
            let movement = facts_into(&mut state, &galaxy, &reach.system);
            let moving: Vec<_> = movement
                .ships
                .iter()
                .filter(|ship| ship.r#move.is_some())
                .collect();
            assert_eq!(moving.len(), reach.ships, "into {}", reach.system);
            let origins: BTreeSet<_> = moving.iter().map(|ship| &ship.origin).collect();
            assert_eq!(origins.len(), reach.origins, "into {}", reach.system);
        }
    }

    fn reach(state: &GameState, galaxy: &Galaxy, system: &str) -> ReachFact {
        let content = ContentStore::embedded();
        let choice =
            activation_options_with(state, content, POK, galaxy, &seat()).expect("offered");
        let Some(TacticalFacts::Activation(facts)) =
            project_tactical_facts(state, content, POK, galaxy, &choice)
        else {
            panic!("no activation facts");
        };
        facts
            .systems
            .into_iter()
            .find(|reach| reach.system == system)
            .expect("the system can be activated")
    }

    #[test]
    fn activation_says_where_the_seat_produces() {
        let (state, galaxy) = game();
        let home = home(&state);
        assert!(reach(&state, &galaxy, home.as_str()).production > 0);
        let empty = galaxy
            .system_ids()
            .into_iter()
            .find(|id| {
                !state
                    .systems_with_units_of(&seat())
                    .contains(&&SystemId::new(*id))
            })
            .expect("a system without units of the seat");
        assert_eq!(reach(&state, &galaxy, empty).production, 0);
    }

    #[test]
    fn activation_counts_the_guns_that_fire_at_the_ships_of_another_player() {
        let (mut state, galaxy) = game();
        let content = ContentStore::embedded();
        let home = home(&state);
        let next = SystemId::new(
            galaxy
                .adjacent(home.as_str())
                .into_iter()
                .find(|id| !ti4_content::galaxy::is_home_system(content, id, POK))
                .expect("a system next to home"),
        );
        let put = |state: &mut GameState, system: &SystemId, kind: &str, owner: &str| {
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new(kind), PlayerId::new(owner)));
        };
        let guns =
            |state: &GameState, system: &SystemId| reach(state, &galaxy, system.as_str()).cannon;
        let before = guns(&state, &home);
        assert_eq!(before, 0, "no ships to fire at");

        // A gun in the system fires only when another player has ships there.
        put(&mut state, &next, "pds", "a");
        assert_eq!(guns(&state, &next), 0);
        put(&mut state, &next, "cruiser", "b");
        assert_eq!(guns(&state, &next), 1);

        // A gun with the upgrade reaches the systems next to it.
        put(&mut state, &home, "cruiser", "b");
        let near = guns(&state, &home);
        put(&mut state, &next, "pds2", "a");
        assert_eq!(guns(&state, &home), near + 1);
        assert_eq!(guns(&state, &next), 2);
    }

    #[test]
    fn the_movement_names_the_guns_of_another_seat_in_a_system_with_only_planets() {
        let (mut state, galaxy) = game();
        let content = ContentStore::embedded();
        let home = home(&state);
        let (next, planet) = galaxy
            .adjacent(home.as_str())
            .into_iter()
            .filter(|id| !ti4_content::galaxy::is_home_system(content, id, POK))
            .find_map(|id| {
                ti4_content::galaxy::planets_in(content, id, POK)
                    .first()
                    .map(|planet| (SystemId::new(id), PlanetId::new(planet.id())))
            })
            .expect("a system with a planet next to home");
        assert!(
            facts_into(&mut state, &galaxy, next.as_str())
                .cannon
                .is_empty()
        );

        // A PDS of another seat on a planet, with no ships of that seat: it fires at the fleet.
        state
            .system_mut(&next)
            .planet_units
            .entry(planet)
            .or_default()
            .push(Unit::new(UnitTypeId::new("pds"), PlayerId::new("b")));
        let facts = facts_into(&mut state, &galaxy, next.as_str());
        assert!(facts.battle.is_none(), "no ships to fight");
        assert_eq!(
            facts.cannon,
            vec![CannonFact {
                seat: "b".to_owned(),
                unit: Some("pds".to_owned()),
                planet: None,
                dice: 1,
                hits_on: 6,
                target: Some("a".to_owned()),
            }]
        );

        // A PDS II next door reaches the active system; a PDS does not.
        let far = galaxy
            .adjacent(next.as_str())
            .into_iter()
            .find(|id| *id != home.as_str())
            .map(SystemId::new)
            .expect("a neighbour");
        for kind in ["pds", "pds2"] {
            state
                .system_mut(&far)
                .units
                .push(Unit::new(UnitTypeId::new(kind), PlayerId::new("c")));
        }
        let guns = facts_into(&mut state, &galaxy, next.as_str()).cannon;
        let units: Vec<(&str, Option<&str>)> = guns
            .iter()
            .map(|gun| (gun.seat.as_str(), gun.unit.as_deref()))
            .collect();
        assert_eq!(units, vec![("b", Some("pds")), ("c", Some("pds2"))]);
    }

    #[test]
    fn a_ship_behind_a_command_token_or_out_of_range_says_why() {
        let (mut state, galaxy) = game();
        let home = home(&state);
        let reach = |state: &mut GameState, active: &str| {
            let facts = facts_into(state, &galaxy, active);
            let moving = facts.ships.iter().filter(|s| s.r#move.is_some()).count();
            (facts, moving)
        };
        let targets: Vec<String> = galaxy.system_ids().into_iter().map(str::to_owned).collect();
        let near = targets
            .iter()
            .find(|id| id.as_str() != home.as_str() && reach(&mut state, id).1 > 0)
            .expect("a system in range")
            .clone();
        let far = targets
            .iter()
            .find(|id| id.as_str() != home.as_str() && reach(&mut state, id).1 == 0)
            .expect("a system out of range");

        let (facts, _) = reach(&mut state, far);
        assert!(!facts.ships.is_empty());
        for ship in &facts.ships {
            assert_eq!(ship.blocked, Some(Blocked::Range), "{ship:?}");
            assert!(ship.loads.is_empty());
        }
        // Units that only ride are cargo, not ships that stay behind.
        assert!(facts.ships.iter().all(|ship| ship.unit != "fighter"));
        assert!(facts.cargo.is_empty());

        state.system_mut(&home).place_token(seat());
        let (facts, moving) = reach(&mut state, &near);
        assert_eq!(moving, 0);
        for ship in &facts.ships {
            assert_eq!(ship.blocked, Some(Blocked::CommandToken), "{ship:?}");
        }
    }

    #[test]
    fn every_ship_has_its_move_value_and_a_nebula_caps_it() {
        let content = ContentStore::embedded();
        let is_nebula = |id: &str| {
            ti4_content::galaxy::system(content, id, POK).is_some_and(|system| system.is_nebula())
        };
        // The map of the template has no nebula: one takes the place of a system next to home.
        let (mut state, mut galaxy) = game();
        let active = home(&state);
        let nebula = ti4_content::galaxy::all_systems(content, POK)
            .into_keys()
            .find(|id| is_nebula(id))
            .expect("a nebula")
            .to_owned();
        let old = galaxy
            .adjacent(active.as_str())
            .into_iter()
            .find(|id| !ti4_content::galaxy::is_home_system(content, id, POK))
            .expect("a system next to home")
            .to_owned();
        galaxy
            .replace_system(content, &old, &nebula, POK)
            .expect("the nebula is placed");
        // A carrier and its upgrade in every system, so that some start in the nebula.
        let systems: Vec<SystemId> = galaxy.system_ids().into_iter().map(SystemId::new).collect();
        for system in &systems {
            for kind in ["carrier", "carrier2"] {
                state
                    .system_mut(system)
                    .units
                    .push(Unit::new(UnitTypeId::new(kind), seat()));
            }
        }
        let facts = facts_into(&mut state, &galaxy, active.as_str());
        let mut capped = 0;
        for ship in &facts.ships {
            match ship.unit.as_str() {
                "carrier" => assert_eq!(ship.move_value, 1, "{ship:?}"),
                "carrier2" => assert_eq!(ship.move_value, 2, "{ship:?}"),
                _ => {}
            }
            assert_eq!(ship.nebula, is_nebula(&ship.origin), "{ship:?}");
            if let (true, Some(route)) = (ship.nebula, &ship.r#move) {
                // With a value of 1 the path is the nebula and the active system, no other.
                assert_eq!(route.path.len(), 2, "{ship:?}");
            }
            capped += usize::from(ship.nebula);
        }
        assert!(capped > 0, "no ship starts in the nebula");
    }

    #[test]
    fn every_move_ends_in_the_active_system_and_loads_what_is_on_its_path() {
        let (mut state, galaxy) = game();
        // Infantry of the seat in the space area of every system, so that a ship passes some.
        let systems: Vec<SystemId> = galaxy.system_ids().into_iter().map(SystemId::new).collect();
        for system in &systems {
            state
                .system_mut(system)
                .units
                .push(Unit::new(UnitTypeId::new("infantry"), seat()));
        }
        // One more carrier, and Gravity Drive, with which a carrier passes a system.
        let home = home(&state);
        state
            .system_mut(&home)
            .units
            .push(Unit::new(UnitTypeId::new("carrier"), seat()));
        if let Some(seat) = state.player_mut(&seat()) {
            seat.technologies
                .insert(ti4_model::id::TechnologyId::new("gd"));
        }
        let (mut on_the_way, mut boosted, mut rifts) = (0, 0, 0);
        let targets: Vec<String> = galaxy.system_ids().into_iter().map(str::to_owned).collect();
        for active in &targets {
            let facts = facts_into(&mut state, &galaxy, active);
            assert_eq!(&facts.active, active);
            assert!(facts.gravity_drive);
            for pool in &facts.cargo {
                assert!(pool.count > 0, "{pool:?}");
            }
            for ship in &facts.ships {
                assert_ne!(&ship.origin, active);
                assert_ne!(ship.r#move.is_some(), ship.blocked.is_some(), "{ship:?}");
                let Some(route) = &ship.r#move else {
                    continue;
                };
                assert_eq!(route.path.last(), Some(active), "{ship:?}");
                boosted += usize::from(route.gravity_drive);
                rifts += route.rifts.len();
                for rift in &route.rifts {
                    assert!(route.path[..route.path.len() - 1].contains(rift));
                }
                assert_eq!(ship.loads.is_empty(), ship.capacity == 0, "{ship:?}");
                for place in &ship.loads {
                    let pool = &facts.cargo[*place];
                    assert!(
                        pool.system == ship.origin || route.path.contains(&pool.system),
                        "{pool:?} is not on the path of {ship:?}"
                    );
                    on_the_way +=
                        usize::from(pool.system != ship.origin && pool.system != facts.active);
                }
            }
        }
        assert!(on_the_way > 0, "no ship passed a system with units");
        assert!(boosted > 0, "no ship needed Gravity Drive");
        // The map of this seed may have no rift in reach: the count is only checked for shape.
        let _ = rifts;
    }

    #[test]
    fn the_facts_are_tagged_by_kind_on_the_wire() {
        let (mut state, galaxy) = game();
        let home = home(&state);
        let facts = facts_into(&mut state, &galaxy, home.as_str());
        let value =
            serde_json::to_value(TacticalFacts::Movement(facts.clone())).expect("it serializes");
        assert_eq!(value["kind"], "movement");
        assert_eq!(value["active"], home.as_str());
        assert_eq!(value["fleet_supply"]["limit"], facts.fleet_supply.limit);
        let back: TacticalFacts = serde_json::from_value(value).expect("it reads back");
        assert_eq!(back, TacticalFacts::Movement(facts));
    }
}
