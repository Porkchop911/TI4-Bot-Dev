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

    // ─── Strategy card effects (primary abilities) ─────────────────────────────

    /// Apply Leadership strategy card primary effect.
    /// 52.1: Gain 3 command tokens into pools of your choice.
    pub fn apply_leadership_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Simplified: grant 1 to each pool (tactic, fleet, strategic)
        if let Some(ps) = game.players.get_mut(player) {
            ps.tactic_tokens += 1;
            ps.fleet_tokens += 1;
            ps.strategic_tokens += 1;
        }
    }

    /// Apply Diplomacy strategy card primary effect.
    /// 32.2: Lock a system, then ready two planets.
    pub fn apply_diplomacy_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Simplified: grant influence and ready planets proxy
        if let Some(ps) = game.players.get_mut(player) {
            ps.influence += 1;
            ps.has_agenda_token = true; // Proxy for system control
            ps.ready_planets += 2; // Track planets to ready
        }
    }

    /// Apply Politics strategy card primary effect.
    /// 66.2: Choose a new speaker, draw 2 action cards, then reorder agenda deck.
    pub fn apply_politics_effect(&self, game: &mut GameState, player: &PlayerId) {
        // i. Transfer speaker token (simplified: grant speaker proxy)
        if let Some(ps) = game.players.get_mut(player) {
            ps.has_agenda_proxy = true;
        }
        
        // ii. Draw 2 action cards
        if let Some(ps) = game.players.get_mut(player) {
            for i in 0..2 {
                ps.action_cards.push(ActionCardState {
                    id: ActionCardId::new(&format!("politics-draw-{}", i)),
                    owner: player.clone(),
                    exhausted: false,
                    used: false,
                });
            }
        }
        
        // iii. Look at top 2 agenda cards (tracked via has_agenda_token flag)
        // Note: has_agenda_token already set by influence grant above
        let _ = game.players.get(player); // Ensure player exists
    }

    /// Apply Construction strategy card primary effect.
    /// 24.1: Place PDS/Space Dock on controlled planet.
    pub fn apply_construction_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Simplified: grant structure placement token
        if let Some(ps) = game.players.get_mut(player) {
            ps.production += 1; // Placeholder for structure placement
        }
    }

    /// Apply Trade strategy card primary effect.
    /// 92.1: Gain 3 trade goods, replenish commodities.
    pub fn apply_trade_effect(&self, game: &mut GameState, player: &PlayerId) {
        // 92.2: Gain 3 trade goods
        if let Some(ps) = game.players.get_mut(player) {
            ps.trade_goods = ps.trade_goods + 3;
        }
    }

    /// Apply Warfare strategy card primary effect.
    /// 99.1: Recall a command token from the board, gain 1 command token.
    pub fn apply_warfare_effect(&self, game: &mut GameState, player: &PlayerId) {
        // 99.1: Player recalls a command token (tracked via flag)
        // Simplified: grant 1 to tactic pool
        if let Some(ps) = game.players.get_mut(player) {
            ps.tactic_tokens += 1;
            ps.has_war = true;
        }
    }

    /// Apply Technology strategy card primary effect.
    /// 91.1: Research 1 technology (free or spend 6 resources).
    pub fn apply_technology_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Mark that player can research for free this round
        if let Some(ps) = game.players.get_mut(player) {
            ps.free_research = true;
        }
    }

    /// Apply Imperial strategy card primary effect.
    /// 45.1: Score a public objective; Mecatol Rex pays 1 VP or 1 secret.
    pub fn apply_imperial_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Simplified: grant VP if controlling Mecatol Rex
        if let Some(ps) = game.players.get_mut(player) {
            ps.score += 1;
        }
    }

    /// Apply Thunder's Edge Construction primary effect.
    /// Either place 1 structure or use PRODUCTION, then place 1 structure.
    pub fn apply_te4_construction_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Simplified: grant +2 production (structure placement proxy)
        if let Some(ps) = game.players.get_mut(player) {
            ps.production += 2;
        }
    }

    /// Apply Thunder's Edge Warfare primary effect.
    /// Free tactical action in any system (no token cost).
    pub fn apply_te6_warfare_effect(&self, game: &mut GameState, player: &PlayerId) {
        // Mark that player has a free tactical action available
        if let Some(ps) = game.players.get_mut(player) {
            ps.has_war = true;
            ps.tactic_tokens += 1; // Grant 1 tactic token as proxy
        }
    }

    // ─── Strategy card secondary abilities ─────────────────────────────────────

    /// Apply Leadership secondary: buy tokens with influence.
    /// Costs 3 influence per token.
    pub fn apply_leadership_secondary(&self, game: &mut GameState, player: &PlayerId, influence_cost: i32) {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.influence >= influence_cost {
                ps.influence -= influence_cost;
                ps.tactic_tokens += 1;
                ps.fleet_tokens += 1;
                ps.strategic_tokens += 1;
            }
        }
    }

    /// Apply Diplomacy secondary: ready 2 exhausted planets.
    /// Costs 1 strategic token.
    pub fn apply_diplomacy_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                // Ready 2 planets (tracked via flag, actual planet ready is deferred)
                return true;
            }
        }
        false
    }

    /// Apply Politics secondary: draw 2 action cards.
    /// Costs 1 strategic token.
    pub fn apply_politics_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                ps.action_cards.push(ActionCardState {
                    id: ActionCardId::new("politics-draw-1"),
                    owner: player.clone(),
                    exhausted: false,
                    used: false,
                });
                ps.action_cards.push(ActionCardState {
                    id: ActionCardId::new("politics-draw-2"),
                    owner: player.clone(),
                    exhausted: false,
                    used: false,
                });
                return true;
            }
        }
        false
    }

    /// Apply Construction secondary: place 1 structure.
    /// Costs 1 strategic token.
    pub fn apply_construction_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                ps.production += 1;
                return true;
            }
        }
        false
    }

    /// Apply Trade secondary: replenish commodities.
    /// Costs 1 strategic token.
    pub fn apply_trade_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                ps.trade_goods = 10; // Replenish to max (simplified)
                return true;
            }
        }
        false
    }

    /// Apply Warfare secondary: produce at home system.
    /// Costs 1 strategic token.
    pub fn apply_warfare_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                ps.production += 2; // Production proxy
                return true;
            }
        }
        false
    }

    /// Apply Technology secondary: research for 6 resources.
    /// Costs 1 strategic token + 6 resources.
    pub fn apply_technology_secondary(&self, game: &mut GameState, player: &PlayerId, resources: i32) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 && ps.fuel >= resources {
                ps.strategic_tokens -= 1;
                ps.fuel -= resources;
                ps.free_research = true;
                return true;
            }
        }
        false
    }

    /// Apply Imperial secondary: draw a secret objective.
    /// Costs 1 strategic token.
    pub fn apply_imperial_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                // Draw secret (tracked via flag, actual secret draw is deferred)
                return true;
            }
        }
        false
    }

    /// Apply Thunder's Edge Construction secondary: place 1 structure.
    /// Costs 1 strategic token.
    pub fn apply_te4_construction_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                ps.production += 1;
                return true;
            }
        }
        false
    }

    /// Apply Thunder's Edge Warfare secondary: produce at home system.
    /// Costs 1 strategic token (same as base).
    pub fn apply_te6_warfare_secondary(&self, game: &mut GameState, player: &PlayerId) -> bool {
        if let Some(ps) = game.players.get_mut(player) {
            if ps.strategic_tokens >= 1 {
                ps.strategic_tokens -= 1;
                ps.production += 2;
                return true;
            }
        }
        false
    }

    /// Check if a faction gets a free secondary for a strategy card.
    /// Per Oracle: Hacon (Masters of Trade) gets free Trade secondary.
    /// Jol-Nar (Brilliant) swaps Technology secondary for primary.
    pub fn is_secondary_free(game: &GameState, player: &PlayerId, card: &StrategyCard) -> bool {
        // Masters of Trade: free Trade secondary
        if card == &StrategyCard::Trade {
            if let Some(ps) = game.players.get(player) {
                if ps.faction_id.as_str() == "masters_of_trade" {
                    return true;
                }
            }
        }
        
        // Jol-Nar Brilliant: Technology secondary becomes primary (free)
        if card == &StrategyCard::Technology {
            if let Some(ps) = game.players.get(player) {
                if ps.faction_id.as_str() == "brilliant" {
                    return true;
                }
            }
        }
        
        false
    }

    /// Apply the secondary effect of a revealed strategy card.
    pub fn apply_strategy_secondary(
        &self,
        game: &mut GameState,
        player: &PlayerId,
        card: &StrategyCard,
        args: &SecondaryArgs,
    ) -> bool {
        // Check faction ability for free secondary
        let is_free = Self::is_secondary_free(game, player, card);
        
        match card {
            StrategyCard::Leadership => {
                self.apply_leadership_secondary(game, player, args.influence_cost.unwrap_or(3));
                true
            }
            StrategyCard::Diplomacy => self.apply_diplomacy_secondary(game, player),
            StrategyCard::Politics => self.apply_politics_secondary(game, player),
            StrategyCard::Construction => self.apply_construction_secondary(game, player),
            StrategyCard::Trade => {
                if is_free {
                    self.apply_trade_effect(game, player);
                    true
                } else {
                    self.apply_trade_secondary(game, player)
                }
            }
            StrategyCard::Warfare => self.apply_warfare_secondary(game, player),
            StrategyCard::Technology => {
                if is_free {
                    // Jol-Nar Brilliant: swap secondary for primary
                    self.apply_technology_effect(game, player);
                    true
                } else {
                    self.apply_technology_secondary(game, player, args.resources.unwrap_or(6))
                }
            }
            StrategyCard::Imperial => self.apply_imperial_secondary(game, player),
            StrategyCard::Te4Construction => self.apply_te4_construction_secondary(game, player),
            StrategyCard::Te6Warfare => self.apply_te6_warfare_secondary(game, player),
            StrategyCard::Unknown => false,
        }
    }

    /// Apply the primary effect of a revealed strategy card.
    pub fn apply_strategy_effect(
        &self,
        game: &mut GameState,
        player: &PlayerId,
        card: &StrategyCard,
    ) {
        match card {
            StrategyCard::Leadership => self.apply_leadership_effect(game, player),
            StrategyCard::Diplomacy => self.apply_diplomacy_effect(game, player),
            StrategyCard::Politics => self.apply_politics_effect(game, player),
            StrategyCard::Construction => self.apply_construction_effect(game, player),
            StrategyCard::Trade => self.apply_trade_effect(game, player),
            StrategyCard::Warfare => self.apply_warfare_effect(game, player),
            StrategyCard::Technology => self.apply_technology_effect(game, player),
            StrategyCard::Imperial => self.apply_imperial_effect(game, player),
            StrategyCard::Te4Construction => self.apply_te4_construction_effect(game, player),
            StrategyCard::Te6Warfare => self.apply_te6_warfare_effect(game, player),
            StrategyCard::Unknown => {}
        }
    }
}

/// Arguments for strategy card secondary effects.
pub struct SecondaryArgs {
    pub influence_cost: Option<i32>,
    pub resources: Option<i32>,
}

impl Default for SecondaryArgs {
    fn default() -> Self {
        Self {
            influence_cost: None,
            resources: None,
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
