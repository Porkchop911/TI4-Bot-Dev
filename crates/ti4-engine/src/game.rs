//! Game loop implementation.
//!
//! Implements the TI4 game loop with phases:
//! Setup → Action (Strategy → Command → Tactical) → Agenda → RoundEnd → Action → ...

use ti4_model::*;
use anyhow::Result;

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
            self.game.revealed_strategies.sort_by(|a, b| {
                let ai = self.game.player_order.iter().position(|p| {
                    self.game.secret_strategies.iter().any(|(p, s)| p == p && s == a)
                }).unwrap_or(0);
                let bi = self.game.player_order.iter().position(|p| {
                    self.game.secret_strategies.iter().any(|(p, s)| p == p && s == b)
                }).unwrap_or(0);
                ai.cmp(&bi)
            });

            // Determine initiative order (reverse of strategy reveal order for War)
            // For now, use player order
            self.game.player_order = self.game.players.keys().cloned().collect();
            self.game.player_order.reverse();

            // Transition to command phase
            self.game.sub_phase = Some(ActionSubPhase::Command);
            return Ok(true);
        }

        Ok(true)
    }

    /// Step through command token phase.
    fn step_command(&mut self) -> Result<bool> {
        // Command token distribution happens here
        // For now, transition to tactical phase
        self.game.sub_phase = Some(ActionSubPhase::Tactical);
        Ok(true)
    }

    /// Step through tactical phase (player activation).
    fn step_tactical(&mut self) -> Result<bool> {
        // Player activation order based on initiative
        // In full implementation, this would activate each player's fleet
        // For now, transition to agenda phase
        self.game.phase = GamePhase::Agenda;
        self.game.agenda_phase = Some(AgendaPhase::Political);
        Ok(true)
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
        // Find the winner of this agenda phase
        // In full implementation, this would:
        // 1. Collect votes from each player
        // 2. Determine winner based on agenda card effects
        // 3. Award victory points
        // 4. Apply agenda effects

        // For now, use the player with the most agenda tokens
        let winner = self.game.agenda_tokens.iter()
            .max_by_key(|(_, count)| *count)
            .map(|(pid, _)| pid.clone());

        if let Some(winner) = winner {
            self.game.record_agenda_result(phase, winner.clone(), 1);
            self.game.current_agenda_player = Some(winner);
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
            loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
    fn test_agenda_phases() {
        let mut game = make_test_game();
        let mut loop_ = GameLoop::new(game);
        loop_.start();

        // Get to agenda phase
        loop_.step().unwrap(); // Setup → Action
        // Reveal strategies
        let pids: Vec<_> = loop_.game.player_order.iter().cloned().collect();
        for pid in pids {
            loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
            loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
                loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
            loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
            loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
                loop_.game.reveal_strategy(pid, StrategyCard::from_code("s"));
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
}
