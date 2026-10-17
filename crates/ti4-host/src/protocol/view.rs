//! Redacted wire projections of game state.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use ti4_model::id::{
    ActionCardId, FactionId, LeaderId, ObjectiveId, PlanetId, PlayerId, RelicId, SecretObjectiveId,
    StrategyCardId, SystemId, TechnologyId, UnitTypeId,
};
use ti4_model::state::{LeaderStatus, Phase};

/// Redacted view of a single player at the table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlayerView {
    pub id: PlayerId,
    pub faction: FactionId,
    pub victory_points: i32,
    pub trade_goods: i32,
    pub commodities: i32,
    pub tactic_tokens: i32,
    pub fleet_tokens: i32,
    pub strategic_tokens: i32,
    pub passed: bool,
    pub strategy_cards: Vec<StrategyCardId>,
    pub exhausted_strategy_cards: BTreeSet<StrategyCardId>,
    pub technologies: BTreeSet<TechnologyId>,
    pub exhausted_technologies: BTreeSet<TechnologyId>,
    pub relics: Vec<RelicId>,
    pub exhausted_relics: BTreeSet<RelicId>,
    /// The legendary planets whose ability card is exhausted. The card exhausts apart from
    /// the planet.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub exhausted_legendary: BTreeSet<PlanetId>,
    /// Relic fragments by deck, faceup in the play area.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub relic_fragments: BTreeMap<String, i32>,
    /// Exploration cards that stay faceup in the play area.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exploration_cards: Vec<String>,
    /// Public count of action cards in hand.
    pub action_cards_count: usize,
    /// Public count of unscored secret objectives held.
    pub secret_objectives_count: usize,
    /// Only present and populated if the viewer IS this player. Strictly empty for opponents/spectators.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_action_cards: Vec<ActionCardId>,
    /// Only present and populated if the viewer IS this player or if revealed by law. Strictly empty otherwise.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_secret_objectives: Vec<SecretObjectiveId>,
    /// Publicly scored secret objectives.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scored_secret_objectives: Vec<SecretObjectiveId>,
    /// Exact progress toward this seat's own held secrets with a countable requirement.
    /// Display-only, and present only in the view of the seat that holds them: opponents and
    /// spectators never receive it, not even for secrets a law has turned face up.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub secret_objective_progress: BTreeMap<SecretObjectiveId, ObjectiveProgressView>,
    pub leaders: BTreeMap<LeaderId, LeaderStatus>,
    /// Other players' command tokens held in this seat's fleet pool (Mahact's Edict), one entry
    /// per token naming its owner, sorted. Public. Absent for every other seat and while the pool
    /// holds none, so older clients and saves are unaffected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub held_foreign_tokens: Vec<PlayerId>,
}

/// Redacted view of a planet on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlanetView {
    pub planet_id: PlanetId,
    pub controlled_by: Option<PlayerId>,
    pub exhausted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
    /// The resources of the planet now, when they differ from the printed value: with its
    /// attachments and the laws on it (`ti4_engine::production::planet_value_now`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resources: Option<i64>,
    /// The influence of the planet now, when it differs from the printed value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub influence: Option<i64>,
}

/// Redacted view of a unit placed on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlacedUnitView {
    pub unit_type: UnitTypeId,
    pub owner: PlayerId,
    pub planet: Option<PlanetId>,
    pub damaged: bool,
    #[serde(default)]
    pub galvanized: bool,
}

/// Redacted view of a star system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SystemView {
    pub system_id: SystemId,
    pub command_tokens: BTreeSet<PlayerId>,
    pub planets: BTreeMap<PlanetId, PlanetView>,
    pub units: Vec<PlacedUnitView>,
}

/// Static metadata of a planet on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct PlanetMetaView {
    pub id: String,
    pub label: String,
    pub resources: i32,
    pub influence: i32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub traits: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tech_specialties: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub legendary: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub space_station: bool,
}

/// Static geometry and metadata of a star system tile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardTileView {
    pub system_id: String,
    pub label: String,
    pub q: i32,
    pub r: i32,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hyperlane: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub special_area: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anomalies: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub wormholes: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub egress: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planets: Vec<PlanetMetaView>,
}

/// A die roll result during combat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CombatDieRoll {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub player: Option<PlayerId>,
    pub unit: String,
    pub roll: u32,
    pub target: u32,
    pub hit: bool,
}

/// Publicly observable space combat state in progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CombatView {
    pub system_id: SystemId,
    pub round: u32,
    pub battle_seq: u32,
    pub phase: String,
    pub round_start: Vec<PlacedUnitView>,
    pub barrage_start: Vec<PlacedUnitView>,
    pub barrage_hits: BTreeMap<PlayerId, u32>,
    pub barrage_dice: Vec<CombatDieRoll>,
    /// Hits still queued against each seat, including the other side's pending return fire.
    pub remaining_hits: BTreeMap<PlayerId, usize>,
    pub attacker: PlayerId,
    pub defender: PlayerId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_player: Option<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hits_to_assign: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attacker_hits: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defender_hits: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dice_rolls: Vec<CombatDieRoll>,
}

/// Public invasion identity and current planet-local progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InvasionView {
    pub system_id: SystemId,
    pub invasion_seq: u64,
    pub invader: PlayerId,
    pub phase: String,
    pub planets: Vec<PlanetId>,
    pub current_planet: Option<PlanetId>,
    pub defender: Option<PlayerId>,
    pub ground_round: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_step: Option<InvasionStepView>,
    /// Authoritative public classification and still-pending modifiers for landing previews.
    #[serde(default)]
    pub odds_context: BTreeMap<PlanetId, InvasionOddsContext>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InvasionOddsContext {
    pub opponent: Option<PlayerId>,
    /// False when a live defense modifier cannot be represented in the ground odds request.
    pub available: bool,
    pub ground_force_types: Vec<String>,
    /// Guns not already carried by the immediate opponent's modeled ground forces.
    pub additional_guns: BTreeMap<String, usize>,
    /// Legal Harrow bombarders in the space area; bombardment already resolved.
    pub harrow_units: BTreeMap<String, usize>,
}

/// Latest resolved automatic step, scoped to one planet and one ground round.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct InvasionStepView {
    pub planet: PlanetId,
    pub kind: String,
    pub round: u32,
    pub before: Vec<PlacedUnitView>,
    pub after: Vec<PlacedUnitView>,
    pub dice: Vec<ti4_model::state::InvasionDie>,
    pub hits: BTreeMap<PlayerId, usize>,
    pub harrow_hits: usize,
}

/// One round of a space combat of the turn as it resolved ([`ti4_model::state::SpaceRoundRecord`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SpaceRoundView {
    pub system_id: SystemId,
    pub attacker: PlayerId,
    pub defender: PlayerId,
    pub battle_seq: u32,
    pub round: u32,
    pub round_start: Vec<PlacedUnitView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub barrage_start: Vec<PlacedUnitView>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub barrage_dice: Vec<CombatDieRoll>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub barrage_hits: BTreeMap<PlayerId, u32>,
    pub dice_rolls: Vec<CombatDieRoll>,
    pub hits: BTreeMap<PlayerId, u32>,
}

/// A step of an invasion of the turn as it resolved ([`ti4_model::state::GroundStepRecord`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GroundStepView {
    pub system_id: SystemId,
    pub invasion_seq: u64,
    pub invader: PlayerId,
    pub step: InvasionStepView,
}

/// Redacted view of the galaxy board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BoardView {
    pub systems: BTreeMap<SystemId, SystemView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_system: Option<SystemId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub map_tiles: Vec<BoardTileView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub combat: Option<CombatView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invasion: Option<InvasionView>,
    /// The rounds of the space combats of the turn, as they resolved: a combat that began and
    /// ended between two updates is here too. Cleared when the turn ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub space_rounds: Vec<SpaceRoundView>,
    /// The steps of the invasions of the turn (bombardment, space cannon, each ground round),
    /// as they resolved. Cleared when the turn ends.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ground_steps: Vec<GroundStepView>,
}

/// Exact progress towards a public objective requirement.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct ObjectiveProgressView {
    pub have: u32,
    pub threshold: u32,
    pub satisfied: bool,
    /// Push Boundaries: this player's planet count against each neighbour's. Display only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub neighbours: Vec<NeighbourProgressView>,
    /// Learn the Secrets of the Cosmos: the systems that count right now. Display only.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub systems: Vec<SystemId>,
}

/// One neighbour row of a planet-count comparison (Push Boundaries).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct NeighbourProgressView {
    pub player: PlayerId,
    /// Planets the progress owner controls.
    pub mine: u32,
    /// Planets this neighbour controls.
    pub theirs: u32,
    /// Strictly more planets than this neighbour, so the row counts toward the objective.
    pub ahead: bool,
}

/// Public table state: objectives, laws, strategy card goods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct TableView {
    pub revealed_objectives: Vec<ObjectiveId>,
    pub scored_objectives: BTreeMap<PlayerId, BTreeSet<ObjectiveId>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objective_progress: BTreeMap<PlayerId, BTreeMap<ObjectiveId, ObjectiveProgressView>>,
    /// Rule 61.16: the home planets a seat does not control, for each seat that may therefore
    /// score no public objective. Seats that may score are absent. Planet control is public.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub home_system_lost: BTreeMap<PlayerId, Vec<PlanetId>>,
    pub unclaimed_strategy_cards: Vec<StrategyCardId>,
    pub strategy_card_goods: BTreeMap<StrategyCardId, i32>,
    pub laws: BTreeMap<String, String>,
}

/// Complete redacted view of the game for one viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GameView {
    pub round: u32,
    /// The turn of the action phase (LRR 3), counted from 1; zero outside it.
    #[serde(default)]
    pub turn: u32,
    pub phase: Phase,
    pub speaker: PlayerId,
    pub seating_order: Vec<PlayerId>,
    /// The order of the turns of the action phase (LRR 3, 47): by the lowest initiative of each
    /// seat, an override such as the Naalu "0" token counted in its place, ties and seats with no
    /// card by seating. Empty, and absent, outside the action phase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub initiative_order: Vec<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_player: Option<PlayerId>,
    pub finished: bool,
    pub players: Vec<PlayerView>,
    pub board: BoardView,
    pub table: TableView,
}
