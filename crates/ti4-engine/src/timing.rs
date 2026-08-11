//! Event timing and resolution.
//!
//! Manages the event queue and resolves events in deterministic order.

use ti4_model::*;
use anyhow::Result;

/// Manages event timing and resolution.
pub struct EventTimer;

impl EventTimer {
    pub fn new() -> Self { Self }

    /// Resolve all pending events in deterministic order.
    pub fn resolve(&mut self, game: &mut GameState) -> Result<()> {
        // Events are resolved in timestamp order
        // For now, clear current events
        game.current_events.clear();
        game.active_event = None;
        Ok(())
    }

    /// Add an event to the current events queue.
    pub fn add_event(&mut self, game: &mut GameState, event: EventRecord) {
        game.current_events.push(event);
    }

    /// Activate an event.
    pub fn activate_event(&mut self, game: &mut GameState, event_id: &EventId) {
        if let Some(event) = game.current_events.iter().find(|e| e.id == *event_id) {
            game.active_event = Some(event.clone());
        }
    }

    /// Deactivate the current event.
    pub fn deactivate_event(&mut self, game: &mut GameState) {
        if let Some(active) = game.active_event.take() {
            game.current_events.retain(|e| e.id != active.id);
        }
    }
}
