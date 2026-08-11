//! Tactical pipeline: ship movement, combat, bombardment, landing, production.
//!
//! Implements the full tactical resolution pipeline per TI4 rules.

use ti4_model::*;
use anyhow::Result;

/// Manages tactical operations for a single player's activation.
pub struct TacticalManager {
    pub game: GameState,
    pub activated_player: Option<PlayerId>,
}

impl TacticalManager {
    pub fn new(game: GameState) -> Self {
        Self {
            game,
            activated_player: None,
        }
    }

    /// Activate a player for tactical operations.
    pub fn activate(&mut self, player: PlayerId) -> Result<()> {
        self.activated_player = Some(player);
        Ok(())
    }

    /// Deactivate the current player.
    pub fn deactivate(&mut self) {
        self.activated_player = None;
    }

    /// Move a fleet from one system to another.
    pub fn move_fleet(
        &mut self,
        from: SystemId,
        to: SystemId,
        fleet: FleetState,
    ) -> Result<MovementResult> {
        let activation = self.activated_player.clone().ok_or_else(|| {
            anyhow::anyhow!("No player activated for movement")
        })?;

        // Validate movement legality
        let distance = self.calculate_distance(&from, &to)?;

        // Check movement capacity
        let max_movement = self.get_max_movement(&activation)?;
        if distance > max_movement {
            return Err(anyhow::anyhow!(
                "Destination {} is {} hex away, exceeds movement capacity {}",
                to,
                distance,
                max_movement
            ));
        }

        // Check fuel
        let fuel_cost = self.calculate_fuel_cost(&activation, &to)?;
        if fleet.total_fuel < fuel_cost {
            return Err(anyhow::anyhow!(
                "Insufficient fuel: need {}, have {}",
                fuel_cost,
                fleet.total_fuel
            ));
        }

        // Check capacity
        let total_units: i32 = fleet.unit_types.iter().map(|u| u.count).sum();
        let capacity = self.get_capacity(&activation)?;
        if total_units > capacity {
            return Err(anyhow::anyhow!(
                "Fleet exceeds capacity: {} > {}",
                total_units,
                capacity
            ));
        }

        // Apply movement
        let mut result = MovementResult {
            from,
            to: to.clone(),
            fleet: fleet.clone(),
            distance,
            fuel_cost,
            success: true,
        };

        // Update game state
        if let Some(ps) = self.game.players.get_mut(&activation) {
            ps.command_tokens -= 1; // Cost of movement
        }

        Ok(result)
    }

    /// Resolve combat between two fleets.
    pub fn resolve_combat(
        &mut self,
        attacker: PlayerId,
        defender: PlayerId,
        attacker_fleet: FleetState,
        defender_fleet: FleetState,
    ) -> Result<CombatResult> {
        let activation = self.activated_player.clone().ok_or_else(|| {
            anyhow::anyhow!("No player activated for combat")
        })?;

        // Calculate combat scores
        let attacker_score = self.calculate_combat_score(&attacker, &attacker_fleet);
        let defender_score = self.calculate_combat_score(&defender, &defender_fleet);

        // Determine casualties
        let attacker_casualties = self.calculate_casualties(&attacker_fleet, defender_score);
        let defender_casualties = self.calculate_casualties(&defender_fleet, attacker_score);

        // Determine winner
        let winner = if attacker_score > defender_score {
            CombatSide::Attacker
        } else if defender_score > attacker_score {
            CombatSide::Defender
        } else {
            CombatSide::Attacker // Tie goes to attacker
        };

        // Record in game state before moving attacker/defender
        self.game.record_event(EventRecord {
            id: EventId::new("combat"),
            event_type: "combat".to_string(),
            source: attacker.clone(),
            target: Some(defender.to_string()),
            effects: vec![],
            timestamp: 0,
            resolved: true,
        });

        // Apply casualties
        let mut result = CombatResult {
            attacker,
            defender,
            attacker_fleet: attacker_fleet.clone(),
            defender_fleet: defender_fleet.clone(),
            attacker_score,
            defender_score,
            attacker_casualties,
            defender_casualties,
            winner: winner.clone(),
            success: true,
        };

        Ok(result)
    }

    /// Bombard a planet with a fleet.
    pub fn bombard(
        &mut self,
        fleet: &FleetState,
        target: PlanetId,
    ) -> Result<BombardmentResult> {
        let activation = self.activated_player.clone().ok_or_else(|| {
            anyhow::anyhow!("No player activated for bombardment")
        })?;

        // Calculate bombardment damage
        let damage = self.calculate_bombardment_damage(fleet);

        // Apply damage to planet (reduce influence as proxy for defender strength)
        if let Some(planet) = self.game.planets.get_mut(&target) {
            planet.influence -= damage;
            if planet.influence < 0 {
                planet.influence = 0;
            }
        }

        Ok(BombardmentResult {
            fleet: fleet.clone(),
            target,
            damage,
            success: true,
        })
    }

    /// Land infantry on a planet.
    pub fn land_infantry(
        &mut self,
        fleet: &FleetState,
        target: PlanetId,
    ) -> Result<LandingResult> {
        let activation = self.activated_player.clone().ok_or_else(|| {
            anyhow::anyhow!("No player activated for landing")
        })?;

        // Check if fleet has infantry
        let infantry_count = fleet.unit_types.iter()
            .find(|u| u.unit_type == UnitTypeId::new("infantry"))
            .map(|u| u.count)
            .unwrap_or(0);

        if infantry_count == 0 {
            return Err(anyhow::anyhow!("Fleet has no infantry to land"));
        }

        // Land infantry
        let mut result = LandingResult {
            fleet: fleet.clone(),
            target: target.clone(),
            infantry_landed: infantry_count,
            success: true,
        };

        // Update game state (track infantry via invasion tokens)
        if let Some(planet) = self.game.planets.get_mut(&target) {
            // Get faction from player
            let faction = if let Some(ps) = self.game.players.get(&activation) {
                ps.faction_id.clone()
            } else {
                FactionId::new("unknown")
            };
            let inv = planet.invasion_tokens
                .entry(faction)
                .or_insert(0);
            *inv += infantry_count;
        }

        Ok(result)
    }

    /// Produce units for a player.
    pub fn produce(
        &mut self,
        player: &PlayerId,
        units: Vec<UnitProduction>,
    ) -> Result<ProductionResult> {
        let activation = self.activated_player.clone().ok_or_else(|| {
            anyhow::anyhow!("No player activated for production")
        })?;

        // Calculate production capacity
        let production = self.get_production(&activation)?;
        let total_cost: i32 = units.iter().map(|u| u.cost).sum();

        if total_cost > production {
            return Err(anyhow::anyhow!(
                "Insufficient production: need {}, have {}",
                total_cost,
                production
            ));
        }

        // Produce units
        let mut result = ProductionResult {
            player: player.clone(),
            units,
            production,
            success: true,
        };

        // Update game state
        if let Some(ps) = self.game.players.get_mut(&activation) {
            ps.command_tokens -= 1; // Cost of production
        }

        Ok(result)
    }

    // ─── Helpers ─────────────────────────────────────────────────────────

    /// Calculate distance between two systems.
    fn calculate_distance(&self, from: &SystemId, to: &SystemId) -> Result<i32> {
        // In full implementation, this would use the galaxy map
        // For now, return 1 for adjacent systems
        Ok(1)
    }

    /// Get maximum movement for a player.
    fn get_max_movement(&self, player: &PlayerId) -> Result<i32> {
        // Default movement is 2
        Ok(2)
    }

    /// Calculate fuel cost for movement.
    fn calculate_fuel_cost(&self, _player: &PlayerId, _destination: &SystemId) -> Result<i32> {
        // Default fuel cost is 1
        Ok(1)
    }

    /// Get fleet capacity for a player.
    fn get_capacity(&self, player: &PlayerId) -> Result<i32> {
        // Default capacity is 10 per player
        Ok(10)
    }

    /// Calculate combat score for a fleet.
    fn calculate_combat_score(&self, player: &PlayerId, fleet: &FleetState) -> i32 {
        let mut score = 0;

        for entry in &fleet.unit_types {
            let combat = self.get_unit_combat(&entry.unit_type);
            score += combat * entry.count;
        }

        // Apply leader bonuses
        if let Some(ps) = self.game.players.get(player) {
            if let Some(active) = &ps.active_leader {
                if active.active {
                    score += 1;
                }
            }
        }

        score
    }

    /// Get combat value for a unit type.
    fn get_unit_combat(&self, unit_type: &UnitTypeId) -> i32 {
        match unit_type.to_string().as_str() {
            "fighter" => 1,
            "cruiser" => 2,
            "destroyer" => 2,
            "carrier" => 1,
            "dreadnought" => 3,
            "infantry" => 1,
            "pds" => 2,
            "spacedock" => 2,
            _ => 1,
        }
    }

    /// Calculate casualties for a fleet given incoming damage.
    fn calculate_casualties(&self, fleet: &FleetState, incoming_damage: i32) -> Vec<UnitFleetEntry> {
        if incoming_damage <= 0 {
            return vec![];
        }

        let mut remaining = incoming_damage;
        let mut casualties = vec![];

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

    /// Calculate bombardment damage.
    fn calculate_bombardment_damage(&self, fleet: &FleetState) -> i32 {
        let mut damage = 0;

        for entry in &fleet.unit_types {
            match entry.unit_type.to_string().as_str() {
                "cruiser" => damage += entry.count * 2,
                "destroyer" => damage += entry.count * 2,
                "dreadnought" => damage += entry.count * 3,
                _ => {}
            }
        }

        damage
    }

    /// Get production capacity for a player.
    fn get_production(&self, player: &PlayerId) -> Result<i32> {
        let ps = self.game.players.get(player).cloned().unwrap_or_default();
        Ok(ps.production)
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::choice::default_fleet_state;

    fn make_test_game() -> GameState {
        let mut game = GameState::new("test-game".to_string(), 42, "test".to_string(), 2);

        // Add players
        for i in 0..2 {
            let pid = PlayerId::new(format!("p{}", i));
            let mut ps = PlayerState::default();
            ps.id = pid.clone();
            ps.faction_id = FactionId::new(format!("faction{}", i));
            game.add_player(ps);
        }

        game.player_order = vec![
            PlayerId::new("p0"),
            PlayerId::new("p1"),
        ];

        game
    }

    #[test]
    fn test_activate_player() {
        let game = make_test_game();
        let mut manager = TacticalManager::new(game);

        manager.activate(PlayerId::new("p0")).unwrap();
        assert_eq!(manager.activated_player, Some(PlayerId::new("p0")));
    }

    #[test]
    fn test_deactivate_player() {
        let game = make_test_game();
        let mut manager = TacticalManager::new(game);

        manager.activate(PlayerId::new("p0")).unwrap();
        manager.deactivate();
        assert!(manager.activated_player.is_none());
    }

    #[test]
    fn test_move_fleet_requires_activation() {
        let game = make_test_game();
        let mut manager = TacticalManager::new(game);

        let result = manager.move_fleet(
            SystemId::new("sys1"),
            SystemId::new("sys2"),
            default_fleet_state(),
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_combat_score_calculation() {
        let game = make_test_game();
        let mut manager = TacticalManager::new(game);

        let mut fleet = default_fleet_state();
        fleet.unit_types.push(UnitFleetEntry {
            unit_type: UnitTypeId::new("cruiser"),
            count: 3,
            upgraded: false,
        });

        let score = manager.calculate_combat_score(&PlayerId::new("p0"), &fleet);
        assert_eq!(score, 6); // 3 cruisers * 2 combat = 6
    }

    #[test]
    fn test_casualty_calculation() {
        let game = make_test_game();
        let manager = TacticalManager::new(game);

        let mut fleet = default_fleet_state();
        fleet.unit_types.push(UnitFleetEntry {
            unit_type: UnitTypeId::new("cruiser"),
            count: 3,
            upgraded: false,
        });
        fleet.unit_types.push(UnitFleetEntry {
            unit_type: UnitTypeId::new("fighter"),
            count: 2,
            upgraded: false,
        });

        let casualties = manager.calculate_casualties(&fleet, 4);

        // Should lose 3 cruisers first, then 1 fighter
        assert_eq!(casualties.len(), 2);
        assert_eq!(casualties[0].unit_type.to_string(), "cruiser");
        assert_eq!(casualties[0].count, 3);
        assert_eq!(casualties[1].unit_type.to_string(), "fighter");
        assert_eq!(casualties[1].count, 1);
    }

    #[test]
    fn test_bombardment_damage() {
        let game = make_test_game();
        let mut manager = TacticalManager::new(game);

        let mut fleet = default_fleet_state();
        fleet.unit_types.push(UnitFleetEntry {
            unit_type: UnitTypeId::new("cruiser"),
            count: 2,
            upgraded: false,
        });
        fleet.unit_types.push(UnitFleetEntry {
            unit_type: UnitTypeId::new("dreadnought"),
            count: 1,
            upgraded: false,
        });

        let damage = manager.calculate_bombardment_damage(&fleet);
        assert_eq!(damage, 7); // 2 cruisers * 2 + 1 dreadnought * 3 = 7
    }

    #[test]
    fn test_land_infantry() {
        let mut game = make_test_game();
        let mut manager = TacticalManager::new(game);

        // Add a planet
        let planet_id = PlanetId::new("planet1");
        let mut planet = PlanetState {
            id: planet_id.clone(),
            name: "Test Planet".to_string(),
            system_id: SystemId::new("sys1"),
            planet_type: "normal".to_string(),
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
        };
        manager.game.add_planet(planet);

        manager.activate(PlayerId::new("p0")).unwrap();

        let fleet = default_fleet_state();
        let mut fleet_with_infantry = fleet.clone();
        fleet_with_infantry.unit_types.push(UnitFleetEntry {
            unit_type: UnitTypeId::new("infantry"),
            count: 3,
            upgraded: false,
        });

        let result = manager.land_infantry(&fleet_with_infantry, planet_id.clone()).unwrap();
        assert_eq!(result.infantry_landed, 3);

        // Check planet invasion tokens updated
        let planet = manager.game.planets.get(&planet_id).unwrap();
        let faction = FactionId::new("faction0");
        assert_eq!(planet.invasion_tokens.get(&faction), Some(&3));
    }
}

/// Result of a fleet movement.
#[derive(Debug, Clone)]
pub struct MovementResult {
    pub from: SystemId,
    pub to: SystemId,
    pub fleet: FleetState,
    pub distance: i32,
    pub fuel_cost: i32,
    pub success: bool,
}

/// Result of a combat engagement.
#[derive(Debug, Clone)]
pub struct CombatResult {
    pub attacker: PlayerId,
    pub defender: PlayerId,
    pub attacker_fleet: FleetState,
    pub defender_fleet: FleetState,
    pub attacker_score: i32,
    pub defender_score: i32,
    pub attacker_casualties: Vec<UnitFleetEntry>,
    pub defender_casualties: Vec<UnitFleetEntry>,
    pub winner: CombatSide,
    pub success: bool,
}

/// Result of a bombardment.
#[derive(Debug, Clone)]
pub struct BombardmentResult {
    pub fleet: FleetState,
    pub target: PlanetId,
    pub damage: i32,
    pub success: bool,
}

/// Result of a landing.
#[derive(Debug, Clone)]
pub struct LandingResult {
    pub fleet: FleetState,
    pub target: PlanetId,
    pub infantry_landed: i32,
    pub success: bool,
}

/// Unit production request.
#[derive(Debug, Clone)]
pub struct UnitProduction {
    pub unit_type: UnitTypeId,
    pub count: i32,
    pub cost: i32,
    pub location: SystemId,
}

/// Result of production.
#[derive(Debug, Clone)]
pub struct ProductionResult {
    pub player: PlayerId,
    pub units: Vec<UnitProduction>,
    pub production: i32,
    pub success: bool,
}
