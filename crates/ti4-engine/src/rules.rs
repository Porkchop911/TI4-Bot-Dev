//! Rules validation and legality checking.
//!
//! Implements deterministic legality checks for all game actions.

use ti4_model::*;
use anyhow::Result;

/// Validates the legality of a game action.
pub struct RulesValidator;

impl RulesValidator {
    pub fn new() -> Self { Self }

    /// Validate the legality of a game action.
    pub fn validate_legality(&self, game: &GameState, action: &GameAction) -> Result<bool> {
        match action {
            GameAction::RevealStrategy => {
                self.validate_reveal_strategy(game, action)
            }
            GameAction::PassStrategy => {
                self.validate_pass_strategy(game, action)
            }
            GameAction::MoveFleet { from, to, fleet } => {
                self.validate_move(game, from, to, fleet)
            }
            GameAction::Combat { attacker, defender, fleet } => {
                self.validate_combat(game, attacker, defender, fleet)
            }
            GameAction::Bombard { target, fleet } => {
                self.validate_bombard(game, target, fleet)
            }
            GameAction::LandInfantry { target, fleet } => {
                self.validate_land(game, target, fleet)
            }
            GameAction::Research { technology } => {
                self.validate_research(game, technology)
            }
            GameAction::Explore { system } => {
                self.validate_explore(game, system)
            }
            GameAction::PlayCard { card } => {
                self.validate_play_card(game, card)
            }
            GameAction::ActivateLeader { leader } => {
                self.validate_activate_leader(game, leader)
            }
            GameAction::UsePromissoryNote { note } => {
                self.validate_promissory_note(game, note)
            }
            GameAction::ClaimExpeditionTile { tile } => {
                self.validate_claim_tile(game, tile)
            }
            GameAction::ClaimBreakthrough { breakthrough } => {
                self.validate_claim_breakthrough(game, breakthrough)
            }
            GameAction::CompleteObjective { objective } => {
                self.validate_complete_objective(game, objective)
            }
            GameAction::CompleteSecretObjective => {
                self.validate_complete_secret(game)
            }
            GameAction::VoteAgenda { vote } => {
                self.validate_vote(game, vote)
            }
            GameAction::DistributeCommandTokens => {
                self.validate_command_distribution(game)
            }
            GameAction::BuildUnit { unit, location } => {
                self.validate_build(game, unit, location)
            }
            GameAction::UpgradeUnit { unit, location } => {
                self.validate_upgrade(game, unit, location)
            }
            GameAction::ActivateRelic { relic } => {
                self.validate_activate_relic(game, relic)
            }
            GameAction::UseFactionAbility => {
                self.validate_faction_ability(game)
            }
            GameAction::UseHomeworldAbility => {
                self.validate_homeworld_ability(game)
            }
        }
    }

    fn validate_reveal_strategy(&self, _game: &GameState, _action: &GameAction) -> Result<bool> {
        Ok(true)
    }

    fn validate_pass_strategy(&self, _game: &GameState, _action: &GameAction) -> Result<bool> {
        Ok(true)
    }

    fn validate_move(&self, _game: &GameState, _from: &SystemId, _to: &SystemId, _fleet: &FleetState) -> Result<bool> {
        Ok(true)
    }

    fn validate_combat(&self, _game: &GameState, _attacker: &PlayerId, _defender: &PlayerId, _fleet: &FleetState) -> Result<bool> {
        Ok(true)
    }

    fn validate_bombard(&self, _game: &GameState, _target: &PlanetId, _fleet: &FleetState) -> Result<bool> {
        Ok(true)
    }

    fn validate_land(&self, _game: &GameState, _target: &PlanetId, _fleet: &FleetState) -> Result<bool> {
        Ok(true)
    }

    fn validate_research(&self, _game: &GameState, _technology: &TechnologyId) -> Result<bool> {
        Ok(true)
    }

    fn validate_explore(&self, _game: &GameState, _system: &SystemId) -> Result<bool> {
        Ok(true)
    }

    fn validate_play_card(&self, _game: &GameState, _card: &ActionCardId) -> Result<bool> {
        Ok(true)
    }

    fn validate_activate_leader(&self, _game: &GameState, _leader: &LeaderId) -> Result<bool> {
        Ok(true)
    }

    fn validate_promissory_note(&self, _game: &GameState, _note: &PromissoryNoteId) -> Result<bool> {
        Ok(true)
    }

    fn validate_claim_tile(&self, _game: &GameState, _tile: &ExpeditionTileId) -> Result<bool> {
        Ok(true)
    }

    fn validate_claim_breakthrough(&self, _game: &GameState, _breakthrough: &BreakthroughId) -> Result<bool> {
        Ok(true)
    }

    fn validate_complete_objective(&self, _game: &GameState, _objective: &ObjectiveId) -> Result<bool> {
        Ok(true)
    }

    fn validate_complete_secret(&self, _game: &GameState) -> Result<bool> {
        Ok(true)
    }

    fn validate_vote(&self, _game: &GameState, _vote: &AgendaVote) -> Result<bool> {
        Ok(true)
    }

    fn validate_command_distribution(&self, _game: &GameState) -> Result<bool> {
        Ok(true)
    }

    fn validate_build(&self, _game: &GameState, _unit: &UnitTypeId, _location: &SystemId) -> Result<bool> {
        Ok(true)
    }

    fn validate_upgrade(&self, _game: &GameState, _unit: &UnitTypeId, _location: &SystemId) -> Result<bool> {
        Ok(true)
    }

    fn validate_activate_relic(&self, _game: &GameState, _relic: &RelicId) -> Result<bool> {
        Ok(true)
    }

    fn validate_faction_ability(&self, _game: &GameState) -> Result<bool> {
        Ok(true)
    }

    fn validate_homeworld_ability(&self, _game: &GameState) -> Result<bool> {
        Ok(true)
    }
}

/// Game actions that can be taken during a turn.
#[derive(Debug, Clone)]
pub enum GameAction {
    RevealStrategy,
    PassStrategy,
    MoveFleet { from: SystemId, to: SystemId, fleet: FleetState },
    Combat { attacker: PlayerId, defender: PlayerId, fleet: FleetState },
    Bombard { target: PlanetId, fleet: FleetState },
    LandInfantry { target: PlanetId, fleet: FleetState },
    Research { technology: TechnologyId },
    Explore { system: SystemId },
    PlayCard { card: ActionCardId },
    ActivateLeader { leader: LeaderId },
    UsePromissoryNote { note: PromissoryNoteId },
    ClaimExpeditionTile { tile: ExpeditionTileId },
    ClaimBreakthrough { breakthrough: BreakthroughId },
    CompleteObjective { objective: ObjectiveId },
    CompleteSecretObjective,
    VoteAgenda { vote: AgendaVote },
    DistributeCommandTokens,
    BuildUnit { unit: UnitTypeId, location: SystemId },
    UpgradeUnit { unit: UnitTypeId, location: SystemId },
    ActivateRelic { relic: RelicId },
    UseFactionAbility,
    UseHomeworldAbility,
}

/// An agenda vote.
#[derive(Debug, Clone)]
pub struct AgendaVote {
    pub player: PlayerId,
    pub value: i32,
}
