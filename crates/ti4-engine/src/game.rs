//! Game loop implementation.
//!
//! Implements the TI4 game loop with phases:
//! Setup → Action (Strategy → Command → Tactical) → Agenda → RoundEnd → Action → ...

use ti4_model::*;
use anyhow::Result;
use std::collections::HashMap;

/// The main game loop that advances through phases.
pub struct GameLoop {
    pub game: GameState,
    pub running: bool,
    pub strategy_index: usize,
    pub agenda_index: usize,
    pub tactical_index: usize,
}

impl GameLoop {
    pub fn new(game: GameState) -> Self {
        Self {
            game,
            running: false,
            strategy_index: 0,
            agenda_index: 0,
            tactical_index: 0,
        }
    }

    /// Start the game loop.
    pub fn start(&mut self) {
        self.running = true;
        self.game.phase = GamePhase::Setup;
    }

    /// Step the game forward one phase/sub-phase.
    pub fn step(&mut self) -> Result<bool> {
        if self.game.game_over {
            return Ok(false);
        }

        match self.game.phase {
            GamePhase::Setup => self.step_setup(),
            GamePhase::Action => self.step_action(),
            GamePhase::Agenda => self.step_agenda(),
            GamePhase::RoundEnd => self.step_round_end(),
            GamePhase::GameEnd => {
                self.game.game_over = true;
                self.running = false;
                Ok(false)
            }
        }
    }

    // ─── Setup phase ─────────────────────────────────────────────────────────

    /// Step through setup phase.
    fn step_setup(&mut self) -> Result<bool> {
        // In a full implementation, setup would:
        // 1. Initialize galaxy from content
        // 2. Seat players in factions
        // 3. Deal strategy cards
        // For now, transition to action phase
        self.game.phase = GamePhase::Action;
        self.game.sub_phase = Some(ActionSubPhase::Strategy);
        Ok(true)
    }

    // ─── Action phase ────────────────────────────────────────────────────────

    /// Step through action phase.
    fn step_action(&mut self) -> Result<bool> {
        match self.game.sub_phase {
            Some(ActionSubPhase::Strategy) => self.step_strategy(),
            Some(ActionSubPhase::Command) => self.step_command(),
            Some(ActionSubPhase::Tactical) => self.step_tactical(),
            None => {
                // Transition to agenda phase
                self.game.phase = GamePhase::Agenda;
                self.game.agenda_phase = Some(AgendaPhase::Political);
                Ok(true)
            }
        }
    }

    /// Step through strategy selection phase.
    fn step_strategy(&mut self) -> Result<bool> {
        // Check if all players have revealed or passed
        let all_done = self.game.player_order.iter().all(|pid| {
            self.game.secret_strategies.contains_key(pid) || self.game.has_passed(pid)
        });

        if all_done && !self.game.revealed_strategies.is_empty() {
            // Sort revealed strategies by player order
            // Initiative goes to the player who revealed first (highest priority)
            // For War strategy, the winner gets first pick of initiative
            let mut revealed_with_index: Vec<_> = self.game.revealed_strategies
                .iter()
                .enumerate()
                .collect();
            
            // Sort by reveal order (first to reveal = highest initiative)
            revealed_with_index.sort_by_key(|(idx, _)| *idx);
            
            // Build initiative order from revealed strategies
            let mut initiative_order = vec![];
            for (_, strategy) in revealed_with_index {
                // Find which player revealed this strategy
                for (pid, s) in self.game.secret_strategies.iter() {
                    if s == strategy && !initiative_order.contains(pid) {
                        initiative_order.push(pid.clone());
                        break;
                    }
                }
            }
            
            // If War strategy was played, the War player gets first pick of initiative
            if self.game.revealed_strategies.iter().any(|s| *s == StrategyCard::Warfare) {
                // War player picks first - for simplicity, put them first
                let war_player = self.game.revealed_strategies.iter()
                    .position(|s| *s == StrategyCard::Warfare)
                    .and_then(|idx| {
                        self.game.revealed_strategies.iter().enumerate()
                            .find(|(_, s)| **s == StrategyCard::Warfare)
                            .and_then(|(i, _)| {
                                self.game.player_order.iter().nth(i)
                            })
                            .cloned()
                    });
                if let Some(wp) = war_player {
                    initiative_order.retain(|p| p != &wp);
                    initiative_order.insert(0, wp);
                }
            }
            
            self.game.player_order = initiative_order;

            // Transition to command phase
            self.game.sub_phase = Some(ActionSubPhase::Command);
            return Ok(true);
        }

        Ok(true)
    }

    /// Reveal a strategy card for a player.
    pub fn reveal_strategy(&mut self, player: PlayerId, strategy: StrategyCard) -> Result<()> {
        if self.game.sub_phase != Some(ActionSubPhase::Strategy) {
            return Err(anyhow::anyhow!("Cannot reveal strategy: not in strategy phase"));
        }

        if self.game.secret_strategies.contains_key(&player) {
            return Err(anyhow::anyhow!("Player {} already revealed strategy", player));
        }

        if self.game.has_passed(&player) {
            return Err(anyhow::anyhow!("Player {} has passed", player));
        }

        // Record the strategy card
        self.game.reveal_strategy(player.clone(), strategy);
        
        // Apply the primary effect of the strategy card (LRR 82)
        let engine = crate::effects::EffectEngine::new();
        engine.apply_strategy_effect(&mut self.game, &player, &strategy);
        
        Ok(())
    }

    /// Have a player pass on strategy selection.
    pub fn pass_strategy(&mut self, player: PlayerId) -> Result<()> {
        if self.game.sub_phase != Some(ActionSubPhase::Strategy) {
            return Err(anyhow::anyhow!("Cannot pass: not in strategy phase"));
        }

        if self.game.secret_strategies.contains_key(&player) {
            return Err(anyhow::anyhow!("Player {} already revealed strategy", player));
        }

        self.game.mark_passed(player);
        Ok(())
    }

    /// Step through command token phase.
    fn step_command(&mut self) -> Result<bool> {
        // Distribute command tokens based on initiative order (LRR 81.5)
        // First player gets more tokens, subsequent players get fewer
        // Tokens are distributed into three pools: tactic, fleet, strategic
        for (i, pid) in self.game.player_order.iter().enumerate() {
            let (tactic, fleet, strategic) = match i {
                0 => (2, 1, 1),  // First player gets 4 total
                1 => (1, 1, 1),  // Second player gets 3 total
                2 => (1, 1, 0),  // Third player gets 2 total
                _ => (1, 0, 0),  // Others get 1 total
            };
            
            if let Some(ps) = self.game.players.get_mut(pid) {
                ps.tactic_tokens += tactic;
                ps.fleet_tokens += fleet;
                ps.strategic_tokens += strategic;
            }
        }

        // Transition to tactical phase
        self.game.sub_phase = Some(ActionSubPhase::Tactical);
        Ok(true)
    }

    /// Step through tactical phase (player activation).
    fn step_tactical(&mut self) -> Result<bool> {
        // Check if all players have been activated
        // In a full implementation, we'd track activation state per player
        // For now, transition to agenda phase after one round of activations
        self.game.phase = GamePhase::Agenda;
        self.game.agenda_phase = Some(AgendaPhase::Political);
        Ok(true)
    }

    /// Get the next player to activate in tactical phase.
    pub fn next_activatable_player(&self) -> Option<&PlayerId> {
        // Returns players in initiative order who haven't been activated yet
        // For now, return the first player in initiative order
        self.game.player_order.first()
    }

    /// Activate a specific player for tactical operations.
    pub fn activate_player(&mut self, player: PlayerId) -> Result<()> {
        if self.game.sub_phase != Some(ActionSubPhase::Tactical) {
            return Err(anyhow::anyhow!("Cannot activate player: not in tactical phase"));
        }

        // In a full implementation, we'd track which players have been activated
        // For now, just log the activation
        self.game.record_event(EventRecord {
            id: EventId::new("activate"),
            event_type: "activate".to_string(),
            source: player.clone(),
            target: None,
            effects: vec![],
            timestamp: 0,
            resolved: true,
        });

        Ok(())
    }

    // ─── Agenda phase ────────────────────────────────────────────────────────

    /// Step through agenda phase.
    fn step_agenda(&mut self) -> Result<bool> {
        if let Some(phase) = self.game.agenda_phase {
            // Resolve current agenda phase
            self.resolve_agenda_phase(phase)?;

            // Advance to next agenda phase
            self.game.advance_agenda_phase();

            if self.game.agenda_phase.is_some() {
                Ok(true)
            } else {
                // All agenda phases complete, go to round end
                self.game.phase = GamePhase::RoundEnd;
                Ok(true)
            }
        } else {
            // Transition to round end
            self.game.phase = GamePhase::RoundEnd;
            Ok(true)
        }
    }

    /// Resolve a single agenda phase.
    fn resolve_agenda_phase(&mut self, phase: AgendaPhase) -> Result<()> {
        // Collect votes from each player
        let mut votes: HashMap<PlayerId, i32> = HashMap::new();
        
        for pid in &self.game.player_order {
            // Each player votes based on their agenda tokens
            let token_count = self.game.agenda_tokens.get(pid).cloned().unwrap_or(0);
            let vote_value = if token_count > 0 { token_count } else { 1 };
            *votes.entry(pid.clone()).or_insert(0) += vote_value;
        }

        // Find the winner (highest vote count)
        let winner = votes.iter()
            .max_by_key(|(_, count)| *count)
            .map(|(pid, _): (&PlayerId, &i32)| pid.clone());

        if let Some(winner) = winner {
            // Award victory points based on agenda phase
            let vp = match phase {
                AgendaPhase::Political => 1,
                AgendaPhase::Economic => 1,
                AgendaPhase::Military => 1,
            };

            // Update winner's score
            if let Some(ps) = self.game.players.get_mut(&winner) {
                ps.score += vp;
            }

            // Record agenda result
            self.game.record_agenda_result(phase, winner.clone(), vp);
            self.game.current_agenda_player = Some(winner.clone());

            // Transfer agenda token to winner
            let player_order: Vec<_> = self.game.player_order.iter().cloned().collect();
            for pid in player_order {
                if &pid != &winner {
                    self.game.transfer_agenda_token(&pid, &winner);
                }
            }
        }

        Ok(())
    }

    // ─── Round end ───────────────────────────────────────────────────────────

    /// Step through round end phase.
    fn step_round_end(&mut self) -> Result<bool> {
        // Resolve round end:
        // 1. Score objectives
        // 2. Clear casualties
        // 3. Reset pips
        // 4. Start next round

        self.start_next_round()?;
        self.game.phase = GamePhase::Action;
        self.game.sub_phase = Some(ActionSubPhase::Strategy);

        // Check victory conditions
        if self.check_victory_conditions() {
            self.game.phase = GamePhase::GameEnd;
            return Ok(false);
        }

        Ok(true)
    }

    /// Start the next round.
    fn start_next_round(&mut self) -> Result<()> {
        self.game.round += 1;
        self.game.reset_passed();

        // Determine initiative player (player with most VP, or previous)
        // For now, use the player who had initiative last round
        let initiative = self.game.initiative_player.clone();
        if let Some(ref initiative) = initiative {
            // Transfer agenda token
            let players: Vec<_> = self.game.players.keys().cloned().collect();
            for pid in players {
                if &pid != initiative {
                    self.game.transfer_agenda_token(&pid, initiative);
                }
            }
        }

        // Rebuild player order based on initiative
        if let Some(ref initiative) = initiative {
            let mut order: Vec<PlayerId> = self.game.players.keys().cloned().collect();
            if let Some(pos) = order.iter().position(|p| p == initiative) {
                order.rotate_left(pos);
            }
            self.game.player_order = order;
        }

        Ok(())
    }

    /// Check victory conditions.
    fn check_victory_conditions(&mut self) -> bool {
        // In full implementation, check:
        // 1. Any player with 7+ VP wins
        // 2. After round 10, player with most VP wins
        // For now, simulate a win condition at round 10
        if self.game.round >= 10 {
            // Find player with most score
            let winner = self.game.players.iter()
                .max_by_key(|(_, ps)| ps.score)
                .map(|(pid, _)| pid.clone());

            if let Some(winner) = winner {
                self.game.winner = Some(winner);
                return true;
            }
        }
        false
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_game() -> GameState {
        let mut game = GameState::new("test-game".to_string(), 42, "test".to_string(), 4);

        // Add players
        for i in 0..4 {
            let pid = PlayerId::new(format!("p{}", i));
            let mut ps = PlayerState::default();
            ps.id = pid.clone();
            ps.faction_id = FactionId::new(format!("faction{}", i));
            game.add_player(ps);
        }

        game.player_order = vec![
            PlayerId::new("p0"),
            PlayerId::new("p1"),
            PlayerId::new("p2"),
            PlayerId::new("p3"),
        ];

        game
    }

    #[test]
    fn test_game_start() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        assert_eq!(loop_.game.phase, GamePhase::Setup);
        assert!(loop_.running);
    }

    #[test]
    fn test_phase_transitions() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Setup → Action (Strategy)
        loop_.step().unwrap();
        assert_eq!(loop_.game.phase, GamePhase::Action);
        assert_eq!(loop_.game.sub_phase, Some(ActionSubPhase::Strategy));

        // Strategy phase needs all players to reveal
        // Simulate by revealing strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }

        // Advance strategy
        loop_.step().unwrap();
        assert_eq!(loop_.game.phase, GamePhase::Action);
        assert_eq!(loop_.game.sub_phase, Some(ActionSubPhase::Command));

        // Command → Tactical
        loop_.step().unwrap();
        assert_eq!(loop_.game.phase, GamePhase::Action);
        assert_eq!(loop_.game.sub_phase, Some(ActionSubPhase::Tactical));

        // Tactical → Agenda
        loop_.step().unwrap();
        assert_eq!(loop_.game.phase, GamePhase::Agenda);
        assert_eq!(loop_.game.agenda_phase, Some(AgendaPhase::Political));
    }

    #[test]
    fn test_strategy_reveal() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Get to strategy phase
        loop_.step().unwrap();
        assert_eq!(loop_.game.sub_phase, Some(ActionSubPhase::Strategy));

        // Reveal strategy for a player
        loop_.reveal_strategy(PlayerId::new("p0"), StrategyCard::Leadership).unwrap();
        assert!(loop_.game.secret_strategies.contains_key(&PlayerId::new("p0")));
        assert_eq!(loop_.game.revealed_strategies.len(), 1);
    }

    #[test]
    fn test_strategy_pass() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Get to strategy phase
        loop_.step().unwrap();

        // Pass strategy for a player
        loop_.pass_strategy(PlayerId::new("p0")).unwrap();
        assert!(loop_.game.has_passed(&PlayerId::new("p0")));
    }

    #[test]
    fn test_command_token_distribution() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Get to command phase
        loop_.step().unwrap(); // Setup → Action
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        loop_.step().unwrap(); // Strategy → Command

        // Distribute command tokens
        loop_.step().unwrap(); // Command → Tactical

        // Check token distribution - collect total tokens per player
        // Default is 3/3/2 = 8 per player, step_command adds:
        // P1 (idx 0): +2/+1/+1 = 4, P2 (idx 1): +1/+1/+1 = 3, P3 (idx 2): +1/+1/+0 = 2, P4 (idx 3): +1/+0/+0 = 1
        let totals: Vec<_> = loop_.game.players.values()
            .map(|ps| ps.tactic_tokens + ps.fleet_tokens + ps.strategic_tokens)
            .collect();
        let mut sorted_totals = totals.clone();
        sorted_totals.sort();
        
        // Should have 12, 11, 10, 9 (8 default + added)
        assert_eq!(sorted_totals, vec![9, 10, 11, 12]);
        
        // Each player should have at least 1 total token
        for pid in loop_.game.player_order.iter() {
            let ps = loop_.game.players.get(pid).unwrap();
            let total = ps.tactic_tokens + ps.fleet_tokens + ps.strategic_tokens;
            assert!(total >= 1, "Player {} should have at least 1 command token", pid);
        }
    }

    #[test]
    fn test_agenda_voting() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Get to agenda phase
        loop_.step().unwrap(); // Setup → Action
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        loop_.step().unwrap(); // Strategy
        loop_.step().unwrap(); // Command
        loop_.step().unwrap(); // Tactical → Agenda

        // Set initiative player for agenda token distribution
        loop_.game.initiative_player = Some(PlayerId::new("p3"));
        loop_.game.init_agenda_tokens();

        // Resolve political agenda
        let phase = loop_.game.agenda_phase.unwrap();
        loop_.resolve_agenda_phase(phase).unwrap();

        // Check that winner was determined
        assert!(loop_.game.current_agenda_player.is_some());
        assert!(!loop_.game.agenda_results.is_empty());
    }

    #[test]
    fn test_agenda_phases() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Get to agenda phase
        loop_.step().unwrap(); // Setup → Action
        // Reveal strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        loop_.step().unwrap(); // Strategy
        loop_.step().unwrap(); // Command
        loop_.step().unwrap(); // Tactical → Agenda

        // Political agenda
        assert_eq!(loop_.game.agenda_phase, Some(AgendaPhase::Political));
        loop_.step().unwrap();
        assert_eq!(loop_.game.agenda_phase, Some(AgendaPhase::Economic));

        // Economic agenda
        loop_.step().unwrap();
        assert_eq!(loop_.game.agenda_phase, Some(AgendaPhase::Military));

        // Military agenda
        loop_.step().unwrap();
        assert!(loop_.game.agenda_phase.is_none());
        assert_eq!(loop_.game.phase, GamePhase::RoundEnd);
    }

    #[test]
    fn test_round_completion() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Complete one full round
        loop_.step().unwrap(); // Setup → Action
        // Reveal strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        loop_.step().unwrap(); // Strategy
        loop_.step().unwrap(); // Command
        loop_.step().unwrap(); // Tactical → Agenda
        loop_.step().unwrap(); // Political
        loop_.step().unwrap(); // Economic
        loop_.step().unwrap(); // Military → RoundEnd
        loop_.step().unwrap(); // RoundEnd → Action (round 2)

        assert_eq!(loop_.game.round, 2);
        assert_eq!(loop_.game.phase, GamePhase::Action);
        assert_eq!(loop_.game.sub_phase, Some(ActionSubPhase::Strategy));
    }

    #[test]
    fn test_game_over_at_round_10() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Fast-forward to round 10 (9 round_ends from round 1)
        for _ in 0..9 {
            loop_.step().unwrap(); // Setup → Action
            // Reveal strategies
            let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
            for pid in pids {
                loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
            }
            loop_.step().unwrap(); // Strategy
            loop_.step().unwrap(); // Command
            loop_.step().unwrap(); // Tactical → Agenda
            loop_.step().unwrap(); // Political
            loop_.step().unwrap(); // Economic
            loop_.step().unwrap(); // Military → RoundEnd
            loop_.step().unwrap(); // RoundEnd → Action (next round)
        }

        // Should now be at round 10
        assert_eq!(loop_.game.round, 10);
        loop_.step().unwrap(); // Setup → Action
        // Reveal strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        loop_.step().unwrap(); // Strategy
        loop_.step().unwrap(); // Command
        loop_.step().unwrap(); // Tactical → Agenda
        loop_.step().unwrap(); // Political
        loop_.step().unwrap(); // Economic
        loop_.step().unwrap(); // Military → RoundEnd
        loop_.step().unwrap(); // RoundEnd → check victory → GameEnd

        // Game should be over
        assert!(loop_.game.game_over);
        assert!(!loop_.running);
    }

    #[test]
    fn test_game_over_no_more_steps() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Complete the game
        loop_.step().unwrap(); // Setup → Action
        // Reveal strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        loop_.step().unwrap(); // Strategy
        loop_.step().unwrap(); // Command
        loop_.step().unwrap(); // Tactical → Agenda
        loop_.step().unwrap(); // Political
        loop_.step().unwrap(); // Economic
        loop_.step().unwrap(); // Military → RoundEnd
        loop_.step().unwrap(); // RoundEnd → Action (round 2)

        // Fast-forward to round 10 (8 more round_ends)
        for _ in 0..8 {
            loop_.step().unwrap(); // Setup → Action
            let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
            for pid in pids {
                loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
            }
            loop_.step().unwrap(); // Strategy
            loop_.step().unwrap(); // Command
            loop_.step().unwrap(); // Tactical → Agenda
            loop_.step().unwrap(); // Political
            loop_.step().unwrap(); // Economic
            loop_.step().unwrap(); // Military → RoundEnd
            loop_.step().unwrap(); // RoundEnd → Action (next round)
        }

        // After game over, no more steps
        let result = loop_.step().unwrap();
        assert!(!result);
    }

    #[test]
    fn test_full_round_simulation() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Simulate one complete round
        loop_.step().unwrap(); // Setup → Action (Strategy)
        
        // All players reveal strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::Leadership);
        }
        
        // Strategy → Command
        loop_.step().unwrap();
        
        // Command → Tactical (tokens distributed)
        loop_.step().unwrap();
        
        // Tactical → Agenda
        loop_.step().unwrap();
        
        // Init agenda tokens
        loop_.game.initiative_player = Some(PlayerId::new("p3"));
        loop_.game.init_agenda_tokens();
        
        // Political agenda
        let phase = loop_.game.agenda_phase.unwrap();
        loop_.resolve_agenda_phase(phase).unwrap();
        
        // Economic agenda
        loop_.game.agenda_phase = Some(AgendaPhase::Economic);
        let phase = loop_.game.agenda_phase.unwrap();
        loop_.resolve_agenda_phase(phase).unwrap();
        
        // Military agenda
        loop_.game.agenda_phase = Some(AgendaPhase::Military);
        let phase = loop_.game.agenda_phase.unwrap();
        loop_.resolve_agenda_phase(phase).unwrap();
        
        // Round end - first step transitions to RoundEnd phase
        loop_.step().unwrap();
        // Second step executes RoundEnd (increments round)
        loop_.step().unwrap();
        
        // Verify game state after one round
        assert_eq!(loop_.game.round, 2);
        assert_eq!(loop_.game.phase, GamePhase::Action);
        assert_eq!(loop_.game.sub_phase, Some(ActionSubPhase::Strategy));
        
        // Verify players have command tokens
        for pid in loop_.game.player_order.iter() {
            let ps = loop_.game.players.get(pid).unwrap();
            let total = ps.tactic_tokens + ps.fleet_tokens + ps.strategic_tokens;
            assert!(total >= 1);
        }
        
        // Verify agenda results recorded
        assert!(!loop_.game.agenda_results.is_empty());
    }

    #[test]
    fn test_production_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        
        // Set up a player with production capacity
        if let Some(ps) = game.players.get_mut(&PlayerId::new("p0")) {
            ps.production = 3;
            ps.trade_income = 1;
        }
        
        let engine = EffectEngine::new();
        let total = engine.apply_production(&mut game, &PlayerId::new("p0"));
        
        // Should be production + trade_income
        assert_eq!(total, 4);
    }

    #[test]
    fn test_objective_scoring() {
        let mut game = make_test_game();
        
        // Create a test objective
        let objective = ObjectiveState {
            id: ObjectiveId::new("test-obj"),
            completed: false,
            score: 1,
        };
        
        // Score the objective for p0 (first time)
        let vp = game.score_objective(&PlayerId::new("p0"), &objective);
        
        // p0 has no control tokens or completed objectives, so vp should be 0
        assert_eq!(vp, 0);
        
        // Add a control token
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .control_tokens.insert(PlanetId::new("test-planet"));
        
        // Score again - should have 1 VP (1 control token) + 1 VP (objective completed) = 2
        let vp = game.score_objective(&PlayerId::new("p0"), &objective);
        assert_eq!(vp, 2);
        
        // Add another control token
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .control_tokens.insert(PlanetId::new("test-planet-2"));
        
        // Score again - should have 3 VP (2 control tokens + 1 completed objective)
        let vp = game.score_objective(&PlayerId::new("p0"), &objective);
        assert_eq!(vp, 3);
    }

    #[test]
    fn test_technology_research() {
        let mut game = make_test_game();
        
        // Add commodity to p0
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .commodity = 5;
        
        // Research a technology
        let tech = TechnologyId::new("superior_weapons");
        let result = game.research_technology(&PlayerId::new("p0"), tech.clone(), 3);
        
        assert!(result.is_ok());
        assert!(result.unwrap());
        
        // Check tech level
        let level = game.get_tech_level(&PlayerId::new("p0"), &tech);
        assert_eq!(level, 1);
        
        // Check commodity spent
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.commodity, 2);
        
        // Try to research again with insufficient commodity
        let result = game.research_technology(&PlayerId::new("p0"), tech.clone(), 3);
        assert!(result.is_ok());
        assert!(!result.unwrap()); // Should fail - not enough commodity
        
        // Tech level should still be 1
        let level = game.get_tech_level(&PlayerId::new("p0"), &tech);
        assert_eq!(level, 1);
    }

    #[test]
    fn test_leader_activation() {
        let mut game = make_test_game();
        
        // Create a test leader
        let leader = LeaderState {
            id: LeaderId::new("test-leader"),
            ability: "test-ability".to_string(),
            active: true,
            fatigued: false,
            system_id: None,
            planet_id: None,
        };
        
        // Activate the leader
        let result = game.activate_leader(&PlayerId::new("p0"), leader.clone());
        assert!(result.is_ok());
        
        // Check leader is active
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert!(ps.active_leader.is_some());
        
        // Fatigue the leader
        game.fatigue_leader(&PlayerId::new("p0"), LeaderId::new("test-leader"));
        
        // Try to activate again - should fail
        let result = game.activate_leader(&PlayerId::new("p0"), leader.clone());
        assert!(result.is_err());
        
        // Refresh leaders
        game.refresh_leaders(&PlayerId::new("p0"));
        
        // Should be able to activate again
        let result = game.activate_leader(&PlayerId::new("p0"), leader);
        assert!(result.is_ok());
    }

    #[test]
    fn test_relic_handling() {
        let mut game = make_test_game();
        
        // Create a test relic
        let relic = RelicState {
            id: RelicId::new("test-relic"),
            owner: None,
            active: true,
        };
        
        // Award relic to p0
        game.award_relic(&PlayerId::new("p0"), relic.clone());
        
        // Check player has the relic
        assert!(game.has_relic(&PlayerId::new("p0"), &RelicId::new("test-relic")));
        
        // Check p1 doesn't have the relic
        assert!(!game.has_relic(&PlayerId::new("p1"), &RelicId::new("test-relic")));
        
        // Try to award again - should not duplicate
        game.award_relic(&PlayerId::new("p0"), relic);
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.relics.len(), 1);
    }

    #[test]
    fn test_secret_objective_check() {
        let mut game = make_test_game();
        
        // Create a test secret objective
        let secret = SecretObjectiveState {
            id: SecretObjectiveId::new("test-secret"),
            completed: false,
            score: 1,
        };
        
        // p0 has no technologies or control tokens
        assert!(!game.check_secret_objective(&PlayerId::new("p0"), &secret));
        
        // Add a technology to p0
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .technologies.insert(TechnologyId::new("test-tech"));
        
        // Now should be complete
        assert!(game.check_secret_objective(&PlayerId::new("p0"), &secret));
    }

    #[test]
    fn test_leadership_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Clear initial tokens
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .tactic_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .fleet_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .strategic_tokens = 0;
        
        // Apply Leadership strategy
        engine.apply_leadership_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have +1 to each pool
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.tactic_tokens, 1);
        assert_eq!(ps.fleet_tokens, 1);
        assert_eq!(ps.strategic_tokens, 1);
    }

    #[test]
    fn test_diplomacy_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Initial influence
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .influence = 3;
        
        // Apply Diplomacy strategy
        engine.apply_diplomacy_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have 4 influence (3 + 1)
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.influence, 4);
    }

    #[test]
    fn test_politics_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Apply Politics strategy
        engine.apply_politics_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have received an action card
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.action_cards.len(), 1);
    }

    #[test]
    fn test_construction_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Initial production
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .production = 2;
        
        // Apply Construction strategy
        engine.apply_construction_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have production +1 (for structure placement)
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.production, 3);
    }

    #[test]
    fn test_trade_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Initial trade goods
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .trade_goods = 5;
        
        // Apply Trade strategy
        engine.apply_trade_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have 8 trade goods (5 + 3)
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.trade_goods, 8);
    }

    #[test]
    fn test_warfare_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Initial tactic tokens
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .tactic_tokens = 1;
        
        // Apply Warfare strategy
        engine.apply_warfare_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have +1 tactic token and has_war flag
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.tactic_tokens, 2);
        assert!(ps.has_war);
    }

    #[test]
    fn test_technology_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Apply Technology strategy
        engine.apply_technology_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have free_research flag set
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert!(ps.free_research);
    }

    #[test]
    fn test_imperial_strategy_effect() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Initial score
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .score = 3;
        
        // Apply Imperial strategy
        engine.apply_imperial_effect(&mut game, &PlayerId::new("p0"));
        
        // Should have +1 score
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.score, 4);
    }

    #[test]
    fn test_strategy_effect_dispatch() {
        use crate::effects::EffectEngine;
        
        let mut game = make_test_game();
        let engine = EffectEngine::new();
        
        // Clear initial tokens
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .tactic_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .fleet_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .strategic_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .trade_goods = 0;
        
        // Test Leadership dispatch (grants 1 to each pool)
        engine.apply_strategy_effect(&mut game, &PlayerId::new("p0"), &StrategyCard::Leadership);
        let ps = game.players.get(&PlayerId::new("p0")).unwrap();
        assert_eq!(ps.tactic_tokens, 1);
        assert_eq!(ps.fleet_tokens, 1);
        assert_eq!(ps.strategic_tokens, 1);
        
        // Clear again
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .tactic_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .fleet_tokens = 0;
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .strategic_tokens = 0;
        
        // Test Trade dispatch
        engine.apply_strategy_effect(&mut game, &PlayerId::new("p0"), &StrategyCard::Trade);
        assert_eq!(game.players.get(&PlayerId::new("p0")).unwrap().trade_goods, 3);
        
        // Test Warfare dispatch
        engine.apply_strategy_effect(&mut game, &PlayerId::new("p0"), &StrategyCard::Warfare);
        assert!(game.players.get(&PlayerId::new("p0")).unwrap().has_war);
        
        // Reset
        game.players.get_mut(&PlayerId::new("p0")).unwrap()
            .has_war = false;
        
        // Test Technology dispatch
        engine.apply_strategy_effect(&mut game, &PlayerId::new("p0"), &StrategyCard::Technology);
        assert!(game.players.get(&PlayerId::new("p0")).unwrap().free_research);
    }
}
