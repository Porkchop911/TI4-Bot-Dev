//! Registry operations of the turn redo (H7): request, auto-play, restore, keep and status.
//!
//! Every operation that changes the timeline takes the per-game gate and goes through
//! [`GameRegistry::publish_history`], the same replacement path as host undo/redo: version
//! `expected + 1`, history generation `+ 1` (so websocket clients resync), a fresh worker that
//! replays the new decision list and waits at the next unanswered question.
//!
//! # Who may do what
//!
//! * Request a redo: the seat for its own turns, or the host for any seat. A guest naming another
//!   seat is refused (403). Casual play: no consent step and no restriction on turns that revealed
//!   something.
//! * Auto-play, restore, keep: the host or the redoing seat.
//! * Read the status: any authenticated player (it says that a redo is in flight and how far it
//!   got, never what the original timeline held).
//!
//! # The saved original
//!
//! The first request of a chain saves the whole pre-redo timeline (`turn_redo.json`). A second
//! request while the first is still undecided keeps that original (restoring always means "before
//! the first redo"). The original is dropped by `keep`, by `restore` (it was used), when a new
//! request finds the previous redo already past its hand-off, and lazily when the status is read
//! after the first live decision was recorded beyond the auto-play hand-off. Until then, restoring
//! is exact: the decision log, events, redo list and batches come back unchanged.

use ti4_model::id::PlayerId;

use super::{
    Arc, GameHistory, GameRegistry, GameSession, HistoryError, InitialSnapshotMsg, RegistryState,
    SessionConfig,
};
use crate::protocol::status::ViewerRole;
use crate::protocol::turn_redo::{TurnRedoOutcome, TurnRedoStage, TurnRedoStatus};
use crate::session::replay::replay_session_with_copy;
use crate::session::turn_redo::{
    AlternateTimeline, TurnRedoError, TurnRedoRecord, autoplay, events_through, find_turns,
    open_segment, prepare_tail, redact_for_viewer, redo_window,
};

fn invalid(error: &TurnRedoError) -> HistoryError {
    match error {
        TurnRedoError::BaselineDiverged(_) | TurnRedoError::PrefixDiverged(_) => {
            HistoryError::Conflict(error.to_string())
        }
        _ => HistoryError::InvalidTarget(error.to_string()),
    }
}

/// Everything read from one live session under the gate.
struct Snapshot {
    session: Arc<GameSession>,
    config: SessionConfig,
    decisions: Vec<ti4_engine::choice::DecisionRecord>,
    redo: Vec<ti4_engine::choice::DecisionRecord>,
    events: Vec<crate::protocol::server::GameEvent>,
    redo_events: Vec<crate::protocol::server::GameEvent>,
    event_counter: u64,
    batches: Vec<crate::storage::BatchRecord>,
    marks: crate::session::RngMarks,
    generation: u64,
}

impl GameRegistry {
    /// Who the credential belongs to and whether that seat is the host.
    fn require_actor(
        state: &RegistryState,
        game_id: &str,
        credential: &str,
    ) -> Result<(PlayerId, bool), HistoryError> {
        let denied = || {
            HistoryError::Forbidden("the session credential is not valid for this game".to_owned())
        };
        if let Some(lobby) = state.player_lobbies.get(game_id) {
            let actor = super::authenticate_player(lobby, credential).map_err(|_| denied())?;
            let host = actor == lobby.host_player_id;
            Ok((actor, host))
        } else if let Some(lobby) = state.lobbies.get(game_id) {
            let actor = super::authenticated_seat(lobby, credential).map_err(|_| denied())?;
            let host = actor == lobby.host_seat;
            Ok((actor, host))
        } else {
            Err(HistoryError::NotFound)
        }
    }

    fn read_record(&self, game_id: &str, config: &SessionConfig) -> Option<TurnRedoRecord> {
        let mut map = self.turn_redos.lock().expect("turn redo lock");
        if let Some(record) = map.get(game_id) {
            return Some(record.clone());
        }
        let record = config
            .store
            .as_ref()
            .and_then(|store| store.load_turn_redo(game_id).ok().flatten())?;
        map.insert(game_id.to_owned(), record.clone());
        Some(record)
    }

    fn write_record(
        &self,
        game_id: &str,
        config: &SessionConfig,
        record: &TurnRedoRecord,
    ) -> Result<(), HistoryError> {
        if let Some(store) = &config.store {
            store
                .save_turn_redo(game_id, record)
                .map_err(|e| HistoryError::Storage(e.to_string()))?;
        }
        self.turn_redos
            .lock()
            .expect("turn redo lock")
            .insert(game_id.to_owned(), record.clone());
        Ok(())
    }

    fn drop_record(&self, game_id: &str, config: &SessionConfig) {
        self.turn_redos
            .lock()
            .expect("turn redo lock")
            .remove(game_id);
        if let Some(store) = &config.store {
            let _ = store.delete_turn_redo(game_id);
        }
    }

    /// Put a previously saved record back (or remove the new one) after a failed publication.
    fn revert_record(
        &self,
        game_id: &str,
        config: &SessionConfig,
        previous: Option<&TurnRedoRecord>,
    ) {
        match previous {
            Some(record) => {
                let _ = self.write_record(game_id, config, record);
            }
            None => self.drop_record(game_id, config),
        }
    }

    /// The original stays restorable while the redo is undecided: before auto-play always, and
    /// after it until a live decision has been recorded past the hand-off.
    fn restorable(record: &TurnRedoRecord, live_len: usize) -> bool {
        match record.stage {
            TurnRedoStage::NewTurn => true,
            TurnRedoStage::AutoPlayed => record.handoff_len.is_some_and(|len| live_len <= len),
        }
    }

    fn snapshot(
        &self,
        state: &RegistryState,
        game_id: &str,
        expected_version: Option<u64>,
    ) -> Result<Snapshot, HistoryError> {
        let session = state
            .sessions
            .get(game_id)
            .ok_or(HistoryError::NotFound)?
            .clone();
        if let Some(expected) = expected_version
            && (session.game_version() != expected || !session.history_ready())
        {
            // Clients retry on this leading phrase, so keep it stable and append details.
            return Err(HistoryError::Conflict(if session.history_ready() {
                format!(
                    "Game advanced or a decision is in flight: the request expected version {expected} but the game is at version {}",
                    session.game_version()
                )
            } else {
                "Game advanced or a decision is in flight: history is still reloading".to_owned()
            }));
        }
        let (redo_events, event_counter) = session.history_events();
        Ok(Snapshot {
            config: session.restart_config(),
            decisions: session.decision_log(),
            redo: session.redo_decisions(),
            events: session.event_log(),
            redo_events,
            event_counter,
            batches: session.batches(),
            marks: session.rng_marks(),
            generation: session.history_status().generation,
            session,
        })
    }

    /// Rewind the seat's last `turns` turns (default 1). The seat plays a new turn live; call
    /// [`GameRegistry::turn_redo_autoplay`] when it is done.
    ///
    /// # Errors
    /// [`HistoryError`]: `Forbidden` for a guest naming another seat, `InvalidTarget` when the seat
    /// has no such turn, `Conflict` when the game moved on or the saved history no longer replays.
    pub fn turn_redo_request(
        &self,
        game_id: &str,
        credential: &str,
        expected_version: u64,
        seat: Option<&str>,
        turns: Option<u8>,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let (actor, is_host) = Self::require_actor(&state, game_id, credential)?;
        let target = seat.map_or_else(|| actor.clone(), PlayerId::new);
        if target != actor && !is_host {
            return Err(HistoryError::Forbidden(
                "only the host may redo another seat's turn".to_owned(),
            ));
        }
        let snap = self.snapshot(&state, game_id, Some(expected_version))?;
        drop(state);
        if !snap.config.player_ids.contains(&target) {
            return Err(HistoryError::InvalidTarget(format!(
                "{target} is not a seat of this game"
            )));
        }
        let window =
            redo_window(&snap.decisions, &target, turns.unwrap_or(1)).map_err(|e| invalid(&e))?;
        let all_events: Vec<_> = snap
            .events
            .iter()
            .cloned()
            .chain(snap.redo_events.iter().cloned())
            .collect();
        let live_events = events_through(&all_events, 0, snap.decisions.len());
        let source = prepare_tail(
            &snap.config.state,
            snap.config.galaxy.as_ref(),
            &snap.decisions,
            &snap.marks,
            &all_events[..live_events],
            &window,
        )
        .map_err(|e| invalid(&e))?;

        let previous = self.read_record(game_id, &snap.config);
        let chained = previous
            .as_ref()
            .filter(|record| Self::restorable(record, snap.decisions.len()));
        let alternate = chained.map_or_else(
            || AlternateTimeline {
                decisions: snap.decisions.clone(),
                redo: snap.redo.clone(),
                events: snap.events.clone(),
                redo_events: snap.redo_events.clone(),
                event_counter: snap.event_counter,
                batches: snap.batches.clone(),
                rng_marks: snap.marks.clone(),
            },
            |record| record.alternate.clone(),
        );
        let open = open_segment(window.start, &source);
        let record = TurnRedoRecord {
            seat: target,
            requested_by: actor.clone(),
            turns_back: window.turns,
            redo_count: chained.map_or(1, |record| record.redo_count + 1),
            rewound_to: window.start,
            stage: TurnRedoStage::NewTurn,
            handoff_len: None,
            source,
            outcome: None,
            alternate,
        };

        let split = events_through(&all_events, 0, window.start);
        let revision = expected_version.checked_add(1).ok_or_else(|| {
            HistoryError::Conflict("the game version counter is exhausted".to_owned())
        })?;
        let mut batches = snap.batches.clone();
        batches.retain(|batch| batch.end_cursor <= window.start);
        let mut marks = snap.marks.clone();
        marks.truncate_to(window.start);
        // The new turn is played live against the cards the recorded tail will draw: its draws
        // skip them (and recovery from disk skips them the same way).
        marks.deck_plan.push(open);
        let history = GameHistory {
            decisions: snap.decisions[..window.start].to_vec(),
            redo: Vec::new(),
            events: all_events[..split].to_vec(),
            redo_events: Vec::new(),
            event_counter: snap.event_counter.max(all_events.len() as u64),
            revision,
            generation: snap.generation.saturating_add(1),
            batches,
            rng_marks: marks,
        };
        self.write_record(game_id, &snap.config, &record)?;
        let published = self.publish_history(
            game_id,
            &snap.session,
            snap.config.clone(),
            history,
            expected_version,
            snap.decisions.len(),
            actor,
            None,
        );
        if published.is_err() {
            self.revert_record(game_id, &snap.config, previous.as_ref());
        }
        published
    }

    /// Replay the other seats' recorded decisions after the seat's new turn.
    ///
    /// # Errors
    /// [`HistoryError`]: `Forbidden` unless the host or the redoing seat, `InvalidTarget` when no
    /// redo is in progress or the new turn is not finished. A redo that was already auto-played is
    /// not an error: the current snapshot comes back and nothing is applied twice.
    pub fn turn_redo_autoplay(
        &self,
        game_id: &str,
        credential: &str,
        expected_version: u64,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let (actor, is_host) = Self::require_actor(&state, game_id, credential)?;
        // Several tabs (the host's and the redoing seat's) ask for the same auto-play. The first
        // one wins; a later one finds the round already replayed and gets the current snapshot
        // back instead of an error, whatever version it expected. The stage is checked before the
        // version for exactly that reason.
        let current = self.snapshot(&state, game_id, None)?;
        drop(state);
        let record = self
            .read_record(game_id, &current.config)
            .ok_or_else(|| HistoryError::InvalidTarget("no turn redo is in progress".to_owned()))?;
        if actor != record.seat && !is_host {
            return Err(HistoryError::Forbidden(
                "only the host or the redoing seat may continue a turn redo".to_owned(),
            ));
        }
        if record.stage != TurnRedoStage::NewTurn {
            return Ok(current.session.get_snapshot(&ViewerRole::Player(actor)));
        }
        if record.source.decisions.is_empty() {
            return self.finish_without_tail(game_id, &current, record, actor);
        }
        let state = self.state.lock().expect("registry lock");
        let snap = self.snapshot(&state, game_id, Some(expected_version))?;
        drop(state);
        let result = autoplay(
            &snap.config.state,
            snap.config.galaxy.as_ref(),
            &snap.decisions,
            &snap.marks,
            record.rewound_to,
            &record.seat,
            &record.source,
        )
        .map_err(|e| invalid(&e))?;
        // The persisted history must replay by itself, with its stored positions.
        let (report, copy) = replay_session_with_copy(
            &snap.config.state,
            snap.config.galaxy.as_ref(),
            &result.decisions,
            &result.marks,
            snap.generation.saturating_add(1),
        )
        .map_err(|e| HistoryError::Conflict(format!("Replay failed: {e}")))?;
        if !report.hashes_match || report.decision_count != result.decisions.len() {
            return Err(HistoryError::Conflict("Replay diverged".to_owned()));
        }
        let all_events: Vec<_> = snap
            .events
            .iter()
            .cloned()
            .chain(snap.redo_events.iter().cloned())
            .collect();
        let split = events_through(&all_events, 0, result.prefix_len);
        let mut events = all_events[..split].to_vec();
        events.extend(result.tail_events.iter().cloned());
        let revision = expected_version.checked_add(1).ok_or_else(|| {
            HistoryError::Conflict("the game version counter is exhausted".to_owned())
        })?;
        let mut batches = snap.batches.clone();
        batches.retain(|batch| batch.end_cursor <= result.prefix_len);
        let history = GameHistory {
            event_counter: snap.event_counter.max(events.len() as u64),
            decisions: result.decisions.clone(),
            redo: Vec::new(),
            events,
            redo_events: Vec::new(),
            revision,
            generation: snap.generation.saturating_add(1),
            batches,
            rng_marks: result.marks.clone(),
        };
        let mut next = record.clone();
        next.stage = TurnRedoStage::AutoPlayed;
        next.handoff_len = Some(result.decisions.len());
        next.outcome = Some(TurnRedoOutcome {
            kept: result.kept,
            tail_total: result.tail_total,
            stop: result.stop.clone(),
            asking_seat: result.asking_seat.clone(),
        });
        self.write_record(game_id, &snap.config, &next)?;
        let published = self.publish_history(
            game_id,
            &snap.session,
            snap.config.clone(),
            history,
            expected_version,
            snap.decisions.len(),
            actor,
            copy,
        );
        if published.is_err() {
            self.revert_record(game_id, &snap.config, Some(&record));
        }
        published
    }

    /// Auto-play of a redo with nothing recorded after the redone turn (the seat redid the turn it
    /// was still playing): there is nothing to replay, so the live game is not replaced. Replacing
    /// it anyway cost a full replay, closed every socket (an answer in flight was lost with an
    /// error) and cut off whatever had been decided live since the new turn ended.
    fn finish_without_tail(
        &self,
        game_id: &str,
        current: &Snapshot,
        record: TurnRedoRecord,
        actor: PlayerId,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let finished = find_turns(&current.decisions)
            .iter()
            .any(|t| t.seat == record.seat && t.start == record.rewound_to && t.complete);
        if !finished {
            let missing = !find_turns(&current.decisions)
                .iter()
                .any(|t| t.seat == record.seat && t.start == record.rewound_to);
            return Err(invalid(&if missing {
                TurnRedoError::NewTurnMissing
            } else {
                TurnRedoError::NewTurnNotComplete
            }));
        }
        let mut next = record;
        next.stage = TurnRedoStage::AutoPlayed;
        next.handoff_len = Some(current.decisions.len());
        next.outcome = Some(TurnRedoOutcome {
            kept: 0,
            tail_total: 0,
            stop: crate::protocol::turn_redo::TurnRedoStop::TailExhausted,
            asking_seat: current
                .session
                .pending_prompt()
                .map(|(seat, _)| seat.to_string()),
        });
        self.write_record(game_id, &current.config, &next)?;
        Ok(current.session.get_snapshot(&ViewerRole::Player(actor)))
    }

    /// Bring the timeline from before the redo back, exactly.
    ///
    /// # Errors
    /// [`HistoryError`]: `Forbidden` unless the host or the redoing seat, `InvalidTarget` when no
    /// original is saved or it is no longer restorable.
    pub fn turn_redo_restore(
        &self,
        game_id: &str,
        credential: &str,
        expected_version: u64,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let (actor, is_host) = Self::require_actor(&state, game_id, credential)?;
        let snap = self.snapshot(&state, game_id, Some(expected_version))?;
        drop(state);
        let record = self.read_record(game_id, &snap.config).ok_or_else(|| {
            HistoryError::InvalidTarget("there is no original timeline to restore".to_owned())
        })?;
        if actor != record.seat && !is_host {
            return Err(HistoryError::Forbidden(
                "only the host or the redoing seat may restore the original timeline".to_owned(),
            ));
        }
        if !Self::restorable(&record, snap.decisions.len()) {
            self.drop_record(game_id, &snap.config);
            return Err(HistoryError::InvalidTarget(
                "the original timeline can no longer be restored: the game has moved on past the redo"
                    .to_owned(),
            ));
        }
        let alt = &record.alternate;
        let (report, copy) = replay_session_with_copy(
            &snap.config.state,
            snap.config.galaxy.as_ref(),
            &alt.decisions,
            &alt.rng_marks,
            snap.generation.saturating_add(1),
        )
        .map_err(|e| HistoryError::Conflict(format!("Replay failed: {e}")))?;
        if !report.hashes_match || report.decision_count != alt.decisions.len() {
            return Err(HistoryError::Conflict("Replay diverged".to_owned()));
        }
        let revision = expected_version.checked_add(1).ok_or_else(|| {
            HistoryError::Conflict("the game version counter is exhausted".to_owned())
        })?;
        let history = GameHistory {
            decisions: alt.decisions.clone(),
            redo: alt.redo.clone(),
            events: alt.events.clone(),
            redo_events: alt.redo_events.clone(),
            // Ids handed out since stay unique.
            event_counter: alt.event_counter.max(snap.event_counter),
            revision,
            generation: snap.generation.saturating_add(1),
            batches: alt.batches.clone(),
            rng_marks: alt.rng_marks.clone(),
        };
        let published = self.publish_history(
            game_id,
            &snap.session,
            snap.config.clone(),
            history,
            expected_version,
            snap.decisions.len(),
            actor,
            copy,
        );
        if published.is_ok() {
            self.drop_record(game_id, &snap.config);
        }
        published
    }

    /// Keep the new timeline: the saved original is dropped.
    ///
    /// # Errors
    /// [`HistoryError`]: `Forbidden` unless the host or the redoing seat.
    pub fn turn_redo_keep(&self, game_id: &str, credential: &str) -> Result<(), HistoryError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let (actor, is_host) = Self::require_actor(&state, game_id, credential)?;
        let snap = self.snapshot(&state, game_id, None)?;
        drop(state);
        let Some(record) = self.read_record(game_id, &snap.config) else {
            return Ok(());
        };
        if actor != record.seat && !is_host {
            return Err(HistoryError::Forbidden(
                "only the host or the redoing seat may keep the new timeline".to_owned(),
            ));
        }
        self.drop_record(game_id, &snap.config);
        Ok(())
    }

    /// Where a redo stands, for any authenticated player; `None` when none is in flight.
    ///
    /// Reading it also discards an original that can no longer be restored.
    ///
    /// # Errors
    /// [`HistoryError`] for an unknown game or credential.
    pub fn turn_redo_status(
        &self,
        game_id: &str,
        credential: &str,
    ) -> Result<Option<TurnRedoStatus>, HistoryError> {
        let (actor, is_host, snap) = {
            let state = self.state.lock().expect("registry lock");
            let (actor, is_host) = Self::require_actor(&state, game_id, credential)?;
            (actor, is_host, self.snapshot(&state, game_id, None)?)
        };
        let Some(record) = self.read_record(game_id, &snap.config) else {
            return Ok(None);
        };
        if !Self::restorable(&record, snap.decisions.len()) {
            let gate = self.game_gate(game_id);
            let _reservation = gate.lock().expect("game gate lock");
            self.drop_record(game_id, &snap.config);
            return Ok(None);
        }
        let turn_complete = record.stage == TurnRedoStage::NewTurn
            && find_turns(&snap.decisions)
                .iter()
                .any(|t| t.seat == record.seat && t.start == record.rewound_to && t.complete);
        let asking = snap
            .session
            .pending_prompt()
            .map(|(seat, _)| seat.to_string());
        let outcome = record.outcome.clone().map(|mut outcome| {
            // The seat asked right now, not the one asked when auto-play stopped.
            if asking.is_some() {
                outcome.asking_seat.clone_from(&asking);
            }
            redact_for_viewer(&mut outcome, &actor);
            outcome
        });
        Ok(Some(TurnRedoStatus {
            seat: record.seat.to_string(),
            requested_by: record.requested_by.to_string(),
            turns_back: record.turns_back,
            redo_count: record.redo_count,
            stage: record.stage,
            original_decisions: record.alternate.decisions.len(),
            rewound_to: record.rewound_to,
            turn_complete,
            handoff_len: record.handoff_len,
            outcome,
            can_control: is_host || actor == record.seat,
        }))
    }
}
