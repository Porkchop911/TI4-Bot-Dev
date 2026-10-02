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
    pub leaders: BTreeMap<LeaderId, LeaderStatus>,
}

/// Redacted view of a planet on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanetView {
    pub planet_id: PlanetId,
    pub controlled_by: Option<PlayerId>,
    pub exhausted: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<String>,
}

/// Redacted view of a unit placed on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlacedUnitView {
    pub unit_type: UnitTypeId,
    pub owner: PlayerId,
    pub planet: Option<PlanetId>,
    pub damaged: bool,
}

/// Redacted view of a star system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemView {
    pub system_id: SystemId,
    pub command_tokens: BTreeSet<PlayerId>,
    pub planets: BTreeMap<PlanetId, PlanetView>,
    pub units: Vec<PlacedUnitView>,
}

/// Static metadata of a planet on the board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
pub struct InvasionOddsContext {
    pub opponent: Option<PlayerId>,
    /// False when a live defense modifier cannot be represented in the advisor request.
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

/// Redacted view of the galaxy board.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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
}

/// Exact progress towards a public objective requirement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectiveProgressView {
    pub have: u32,
    pub threshold: u32,
    pub satisfied: bool,
}

/// Public table state: objectives, laws, strategy card goods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TableView {
    pub revealed_objectives: Vec<ObjectiveId>,
    pub scored_objectives: BTreeMap<PlayerId, BTreeSet<ObjectiveId>>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub objective_progress: BTreeMap<PlayerId, BTreeMap<ObjectiveId, ObjectiveProgressView>>,
    pub unclaimed_strategy_cards: Vec<StrategyCardId>,
    pub strategy_card_goods: BTreeMap<StrategyCardId, i32>,
    pub laws: BTreeMap<String, String>,
}

/// Complete redacted view of the game for one viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameView {
    pub round: u32,
    pub phase: Phase,
    pub speaker: PlayerId,
    pub seating_order: Vec<PlayerId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_player: Option<PlayerId>,
    pub finished: bool,
    pub players: Vec<PlayerView>,
    pub board: BoardView,
    pub table: TableView,
}
