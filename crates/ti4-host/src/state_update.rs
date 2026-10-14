//! The state update that a host sends to each viewer after the game moved, and the answer to a
//! reach query.
//!
//! The server computes the inputs under its session lock and broadcasts the result to its
//! subscribers; the browser host computes them from its single game and posts one message.

use std::collections::BTreeMap;

use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, DecisionRecord};
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::{GameState, ReactionMode};
use ti4_view::map::GalaxyLayout;
use ti4_view::secondary::SecondaryWatch;

use crate::PROTOCOL_VERSION;
use crate::projection::project_state_update_with_map;
use crate::protocol::reach::{ReachIntoMsg, ReachRefusal};
use crate::protocol::server::{AutoResolvedNote, CurrentLogPath, ServerMessage, current_log_path};
use crate::protocol::status::ViewerRole;
use crate::protocol::view::BoardTileView;
use crate::setup::TableSeats;

/// Everything a `StateUpdate` for any viewer is made from.
pub struct StateUpdateInputs<'a> {
    pub game_id: &'a str,
    pub version: u64,
    pub state: &'a GameState,
    /// The decision that waits for an answer and its nonce.
    pub pending: Option<(&'a Choice, &'a str)>,
    pub map_tiles: &'a [BoardTileView],
    pub galaxy_layout: &'a GalaxyLayout,
    pub galaxy: Option<&'a Galaxy>,
    pub watch: &'a SecondaryWatch,
    /// Decisions in the log: the history cursor.
    pub decision_count: usize,
    pub redo_count: usize,
    pub history_generation: u64,
    /// The decision log, for the path of the action in progress.
    pub decision_log: &'a [DecisionRecord],
    /// Notes of decisions that were resolved for a seat, each told to that seat only.
    pub auto_resolved: &'a [(PlayerId, AutoResolvedNote)],
    /// Every seat's "never offer" cards.
    pub reaction_modes: &'a BTreeMap<PlayerId, BTreeMap<String, ReactionMode>>,
    /// The one viewer that may take its last decision back now, if any.
    pub may_undo: Option<&'a ViewerRole>,
    /// Who sits in each seat, told to every viewer; `None` for a game with no table.
    pub seats: Option<&'a TableSeats>,
}

impl StateUpdateInputs<'_> {
    /// The log path shared by all viewers.
    #[must_use]
    pub fn path(&self) -> Option<CurrentLogPath> {
        current_log_path(
            self.state,
            self.pending.map(|(choice, _)| choice),
            self.decision_log,
        )
    }
}

/// The `StateUpdate` of `viewer`: the redacted state and facts, the history status, the path of
/// the current action, the viewer's reaction modes and the notes meant for the viewer's seat.
#[must_use]
pub fn state_update_for(
    inputs: &StateUpdateInputs<'_>,
    path: &Option<CurrentLogPath>,
    viewer: &ViewerRole,
) -> ServerMessage {
    let mut update = project_state_update_with_map(
        inputs.game_id,
        inputs.version,
        inputs.state,
        viewer,
        inputs.pending,
        inputs.map_tiles,
        inputs.galaxy_layout,
        inputs.galaxy,
        Some(inputs.watch),
    )
    .with_history(
        inputs.decision_count,
        inputs.redo_count,
        inputs.history_generation,
    );
    update.history.can_undo = inputs.may_undo == Some(viewer);
    update.current_path.clone_from(path);
    update.seats = inputs.seats.cloned();
    update.reaction_modes = match viewer {
        ViewerRole::Player(seat) => inputs.reaction_modes.get(seat).cloned().unwrap_or_default(),
        _ => return ServerMessage::StateUpdate(update),
    };
    // Only the seat the decision belonged to is told; it already saw those options.
    update.auto_resolved = inputs
        .auto_resolved
        .iter()
        .filter(|(seat, _)| viewer.is_actor(seat))
        .map(|(_, note)| note.clone())
        .collect();
    ServerMessage::StateUpdate(update)
}

/// Queues a note for the seat's next state update, merging a repeat of the same reason.
///
/// `ordinal_prefix` is the part of the note's ID that makes it unique within the game: the
/// log length and the game version are added to it by the caller's counters.
pub fn queue_auto_resolved(
    pending: &mut Vec<(PlayerId, AutoResolvedNote)>,
    decision_count: usize,
    game_version: u64,
    seat: &PlayerId,
    prompt: &str,
    selected: &str,
    reason: &str,
    cards: &[String],
) {
    if let Some((_, existing)) = pending
        .iter_mut()
        .find(|(who, note)| who == seat && note.reason == reason && note.selected == selected)
    {
        existing.count += 1;
        existing.prompt = prompt.to_owned();
        return;
    }
    let id = format!("auto-{decision_count}-{}-{game_version}", pending.len());
    pending.push((
        seat.clone(),
        AutoResolvedNote {
            id,
            prompt: prompt.to_owned(),
            selected: selected.to_owned(),
            reason: reason.to_owned(),
            count: 1,
            passed: cards.to_vec(),
        },
    ));
}

/// Answers one reach query ("who can go here") for `system_id` on a copy of the position.
///
/// `position` is the newest published state and the map of the game, or `None` when the game is
/// not in a state to answer (stopped, finished, still replaying): the answer is then `Busy`.
/// Only units that stand on the board (public) are read.
#[must_use]
pub fn answer_reach(
    game_id: &str,
    request_id: u64,
    system_id: &str,
    version: u64,
    position: Option<(&GameState, &Galaxy)>,
) -> ReachIntoMsg {
    let reply = |origins, refused| ReachIntoMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        request_id,
        system_id: system_id.to_owned(),
        as_of_version: version,
        origins,
        refused,
    };
    let Some((state, galaxy)) = position else {
        return reply(Vec::new(), Some(ReachRefusal::Busy));
    };
    if !galaxy.system_ids().contains(&system_id) {
        return reply(Vec::new(), Some(ReachRefusal::UnknownSystem));
    }
    let origins = ti4_engine::reach_query::who_can_reach(
        state,
        ContentStore::embedded(),
        POK,
        galaxy,
        &SystemId::new(system_id),
    );
    reply(origins, None)
}
