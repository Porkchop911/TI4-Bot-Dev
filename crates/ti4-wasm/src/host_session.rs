//! `HostSession`: what the server's `SessionShared` (and the thread of `worker.rs`) does around the
//! engine for one game with one person at the table, as plain data with no threads and no
//! sockets. It sits on `ti4-host` (the event journal, the state update, the reach answer) and
//! turns what the engine does into the `ServerMessage`s a `web/` client reads.
//!
//! The session owns no `Game`. The crate's play loop (`play` in `lib.rs`) owns the game and calls
//! the hooks below at the points where the server's worker acts:
//!
//! | Server (`worker.rs`)                         | Here                                   |
//! |----------------------------------------------|----------------------------------------|
//! | `table.on_observed_offer` closure            | [`HostSession::offer`]                 |
//! | `ObservedDecider` (what was chosen)          | [`HostSession::selected`]              |
//! | `RemoteHumanDecider::choose` (state update)  | [`HostSession::ask`]                   |
//! | the code after `game.step()`                 | [`HostSession::after_step`]            |
//! | `UndoTrack` / registry undo                  | checkpoints: [`HostSession::mark`], [`HostSession::took_back`] |
//! | `SessionShared::*` commands                  | [`HostSession::command`]               |
//!
//! Differences from the server, all on purpose:
//!
//! - One `state_update` per question of the person (and one at the end), not one per engine step:
//!   nobody can read the steps in between, and a state of a game is large. `game_version` counts
//!   those updates.
//! - The pending choice travels inside the `state_update` / `initial_snapshot` only; the separate
//!   `pending_choice` message is not sent.
//! - Undo is the engine checkpoint of this crate (no `UndoTrack` barriers: one person, and a
//!   checkpoint restores the dice, so an undo cannot be used to re-roll).
//! - Redo and rewind are by replay: the session says what to replay ([`Restart`]); the host starts
//!   the game again with those answers.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, DecisionRecord};
use ti4_engine::game::Game;
use ti4_host::PROTOCOL_VERSION;
use ti4_host::batch::{LiveBatch, LiveStep, MovementPlan};
use ti4_host::clock::Clock;
use ti4_host::journal::{self, EventDraft};
use ti4_host::projection::{project_initial_snapshot_with_map, project_turn_status};
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::error::ErrorKind;
use ti4_host::protocol::reach::ReachIntoMsg;
use ti4_host::protocol::server::{
    ActionAcceptedMsg, ActionRejectedMsg, AutoResolvedNote, DraftAppliedMsg, EventVisibility,
    GameEvent, GameEventKind, GameEventMsg, GameOverMsg, MovementFact, PongMsg, ProtocolErrorMsg,
    ScriptStopped, SeatDecisionDetail, ServerMessage, StateUpdateMsg, current_log_path,
    public_roll, verified_decision_facts,
};
use ti4_host::protocol::status::{RejectionReason, ViewerRole};
use ti4_host::state_update::{
    StateUpdateInputs, answer_reach, queue_auto_resolved, state_update_for,
};
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, Phase, ReactionMode};
use ti4_view::map::GalaxyLayout;
use ti4_view::planning::draft::{DraftStep, ScriptAnswer, needless, script_answer};
use ti4_view::planning::fork::TACTICAL_COMPLETE;
use ti4_view::planning::steps::{INTERRUPTED, PlanStep};
use ti4_view::reaction_modes::NeverSet;
use ti4_view::view::BoardTileView;

/// The most checkpoints (and so session marks) kept; see `CHECKPOINT_LIMIT` in `lib.rs`.
pub const MARK_LIMIT: usize = 32;

/// What the host does with the answer of [`HostSession::command`] for the parked game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resume {
    /// Answer the pending choice with the option at this place.
    Answer(usize),
    /// Take back the last answer (the engine's `UNDO`).
    Undo,
    /// End the run (the engine's `LEAVE`).
    Leave,
}

/// A history change that needs the game to be played again: the host ends the parked run and
/// starts a new one with these answers (`HostSession::command` also returns [`Resume::Leave`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Restart {
    /// The ids of the person's answers to play again, in order.
    pub answers: Vec<String>,
    /// The answers after them that a redo can bring back, next first.
    pub redo: Vec<String>,
    /// The history generation of the new run.
    pub generation: u64,
    /// The game version so far: the new run goes on from it, so that a client that keeps what it
    /// has does not take the new snapshot for an old message.
    pub version: u64,
}

/// The result of a command, besides the messages (those are in [`HostSession::drain`]).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Handled {
    pub resume: Option<Resume>,
    pub restart: Option<Restart>,
    /// Set by `save_state`.
    pub save: Option<serde_json::Value>,
}

/// Commands that are not `ClientMessage`s: the server has them as HTTP calls, or not at all.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum LocalCommand {
    /// One of the history actions of `changeHistory` in `web/`.
    History { action: HistoryAction },
    /// The save of the game as it stands (answers, seed, hashes).
    SaveState {},
    /// End the run without a restart (leave the game).
    Leave {},
    /// A staged movement, production or payment of `web/` (`BatchPlan`), sent as one request.
    SubmitBatch {
        nonce: String,
        expected_version: u64,
        plan: serde_json::Value,
    },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HistoryAction {
    Undo,
    UndoBatch,
    UndoPipeline,
    Redo,
    RedoBatch,
    RedoPipeline,
    RestoreCursor { cursor: usize },
    Restore { event_id: String },
}

/// A script (a draft or a batch) that answers the person's choices from the engine thread.
#[derive(Debug, Clone)]
pub struct ScriptRun {
    nonce: String,
    script: Vec<DraftStep>,
    next: usize,
    /// Whose turn it was. The script does not outlive that turn.
    active: Option<PlayerId>,
}

/// A batch (a staged movement, production or payment) that answers the person's choices.
struct BatchRun {
    live: LiveBatch,
    nonce: String,
    /// The number of the choice the batch began at: a failed batch goes back to it.
    start: u64,
}

/// What the session says to a question of the person while a batch or a script is running.
pub enum Scripted {
    /// Answer with this option.
    Answer(ChoiceOption),
    /// Nothing answers: the person is asked.
    Ask,
    /// A step of the batch failed: go back to the choice with this number (nothing of the batch
    /// stays in the game) and ask the person again.
    Rollback(u64),
}

/// The session as it was at a checkpoint of the play loop.
#[derive(Debug, Clone)]
struct Mark {
    /// `asked` of the checkpoint (the key the play loop restores by).
    asked: u64,
    decisions: usize,
    events: usize,
    event_counter: u64,
    rolls_seen: usize,
    answers: usize,
    phase: Phase,
    round: u32,
}

/// The facts of a game that its save needs, set once by [`HostSession::new`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub game_id: String,
    pub seed: u64,
    pub players: usize,
    /// 0-based place of the person.
    pub human_seat: usize,
    /// The setup as the host was given it (card set, map, factions...): a save replays with it.
    pub setup: serde_json::Value,
}

/// The session of one game.
pub struct HostSession {
    identity: Identity,
    clock: Box<dyn Clock>,
    viewer: PlayerId,
    never: NeverSet,
    // The timeline.
    version: u64,
    generation: u64,
    decision_log: Vec<DecisionRecord>,
    events: Vec<GameEvent>,
    event_counter: u64,
    rolls_seen: usize,
    prev_phase: Phase,
    prev_round: u32,
    selected: VecDeque<(Choice, ChoiceOption)>,
    /// The person's answers (ids), in order, and the plan step each one is, for `undone`.
    answers: Vec<String>,
    answer_steps: Vec<Option<PlanStep>>,
    redo: Vec<String>,
    marks: VecDeque<Mark>,
    // The map: it does not change during a game.
    tiles: Vec<BoardTileView>,
    layout: GalaxyLayout,
    galaxy: Option<Galaxy>,
    // The position the person sees.
    state: GameState,
    pending: Option<(Choice, String)>,
    can_undo: bool,
    /// The option the person submitted, until its decision is recorded.
    awaiting: Option<String>,
    /// The next question starts with a full `initial_snapshot` (a new run, an undo).
    fresh: bool,
    /// Notes of the game's auto-resolved single-option steps, for the next update.
    auto_resolved: Vec<(PlayerId, AutoResolvedNote)>,
    undone: Vec<PlanStep>,
    script: Option<ScriptRun>,
    batch: Option<BatchRun>,
    /// How the last batch ended, for the host (taken by [`HostSession::take_batch_result`]).
    batch_result: Option<serde_json::Value>,
    // Replay of a save.
    /// The person's answers that are replayed from a save: no redo bookkeeping, no notes.
    quiet: usize,
    saved_events: Vec<(String, Option<u64>)>,
    // The end.
    finished: bool,
    error: Option<String>,
    /// The messages for the host, oldest first.
    out: Vec<ServerMessage>,
}

fn is_action_card_name(name: &str) -> bool {
    ContentStore::embedded()
        .records(ti4_model::content_types::ContentType::ActionCards)
        .iter()
        .any(|record| record.text("name") == Some(name))
}

impl HostSession {
    /// A session for the game that begins at `state`.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: Identity,
        clock: Box<dyn Clock>,
        never: NeverSet,
        state: &GameState,
        tiles: Vec<BoardTileView>,
        galaxy: Option<Galaxy>,
        answers_to_replay: usize,
        redo: Vec<String>,
        generation: u64,
        version: u64,
        saved_events: Vec<(String, Option<u64>)>,
    ) -> Self {
        let layout = galaxy.as_ref().map_or_else(
            || GalaxyLayout {
                version: 1,
                active_sources: Vec::new(),
                placements: Vec::new(),
                off_map_system_ids: Vec::new(),
            },
            |galaxy| GalaxyLayout::from_galaxy(galaxy, ti4_model::content_types::POK),
        );
        let viewer = PlayerId::new(crate::SEATS[identity.human_seat]);
        let mut session = Self {
            identity,
            clock,
            viewer,
            never,
            version: version.max(1),
            generation,
            decision_log: Vec::new(),
            events: Vec::new(),
            event_counter: 0,
            rolls_seen: 0,
            prev_phase: state.phase,
            prev_round: state.round,
            selected: VecDeque::new(),
            answers: Vec::new(),
            answer_steps: Vec::new(),
            redo,
            marks: VecDeque::new(),
            tiles,
            layout,
            galaxy,
            state: state.clone(),
            pending: None,
            can_undo: false,
            awaiting: None,
            fresh: true,
            auto_resolved: Vec::new(),
            undone: Vec::new(),
            script: None,
            batch: None,
            batch_result: None,
            quiet: answers_to_replay,
            saved_events,
            finished: false,
            error: None,
            out: Vec::new(),
        };
        // The first event of every game, as the server's worker records it.
        session.push_event(
            EventVisibility::Public,
            GameEventKind::GameInitialized {
                round: state.round,
                phase: state.phase,
                speaker: state.speaker.clone(),
            },
            0,
            None,
            None,
            None,
        );
        session
    }

    #[must_use]
    pub fn game_id(&self) -> &str {
        &self.identity.game_id
    }

    #[must_use]
    pub fn viewer(&self) -> ViewerRole {
        ViewerRole::Player(self.viewer.clone())
    }

    #[must_use]
    pub fn finished(&self) -> bool {
        self.finished
    }

    #[must_use]
    pub fn decision_log(&self) -> &[DecisionRecord] {
        &self.decision_log
    }

    #[must_use]
    pub fn events(&self) -> &[GameEvent] {
        &self.events
    }

    #[must_use]
    pub fn answers(&self) -> &[String] {
        &self.answers
    }

    #[must_use]
    pub fn version(&self) -> u64 {
        self.version
    }

    #[must_use]
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Queue a message for the host.
    pub fn push_message(&mut self, message: ServerMessage) {
        self.out.push(message);
    }

    /// Take the messages for the host.
    pub fn drain(&mut self) -> Vec<ServerMessage> {
        std::mem::take(&mut self.out)
    }

    /// The person's answers that a save replays and that have not been replayed yet.
    #[must_use]
    pub fn replaying(&self) -> bool {
        self.quiet > 0
    }

    /// The play loop is going to re-answer `count` answers of the person without asking (after
    /// an undo).
    pub fn replaying_again(&mut self, count: usize) {
        self.quiet = count;
    }

    fn protocol_error(&mut self, kind: ErrorKind, message: impl Into<String>) {
        self.out.push(ServerMessage::Error(ProtocolErrorMsg {
            protocol_version: PROTOCOL_VERSION,
            kind,
            message: message.into(),
        }));
    }

    fn reject(&mut self, reason: RejectionReason) {
        self.out
            .push(ServerMessage::ActionRejected(ActionRejectedMsg {
                protocol_version: PROTOCOL_VERSION,
                game_id: self.identity.game_id.clone(),
                game_version: self.version,
                reason,
            }));
    }

    // --- events -------------------------------------------------------------------------

    fn emit_event(&mut self, mut entry: GameEvent) {
        if let Some((time, version)) = self.saved_events.get(self.events.len()) {
            entry.timestamp.clone_from(time);
            if version.is_some() {
                entry.version = *version;
            }
        }
        if !self.fresh
            && let Some(projected) = entry.for_viewer(&self.viewer())
        {
            self.out.push(ServerMessage::Event(GameEventMsg {
                protocol_version: PROTOCOL_VERSION,
                game_id: self.identity.game_id.clone(),
                entry: projected,
            }));
        }
        self.events.push(entry);
    }

    fn push_event(
        &mut self,
        visibility: EventVisibility,
        event: GameEventKind,
        decision_count: usize,
        detail: Option<String>,
        movement: Option<MovementFact>,
        seat_detail: Option<SeatDecisionDetail>,
    ) {
        self.event_counter += 1;
        let entry = journal::build_event(
            self.clock.as_ref(),
            &self.identity.game_id,
            self.event_counter,
            &self.decision_log,
            EventDraft {
                visibility,
                event,
                version: Some(self.version),
                decision_count,
                detail,
                movement,
                seat_detail,
            },
        );
        self.emit_event(entry);
    }

    // --- hooks of the play loop ---------------------------------------------------------

    /// What a seat chose (any seat): kept until its record is published, for the facts of the
    /// event.
    pub fn selected(&mut self, choice: &Choice, option: &ChoiceOption) {
        self.selected.push_back((choice.clone(), option.clone()));
    }

    fn selected_for(&mut self, record: &DecisionRecord, remove: bool) -> Option<ChoiceOption> {
        let index = self.selected.iter().position(|(choice, option)| {
            choice.player == record.player
                && choice.prompt == record.prompt
                && option.id == record.chosen
        })?;
        if remove {
            self.selected.remove(index).map(|(_, option)| option)
        } else {
            self.selected.get(index).map(|(_, option)| option.clone())
        }
    }

    /// The engine reaches an offer (before its seat decides): the decisions since the last look
    /// are published, on the position this offer is asked in. `state` is that position.
    pub fn offer(&mut self, records: &[DecisionRecord], state: &GameState) {
        self.publish_decisions(records, state, true);
    }

    fn publish_decisions(&mut self, records: &[DecisionRecord], state: &GameState, nested: bool) {
        while self.decision_log.len() < records.len() {
            let index = self.decision_log.len();
            let record = &records[index];
            if nested {
                // Another card may be played inside this card's timing window: its outer choice
                // has settled but the announcement has not happened yet. Leave it for the next
                // boundary (the server does the same).
                let selected = self.selected_for(record, false);
                if record.context.as_ref().zip(selected.as_ref()).is_some_and(
                    |(context, option)| {
                        ti4_host::protocol::server::played_card_detail(
                            context,
                            option,
                            &record.player,
                        )
                        .is_some()
                            && !option
                                .payload
                                .get("card")
                                .and_then(serde_json::Value::as_str)
                                .is_some_and(|alias| {
                                    state.action_card_plays.iter().any(|(seat, card)| {
                                        seat == &record.player && card.as_str() == alias
                                    })
                                })
                    },
                ) {
                    break;
                }
            }
            let selected = self.selected_for(record, true);
            self.decision_log.push(record.clone());
            let (detail, movement, seat_detail) = verified_decision_facts(
                record,
                selected.as_ref(),
                state.active_system.as_ref().map(|id| id.as_str()),
                state,
            );
            self.push_event(
                EventVisibility::Public,
                GameEventKind::DecisionResolved,
                index + 1,
                detail,
                movement,
                seat_detail,
            );
            if record.player == self.viewer {
                self.note_answer(record, selected.as_ref());
            }
        }
    }

    /// A decision of the person is in the log.
    fn note_answer(&mut self, record: &DecisionRecord, selected: Option<&ChoiceOption>) {
        self.answers.push(record.chosen.clone());
        self.answer_steps
            .push(selected.and_then(PlanStep::of_option));
        // A redo list survives only while the person plays it again (as the server's does). An
        // answer that is played again from a save or after an undo is no new decision.
        if self.quiet > 0 {
            self.quiet -= 1;
        } else {
            if self.redo.first() == Some(&record.chosen) {
                self.redo.remove(0);
            } else {
                self.redo.clear();
            }
        }
        if self.awaiting.as_deref() == Some(record.chosen.as_str()) {
            self.awaiting = None;
            self.out
                .push(ServerMessage::ActionAccepted(ActionAcceptedMsg {
                    protocol_version: PROTOCOL_VERSION,
                    game_id: self.identity.game_id.clone(),
                    game_version: self.version,
                    option_id: record.chosen.clone(),
                }));
        }
    }

    /// A single-option step of the game that no seat was asked, or a window that was declined
    /// for a Never card: noted for the next update of its seat.
    pub fn auto_resolved(&mut self, seat: &PlayerId, prompt: &str, selected: &str, reason: &str) {
        self.passed_window(seat, prompt, selected, reason, &[]);
    }

    pub fn passed_window(
        &mut self,
        seat: &PlayerId,
        prompt: &str,
        selected: &str,
        reason: &str,
        cards: &[String],
    ) {
        if self.quiet > 0 || seat != &self.viewer {
            return;
        }
        queue_auto_resolved(
            &mut self.auto_resolved,
            self.decision_log.len(),
            self.version,
            seat,
            prompt,
            selected,
            reason,
            cards,
        );
    }

    /// The game finished a step. `events_before` is `game.events.len()` before it.
    pub fn after_step(&mut self, game: &Game<'_>, events_before: usize) {
        self.publish_decisions(&game.table.log.records, &game.state, false);
        // Dice rolled by this step are public. A game replayed from a save rolled them before
        // the person looked, but the event log is made again, so they are recorded all the same.
        let rolled = game.rolls().len();
        self.rolls_seen = self.rolls_seen.min(rolled);
        if rolled > self.rolls_seen {
            let active = game.state.active_system.as_ref().map(|id| id.as_str());
            let fresh: Vec<_> = game.rolls()[self.rolls_seen..]
                .iter()
                .enumerate()
                .filter_map(|(offset, roll)| public_roll(self.rolls_seen + offset, roll, active))
                .collect();
            for roll in fresh {
                if journal::roll_is_known(&self.events, &roll) {
                    continue;
                }
                let cursor = journal::roll_cursor(&self.events);
                self.event_counter += 1;
                let entry = journal::build_roll_event(
                    self.clock.as_ref(),
                    &self.identity.game_id,
                    self.event_counter,
                    cursor,
                    game.state.round,
                    game.state.phase,
                    Some(self.version),
                    roll,
                );
                self.emit_event(entry);
            }
            self.rolls_seen = rolled;
        }
        if game.state.phase != self.prev_phase || game.state.round != self.prev_round {
            self.push_event(
                EventVisibility::Public,
                GameEventKind::PhaseTransition {
                    phase: game.state.phase,
                    round: game.state.round,
                },
                self.decision_log.len(),
                None,
                None,
                None,
            );
            self.prev_phase = game.state.phase;
            self.prev_round = game.state.round;
        }
        if game.state.finished {
            self.state.clone_from(&game.state);
            self.finish(&game.state);
        }
        let tactical_over = game
            .events
            .get(events_before..)
            .unwrap_or_default()
            .iter()
            .any(|event| event == TACTICAL_COMPLETE);
        self.script_after_step(tactical_over, &game.state);
    }

    fn finish(&mut self, state: &GameState) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.pending = None;
        self.can_undo = false;
        let winner = ti4_engine::objectives::leader(state);
        self.push_event(
            EventVisibility::Public,
            GameEventKind::GameFinished {
                winner: winner.clone(),
            },
            self.decision_log.len(),
            None,
            None,
            None,
        );
        self.version += 1;
        let update = self.state_update();
        self.out.push(update);
        self.out.push(ServerMessage::GameOver(GameOverMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: self.identity.game_id.clone(),
            game_version: self.version,
            winner,
            final_scores: state
                .players
                .iter()
                .map(|player| {
                    (
                        player.id.clone(),
                        player.victory_points.max(0).cast_unsigned(),
                    )
                })
                .collect(),
        }));
    }

    /// The engine failed: the run ends.
    pub fn failed(&mut self, message: &str) {
        self.error = Some(message.to_owned());
        self.protocol_error(ErrorKind::EngineError, message);
    }

    // --- the question of the person -------------------------------------------------------

    fn reaction_modes(&self) -> BTreeMap<String, ReactionMode> {
        self.never.lock().expect("never set lock").clone()
    }

    fn state_update(&mut self) -> ServerMessage {
        let pending = self
            .pending
            .as_ref()
            .map(|(choice, nonce)| (choice, nonce.as_str()));
        let auto_resolved = std::mem::take(&mut self.auto_resolved);
        let modes: BTreeMap<PlayerId, BTreeMap<String, ReactionMode>> =
            BTreeMap::from([(self.viewer.clone(), self.reaction_modes())]);
        let viewer = self.viewer();
        let inputs = StateUpdateInputs {
            game_id: &self.identity.game_id,
            version: self.version,
            state: &self.state,
            pending,
            map_tiles: &self.tiles,
            galaxy_layout: &self.layout,
            galaxy: self.galaxy.as_ref(),
            watch: &crate::SECONDARY,
            decision_count: self.decision_log.len(),
            redo_count: self.redo.len(),
            history_generation: self.generation,
            decision_log: &self.decision_log,
            auto_resolved: &auto_resolved,
            reaction_modes: &modes,
            may_undo: self.can_undo.then_some(&viewer),
            // The offline game of `web/` has no table to tell.
            seats: None,
        };
        let path = inputs.path();
        state_update_for(&inputs, &path, &viewer)
    }

    /// The full state of the game for the person, as a new connection gets it.
    fn snapshot(&mut self) -> ServerMessage {
        let viewer = self.viewer();
        let pending = self
            .pending
            .as_ref()
            .map(|(choice, nonce)| (choice, nonce.as_str()));
        let mut snapshot = project_initial_snapshot_with_map(
            &self.identity.game_id,
            self.version,
            &self.state,
            &viewer,
            pending,
            &self.tiles,
            &self.layout,
            self.galaxy.as_ref(),
            Some(&crate::SECONDARY),
            &self.events,
        )
        .with_history(self.decision_log.len(), self.redo.len(), self.generation);
        snapshot.history.can_undo = self.can_undo;
        snapshot.reaction_modes = self.reaction_modes();
        snapshot.current_path = current_log_path(
            &self.state,
            pending.map(|(choice, _)| choice),
            &self.decision_log,
        );
        snapshot.undone = std::mem::take(&mut self.undone);
        ServerMessage::InitialSnapshot(snapshot)
    }

    /// The game waits for the person in `choice`, numbered `nonce`, at `state`. `can_undo` is
    /// whether the play loop can take back the last answer.
    pub fn ask(&mut self, choice: &Choice, nonce: &str, state: &GameState, can_undo: bool) {
        self.state.clone_from(state);
        self.version += 1;
        self.pending = Some((choice.clone(), nonce.to_owned()));
        self.can_undo = can_undo;
        self.awaiting = None;
        let message = if self.fresh {
            self.fresh = false;
            self.snapshot()
        } else {
            self.state_update()
        };
        self.out.push(message);
    }

    /// The person answered: no question is pending until the next `ask`.
    pub fn answered(&mut self) {
        self.pending = None;
    }

    // --- checkpoints and undo -------------------------------------------------------------

    /// The play loop made a checkpoint (`asked` choices of the person so far).
    pub fn mark(&mut self, asked: u64, decisions: usize) {
        self.marks.push_back(Mark {
            asked,
            decisions,
            events: self.events.len(),
            event_counter: self.event_counter,
            rolls_seen: self.rolls_seen,
            answers: self.answers.len(),
            phase: self.prev_phase,
            round: self.prev_round,
        });
        while self.marks.len() > MARK_LIMIT {
            self.marks.pop_front();
        }
    }

    /// The play loop went back to the checkpoint of `asked`: the session goes back with it, and
    /// the next question starts with a snapshot.
    ///
    /// With `rollback` the answers that go were a batch that failed: they are not a decision of
    /// the person to undo (no redo, no `undone`, the history stays the same generation).
    pub fn took_back(&mut self, asked: u64, rollback: bool) {
        while self.marks.back().is_some_and(|mark| mark.asked > asked) {
            self.marks.pop_back();
        }
        let Some(mark) = self.marks.back().cloned() else {
            return;
        };
        let undone_answers: Vec<String> = self.answers.split_off(mark.answers);
        let undone_steps = self.answer_steps.split_off(mark.answers);
        if !rollback {
            self.undone = undone_steps.into_iter().flatten().collect();
            let mut redo = undone_answers;
            redo.append(&mut self.redo);
            self.redo = redo;
            self.generation += 1;
        }
        self.decision_log.truncate(mark.decisions);
        self.events.truncate(mark.events);
        self.event_counter = mark.event_counter;
        self.rolls_seen = mark.rolls_seen;
        self.prev_phase = mark.phase;
        self.prev_round = mark.round;
        self.selected.clear();
        self.saved_events.clear();
        self.pending = None;
        self.awaiting = None;
        self.script = None;
        self.auto_resolved.clear();
        self.batch = None;
        self.fresh = true;
    }

    // --- scripts --------------------------------------------------------------------------

    /// The answer of a running script to `choice` of the person, when it has one.
    pub fn script_option(&mut self, choice: &Choice) -> Option<ChoiceOption> {
        let application = self.script.as_mut()?;
        match script_answer(&application.script, application.next, choice, &self.viewer) {
            ScriptAnswer::Answer {
                option,
                next,
                script,
            } => {
                application.next = next;
                if let Some(script) = script {
                    application.script = script;
                }
                if application.next >= application.script.len() {
                    self.end_script(|_| None);
                }
                Some(option)
            }
            ScriptAnswer::Done => {
                self.end_script(|_| None);
                None
            }
            ScriptAnswer::Interrupted { reason } | ScriptAnswer::Mismatch { reason, .. } => {
                // The entries before this one are in the game and stay there.
                self.end_script(|application| Some(stopped(application, &reason)));
                None
            }
        }
    }

    fn end_script(&mut self, stop: impl FnOnce(&ScriptRun) -> Option<ScriptStopped>) {
        if let Some(application) = self.script.take() {
            let stopped = stop(&application);
            self.out.push(ServerMessage::DraftApplied(DraftAppliedMsg {
                protocol_version: PROTOCOL_VERSION,
                game_id: self.identity.game_id.clone(),
                nonce: application.nonce,
                stopped,
            }));
        }
    }

    fn script_after_step(&mut self, tactical_over: bool, state: &GameState) {
        let over = self
            .script
            .as_ref()
            .is_some_and(|application| tactical_over || application.active != state.active);
        if over {
            self.end_script(|application| {
                (!needless(&application.script[application.next..]))
                    .then(|| stopped(application, ACTION_OVER))
            });
        }
    }

    // --- commands -------------------------------------------------------------------------

    /// A command of the host. Messages are queued for [`HostSession::drain`].
    ///
    /// `precheck` plays a script on a copy of the game and says why it would be refused
    /// (`None`: it plays). `can_undo` is [`HostSession::ask`]'s.
    pub fn client_message(
        &mut self,
        message: ClientMessage,
        precheck: &mut dyn FnMut(&[DraftStep]) -> Option<String>,
    ) -> Handled {
        let mut handled = Handled::default();
        match message {
            ClientMessage::Subscribe { .. } => {
                let snapshot = self.snapshot();
                self.out.push(snapshot);
            }
            ClientMessage::Ping { sequence, .. } => {
                self.out.push(ServerMessage::Pong(PongMsg {
                    protocol_version: PROTOCOL_VERSION,
                    sequence,
                }));
            }
            ClientMessage::SubmitChoice {
                nonce,
                expected_version,
                option_id,
                ..
            } => {
                if let Some(index) = self.check_pending(&nonce, expected_version, Some(&option_id))
                {
                    self.awaiting = Some(option_id);
                    handled.resume = Some(Resume::Answer(index));
                }
            }
            ClientMessage::ApplyDraft {
                nonce,
                expected_version,
                script,
                ..
            } => {
                handled.resume = self.apply_script(&nonce, expected_version, script, precheck);
            }
            ClientMessage::Undo {
                expected_version, ..
            } => {
                if expected_version != self.version {
                    self.reject(RejectionReason::StaleVersion {
                        expected: expected_version,
                        current: self.version,
                    });
                } else {
                    handled.resume = self.undo();
                }
            }
            ClientMessage::SetReactionMode { card, mode, .. } => {
                if !is_action_card_name(&card) {
                    self.protocol_error(
                        ErrorKind::MalformedMessage,
                        format!("unknown action card '{card}'"),
                    );
                } else {
                    {
                        let mut never = self.never.lock().expect("never set lock");
                        ti4_view::reaction_modes::set_mode(&mut never, &card, mode);
                    }
                    let update = self.state_update();
                    self.out.push(update);
                }
            }
            ClientMessage::QueryReach {
                request_id,
                system_id,
                ..
            } => {
                let reply = self.reach(request_id, &system_id);
                self.out.push(ServerMessage::ReachInto(reply));
            }
            ClientMessage::PreviewSecondary { .. } => {
                self.protocol_error(
                    ErrorKind::MalformedMessage,
                    "preview_secondary is not available in an offline game",
                );
            }
            // A bluff hold needs a second person; there is none.
            ClientMessage::SetReactionIntent { .. } | ClientMessage::PassReactionHold { .. } => {}
            // Staged work is kept by the server so that the tabs of one seat share it; an
            // offline game lives in one page, which keeps its staged work itself. Ignored, as
            // `Subscribe::staged` is (no `SeatStaged` is ever sent).
            ClientMessage::PutStaged { .. } => {}
            ClientMessage::RunDraft { .. }
            | ClientMessage::StartPlanning { .. }
            | ClientMessage::ResetPlanning { .. }
            | ClientMessage::EditPlanningMovement { .. }
            | ClientMessage::ApplyPlanning { .. }
            | ClientMessage::SubmitPlanningChoice { .. }
            | ClientMessage::SecondaryPlanning { .. } => {
                // `RunDraft` is answered by the caller (it needs the copy of the game); the
                // server-side planning protocol is replaced by drafts.
                self.protocol_error(
                    ErrorKind::MalformedMessage,
                    "this message is answered by the host, not by the session",
                );
            }
        }
        handled
    }

    /// The answer of `ClientMessage::QueryReach`.
    #[must_use]
    pub fn reach(&self, request_id: u64, system_id: &str) -> ReachIntoMsg {
        answer_reach(
            &self.identity.game_id,
            request_id,
            system_id,
            self.version,
            self.galaxy.as_ref().map(|galaxy| (&self.state, galaxy)),
        )
    }

    /// Checks the nonce and the version of a request against the pending choice. On success the
    /// place of `option_id` in it (0 when none is asked for).
    fn check_pending(
        &mut self,
        nonce: &str,
        expected_version: u64,
        option_id: Option<&str>,
    ) -> Option<usize> {
        let Some((choice, pending_nonce)) = &self.pending else {
            self.reject(RejectionReason::NoPendingChoice);
            return None;
        };
        if pending_nonce != nonce {
            self.reject(RejectionReason::StaleNonce);
            return None;
        }
        if expected_version != self.version {
            self.reject(RejectionReason::StaleVersion {
                expected: expected_version,
                current: self.version,
            });
            return None;
        }
        let Some(option_id) = option_id else {
            return Some(0);
        };
        match choice
            .options
            .iter()
            .position(|option| option.id == option_id)
        {
            Some(index) => Some(index),
            None if choice.answer_for(option_id).is_some() => {
                // A free-form answer (a deal) has no place among the options, and the engine is
                // answered by place.
                self.reject(RejectionReason::ValidationFailed {
                    message: "free-form answers are not available in an offline game".to_owned(),
                });
                None
            }
            None => {
                self.reject(RejectionReason::UnknownOption {
                    option_id: option_id.to_owned(),
                });
                None
            }
        }
    }

    fn apply_script(
        &mut self,
        nonce: &str,
        expected_version: u64,
        script: Vec<DraftStep>,
        precheck: &mut dyn FnMut(&[DraftStep]) -> Option<String>,
    ) -> Option<Resume> {
        self.check_pending(nonce, expected_version, None)?;
        let (choice, _) = self.pending.clone()?;
        let refuse = |this: &mut Self, reason: String| {
            this.out.push(ServerMessage::DraftApplied(DraftAppliedMsg {
                protocol_version: PROTOCOL_VERSION,
                game_id: this.identity.game_id.clone(),
                nonce: nonce.to_owned(),
                stopped: Some(refused(&script, reason)),
            }));
        };
        let first = script_answer(&script, 0, &choice, &self.viewer);
        let (option, next, reordered) = match first {
            ScriptAnswer::Answer {
                option,
                next,
                script,
            } => (option, next, script),
            ScriptAnswer::Done => {
                refuse(self, "The script has no answer for this choice.".to_owned());
                return None;
            }
            ScriptAnswer::Interrupted { reason } | ScriptAnswer::Mismatch { reason, .. } => {
                refuse(self, reason);
                return None;
            }
        };
        if let Some(reason) = precheck(&script) {
            refuse(self, reason);
            return None;
        }
        let index = choice.options.iter().position(|o| o.id == option.id)?;
        let script_vec = reordered.unwrap_or_else(|| script.clone());
        let whole = next >= script_vec.len();
        self.script = Some(ScriptRun {
            nonce: nonce.to_owned(),
            script: script_vec,
            next,
            active: self.state.active.clone(),
        });
        self.awaiting = Some(option.id);
        if whole {
            self.end_script(|_| None);
        }
        Some(Resume::Answer(index))
    }

    /// A live undo through the checkpoints.
    fn undo(&mut self) -> Option<Resume> {
        if self.pending.is_none() || self.finished {
            self.reject(RejectionReason::NoPendingChoice);
            return None;
        }
        if !self.can_undo {
            self.reject(RejectionReason::NoPendingChoice);
            return None;
        }
        Some(Resume::Undo)
    }

    /// A command that is not a `ClientMessage`.
    pub fn local(&mut self, command: LocalCommand, engine: serde_json::Value) -> Handled {
        let mut handled = Handled::default();
        match command {
            LocalCommand::SaveState {} => handled.save = Some(self.save_state(engine)),
            LocalCommand::Leave {} => handled.resume = Some(Resume::Leave),
            LocalCommand::SubmitBatch {
                nonce,
                expected_version,
                plan,
            } => handled.resume = self.submit_batch(&nonce, expected_version, &plan),
            LocalCommand::History { action } => match action {
                HistoryAction::Undo | HistoryAction::UndoBatch | HistoryAction::UndoPipeline => {
                    handled.resume = self.undo();
                }
                HistoryAction::Redo | HistoryAction::RedoBatch | HistoryAction::RedoPipeline => {
                    if self.redo.is_empty() {
                        self.reject(RejectionReason::NoPendingChoice);
                    } else {
                        let mut answers = self.answers.clone();
                        answers.push(self.redo[0].clone());
                        handled.restart = Some(Restart {
                            answers,
                            redo: self.redo[1..].to_vec(),
                            generation: self.generation + 1,
                            version: self.version,
                        });
                        handled.resume = Some(Resume::Leave);
                    }
                }
                HistoryAction::RestoreCursor { cursor } => {
                    self.restore(cursor, &mut handled);
                }
                HistoryAction::Restore { event_id } => {
                    let cursor = self
                        .events
                        .iter()
                        .find(|event| event.id == event_id)
                        .and_then(|event| event.decision_count);
                    match cursor {
                        Some(cursor) => self.restore(cursor, &mut handled),
                        None => self.reject(RejectionReason::ValidationFailed {
                            message: format!("no event {event_id}"),
                        }),
                    }
                }
            },
        }
        handled
    }

    /// Go back to the position after `cursor` decisions: the person's answers among them are
    /// played again, and the game then runs on to the person's next question.
    fn restore(&mut self, cursor: usize, handled: &mut Handled) {
        if cursor > self.decision_log.len() {
            self.reject(RejectionReason::ValidationFailed {
                message: "that point of the game has not been reached".to_owned(),
            });
            return;
        }
        let keep = self.decision_log[..cursor]
            .iter()
            .filter(|record| record.player == self.viewer)
            .count();
        let mut redo = self.answers[keep..].to_vec();
        redo.extend(self.redo.iter().cloned());
        handled.restart = Some(Restart {
            answers: self.answers[..keep].to_vec(),
            redo,
            generation: self.generation + 1,
            version: self.version,
        });
        handled.resume = Some(Resume::Leave);
    }

    /// A staged plan of the page (`BatchPlan` of `web/`) sent as one request: the person's
    /// choices are answered from it as the engine asks them. The first step is checked at once;
    /// a later step that fails takes the game back to this question ([`Scripted::Rollback`]).
    fn submit_batch(
        &mut self,
        nonce: &str,
        expected_version: u64,
        plan: &serde_json::Value,
    ) -> Option<Resume> {
        let refuse = |this: &mut Self, message: String, failure: Option<serde_json::Value>| {
            this.batch_result = Some(serde_json::json!({
                "nonce": nonce,
                "status": "refused",
                "applied": 0,
                "message": message,
                "failure": failure,
            }));
        };
        self.check_pending(nonce, expected_version, None)?;
        let (choice, _) = self.pending.clone()?;
        let plan: MovementPlan = match serde_json::from_value(plan.clone()) {
            Ok(plan) => plan,
            Err(error) => {
                refuse(self, format!("the plan is not read: {error}"), None);
                return None;
            }
        };
        let live = match LiveBatch::new(&plan, &self.viewer) {
            Ok(live) => live,
            Err(failure) => {
                let message = failure.message.clone();
                refuse(self, message, serde_json::to_value(&failure).ok());
                return None;
            }
        };
        match live.step(&choice) {
            LiveStep::Answer(option) => {
                let index = choice.options.iter().position(|o| o.id == option.id)?;
                self.awaiting = Some(option.id);
                self.batch = Some(BatchRun {
                    live,
                    nonce: nonce.to_owned(),
                    start: nonce.parse().unwrap_or(0),
                });
                Some(Resume::Answer(index))
            }
            LiveStep::Failed(failure) => {
                let message = failure.message.clone();
                refuse(self, message, serde_json::to_value(&failure).ok());
                None
            }
            LiveStep::Done | LiveStep::Interrupted(_) => {
                refuse(
                    self,
                    "the plan has no answer for this choice".to_owned(),
                    None,
                );
                None
            }
        }
    }

    /// What a running script or batch answers to `choice` of the person.
    pub fn scripted(&mut self, choice: &Choice) -> Scripted {
        if let Some(option) = self.script_option(choice) {
            return Scripted::Answer(option);
        }
        let Some(run) = self.batch.as_ref() else {
            return Scripted::Ask;
        };
        match run.live.step(choice) {
            LiveStep::Answer(option) => Scripted::Answer(option),
            LiveStep::Done => {
                let run = self.batch.take().expect("a batch is running");
                self.batch_result = Some(serde_json::json!({
                    "nonce": run.nonce,
                    "status": "applied",
                    "applied": run.live.applied(),
                    "planned": run.live.planned(),
                }));
                Scripted::Ask
            }
            LiveStep::Interrupted(interruption) => {
                let run = self.batch.take().expect("a batch is running");
                self.batch_result = Some(serde_json::json!({
                    "nonce": run.nonce,
                    "status": "interrupted",
                    "applied": interruption.applied_steps,
                    "planned": run.live.planned(),
                    "remaining_steps": interruption.remaining_steps,
                    "waiting": interruption.offered,
                }));
                Scripted::Ask
            }
            LiveStep::Failed(failure) => {
                let run = self.batch.take().expect("a batch is running");
                self.batch_result = Some(serde_json::json!({
                    "nonce": run.nonce,
                    "status": "refused",
                    "applied": 0,
                    "message": failure.message,
                    "failure": failure,
                }));
                Scripted::Rollback(run.start)
            }
        }
    }

    /// How the last batch ended, once.
    pub fn take_batch_result(&mut self) -> Option<serde_json::Value> {
        self.batch_result.take()
    }

    /// The save of the game as it stands.
    #[must_use]
    pub fn save_state(&self, extra: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "format": 1,
            "game_id": self.identity.game_id,
            "seed": self.identity.seed,
            "players": self.identity.players,
            "human_seat": self.identity.human_seat,
            "setup": self.identity.setup,
            "answers": self.answers,
            "redo": self.redo,
            // A save names the cards that are never offered; an own-turn card is not kept.
            "never": self.never.lock().expect("never set lock").iter()
                .filter(|(_, mode)| **mode == ReactionMode::Never)
                .map(|(card, _)| card)
                .collect::<BTreeSet<_>>(),
            "event_times": self.events.iter().map(|event| event.timestamp.clone()).collect::<Vec<_>>(),
            "event_versions": self.events.iter().map(|event| event.version).collect::<Vec<_>>(),
            "version": self.version,
            "generation": self.generation,
            "decisions": self.decision_log.len(),
            "decisions_hash": decisions_hash(&self.decision_log),
            "finished": self.finished,
            "round": self.state.round,
            "phase": self.state.phase,
            "engine": extra,
        })
    }

    /// The turn status the game shows while it waits (for a host that wants to show who plays).
    #[must_use]
    pub fn turn_status(&self) -> ti4_host::protocol::status::PublicTurnStatus {
        project_turn_status(&self.state, self.pending.as_ref().map(|(choice, _)| choice))
    }

    /// A `state_update` as of now (a host that wants a refresh).
    pub fn refreshed(&mut self) -> StateUpdateMsg {
        match self.state_update() {
            ServerMessage::StateUpdate(update) => update,
            _ => unreachable!("state_update_for builds a StateUpdate"),
        }
    }
}

const ACTION_OVER: &str = "The action is over before this.";

fn stopped(application: &ScriptRun, reason: &str) -> ScriptStopped {
    ScriptStopped {
        applied: application.next,
        remaining: application.script[application.next..].to_vec(),
        interrupted: true,
        reason: if reason.is_empty() {
            INTERRUPTED
        } else {
            reason
        }
        .to_owned(),
    }
}

/// A script that changes nothing: the game refuses it before its first answer.
fn refused(script: &[DraftStep], reason: String) -> ScriptStopped {
    ScriptStopped {
        applied: 0,
        remaining: script.to_vec(),
        interrupted: false,
        reason,
    }
}

/// A short hash of the decisions of a game, to tell on replay that the engine still decides the
/// same (FNV-1a over the canonical V1 fingerprint of each record).
#[must_use]
pub fn decisions_hash(records: &[DecisionRecord]) -> String {
    use ti4_engine::fingerprint::{CanonicalHashVersion, decision_hash};
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for record in records {
        let digest = decision_hash(CanonicalHashVersion::V1, record);
        for byte in format!("{digest:?}").bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}
