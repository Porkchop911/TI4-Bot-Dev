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
}

/// Outcome of a combat engagement.
#[derive(Debug, Clone)]
pub struct CombatOutcome {
    pub attacker_casualties: Vec<UnitFleetEntry>,
    pub defender_casualties: Vec<UnitFleetEntry>,
    pub winner: CombatSide,
}
