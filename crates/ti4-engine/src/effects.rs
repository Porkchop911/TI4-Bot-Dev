//! Game state mutation effects.
//!
//! All effects are pure functions that take a GameState and return a new GameState.
//! This ensures deterministic, testable state transitions.

use ti4_model::*;

/// Apply a game effect to the state.
pub struct EffectEngine;

impl EffectEngine {
    pub fn new() -> Self { Self }

    /// Apply a combat effect.
    pub fn apply_combat(
        &self,
        game: &mut GameState,
        attacker: &PlayerId,
        defender: &PlayerId,
        attacker_fleet: &FleetState,
        defender_fleet: &FleetState,
    ) -> CombatOutcome {
        // Calculate combat results
        let attacker_score = self.calculate_combat_score(game, attacker, attacker_fleet);
        let defender_score = self.calculate_combat_score(game, defender, defender_fleet);

        // Determine casualties
        let attacker_casualties = self.calculate_casualties(attacker_fleet, defender_score);
        let defender_casualties = self.calculate_casualties(defender_fleet, attacker_score);

        CombatOutcome {
            attacker_casualties,
            defender_casualties,
            winner: if attacker_score > defender_score {
                CombatSide::Attacker
            } else if defender_score > attacker_score {
                CombatSide::Defender
            } else {
                CombatSide::Attacker // Tie goes to attacker
            },
        }
    }

    /// Calculate combat score for a fleet.
    fn calculate_combat_score(&self, game: &GameState, player: &PlayerId, fleet: &FleetState) -> i32 {
        let mut score = 0;

        // Base combat values
        for entry in &fleet.unit_types {
            // In full implementation, lookup unit combat value from content
            let combat = 1; // Default combat value
            score += combat * entry.count;
        }

        // Apply leader bonuses
        if let Some(ps) = game.players.get(player) {
            if let Some(active) = &ps.active_leader {
                if active.active {
                    score += 1; // Leader bonus
                }
            }
        }

        score
    }

    /// Calculate casualties for a fleet given incoming damage.
    fn calculate_casualties(&self, fleet: &FleetState, incoming_damage: i32) -> Vec<UnitFleetEntry> {
        if incoming_damage <= 0 {
            return vec![];
        }

        let mut remaining = incoming_damage;
        let mut casualties = vec![];

        // Casualties are distributed by unit type priority
        for entry in &fleet.unit_types {
            if remaining <= 0 {
                break;
            }
            let lost = std::cmp::min(entry.count, remaining);
            casualties.push(UnitFleetEntry {
                unit_type: entry.unit_type.clone(),
                count: lost,
                upgraded: entry.upgraded,
            });
            remaining -= lost;
        }

        casualties
    }

    /// Apply production effects for a player.
    pub fn apply_production(
        &self,
        game: &mut GameState,
        player: &PlayerId,
    ) -> i32 {
        let ps = game.players.get(player).cloned().unwrap_or_default();
        let production = ps.production;

        // Apply trade income
        let trade_income = ps.trade_income;

        production + trade_income
    }

    /// Apply agenda effects.
    pub fn apply_agenda_effects(
        &self,
        game: &mut GameState,
        phase: AgendaPhase,
        winner: &PlayerId,
        effects: &[AgendaEffect],
    ) {
        for effect in effects {
            match effect.target.as_str() {
                "vp" => {
                    if let Some(ps) = game.players.get_mut(winner) {
                        ps.score += effect.value;
                    }
                }
                "commodity" => {
                    if let Some(ps) = game.players.get_mut(winner) {
                        ps.commodity += effect.value;
                    }
                }
                "influence" => {
                    if let Some(ps) = game.players.get_mut(winner) {
                        ps.influence += effect.value;
                    }
                }
                _ => {}
            }
        }
    }

    // ─── Strategy card effects ───────────────────────────────────────────────

    /// Apply Trade strategy card effects.
    pub fn apply_trade_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Gain commodity based on strategy card value (simplified: +2 commodity)
        if let Some(ps) = game.players.get_mut(player) {
            ps.commodity += 2;
        }
    }

    /// Apply Diplomacy strategy card effects.
    pub fn apply_diplomacy_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Gain influence tokens
        if let Some(ps) = game.players.get_mut(player) {
            ps.influence += 2;
        }
    }

    /// Apply War strategy card effects.
    pub fn apply_war_effect(&self, game: &mut GameState, player: &PlayerId) {
        // War strategy provides combat bonus (tracked via leader or direct modifier)
        // For now, mark that player has War strategy for initiative priority
        if let Some(ps) = game.players.get_mut(player) {
            ps.has_war = true;
        }
    }

    /// Apply Rebellion strategy card effects.
    pub fn apply_rebellion_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Rebellion removes control tokens from other players
        // Simplified: gain influence for each control token removed
        // First pass: collect control tokens to remove
        let mut total_removed = 0i32;
        for other_pid in game.player_order.iter() {
            if other_pid != player {
                if let Some(other_ps) = game.players.get(other_pid) {
                    total_removed += other_ps.control_tokens.len() as i32;
                }
            }
        }
        
        // Second pass: clear control tokens
        for other_pid in game.player_order.iter() {
            if other_pid != player {
                if let Some(other_ps) = game.players.get_mut(other_pid) {
                    other_ps.control_tokens.clear();
                }
            }
        }
        
        // Finally: add influence to the player
        if let Some(ps) = game.players.get_mut(player) {
            ps.influence += total_removed;
        }
    }

    /// Apply Technology strategy card effects.
    pub fn apply_technology_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Technology strategy allows free research
        // Mark that player can research for free this round
        if let Some(ps) = game.players.get_mut(player) {
            ps.free_research = true;
        }
    }

    /// Apply the effect of a revealed strategy card.
    pub fn apply_strategy_effect(
        &self,
        game: &mut GameState,
        player: &PlayerId,
        card: &StrategyCard,
    ) {
        match card {
            StrategyCard::Trade => self.apply_trade_effect(game, player),
            StrategyCard::Diplomacy => self.apply_diplomacy_effect(game, player),
            StrategyCard::War => self.apply_war_effect(game, player),
            StrategyCard::Rebellion => self.apply_rebellion_effect(game, player),
            StrategyCard::Technology => self.apply_technology_effect(game, player),
            StrategyCard::Unknown => {}
        }
    }
}

/// Outcome of a combat engagement.
#[derive(Debug, Clone)]
pub struct CombatOutcome {
    pub attacker_casualties: Vec<UnitFleetEntry>,
    pub defender_casualties: Vec<UnitFleetEntry>,
    pub winner: CombatSide,
}
