//! Projections that wrap a redacted view in a protocol message.

use crate::view::secondary::SecondaryWatch;
use std::collections::BTreeMap;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::Choice;
use ti4_model::state::GameState;

use super::{FactsContext, SessionUpdate, project_session_update, redacted_state};
use crate::map::GalaxyLayout;
use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::server::{GameEvent, InitialSnapshotMsg, StateUpdateMsg};
use crate::protocol::status::ViewerRole;
use crate::protocol::view::BoardTileView;

/// What the facts of a message are made from: the games of the server are of one content.
/// Without the map of the game a message has no `tactical` and no `exploration`.
#[must_use]
pub fn facts_context(galaxy: Option<&Galaxy>) -> FactsContext<'_> {
    FactsContext {
        content: ti4_content::ContentStore::embedded(),
        sources: ti4_model::content_types::POK,
        galaxy,
        secondary: None,
    }
}

fn empty_layout() -> GalaxyLayout {
    GalaxyLayout {
        version: 1,
        active_sources: Vec::new(),
        placements: Vec::new(),
        off_map_system_ids: Vec::new(),
    }
}

/// Projects an initial snapshot for a connecting viewer with static map tiles.
///
/// `state` is the full state of the game: the view and the facts are made for `viewer` from it
/// ([`project_session_update`]), and `state` of the message is redacted.
#[must_use]
pub fn project_initial_snapshot_with_map(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
    map_tiles: &[BoardTileView],
    galaxy_layout: &GalaxyLayout,
    galaxy: Option<&Galaxy>,
    watch: Option<&SecondaryWatch>,
    events: &[GameEvent],
) -> InitialSnapshotMsg {
    let SessionUpdate {
        viewer: _,
        view,
        pending_choice: pending,
        turn_status,
        tactical,
        exploration,
        action_cards,
        secret_objectives,
        score,
        agendas,
        secondary,
        reaction,
        research,
        seats,
    } = project_session_update(
        state,
        viewer,
        pending_choice,
        map_tiles,
        &FactsContext {
            secondary: watch,
            ..facts_context(galaxy)
        },
    );
    InitialSnapshotMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        game_version,
        viewer: viewer.clone(),
        view,
        state: redacted_state(state, viewer),
        galaxy_layout: galaxy_layout.clone(),
        pending_choice: pending,
        turn_status,
        events: events
            .iter()
            .filter_map(|event| event.for_viewer(viewer))
            .collect(),
        history: crate::protocol::server::HistoryStatus::default(),
        current_path: None,
        reaction_modes: BTreeMap::new(),
        tactical,
        exploration,
        action_cards,
        secret_objectives,
        score,
        agendas,
        secondary,
        reaction,
        research,
        seats,
        content: content_digest().to_owned(),
        undone: Vec::new(),
    }
}

/// The digest of the corpus of this build. It is the same for every snapshot.
fn content_digest() -> &'static str {
    static DIGEST: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    DIGEST.get_or_init(|| ti4_content::provenance::embedded_digest().corpus)
}

/// Projects an initial snapshot for a connecting viewer.
#[must_use]
pub fn project_initial_snapshot(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
) -> InitialSnapshotMsg {
    project_initial_snapshot_with_map(
        game_id,
        game_version,
        state,
        viewer,
        pending_choice,
        &[],
        &empty_layout(),
        None,
        None,
        &[],
    )
}

/// Projects a versioned state update message with static map tiles, as
/// [`project_initial_snapshot_with_map`] does. `watch` is the strategic action of the game, as
/// its session follows it: without it the message has no `secondary`.
#[must_use]
pub fn project_state_update_with_map(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
    map_tiles: &[BoardTileView],
    galaxy_layout: &GalaxyLayout,
    galaxy: Option<&Galaxy>,
    watch: Option<&SecondaryWatch>,
) -> StateUpdateMsg {
    let SessionUpdate {
        viewer: _,
        view,
        pending_choice: pending,
        turn_status,
        tactical,
        exploration,
        action_cards,
        secret_objectives,
        score,
        agendas,
        secondary,
        reaction,
        research,
        seats,
    } = project_session_update(
        state,
        viewer,
        pending_choice,
        map_tiles,
        &FactsContext {
            secondary: watch,
            ..facts_context(galaxy)
        },
    );
    StateUpdateMsg {
        history: crate::protocol::server::HistoryStatus::default(),
        current_path: None,
        protocol_version: PROTOCOL_VERSION,
        game_id: game_id.to_owned(),
        game_version,
        viewer: viewer.clone(),
        view,
        state: redacted_state(state, viewer),
        galaxy_layout: galaxy_layout.clone(),
        pending_choice: pending,
        turn_status,
        auto_resolved: Vec::new(),
        reaction_modes: BTreeMap::new(),
        tactical,
        exploration,
        action_cards,
        secret_objectives,
        score,
        agendas,
        secondary,
        reaction,
        research,
        seats,
    }
}

/// Projects a versioned state update message.
#[must_use]
pub fn project_state_update(
    game_id: &str,
    game_version: u64,
    state: &GameState,
    viewer: &ViewerRole,
    pending_choice: Option<(&Choice, &str)>,
) -> StateUpdateMsg {
    project_state_update_with_map(
        game_id,
        game_version,
        state,
        viewer,
        pending_choice,
        &[],
        &empty_layout(),
        None,
        None,
    )
}
