//! GameState and PlayerState structures.
//!
//! GameState owns ~44 fields; PlayerState owns ~46 fields.
//! This module defines the core game state that the engine mutates.

use crate::id::*;
use crate::units::*;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, BTreeMap};

// ─── Phase and timing ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GamePhase {
    Setup,
    Action,
    Agenda,
    RoundEnd,
    GameEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionSubPhase {
    Strategy,
    Command,
    Tactical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgendaPhase {
    Political,
    Economic,
    Military,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrategyCard {
    Leadership,
    Diplomacy,
    Politics,
    Construction,
    Trade,
    Warfare,
    Technology,
    Imperial,
    /// Thunder's Edge variant: production instead of first structure placement
    Te4Construction,
    /// Thunder's Edge variant: free tactical action instead of token recall
    Te6Warfare,
    Unknown,
}

impl StrategyCard {
    pub fn from_code(code: &str) -> Self {
        match code {
            "leadership" => Self::Leadership,
            "diplomacy" => Self::Diplomacy,
            "politics" => Self::Politics,
            "construction" => Self::Construction,
            "trade" => Self::Trade,
            "warfare" => Self::Warfare,
            "technology" => Self::Technology,
            "imperial" => Self::Imperial,
            "te4construction" => Self::Te4Construction,
            "te6warfare" => Self::Te6Warfare,
            _ => Self::Unknown,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Leadership => "leadership",
            Self::Diplomacy => "diplomacy",
            Self::Politics => "politics",
            Self::Construction => "construction",
            Self::Trade => "trade",
            Self::Warfare => "warfare",
            Self::Technology => "technology",
            Self::Imperial => "imperial",
            Self::Te4Construction => "te4construction",
            Self::Te6Warfare => "te6warfare",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Token {
    Invasion,
    Control,
    Casualty,
    Retreat,
    Agenda,
    Initiative,
    AgendaProxy,
}

// ─── GameState ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameState {
    // Core identity
    pub id: String,
    pub round: i32,
    pub phase: GamePhase,
    pub sub_phase: Option<ActionSubPhase>,
    pub agenda_phase: Option<AgendaPhase>,

    // Player tracking
    pub players: BTreeMap<PlayerId, PlayerState>,
    pub player_order: Vec<PlayerId>,
    pub player_count: i32,

    // Initiative and agenda
    pub initiative_player: Option<PlayerId>,
    pub current_agenda_player: Option<PlayerId>,
    pub agenda_tokens: HashMap<PlayerId, i32>,

    // Strategy cards
    pub strategy_deck: Vec<StrategyCard>,
    pub revealed_strategies: Vec<StrategyCard>,
    pub secret_strategies: HashMap<PlayerId, StrategyCard>,
    pub passed: HashSet<PlayerId>,

    // Galaxy
    pub systems: BTreeMap<SystemId, SystemState>,
    pub planets: BTreeMap<PlanetId, PlanetState>,
    pub planet_to_system: HashMap<PlanetId, SystemId>,
    pub exploration_map: HashMap<PlayerId, HashSet<SystemId>>,

    // Thunder's Edge
    pub expedition_tiles: Vec<ExpeditionTileState>,
    pub edge_token: Option<PlayerId>,
    pub edge_faction: Option<FactionId>,

    // Agenda state
    pub agenda_card: Option<AgendaCardState>,
    pub laws: Vec<LawState>,
    pub agenda_results: Vec<AgendaResult>,

    // Game state
    pub victory_conditions: Vec<VictoryCondition>,
    pub winner: Option<PlayerId>,
    pub game_over: bool,

    // Event and timing
    pub event_log: Vec<EventRecord>,
    pub current_events: Vec<EventRecord>,
    pub active_event: Option<EventRecord>,

    // RNG state
    pub rng_seed: u64,

    // Metadata
    pub content_version: String,
    pub schema_version: i32,
}

// ─── PlayerState ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerState {
    // Identity
    pub id: PlayerId,
    pub faction_id: FactionId,

    // Fleet
    pub fleet: HashMap<UnitTypeId, i32>,
    pub fleet_in_systems: HashMap<SystemId, HashMap<UnitTypeId, i32>>,
    pub fleet_on_planets: HashMap<PlanetId, HashMap<UnitTypeId, i32>>,

    // Resources
    pub commodity: i32,
    pub influence: i32,
    pub production: i32,
    pub fuel: i32,
    pub pips: i32,
    pub action_pips: i32,
    // Command token pools (LRR 52.4)
    pub tactic_tokens: i32,
    pub fleet_tokens: i32,
    pub strategic_tokens: i32,

    // Technology
    pub technologies: HashSet<TechnologyId>,
    pub tech_levels: HashMap<TechnologyId, i32>,

    // Cards
    pub action_cards: Vec<ActionCardState>,
    pub strategy_discard: Vec<StrategyCard>,
    pub secret_objective: Option<SecretObjectiveState>,
    pub objectives: Vec<ObjectiveState>,
    pub completed_objectives: Vec<ObjectiveState>,
    pub secret_completed: Vec<SecretObjectiveState>,

    // Leader
    pub leaders: Vec<LeaderState>,
    pub active_leader: Option<LeaderState>,
    pub leader_fatigue: Vec<LeaderId>,

    // Promissory notes
    pub promissory_notes_given: Vec<PromissoryNoteState>,
    pub promissory_notes_received: Vec<PromissoryNoteState>,

    // Relics
    pub relics: Vec<RelicState>,

    // Fragments
    pub fragments: HashMap<FragmentId, i32>,

    // Tokens
    pub tokens: HashMap<Token, i32>,

    // Control and casualties
    pub control_tokens: HashSet<PlanetId>,
    pub casualties: HashMap<SystemId, i32>,
    pub retreat_tokens: HashMap<SystemId, i32>,
    pub invasion_tokens: HashMap<PlanetId, i32>,

    // Thunder's Edge
    pub expedition_tokens: i32,
    pub edge_fragments: HashMap<FragmentId, i32>,
    pub edge_breakthroughs: Vec<BreakthroughState>,

    // Scoring
    pub score: i32,
    pub objective_score: i32,
    pub secret_score: i32,
    pub relic_score: i32,
    pub fragment_score: i32,
    pub edge_score: i32,

    // Economy
    pub trade_routes: i32,
    pub trade_goods: i32,
    pub trade_income: i32,
    pub economic_score: i32,
    pub economic_tokens: i32,

    // Military
    pub military_score: i32,
    pub military_tokens: i32,

    // Economy/production
    pub production_tokens: i32,
    pub home_planets_controlled: i32,

    // Other
    pub home_system: SystemId,
    pub home_planets: Vec<PlanetId>,
    pub homeworld_ability: bool,
    pub has_initiative: bool,
    pub has_agenda_token: bool,
    pub has_agenda_proxy: bool,
    pub has_victory_point: bool,
    pub has_fatigued_leader: bool,
    pub has_fatigued_leader_ability: bool,
    pub has_command_token: bool,
    pub has_exhausted_fleet: bool,
    pub has_exhausted_planet: bool,
    pub has_broken_promissory: bool,
    pub has_rebel_fleet: bool,
    pub has_sabotage_token: bool,
    pub has_infantry_in_play: bool,
    pub has_pds_in_play: bool,
    pub has_home_planet_in_play: bool,
    pub has_fleet_in_home: bool,
    pub has_fleet_in_play: bool,
    pub has_leader_in_play: bool,
    pub has_tech_ability: bool,
    pub has_unit_ability: bool,
    pub has_card_effect: bool,
    pub has_relic: bool,
    pub has_fragment: bool,
    pub has_edge: bool,
    pub has_completed_objective: bool,
    pub has_completed_secret: bool,
    pub has_promissory: bool,
    pub has_law_effect: bool,
    pub has_agenda_effect: bool,
    
    // Strategy card flags
    pub has_war: bool,
    pub free_research: bool,
    
    pub edge_faction: Option<FactionId>,
    pub edge_token: Option<PlayerId>,
}

impl Default for PlayerState {
    fn default() -> Self {
        Self {
            id: PlayerId::new(""),
            faction_id: FactionId::new(""),
            fleet: HashMap::new(),
            fleet_in_systems: HashMap::new(),
            fleet_on_planets: HashMap::new(),
            commodity: 0,
            influence: 0,
            production: 0,
            fuel: 0,
            pips: 0,
            action_pips: 0,
            // LRR 52.4: each player starts with 3 tactic, 3 fleet, 2 strategic tokens
            tactic_tokens: 3,
            fleet_tokens: 3,
            strategic_tokens: 2,
            technologies: HashSet::new(),
            tech_levels: HashMap::new(),
            action_cards: vec![],
            strategy_discard: vec![],
            secret_objective: None,
            objectives: vec![],
            completed_objectives: vec![],
            secret_completed: vec![],
            leaders: vec![],
            active_leader: None,
            leader_fatigue: vec![],
            promissory_notes_given: vec![],
            promissory_notes_received: vec![],
            relics: vec![],
            fragments: HashMap::new(),
            tokens: HashMap::new(),
            control_tokens: HashSet::new(),
            casualties: HashMap::new(),
            retreat_tokens: HashMap::new(),
            invasion_tokens: HashMap::new(),
            score: 0,
            objective_score: 0,
            secret_score: 0,
            relic_score: 0,
            fragment_score: 0,
            edge_score: 0,
            trade_routes: 0,
            trade_goods: 0,
            trade_income: 0,
            economic_score: 0,
            economic_tokens: 0,
            military_score: 0,
            military_tokens: 0,
            production_tokens: 0,
            home_planets_controlled: 0,
            home_system: SystemId::new(""),
            home_planets: vec![],
            homeworld_ability: false,
            has_initiative: false,
            has_agenda_token: false,
            has_agenda_proxy: false,
            has_victory_point: false,
            has_fatigued_leader: false,
            has_fatigued_leader_ability: false,
            has_command_token: false,
            has_exhausted_fleet: false,
            has_exhausted_planet: false,
            has_broken_promissory: false,
            has_rebel_fleet: false,
            has_sabotage_token: false,
            has_infantry_in_play: false,
            has_pds_in_play: false,
            has_home_planet_in_play: false,
            has_fleet_in_home: false,
            has_fleet_in_play: false,
            has_leader_in_play: false,
            has_tech_ability: false,
            has_unit_ability: false,
            has_card_effect: false,
            has_relic: false,
            has_fragment: false,
            has_edge: false,
            has_completed_objective: false,
            has_completed_secret: false,
            has_promissory: false,
            has_law_effect: false,
            has_agenda_effect: false,
            has_war: false,
            free_research: false,
            expedition_tokens: 0,
            edge_fragments: HashMap::new(),
            edge_breakthroughs: vec![],
            edge_faction: None,
            edge_token: None,
        }
    }
}

// ─── Galaxy structures ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemState {
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
    pub faction_leaders: HashMap<FactionId, Vec<LeaderState>>,
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
pub struct PlanetState {
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
    pub leaders: HashMap<FactionId, Vec<LeaderState>>,
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

impl Default for PlanetState {
    fn default() -> Self {
        Self {
            id: PlanetId::new(""),
            name: String::new(),
            system_id: SystemId::new(""),
            planet_type: String::new(),
            influence: 0,
            production: 0,
            fuel: 0,
            home_faction: None,
            owner: None,
            control_tokens: HashMap::new(),
            invasion_tokens: HashMap::new(),
            casualties: HashMap::new(),
            pds: HashMap::new(),
            leaders: HashMap::new(),
            faction_fleets: HashMap::new(),
            has_capital: false,
            has_influence: false,
            has_production: false,
            has_fuel: false,
            has_control_token: false,
            has_invasion_token: false,
            has_casualty: false,
            has_pds: false,
            has_leader: false,
            has_fleet: false,
            has_home: false,
            has_owner: false,
            has_exhausted: false,
            has_rebel_fleet: false,
            has_fatigued_leader: false,
            has_broken_promissory: false,
            has_sabotage: false,
            has_infantry: false,
            has_infiltration: false,
        }
    }
}

// ─── Card and artifact structures ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionCardState {
    pub id: ActionCardId,
    pub owner: PlayerId,
    pub exhausted: bool,
    pub used: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderState {
    pub id: LeaderId,
    pub ability: String,
    pub active: bool,
    pub fatigued: bool,
    pub system_id: Option<SystemId>,
    pub planet_id: Option<PlanetId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromissoryNoteState {
    pub id: PromissoryNoteId,
    pub giver: PlayerId,
    pub receiver: PlayerId,
    pub broken: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelicState {
    pub id: RelicId,
    pub owner: Option<PlayerId>,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectiveState {
    pub id: ObjectiveId,
    pub completed: bool,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecretObjectiveState {
    pub id: SecretObjectiveId,
    pub completed: bool,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaCardState {
    pub id: AgendaId,
    pub title: String,
    pub effects: Vec<AgendaEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaEffect {
    pub target: String,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawState {
    pub id: LawId,
    pub active: bool,
    pub effects: Vec<LawEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LawEffect {
    pub target: String,
    pub value: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgendaResult {
    pub phase: AgendaPhase,
    pub winner: PlayerId,
    pub score: i32,
    pub effects: Vec<AgendaEffect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpeditionTileState {
    pub id: ExpeditionTileId,
    pub revealed: bool,
    pub claimed: Option<PlayerId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakthroughState {
    pub id: BreakthroughId,
    pub claimed: bool,
}

// ─── Event and timing ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventRecord {
    pub id: EventId,
    pub event_type: String,
    pub source: PlayerId,
    pub target: Option<String>,
    pub timestamp: i32,
    pub effects: Vec<EventEffect>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEffect {
    pub target: String,
    pub value: i32,
}

// ─── GameState builder ─────────────────────────────────────────────────────────

impl GameState {
    /// Create a new game state with the given parameters.
    pub fn new(id: String, rng_seed: u64, content_version: String, player_count: i32) -> Self {
        Self {
            id,
            round: 1,
            phase: GamePhase::Setup,
            sub_phase: None,
            agenda_phase: None,
            players: BTreeMap::new(),
            player_order: vec![],
            player_count,
            initiative_player: None,
            current_agenda_player: None,
            agenda_tokens: HashMap::new(),
            strategy_deck: vec![],
            revealed_strategies: vec![],
            secret_strategies: HashMap::new(),
            passed: HashSet::new(),
            systems: BTreeMap::new(),
            planets: BTreeMap::new(),
            planet_to_system: HashMap::new(),
            exploration_map: HashMap::new(),
            expedition_tiles: vec![],
            edge_token: None,
            edge_faction: None,
            agenda_card: None,
            laws: vec![],
            agenda_results: vec![],
            victory_conditions: vec![],
            winner: None,
            game_over: false,
            event_log: vec![],
            current_events: vec![],
            active_event: None,
            rng_seed,
            content_version,
            schema_version: 1,
        }
    }

    /// Add a player to the game state.
    pub fn add_player(&mut self, ps: PlayerState) {
        self.players.insert(ps.id.clone(), ps);
    }

    /// Get a mutable reference to a player's state.
    pub fn player_mut(&mut self, pid: &PlayerId) -> Option<&mut PlayerState> {
        self.players.get_mut(pid)
    }

    /// Get a reference to a player's state.
    pub fn player(&self, pid: &PlayerId) -> Option<&PlayerState> {
        self.players.get(pid)
    }

    /// Check if a player has passed for strategy selection.
    pub fn has_passed(&self, pid: &PlayerId) -> bool {
        self.passed.contains(pid)
    }

    /// Mark a player as having passed for strategy selection.
    pub fn mark_passed(&mut self, pid: PlayerId) {
        self.passed.insert(pid);
    }

    /// Reset passed set for a new round.
    pub fn reset_passed(&mut self) {
        self.passed.clear();
    }

    /// Get players in clockwise order starting from the given player.
    pub fn clockwise_from(&self, start: &PlayerId) -> Vec<PlayerId> {
        if let Some(pos) = self.player_order.iter().position(|p| p == start) {
            let mut result = vec![];
            for i in 0..self.player_order.len() {
                let idx = (pos + i) % self.player_order.len();
                result.push(self.player_order[idx].clone());
            }
            result
        } else {
            self.player_order.clone()
        }
    }

    /// Reveal a strategy card for a player.
    pub fn reveal_strategy(&mut self, pid: PlayerId, strategy: StrategyCard) {
        self.revealed_strategies.push(strategy);
        self.secret_strategies.insert(pid, strategy);
    }

    /// Advance to the next agenda phase.
    pub fn advance_agenda_phase(&mut self) {
        let next = match self.agenda_phase {
            None | Some(AgendaPhase::Political) => Some(AgendaPhase::Economic),
            Some(AgendaPhase::Economic) => Some(AgendaPhase::Military),
            Some(AgendaPhase::Military) => None,
        };
        self.agenda_phase = next;
    }

    /// Check if agenda phase is complete.
    pub fn agenda_complete(&self) -> bool {
        self.agenda_phase.is_none()
    }

    /// Record an agenda result.
    pub fn record_agenda_result(&mut self, phase: AgendaPhase, winner: PlayerId, score: i32) {
        self.agenda_results.push(AgendaResult {
            phase,
            winner,
            score,
            effects: vec![],
        });
    }

    /// Record an event in the event log.
    pub fn record_event(&mut self, event: EventRecord) {
        self.event_log.push(event);
    }

    /// Add a system to the game state.
    pub fn add_system(&mut self, sys: SystemState) {
        self.systems.insert(sys.id.clone(), sys);
    }

    /// Add a planet to the game state.
    pub fn add_planet(&mut self, planet: PlanetState) {
        self.planets.insert(planet.id.clone(), planet);
    }

    /// Map a planet to its system.
    pub fn map_planet_to_system(&mut self, planet_id: PlanetId, system_id: SystemId) {
        self.planet_to_system.insert(planet_id, system_id);
    }

    /// Initialize agenda tokens for all players.
    pub fn init_agenda_tokens(&mut self) {
        for pid in &self.player_order {
            let count = if pid == self.initiative_player.as_ref().unwrap() {
                2
            } else {
                1
            };
            self.agenda_tokens.insert(pid.clone(), count);
        }
    }

    /// Transfer agenda token from one player to another.
    pub fn transfer_agenda_token(&mut self, from: &PlayerId, to: &PlayerId) {
        if let Some(count) = self.agenda_tokens.get_mut(from) {
            if *count > 0 {
                *count -= 1;
                let to_count = self.agenda_tokens.entry(to.clone()).or_insert(0);
                *to_count += 1;
            }
        }
    }

    // ─── Objective scoring ─────────────────────────────────────────────────────

    /// Score a public objective for a player.
    pub fn score_objective(&mut self, player: &PlayerId, objective: &ObjectiveState) -> i32 {
        let mut vp = 0;
        
        // Check if objective conditions are met (simplified)
        if let Some(ps) = self.players.get(player) {
            // Control tokens count toward objectives
            vp += ps.control_tokens.len() as i32;
            
            // Completed objectives count
            vp += ps.completed_objectives.len() as i32;
        }
        
        // Record completion
        if let Some(ps) = self.players.get_mut(player) {
            if !ps.completed_objectives.iter().any(|o| o.id == objective.id) {
                ps.completed_objectives.push(objective.clone());
            }
        }
        
        vp
    }

    /// Check if a secret objective is completed for a player.
    pub fn check_secret_objective(&self, player: &PlayerId, secret: &SecretObjectiveState) -> bool {
        if let Some(ps) = self.players.get(player) {
            // Simplified check - in full implementation, check specific conditions
            !ps.control_tokens.is_empty() || !ps.technologies.is_empty() || !ps.fleet.is_empty()
        } else {
            false
        }
    }

    // ─── Technology research ───────────────────────────────────────────────────

    /// Research a technology for a player.
    pub fn research_technology(
        &mut self,
        player: &PlayerId,
        tech: TechnologyId,
        cost: i32,
    ) -> std::result::Result<bool, String> {
        if let Some(ps) = self.players.get_mut(player) {
            // Check if player can afford the technology
            if ps.commodity >= cost {
                ps.commodity -= cost;
                ps.technologies.insert(tech.clone());
                
                // Update tech level
                let level = ps.tech_levels.entry(tech.clone()).or_insert(0);
                *level += 1;
                
                Ok(true)
            } else {
                Ok(false)
            }
        } else {
            Err("Player not found".to_string())
        }
    }

    /// Get the tech level for a player and technology.
    pub fn get_tech_level(&self, player: &PlayerId, tech: &TechnologyId) -> i32 {
        self.players.get(player)
            .and_then(|ps| ps.tech_levels.get(tech))
            .copied()
            .unwrap_or(0)
    }

    // ─── Leader abilities ──────────────────────────────────────────────────────

    /// Activate a leader for a player.
    pub fn activate_leader(&mut self, player: &PlayerId, leader: LeaderState) -> std::result::Result<(), String> {
        if let Some(ps) = self.players.get_mut(player) {
            // Check if leader is available (not fatigued)
            if !ps.leader_fatigue.contains(&leader.id) {
                ps.active_leader = Some(leader.clone());
                Ok(())
            } else {
                Err("Leader is fatigued".to_string())
            }
        } else {
            Err("Player not found".to_string())
        }
    }

    /// Fatigue a leader.
    pub fn fatigue_leader(&mut self, player: &PlayerId, leader_id: LeaderId) {
        if let Some(ps) = self.players.get_mut(player) {
            if !ps.leader_fatigue.contains(&leader_id) {
                ps.leader_fatigue.push(leader_id);
            }
        }
    }

    /// Refresh all leaders for a player (typically at round start).
    pub fn refresh_leaders(&mut self, player: &PlayerId) {
        if let Some(ps) = self.players.get_mut(player) {
            ps.leader_fatigue.clear();
        }
    }

    // ─── Relic handling ────────────────────────────────────────────────────────

    /// Award a relic to a player.
    pub fn award_relic(&mut self, player: &PlayerId, relic: RelicState) {
        if let Some(ps) = self.players.get_mut(player) {
            if !ps.relics.iter().any(|r| r.id == relic.id) {
                ps.relics.push(relic);
            }
        }
    }

    /// Check if a player has a specific relic.
    pub fn has_relic(&self, player: &PlayerId, relic_id: &RelicId) -> bool {
        self.players.get(player)
            .map(|ps| ps.relics.iter().any(|r| r.id == *relic_id))
            .unwrap_or(false)
    }
}

// ─── Victory conditions ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VictoryCondition {
    pub type_: String,
    pub value: i32,
}
