//! View types for bots, TTS, and external consumers.
//!
//! Views provide redacted access to GameState: no hidden information,
//! no opponent cards, no secret objectives, etc.

use crate::factions::FactionRecord;
use crate::id::*;
use crate::state::*;
use crate::units::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

// ─── Bot view ──────────────────────────────────────────────────────────────────

/// A view of the game state suitable for bot policy evaluation.
/// Contains all public information plus faction-specific private data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotView {
    pub player_id: PlayerId,
    pub faction_id: FactionId,
    pub round: i32,
    pub phase: GamePhase,
    pub sub_phase: Option<ActionSubPhase>,
    pub agenda_phase: Option<AgendaPhase>,
    pub initiative_player: Option<PlayerId>,
    pub current_agenda_player: Option<PlayerId>,
    pub player_order: Vec<PlayerId>,
    pub player_count: i32,
    pub my_fleet: HashMap<UnitTypeId, i32>,
    pub my_commodity: i32,
    pub my_influence: i32,
    pub my_production: i32,
    pub my_fuel: i32,
    pub my_pips: i32,
    pub my_action_pips: i32,
    pub my_tactic_tokens: i32,
    pub my_fleet_tokens: i32,
    pub my_strategic_tokens: i32,
    pub my_technologies: HashSet<TechnologyId>,
    pub my_tech_levels: HashMap<TechnologyId, i32>,
    pub my_action_cards: Vec<ActionCardView>,
    pub my_strategy_discard: Vec<StrategyCard>,
    pub my_secret_objective: Option<SecretObjectiveView>,
    pub my_objectives: Vec<ObjectiveView>,
    pub my_completed_objectives: Vec<ObjectiveView>,
    pub my_secret_completed: Vec<SecretObjectiveView>,
    pub my_leaders: Vec<LeaderView>,
    pub my_active_leader: Option<LeaderView>,
    pub my_leader_fatigue: Vec<LeaderId>,
    pub my_promissory_notes_given: Vec<PromissoryNoteView>,
    pub my_promissory_notes_received: Vec<PromissoryNoteView>,
    pub my_relics: Vec<RelicView>,
    pub my_fragments: HashMap<FragmentId, i32>,
    pub my_tokens: HashMap<Token, i32>,
    pub my_control_tokens: HashSet<PlanetId>,
    pub my_casualties: HashMap<SystemId, i32>,
    pub my_retreat_tokens: HashMap<SystemId, i32>,
    pub my_invasion_tokens: HashMap<PlanetId, i32>,
    pub my_score: i32,
    pub my_objective_score: i32,
    pub my_secret_score: i32,
    pub my_relic_score: i32,
    pub my_fragment_score: i32,
    pub my_edge_score: i32,
    pub my_trade_routes: i32,
    pub my_trade_income: i32,
    pub my_economic_score: i32,
    pub my_economic_tokens: i32,
    pub my_military_score: i32,
    pub my_military_tokens: i32,
    pub my_production_tokens: i32,
    pub my_home_planets_controlled: i32,
    pub my_home_system: SystemId,
    pub my_home_planets: Vec<PlanetId>,
    pub my_homeworld_ability: bool,
    pub my_has_initiative: bool,
    pub my_has_agenda_token: bool,
    pub my_has_agenda_proxy: bool,
    pub my_has_victory_point: bool,
    pub my_has_fatigued_leader: bool,
    pub my_has_fatigued_leader_ability: bool,
    pub my_has_command_token: bool,
    pub my_has_exhausted_fleet: bool,
    pub my_has_exhausted_planet: bool,
    pub my_has_broken_promissory: bool,
    pub my_has_rebel_fleet: bool,
    pub my_has_sabotage_token: bool,
    pub my_has_infantry_in_play: bool,
    pub my_has_pds_in_play: bool,
    pub my_has_home_planet_in_play: bool,
    pub my_has_fleet_in_home: bool,
    pub my_has_fleet_in_play: bool,
    pub my_has_leader_in_play: bool,
    pub my_has_tech_ability: bool,
    pub my_has_unit_ability: bool,
    pub my_has_card_effect: bool,
    pub my_has_relic: bool,
    pub my_has_fragment: bool,
    pub my_has_edge: bool,
    pub my_has_completed_objective: bool,
    pub my_has_completed_secret: bool,
    pub my_has_promissory: bool,
    pub my_has_law_effect: bool,
    pub my_has_agenda_effect: bool,
    pub expedition_tokens: i32,
    pub edge_fragments: HashMap<FragmentId, i32>,
    pub edge_breakthroughs: Vec<BreakthroughView>,
    pub edge_faction: Option<FactionId>,
    pub edge_token: Option<PlayerId>,
    pub revealed_strategies: Vec<StrategyCard>,
    pub secret_strategies: HashMap<PlayerId, StrategyCard>,
    pub passed: HashSet<PlayerId>,
    pub systems: BTreeMap<SystemId, SystemView>,
    pub planets: BTreeMap<PlanetId, PlanetView>,
    pub exploration_map: HashMap<PlayerId, HashSet<SystemId>>,
    pub agenda_card: Option<AgendaCardView>,
    pub laws: Vec<LawView>,
    pub agenda_results: Vec<AgendaResultView>,
    pub victory_conditions: Vec<VictoryCondition>,
    pub winner: Option<PlayerId>,
    pub game_over: bool,
    pub event_log: Vec<EventView>,
    pub current_events: Vec<EventView>,
    pub active_event: Option<EventView>,
    pub faction_records: HashMap<FactionId, FactionRecord>,
    pub unit_types: HashMap<UnitTypeId, UnitType>,
}

// ─── TTS view ──────────────────────────────────────────────────────────────────

/// A view of the game state suitable for TTS wire format.
/// Contains only public information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TtsView {
    pub id: String,
    pub round: i32,
    pub phase: GamePhase,
    pub sub_phase: Option<ActionSubPhase>,
    pub agenda_phase: Option<AgendaPhase>,
    pub player_order: Vec<PlayerId>,
    pub player_count: i32,
    pub initiative_player: Option<PlayerId>,
    pub current_agenda_player: Option<PlayerId>,
    pub agenda_tokens: HashMap<PlayerId, i32>,
    pub revealed_strategies: Vec<StrategyCard>,
    pub secret_strategies: HashMap<PlayerId, StrategyCard>,
    pub passed: HashSet<PlayerId>,
    pub systems: BTreeMap<SystemId, SystemView>,
    pub planets: BTreeMap<PlanetId, PlanetView>,
    pub exploration_map: HashMap<PlayerId, HashSet<SystemId>>,
    pub agenda_card: Option<AgendaCardView>,
    pub laws: Vec<LawView>,
    pub agenda_results: Vec<AgendaResultView>,
    pub winner: Option<PlayerId>,
    pub game_over: bool,
    pub expedition_tiles: Vec<ExpeditionTileView>,
    pub edge_token: Option<PlayerId>,
    pub edge_faction: Option<FactionId>,
}

// ─── View sub-structures ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCardView {
    pub id: ActionCardId,
    pub owner: PlayerId,
    pub exhausted: bool,
    pub used: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretObjectiveView {
    pub id: SecretObjectiveId,
    pub completed: bool,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveView {
    pub id: ObjectiveId,
    pub completed: bool,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderView {
    pub id: LeaderId,
    pub ability: String,
    pub active: bool,
    pub fatigued: bool,
    pub system_id: Option<SystemId>,
    pub planet_id: Option<PlanetId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromissoryNoteView {
    pub id: PromissoryNoteId,
    pub giver: PlayerId,
    pub receiver: PlayerId,
    pub broken: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelicView {
    pub id: RelicId,
    pub owner: Option<PlayerId>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakthroughView {
    pub id: BreakthroughId,
    pub claimed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaCardView {
    pub id: AgendaId,
    pub title: String,
    pub effects: Vec<AgendaEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawView {
    pub id: LawId,
    pub active: bool,
    pub effects: Vec<LawEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaResultView {
    pub phase: AgendaPhase,
    pub winner: PlayerId,
    pub score: i32,
    pub effects: Vec<AgendaEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryConditionView {
    pub type_: String,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemView {
    pub id: SystemId,
    pub name: String,
    pub planet_ids: Vec<PlanetId>,
    pub space_tokens: HashMap<Token, i32>,
    pub system_tokens: HashMap<Token, i32>,
    pub faction_tokens: HashMap<FactionId, i32>,
    pub faction_fleets: HashMap<FactionId, HashMap<UnitTypeId, i32>>,
    pub faction_casualties: HashMap<FactionId, i32>,
    pub faction_retreats: HashMap<FactionId, i32>,
    pub faction_invasion: HashMap<PlanetId, i32>,
    pub faction_pds: HashMap<FactionId, i32>,
    pub faction_leaders: HashMap<FactionId, Vec<LeaderView>>,
    pub is_home: bool,
    pub is_capital: bool,
    pub home_faction: Option<FactionId>,
    pub home_planet: Option<PlanetId>,
    pub home_planet_count: i32,
    pub home_system: bool,
    pub has_pds: bool,
    pub has_capital: bool,
    pub has_fleet: bool,
    pub has_casualty: bool,
    pub has_retreat: bool,
    pub has_invasion: bool,
    pub has_leaders: bool,
    pub has_influence: bool,
    pub has_production: bool,
    pub has_fuel: bool,
    pub has_command: bool,
    pub has_exhausted: bool,
    pub has_rebel_fleet: bool,
    pub has_fatigued_leader: bool,
    pub has_broken_promissory: bool,
    pub has_sabotage: bool,
    pub has_infiltration: bool,
    pub has_infantry: bool,
    pub has_pds_token: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanetView {
    pub id: PlanetId,
    pub name: String,
    pub system_id: SystemId,
    pub planet_type: String,
    pub influence: i32,
    pub production: i32,
    pub fuel: i32,
    pub home_faction: Option<FactionId>,
    pub owner: Option<FactionId>,
    pub control_tokens: HashMap<FactionId, i32>,
    pub invasion_tokens: HashMap<FactionId, i32>,
    pub casualties: HashMap<FactionId, i32>,
    pub pds: HashMap<FactionId, i32>,
    pub leaders: HashMap<FactionId, Vec<LeaderView>>,
    pub faction_fleets: HashMap<FactionId, HashMap<UnitTypeId, i32>>,
    pub has_capital: bool,
    pub has_influence: bool,
    pub has_production: bool,
    pub has_fuel: bool,
    pub has_control_token: bool,
    pub has_invasion_token: bool,
    pub has_casualty: bool,
    pub has_pds: bool,
    pub has_leader: bool,
    pub has_fleet: bool,
    pub has_home: bool,
    pub has_owner: bool,
    pub has_exhausted: bool,
    pub has_rebel_fleet: bool,
    pub has_fatigued_leader: bool,
    pub has_broken_promissory: bool,
    pub has_sabotage: bool,
    pub has_infantry: bool,
    pub has_infiltration: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpeditionTileView {
    pub id: ExpeditionTileId,
    pub revealed: bool,
    pub claimed: Option<PlayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventView {
    pub id: EventId,
    pub event_type: String,
    pub source: PlayerId,
    pub target: Option<String>,
    pub timestamp: i32,
    pub effects: Vec<EventEffect>,
    pub resolved: bool,
}

// ─── Builder ───────────────────────────────────────────────────────────────────

impl BotView {
    pub fn from_game_state(
        game: &GameState,
        player_id: &PlayerId,
        faction_records: &HashMap<FactionId, FactionRecord>,
        unit_types: &HashMap<UnitTypeId, UnitType>,
    ) -> Self {
        let ps = game.players.get(player_id).cloned().unwrap_or_default();
        BotView {
            player_id: player_id.clone(),
            faction_id: ps.faction_id.clone(),
            round: game.round,
            phase: game.phase,
            sub_phase: game.sub_phase,
            agenda_phase: game.agenda_phase,
            initiative_player: game.initiative_player.clone(),
            current_agenda_player: game.current_agenda_player.clone(),
            player_order: game.player_order.clone(),
            player_count: game.player_count,
            my_fleet: ps.fleet.clone(),
            my_commodity: ps.commodity,
            my_influence: ps.influence,
            my_production: ps.production,
            my_fuel: ps.fuel,
            my_pips: ps.pips,
            my_action_pips: ps.action_pips,
            my_tactic_tokens: ps.tactic_tokens,
            my_fleet_tokens: ps.fleet_tokens,
            my_strategic_tokens: ps.strategic_tokens,
            my_technologies: ps.technologies.clone(),
            my_tech_levels: ps.tech_levels.clone(),
            my_action_cards: vec![],
            my_strategy_discard: ps.strategy_discard.clone(),
            my_secret_objective: ps.secret_objective.map(|s| SecretObjectiveView {
                id: s.id,
                completed: s.completed,
                score: s.score,
            }),
            my_objectives: ps
                .objectives
                .iter()
                .map(|o| ObjectiveView {
                    id: o.id.clone(),
                    completed: o.completed,
                    score: o.score,
                })
                .collect(),
            my_completed_objectives: ps
                .completed_objectives
                .iter()
                .map(|o| ObjectiveView {
                    id: o.id.clone(),
                    completed: o.completed,
                    score: o.score,
                })
                .collect(),
            my_secret_completed: ps
                .secret_completed
                .iter()
                .map(|s| SecretObjectiveView {
                    id: s.id.clone(),
                    completed: s.completed,
                    score: s.score,
                })
                .collect(),
            my_leaders: vec![],
            my_active_leader: None,
            my_leader_fatigue: ps.leader_fatigue.clone(),
            my_promissory_notes_given: vec![],
            my_promissory_notes_received: vec![],
            my_relics: vec![],
            my_fragments: ps.fragments.clone(),
            my_tokens: ps.tokens.clone(),
            my_control_tokens: ps.control_tokens.clone(),
            my_casualties: ps.casualties.clone(),
            my_retreat_tokens: ps.retreat_tokens.clone(),
            my_invasion_tokens: ps.invasion_tokens.clone(),
            my_score: ps.score,
            my_objective_score: ps.objective_score,
            my_secret_score: ps.secret_score,
            my_relic_score: ps.relic_score,
            my_fragment_score: ps.fragment_score,
            my_edge_score: ps.edge_score,
            my_trade_routes: ps.trade_routes,
            my_trade_income: ps.trade_income,
            my_economic_score: ps.economic_score,
            my_economic_tokens: ps.economic_tokens,
            my_military_score: ps.military_score,
            my_military_tokens: ps.military_tokens,
            my_production_tokens: ps.production_tokens,
            my_home_planets_controlled: ps.home_planets_controlled,
            my_home_system: ps.home_system.clone(),
            my_home_planets: ps.home_planets.clone(),
            my_homeworld_ability: ps.homeworld_ability,
            my_has_initiative: ps.has_initiative,
            my_has_agenda_token: ps.has_agenda_token,
            my_has_agenda_proxy: ps.has_agenda_proxy,
            my_has_victory_point: ps.has_victory_point,
            my_has_fatigued_leader: ps.has_fatigued_leader,
            my_has_fatigued_leader_ability: ps.has_fatigued_leader_ability,
            my_has_command_token: ps.has_command_token,
            my_has_exhausted_fleet: ps.has_exhausted_fleet,
            my_has_exhausted_planet: ps.has_exhausted_planet,
            my_has_broken_promissory: ps.has_broken_promissory,
            my_has_rebel_fleet: ps.has_rebel_fleet,
            my_has_sabotage_token: ps.has_sabotage_token,
            my_has_infantry_in_play: ps.has_infantry_in_play,
            my_has_pds_in_play: ps.has_pds_in_play,
            my_has_home_planet_in_play: ps.has_home_planet_in_play,
            my_has_fleet_in_home: ps.has_fleet_in_home,
            my_has_fleet_in_play: ps.has_fleet_in_play,
            my_has_leader_in_play: ps.has_leader_in_play,
            my_has_tech_ability: ps.has_tech_ability,
            my_has_unit_ability: ps.has_unit_ability,
            my_has_card_effect: ps.has_card_effect,
            my_has_relic: ps.has_relic,
            my_has_fragment: ps.has_fragment,
            my_has_edge: ps.has_edge,
            my_has_completed_objective: ps.has_completed_objective,
            my_has_completed_secret: ps.has_completed_secret,
            my_has_promissory: ps.has_promissory,
            my_has_law_effect: ps.has_law_effect,
            my_has_agenda_effect: ps.has_agenda_effect,
            expedition_tokens: ps.expedition_tokens,
            edge_fragments: ps.edge_fragments.clone(),
            edge_breakthroughs: ps
                .edge_breakthroughs
                .iter()
                .map(|b| BreakthroughView {
                    id: b.id.clone(),
                    claimed: b.claimed,
                })
                .collect(),
            edge_faction: ps.edge_faction.clone(),
            edge_token: ps.edge_token.clone(),
            revealed_strategies: game.revealed_strategies.clone(),
            secret_strategies: game.secret_strategies.clone(),
            passed: game.passed.clone(),
            systems: game
                .systems
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        SystemView {
                            id: v.id.clone(),
                            name: v.name.clone(),
                            planet_ids: v.planet_ids.clone(),
                            space_tokens: v.space_tokens.clone(),
                            system_tokens: v.system_tokens.clone(),
                            faction_tokens: v.faction_tokens.clone(),
                            faction_fleets: v.faction_fleets.clone(),
                            faction_casualties: v.faction_casualties.clone(),
                            faction_retreats: v.faction_retreats.clone(),
                            faction_invasion: v.faction_invasion.clone(),
                            faction_pds: v.faction_pds.clone(),
                            faction_leaders: v
                                .faction_leaders
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        k.clone(),
                                        v.iter()
                                            .map(|l| LeaderView {
                                                id: l.id.clone(),
                                                ability: l.ability.clone(),
                                                active: l.active,
                                                fatigued: l.fatigued,
                                                system_id: l.system_id.clone(),
                                                planet_id: l.planet_id.clone(),
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
                            is_home: v.is_home,
                            is_capital: v.is_capital,
                            home_faction: v.home_faction.clone(),
                            home_planet: v.home_planet.clone(),
                            home_planet_count: v.home_planet_count,
                            home_system: v.home_system,
                            has_pds: v.has_pds,
                            has_capital: v.has_capital,
                            has_fleet: v.has_fleet,
                            has_casualty: v.has_casualty,
                            has_retreat: v.has_retreat,
                            has_invasion: v.has_invasion,
                            has_leaders: v.has_leaders,
                            has_influence: v.has_influence,
                            has_production: v.has_production,
                            has_fuel: v.has_fuel,
                            has_command: v.has_command,
                            has_exhausted: v.has_exhausted,
                            has_rebel_fleet: v.has_rebel_fleet,
                            has_fatigued_leader: v.has_fatigued_leader,
                            has_broken_promissory: v.has_broken_promissory,
                            has_sabotage: v.has_sabotage,
                            has_infiltration: v.has_infiltration,
                            has_infantry: v.has_infantry,
                            has_pds_token: v.has_pds_token,
                        },
                    )
                })
                .collect(),
            planets: game
                .planets
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        PlanetView {
                            id: v.id.clone(),
                            name: v.name.clone(),
                            system_id: v.system_id.clone(),
                            planet_type: v.planet_type.clone(),
                            influence: v.influence,
                            production: v.production,
                            fuel: v.fuel,
                            home_faction: v.home_faction.clone(),
                            owner: v.owner.clone(),
                            control_tokens: v.control_tokens.clone(),
                            invasion_tokens: v.invasion_tokens.clone(),
                            casualties: v.casualties.clone(),
                            pds: v.pds.clone(),
                            leaders: v
                                .leaders
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        k.clone(),
                                        v.iter()
                                            .map(|l| LeaderView {
                                                id: l.id.clone(),
                                                ability: l.ability.clone(),
                                                active: l.active,
                                                fatigued: l.fatigued,
                                                system_id: l.system_id.clone(),
                                                planet_id: l.planet_id.clone(),
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
                            faction_fleets: v.faction_fleets.clone(),
                            has_capital: v.has_capital,
                            has_influence: v.has_influence,
                            has_production: v.has_production,
                            has_fuel: v.has_fuel,
                            has_control_token: v.has_control_token,
                            has_invasion_token: v.has_invasion_token,
                            has_casualty: v.has_casualty,
                            has_pds: v.has_pds,
                            has_leader: v.has_leader,
                            has_fleet: v.has_fleet,
                            has_home: v.has_home,
                            has_owner: v.has_owner,
                            has_exhausted: v.has_exhausted,
                            has_rebel_fleet: v.has_rebel_fleet,
                            has_fatigued_leader: v.has_fatigued_leader,
                            has_broken_promissory: v.has_broken_promissory,
                            has_sabotage: v.has_sabotage,
                            has_infantry: v.has_infantry,
                            has_infiltration: v.has_infiltration,
                        },
                    )
                })
                .collect(),
            exploration_map: game.exploration_map.clone(),
            agenda_card: game.agenda_card.as_ref().map(|c| AgendaCardView {
                id: c.id.clone(),
                title: c.title.clone(),
                effects: c.effects.clone(),
            }),
            laws: game
                .laws
                .iter()
                .map(|l| LawView {
                    id: l.id.clone(),
                    active: l.active,
                    effects: l.effects.clone(),
                })
                .collect(),
            agenda_results: game
                .agenda_results
                .iter()
                .map(|r| AgendaResultView {
                    phase: r.phase,
                    winner: r.winner.clone(),
                    score: r.score,
                    effects: r.effects.clone(),
                })
                .collect(),
            victory_conditions: game.victory_conditions.clone(),
            winner: game.winner.clone(),
            game_over: game.game_over,
            event_log: game
                .event_log
                .iter()
                .map(|e| EventView {
                    id: e.id.clone(),
                    event_type: e.event_type.clone(),
                    source: e.source.clone(),
                    target: e.target.clone(),
                    timestamp: e.timestamp,
                    effects: e.effects.clone(),
                    resolved: e.resolved,
                })
                .collect(),
            current_events: game
                .current_events
                .iter()
                .map(|e| EventView {
                    id: e.id.clone(),
                    event_type: e.event_type.clone(),
                    source: e.source.clone(),
                    target: e.target.clone(),
                    timestamp: e.timestamp,
                    effects: e.effects.clone(),
                    resolved: e.resolved,
                })
                .collect(),
            active_event: game.active_event.as_ref().map(|e| EventView {
                id: e.id.clone(),
                event_type: e.event_type.clone(),
                source: e.source.clone(),
                target: e.target.clone(),
                timestamp: e.timestamp,
                effects: e.effects.clone(),
                resolved: e.resolved,
            }),
            faction_records: faction_records.clone(),
            unit_types: unit_types.clone(),
        }
    }
}

impl TtsView {
    pub fn from_game_state(
        game: &GameState,
        faction_records: &HashMap<FactionId, FactionRecord>,
    ) -> Self {
        let _ = faction_records;
        TtsView {
            id: game.id.clone(),
            round: game.round,
            phase: game.phase,
            sub_phase: game.sub_phase,
            agenda_phase: game.agenda_phase,
            player_order: game.player_order.clone(),
            player_count: game.player_count,
            initiative_player: game.initiative_player.clone(),
            current_agenda_player: game.current_agenda_player.clone(),
            agenda_tokens: game.agenda_tokens.clone(),
            revealed_strategies: game.revealed_strategies.clone(),
            secret_strategies: game.secret_strategies.clone(),
            passed: game.passed.clone(),
            systems: game
                .systems
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        SystemView {
                            id: v.id.clone(),
                            name: v.name.clone(),
                            planet_ids: v.planet_ids.clone(),
                            space_tokens: v.space_tokens.clone(),
                            system_tokens: v.system_tokens.clone(),
                            faction_tokens: v.faction_tokens.clone(),
                            faction_fleets: v.faction_fleets.clone(),
                            faction_casualties: v.faction_casualties.clone(),
                            faction_retreats: v.faction_retreats.clone(),
                            faction_invasion: v.faction_invasion.clone(),
                            faction_pds: v.faction_pds.clone(),
                            faction_leaders: v
                                .faction_leaders
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        k.clone(),
                                        v.iter()
                                            .map(|l| LeaderView {
                                                id: l.id.clone(),
                                                ability: l.ability.clone(),
                                                active: l.active,
                                                fatigued: l.fatigued,
                                                system_id: l.system_id.clone(),
                                                planet_id: l.planet_id.clone(),
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
                            is_home: v.is_home,
                            is_capital: v.is_capital,
                            home_faction: v.home_faction.clone(),
                            home_planet: v.home_planet.clone(),
                            home_planet_count: v.home_planet_count,
                            home_system: v.home_system,
                            has_pds: v.has_pds,
                            has_capital: v.has_capital,
                            has_fleet: v.has_fleet,
                            has_casualty: v.has_casualty,
                            has_retreat: v.has_retreat,
                            has_invasion: v.has_invasion,
                            has_leaders: v.has_leaders,
                            has_influence: v.has_influence,
                            has_production: v.has_production,
                            has_fuel: v.has_fuel,
                            has_command: v.has_command,
                            has_exhausted: v.has_exhausted,
                            has_rebel_fleet: v.has_rebel_fleet,
                            has_fatigued_leader: v.has_fatigued_leader,
                            has_broken_promissory: v.has_broken_promissory,
                            has_sabotage: v.has_sabotage,
                            has_infiltration: v.has_infiltration,
                            has_infantry: v.has_infantry,
                            has_pds_token: v.has_pds_token,
                        },
                    )
                })
                .collect(),
            planets: game
                .planets
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        PlanetView {
                            id: v.id.clone(),
                            name: v.name.clone(),
                            system_id: v.system_id.clone(),
                            planet_type: v.planet_type.clone(),
                            influence: v.influence,
                            production: v.production,
                            fuel: v.fuel,
                            home_faction: v.home_faction.clone(),
                            owner: v.owner.clone(),
                            control_tokens: v.control_tokens.clone(),
                            invasion_tokens: v.invasion_tokens.clone(),
                            casualties: v.casualties.clone(),
                            pds: v.pds.clone(),
                            leaders: v
                                .leaders
                                .iter()
                                .map(|(k, v)| {
                                    (
                                        k.clone(),
                                        v.iter()
                                            .map(|l| LeaderView {
                                                id: l.id.clone(),
                                                ability: l.ability.clone(),
                                                active: l.active,
                                                fatigued: l.fatigued,
                                                system_id: l.system_id.clone(),
                                                planet_id: l.planet_id.clone(),
                                            })
                                            .collect(),
                                    )
                                })
                                .collect(),
                            faction_fleets: v.faction_fleets.clone(),
                            has_capital: v.has_capital,
                            has_influence: v.has_influence,
                            has_production: v.has_production,
                            has_fuel: v.has_fuel,
                            has_control_token: v.has_control_token,
                            has_invasion_token: v.has_invasion_token,
                            has_casualty: v.has_casualty,
                            has_pds: v.has_pds,
                            has_leader: v.has_leader,
                            has_fleet: v.has_fleet,
                            has_home: v.has_home,
                            has_owner: v.has_owner,
                            has_exhausted: v.has_exhausted,
                            has_rebel_fleet: v.has_rebel_fleet,
                            has_fatigued_leader: v.has_fatigued_leader,
                            has_broken_promissory: v.has_broken_promissory,
                            has_sabotage: v.has_sabotage,
                            has_infantry: v.has_infantry,
                            has_infiltration: v.has_infiltration,
                        },
                    )
                })
                .collect(),
            exploration_map: game.exploration_map.clone(),
            agenda_card: game.agenda_card.as_ref().map(|c| AgendaCardView {
                id: c.id.clone(),
                title: c.title.clone(),
                effects: c.effects.clone(),
            }),
            laws: game
                .laws
                .iter()
                .map(|l| LawView {
                    id: l.id.clone(),
                    active: l.active,
                    effects: l.effects.clone(),
                })
                .collect(),
            agenda_results: game
                .agenda_results
                .iter()
                .map(|r| AgendaResultView {
                    phase: r.phase,
                    winner: r.winner.clone(),
                    score: r.score,
                    effects: r.effects.clone(),
                })
                .collect(),
            winner: game.winner.clone(),
            game_over: game.game_over,
            expedition_tiles: game
                .expedition_tiles
                .iter()
                .map(|t| ExpeditionTileView {
                    id: t.id.clone(),
                    revealed: t.revealed,
                    claimed: t.claimed.clone(),
                })
                .collect(),
            edge_token: game.edge_token.clone(),
            edge_faction: game.edge_faction.clone(),
        }
    }
}
