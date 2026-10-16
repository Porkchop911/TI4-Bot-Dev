//! How a disposable fork is guarded before it plays a hypothetical turn.
//!
//! The server's planning runner and [`super::draft`] both prepare their copy here, so the two
//! stop at the same boundaries.

use std::sync::Arc;

use ti4_engine::game::Game;
use ti4_engine::observation::ExecutionObservation;
use ti4_model::id::PlayerId;

use super::audit::StopReason;

/// The event that ends a tactical action.
pub const TACTICAL_COMPLETE: &str = "TACTICAL_ACTION_COMPLETE";

/// Bind what a disposable copy must tell its caller, before its hypothetical turn is prepared.
///
/// One structural rule, no per-ability list: other seats take no optional reactions and their
/// conditions are never read. The planner's own abilities and every mandatory one run on the
/// copy. What may be shown is decided only by the publication gate: a question outside the
/// audited offers, new knowledge or a random outcome stops the preview.
pub fn guard_fork(game: &mut Game<'_>, planner: &PlayerId) -> ExecutionObservation {
    let observation = ExecutionObservation::default();
    game.bind_observation(observation.clone());
    let seat = planner.clone();
    game.timing.set_participation(Arc::new(move |ability| {
        ability.owner == seat || !ability.optional
    }));
    let aftermath = observation.clone();
    game.on_aftermath(move |state, _content, _sources| {
        let (Some(system), Some(player)) = (&state.active_system, &state.active) else {
            aftermath.unsupported_segment();
            return;
        };
        // Space combat remains outside this slice. Invasion and production proceed through
        // audited offers; dice and ordered draws latch uncertainty before any resulting
        // position can be published.
        if state
            .system_state(system)
            .units
            .iter()
            .any(|unit| &unit.owner != player)
        {
            aftermath.unsupported_segment();
        }
    });
    observation
}

/// Why the copy may not be shown any further, from what it did so far. Uncertainty wins over
/// any speculative error or later unsupported path.
#[must_use]
pub fn activity_stop(observation: &ExecutionObservation) -> Option<StopReason> {
    let activity = observation.activity();
    if activity.randomness || activity.hidden_information {
        Some(StopReason::Uncertainty)
    } else if activity.unsupported_participation {
        Some(StopReason::UnsupportedParticipation)
    } else if activity.unsupported_segment {
        Some(StopReason::UnsupportedSegment)
    } else {
        None
    }
}
