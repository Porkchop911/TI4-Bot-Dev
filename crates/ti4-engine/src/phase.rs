//! Phase management stub.

use ti4_model::*;

pub struct PhaseManager {
    pub current: GamePhase,
    pub sub_phase: Option<ActionSubPhase>,
}

impl PhaseManager {
    pub fn new(phase: GamePhase) -> Self {
        Self { current: phase, sub_phase: None }
    }

    pub fn transition(&mut self, phase: GamePhase) {
        self.current = phase;
    }
}
