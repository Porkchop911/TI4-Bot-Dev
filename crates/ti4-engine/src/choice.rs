//! Legal action generation and choice system.
//!
//! Generates all legal actions for a player in the current game state.

use ti4_model::*;
use crate::rules::{GameAction, AgendaVote};

/// Generates legal actions for the current player.
pub struct LegalActionGenerator;

impl LegalActionGenerator {
    pub fn new() -> Self { Self }

    /// Generate all legal actions for the current player.
    pub fn generate(&self, game: &GameState) -> Vec<GameAction> {
        let mut actions = vec![];

        match game.phase {
            GamePhase::Setup => {}
            GamePhase::Action => {
                actions.extend(self.generate_strategy_actions(game));
                actions.extend(self.generate_command_actions(game));
                actions.extend(self.generate_tactical_actions(game));
            }
            GamePhase::Agenda => {
                actions.extend(self.generate_agenda_actions(game));
            }
            GamePhase::RoundEnd => {}
            GamePhase::GameEnd => {}
        }

        actions
    }

    fn generate_strategy_actions(&self, game: &GameState) -> Vec<GameAction> {
        let mut actions = vec![];

        if game.sub_phase == Some(ActionSubPhase::Strategy) {
            for pid in &game.player_order {
                if !game.secret_strategies.contains_key(pid) && !game.has_passed(pid) {
                    actions.push(GameAction::RevealStrategy);
                    actions.push(GameAction::PassStrategy);
                }
            }
        }

        actions
    }

    fn generate_command_actions(&self, game: &GameState) -> Vec<GameAction> {
        let mut actions = vec![];

        if game.sub_phase == Some(ActionSubPhase::Command) {
            actions.push(GameAction::DistributeCommandTokens);
        }

        actions
    }

    fn generate_tactical_actions(&self, game: &GameState) -> Vec<GameAction> {
        let mut actions = vec![];

        if game.sub_phase == Some(ActionSubPhase::Tactical) {
            // Generate move actions
            for system_id in game.systems.keys() {
                for planet_id in game.planets.keys() {
                    if let Some(sys_id) = game.planet_to_system.get(planet_id) {
                        if sys_id == system_id {
                            actions.push(GameAction::MoveFleet {
                                from: system_id.clone(),
                                to: system_id.clone(),
                                fleet: crate::choice::default_fleet_state(),
                            });
                        }
                    }
                }
            }

            // Generate research actions
            actions.push(GameAction::Research {
                technology: TechnologyId::new("infantry"),
            });

            // Generate explore actions
            for system_id in game.systems.keys() {
                actions.push(GameAction::Explore {
                    system: system_id.clone(),
                });
            }

            // Generate build actions
            for unit_type in [
                UnitTypeId::new("fighter"),
                UnitTypeId::new("cruiser"),
                UnitTypeId::new("destroyer"),
                UnitTypeId::new("carrier"),
                UnitTypeId::new("dreadnought"),
                UnitTypeId::new("infantry"),
                UnitTypeId::new("pds"),
                UnitTypeId::new("spacedock"),
            ] {
                actions.push(GameAction::BuildUnit {
                    unit: unit_type,
                    location: SystemId::new("test"),
                });
            }

            // Generate faction abilities
            actions.push(GameAction::UseFactionAbility);
            actions.push(GameAction::UseHomeworldAbility);
        }

        actions
    }

    fn generate_agenda_actions(&self, game: &GameState) -> Vec<GameAction> {
        let mut actions = vec![];

        if game.phase == GamePhase::Agenda {
            for pid in &game.player_order {
                actions.push(GameAction::VoteAgenda {
                    vote: AgendaVote {
                        player: pid.clone(),
                        value: 1,
                    },
                });
            }
        }

        actions
    }
}

/// Default fleet state for testing.
pub fn default_fleet_state() -> FleetState {
    FleetState {
        unit_types: vec![],
        total_movement: 0,
        total_fuel: 0,
        total_capacity: 0,
        total_casualties: 0,
        has_flagship: false,
        has_warsun: false,
        has_pds: false,
        has_infantry: false,
        has_mech: false,
        has_fighter: false,
        has_cruiser: false,
        has_destroyer: false,
        has_carrier: false,
        has_dreadnought: false,
        has_spacedock: false,
    }
}
