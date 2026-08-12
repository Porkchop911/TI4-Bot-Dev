//! Game-level choice stepping and bounded execution.

use ti4_content::ContentStore;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::{GameState, Phase};

use crate::agenda::{AgendaPhaseError, resolve_agenda_phase};
use crate::choice::{Choice, ChoiceOption, IllegalChoice, Table};
use crate::draft::{DraftError, strategy_options, take_strategy_card};
use crate::phase::{PhaseOutcome, advance_phase, advance_turn, begin_next_round};
use crate::status::{StatusPhaseError, resolve_status_phase};
use crate::strategy::{
    ACTION_KIND, SecondaryResolution, StrategyActionError, StrategySecondaryError,
    StrategySecondaryWindow, begin_strategic_action, strategic_action_options,
};

/// Metadata returned after one attempted game step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepResult {
    /// The phase after this step.
    pub phase: Phase,
    /// The player whose action is next, if the phase has one.
    pub active: Option<PlayerId>,
    /// Whether the game has reached its terminal state.
    pub finished: bool,
    /// A precise reason this driver cannot safely continue.
    pub error: Option<GameError>,
    /// Whether this step resolved one generated player choice.
    pub resolved_choice: bool,
}

/// A game-level failure that is safe to expose to a caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GameError {
    #[error(transparent)]
    IllegalChoice(#[from] IllegalChoice),
    #[error(transparent)]
    Draft(#[from] DraftError),
    #[error(transparent)]
    StrategyAction(#[from] StrategyActionError),
    #[error(transparent)]
    StrategySecondary(#[from] StrategySecondaryError),
    #[error(transparent)]
    Status(#[from] StatusPhaseError),
    #[error(transparent)]
    Agenda(#[from] AgendaPhaseError),
    #[error("action phase has no active player while players remain")]
    MissingActivePlayer,
    #[error("action {0:?} is not implemented by the structural game driver")]
    UnsupportedAction(String),
    #[error("status scoring and command-token allocation choices are not implemented")]
    StatusChoicesUnimplemented,
    #[error("agenda voting, ties, and effects are not implemented")]
    AgendaChoicesUnimplemented,
}

/// A bounded `run` stopped instead of silently looping forever.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RunError {
    #[error(transparent)]
    Step(#[from] GameError),
    #[error("game did not progress within {max_steps} steps (round {round}, phase {phase:?})")]
    StepLimit {
        max_steps: usize,
        round: u32,
        phase: Phase,
    },
}

/// The stateful owner of generated choices, their decision log, and observable events.
///
/// The structure mirrors the oracle's `Game`: state remains public for inspection, while all
/// external decisions pass through [`Table`] and only generated choices are applied.
pub struct Game<'a> {
    pub state: GameState,
    pub table: Table,
    pub events: Vec<String>,
    content: &'a ContentStore,
    strategy_cards: Vec<StrategyCardId>,
    secondary: Option<StrategySecondaryWindow>,
    status_resolved: bool,
    agenda_resolved: bool,
    blocked: Option<GameError>,
}

impl<'a> Game<'a> {
    /// Create a game with the default first-option table.
    #[must_use]
    pub fn new(state: GameState, content: &'a ContentStore) -> Self {
        Self::with_table(state, content, Table::new())
    }

    /// Create a game with explicit deciders for generated choices.
    #[must_use]
    pub fn with_table(state: GameState, content: &'a ContentStore, table: Table) -> Self {
        Self {
            strategy_cards: state.unclaimed_strategy_cards.clone(),
            state,
            table,
            events: Vec::new(),
            content,
            secondary: None,
            status_resolved: false,
            agenda_resolved: false,
            blocked: None,
        }
    }

    /// The choice currently offered, without resolving automatic followers or phase work.
    #[must_use]
    pub fn legal_options(&self) -> Option<Choice> {
        if self.state.finished || self.blocked.is_some() {
            return None;
        }
        match self.state.phase {
            Phase::Strategy => strategy_options(&self.state, self.content),
            Phase::Action => self.action_options(),
            Phase::Status | Phase::Agenda => None,
        }
    }

    /// Resolve one generated decision, or one choice-free phase/window transition.
    #[must_use]
    pub fn step(&mut self) -> StepResult {
        if self.state.finished {
            return self.result(false, None);
        }
        if let Some(error) = self.blocked.clone() {
            return self.result(false, Some(error));
        }

        if self.secondary.is_some() {
            return self.step_secondary();
        }
        if self.state.phase == Phase::Status && !self.status_resolved {
            return self.step_status();
        }
        if self.state.phase == Phase::Agenda && !self.agenda_resolved {
            return self.step_agenda();
        }

        let Some(choice) = self.legal_options() else {
            return self.step_phase();
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        match self.apply_choice(&choice, answer) {
            Ok(()) => self.result(true, None),
            Err(error) => self.result(false, Some(error)),
        }
    }

    /// Play whole rounds, bounded so an incomplete or stalled model fails loudly.
    ///
    /// # Errors
    /// [`RunError::StepLimit`] when `max_steps` is reached, or [`RunError::Step`] when a required
    /// rule/choice remains deliberately unimplemented.
    pub fn run(&mut self, rounds: u32, max_steps: usize) -> Result<&GameState, RunError> {
        let target = self.state.round.saturating_add(rounds);
        let mut steps = 0;
        while self.state.round < target && !self.state.finished {
            if steps >= max_steps {
                return Err(RunError::StepLimit {
                    max_steps,
                    round: self.state.round,
                    phase: self.state.phase,
                });
            }
            let result = self.step();
            if let Some(error) = result.error {
                return Err(error.into());
            }
            steps += 1;
        }
        Ok(&self.state)
    }

    fn action_options(&self) -> Option<Choice> {
        if let Some(window) = &self.secondary {
            return window.pending_choice(&self.state);
        }
        let active = self.state.active.as_ref()?;
        strategic_action_options(&self.state, self.content, active).or_else(|| {
            Some(Choice::new(
                active.clone(),
                "action phase",
                vec![ChoiceOption::labelled("pass", ACTION_KIND, "pass")],
            ))
        })
    }

    fn apply_choice(&mut self, choice: &Choice, answer: ChoiceOption) -> Result<(), GameError> {
        match self.state.phase {
            Phase::Strategy => {
                let player = take_strategy_card(&mut self.state, self.content, answer)?;
                self.emit("STRATEGY_CARD_CHOSEN");
                debug_assert_eq!(player, choice.player);
                Ok(())
            }
            Phase::Action => {
                let active = self
                    .state
                    .active
                    .clone()
                    .ok_or(GameError::MissingActivePlayer)?;
                if answer.id == "pass" {
                    self.state
                        .player_mut(&active)
                        .expect("active player exists")
                        .passed = true;
                    self.emit("PLAYER_PASSED");
                    self.advance_turn();
                    return Ok(());
                }
                if answer.kind != ACTION_KIND {
                    return Err(GameError::UnsupportedAction(answer.id));
                }
                self.secondary = Some(begin_strategic_action(
                    &mut self.state,
                    self.content,
                    &active,
                    answer,
                )?);
                self.emit("STRATEGIC_ACTION_BEGAN");
                Ok(())
            }
            Phase::Status | Phase::Agenda => {
                Err(GameError::UnsupportedAction(choice.prompt.clone()))
            }
        }
    }

    fn step_secondary(&mut self) -> StepResult {
        let choice = self
            .secondary
            .as_mut()
            .expect("checked above")
            .next_choice(&mut self.state);
        let Some(choice) = choice else {
            self.secondary = None;
            self.emit("STRATEGIC_ACTION_COMPLETE");
            self.advance_turn();
            return self.result(false, None);
        };
        let answer = match self.table.ask(&choice) {
            Ok(answer) => answer,
            Err(error) => return self.result(false, Some(error.into())),
        };
        let (resolution, complete) = match self
            .secondary
            .as_mut()
            .expect("window remains open")
            .take_choice(&mut self.state, answer)
        {
            Ok(resolution) => (
                resolution,
                self.secondary
                    .as_ref()
                    .expect("window remains open")
                    .is_complete(),
            ),
            Err(error) => return self.result(false, Some(error.into())),
        };
        self.emit(match resolution {
            SecondaryResolution::Declined => "STRATEGY_SECONDARY_DECLINED",
            SecondaryResolution::Followed => "STRATEGY_SECONDARY_FOLLOWED",
            SecondaryResolution::Ineligible => unreachable!("ineligible followers are automatic"),
        });
        if complete {
            self.secondary = None;
            self.emit("STRATEGIC_ACTION_COMPLETE");
            self.advance_turn();
        }
        self.result(true, None)
    }

    fn step_status(&mut self) -> StepResult {
        self.status_resolved = true;
        match resolve_status_phase(&mut self.state) {
            Ok(report) if report.game_ended => {
                self.emit("GAME_FINISHED");
                self.result(false, None)
            }
            Ok(_) => {
                self.emit("STATUS_BOOKKEEPING_RESOLVED");
                let error = GameError::StatusChoicesUnimplemented;
                self.blocked = Some(error.clone());
                self.result(false, Some(error))
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    fn step_agenda(&mut self) -> StepResult {
        self.agenda_resolved = true;
        match resolve_agenda_phase(&mut self.state) {
            Ok(report) if report.agendas.is_empty() => {
                self.emit("AGENDA_PHASE_RESOLVED");
                self.result(false, None)
            }
            Ok(_) => {
                self.emit("AGENDA_REVEALED");
                let error = GameError::AgendaChoicesUnimplemented;
                self.blocked = Some(error.clone());
                self.result(false, Some(error))
            }
            Err(error) => self.result(false, Some(error.into())),
        }
    }

    fn step_phase(&mut self) -> StepResult {
        if self.state.phase == Phase::Action && !self.state.all_passed() {
            return self.result(false, Some(GameError::MissingActivePlayer));
        }
        let outcome = advance_phase(&mut self.state);
        match outcome {
            PhaseOutcome::ActionBegan(_) => self.emit("ACTION_PHASE_BEGAN"),
            PhaseOutcome::StatusBegan => self.emit("STATUS_PHASE_BEGAN"),
            PhaseOutcome::AgendaBegan => self.emit("AGENDA_PHASE_BEGAN"),
            PhaseOutcome::RoundEnded => {
                begin_next_round(&mut self.state, self.strategy_cards.clone());
                self.status_resolved = false;
                self.agenda_resolved = false;
                self.emit("ROUND_BEGAN");
            }
        }
        self.result(false, None)
    }

    fn advance_turn(&mut self) {
        if advance_turn(&mut self.state).is_some() {
            self.emit("TURN_PASSED");
        }
    }

    fn emit(&mut self, event: &str) {
        self.events.push(event.to_owned());
    }

    fn result(&self, resolved_choice: bool, error: Option<GameError>) -> StepResult {
        StepResult {
            phase: self.state.phase,
            active: self.state.active.clone(),
            finished: self.state.finished,
            error,
            resolved_choice,
        }
    }
}

#[cfg(test)]
mod tests {
    use ti4_content::ContentStore;
    use ti4_model::content_types::POK;
    use ti4_model::id::PlayerId;
    use ti4_model::state::Phase;

    use super::*;
    use crate::choice::AlwaysDecline;
    use crate::setup::start_game;

    #[test]
    fn one_step_resolves_exactly_one_generated_strategy_choice() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());

        let result = game.step();

        assert!(result.error.is_none());
        assert!(result.resolved_choice);
        assert_eq!(game.table.log.len(), 1);
        assert_eq!(game.state.unclaimed_strategy_cards.len(), 7);
        assert_eq!(game.events, vec!["STRATEGY_CARD_CHOSEN"]);
    }

    #[test]
    fn strategy_primary_and_each_secondary_are_separate_steps() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::with_table(
            state,
            ContentStore::embedded(),
            Table::with_default(Box::new(AlwaysDecline)),
        );
        while game.state.phase == Phase::Strategy {
            assert!(game.step().error.is_none());
        }
        let primary = game.state.active.clone().unwrap();

        let primary_step = game.step();
        assert!(primary_step.resolved_choice);
        assert_eq!(game.state.active, Some(primary.clone()));
        assert_eq!(game.table.log.len(), 7, "six draft picks and one primary");
        let before_inspection = game.state.clone();
        assert_eq!(game.legal_options().unwrap().player, PlayerId::new("b"));
        assert!(game.state.identical(&before_inspection));

        let first_secondary = game.step();
        assert!(first_secondary.resolved_choice);
        assert_eq!(game.state.active, Some(primary.clone()));

        let final_secondary = game.step();
        assert!(final_secondary.resolved_choice);
        assert_ne!(game.state.active, Some(primary));
        assert_eq!(game.table.log.len(), 9);
        assert!(game.events.contains(&"STRATEGIC_ACTION_BEGAN".to_owned()));
        assert_eq!(
            game.events
                .iter()
                .filter(|event| event.as_str() == "STRATEGY_SECONDARY_DECLINED")
                .count(),
            2
        );
    }

    #[test]
    fn status_bookkeeping_stops_at_the_unimplemented_choice_boundary() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Status;
        let mut game = Game::new(state, ContentStore::embedded());

        let result = game.step();

        assert_eq!(result.error, Some(GameError::StatusChoicesUnimplemented));
        assert_eq!(game.events, vec!["STATUS_BOOKKEEPING_RESOLVED"]);
        let after_first_step = game.state.clone();
        assert_eq!(
            game.step().error,
            Some(GameError::StatusChoicesUnimplemented)
        );
        assert!(game.state.identical(&after_first_step));
    }

    #[test]
    fn an_empty_agenda_needs_no_invented_vote_and_then_starts_a_round() {
        let players = [PlayerId::new("a")];
        let mut state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        state.phase = Phase::Agenda;
        state.custodians_removed = true;
        state.agenda_deck.clear();
        let mut game = Game::new(state, ContentStore::embedded());

        assert!(game.step().error.is_none());
        assert_eq!(game.events, vec!["AGENDA_PHASE_RESOLVED"]);
        assert!(game.step().error.is_none());
        assert_eq!(game.state.phase, Phase::Strategy);
        assert_eq!(game.state.round, 2);
        assert_eq!(game.events.last().unwrap(), "ROUND_BEGAN");
    }

    #[test]
    fn run_reports_its_step_horizon_instead_of_looping() {
        let players = [PlayerId::new("a"), PlayerId::new("b"), PlayerId::new("c")];
        let state = start_game(ContentStore::embedded(), &players, POK, None).unwrap();
        let mut game = Game::new(state, ContentStore::embedded());

        assert_eq!(
            game.run(1, 3),
            Err(RunError::StepLimit {
                max_steps: 3,
                round: 1,
                phase: Phase::Strategy,
            })
        );
    }
}
