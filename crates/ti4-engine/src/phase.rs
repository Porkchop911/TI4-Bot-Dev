//! Phase management and transitions.
//!
//! Implements the TI4 phase state machine with proper ordering:
//! Setup → Action (Strategy → Command → Tactical) → Agenda → RoundEnd → Action → ...

use ti4_model::*;
use std::collections::BTreeMap;

/// Manages game phase state and transitions.
pub struct PhaseManager {
    pub current: GamePhase,
    pub sub_phase: Option<ActionSubPhase>,
    pub agenda_phase: Option<AgendaPhase>,
    pub current_player: Option<PlayerId>,
    pub strategy_order: Vec<PlayerId>,
    pub agenda_order: Vec<PlayerId>,
}

impl PhaseManager {
    pub fn new(phase: GamePhase) -> Self {
        Self {
            current: phase,
            sub_phase: None,
            agenda_phase: None,
            current_player: None,
            strategy_order: vec![],
            agenda_order: vec![],
        }
    }

    /// Transition to a new phase, resetting sub-state appropriately.
    pub fn transition(&mut self, phase: GamePhase) {
        self.current = phase;
        match phase {
            GamePhase::Setup => {
                self.sub_phase = None;
                self.agenda_phase = None;
            }
            GamePhase::Action => {
                self.sub_phase = Some(ActionSubPhase::Strategy);
                self.agenda_phase = None;
            }
            GamePhase::Agenda => {
                self.sub_phase = None;
                self.agenda_phase = Some(AgendaPhase::Political);
            }
            GamePhase::RoundEnd | GamePhase::GameEnd => {
                self.sub_phase = None;
                self.agenda_phase = None;
            }
        }
    }

    /// Advance from one sub-phase to the next within the Action phase.
    pub fn advance_sub_phase(&mut self) -> bool {
        match self.sub_phase {
            None => {
                self.sub_phase = Some(ActionSubPhase::Strategy);
                true
            }
            Some(ActionSubPhase::Strategy) => {
                self.sub_phase = Some(ActionSubPhase::Command);
                true
            }
            Some(ActionSubPhase::Command) => {
                self.sub_phase = Some(ActionSubPhase::Tactical);
                true
            }
            Some(ActionSubPhase::Tactical) => {
                self.sub_phase = None;
                false // Transition to agenda phase
            }
        }
    }

    /// Advance to the next agenda phase.
    pub fn advance_agenda_phase(&mut self) {
        self.agenda_phase = match self.agenda_phase {
            None | Some(AgendaPhase::Political) => Some(AgendaPhase::Economic),
            Some(AgendaPhase::Economic) => Some(AgendaPhase::Military),
            Some(AgendaPhase::Military) => None,
        };
    }

    /// Check if the agenda phase is complete.
    pub fn agenda_complete(&self) -> bool {
        self.agenda_phase.is_none()
    }

    /// Set the current player for activation.
    pub fn set_current_player(&mut self, pid: PlayerId) {
        self.current_player = Some(pid);
    }

    /// Get the current player for activation.
    pub fn current_player(&self) -> Option<&PlayerId> {
        self.current_player.as_ref()
    }

    /// Set the strategy selection order.
    pub fn set_strategy_order(&mut self, order: Vec<PlayerId>) {
        self.strategy_order = order;
    }

    /// Get the next player in strategy order.
    pub fn next_strategy_player(&self, index: usize) -> Option<&PlayerId> {
        self.strategy_order.get(index)
    }

    /// Set the agenda order.
    pub fn set_agenda_order(&mut self, order: Vec<PlayerId>) {
        self.agenda_order = order;
    }

    /// Get the current agenda player.
    pub fn current_agenda_player(&self) -> Option<&PlayerId> {
        self.current_player.as_ref()
    }
}
