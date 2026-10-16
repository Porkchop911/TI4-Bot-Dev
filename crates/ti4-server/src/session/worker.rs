//! Session worker thread managing the single authoritative `ti4_engine::Game`.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use ti4_content::ContentStore;
use ti4_engine::choice::{
    AlwaysDecline, Choice, ChoiceOption, Decider, DecisionRecord, FirstOption, IllegalChoice,
    Scripted, SeatObservation, Table,
};
use ti4_engine::fingerprint::{CanonicalHash, CanonicalHashVersion, decision_hash};
use ti4_engine::game::Game;
use ti4_host::clock::SystemClock;
use ti4_host::journal::{self, EventDraft, Reconciled, RedoOutcome};
use ti4_host::planning::script::{OnMismatch, ScriptRuns};
use ti4_host::projection::project_turn_status;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::server::{
    EventVisibility, GameEvent, GameEventKind, GameOverMsg, PendingChoiceMsg, ServerMessage,
    TurnStatusMsg,
};
use ti4_host::protocol::status::ViewerRole;
use ti4_host::state_update::{self, StateUpdateInputs, state_update_for};
use ti4_host::view::reaction_modes::NeverOffer;
use ti4_host::view::scenario::OpeningRun;
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, ReactionMode};

use crate::session::decider::{ChoiceSubmission, RemoteHumanDecider};
use crate::session::{SeatController, SessionConfig};
use crate::storage::FileGameStore;

/// State of a currently pending decision awaiting a human answer.
#[derive(Debug, Clone)]
pub struct PendingDecision {
    pub seat: PlayerId,
    pub nonce: String,
    pub game_version: u64,
    pub choice: Choice,
    pub submission_state: PendingSubmissionState,
    /// A step can resolve a bot's earlier decision before logging this human reaction.
    pub submitted_option_id: Option<String>,
    pub reply_tx: Option<
        mpsc::Sender<
            Result<
                ti4_host::protocol::server::ActionAcceptedMsg,
                ti4_host::protocol::status::RejectionReason,
            >,
        >,
    >,
}

impl PendingDecision {
    /// The choice with its nonce while it can take an answer, as
    /// [`ti4_host::protocol::check_submission`] reads it: `None` once a submission reserved it.
    #[must_use]
    pub fn open(&self) -> Option<(&Choice, &str)> {
        (self.submission_state == PendingSubmissionState::AwaitingSubmission)
            .then_some((&self.choice, self.nonce.as_str()))
    }
}

/// Lifecycle of an engine-generated choice while the worker waits for a client submission.
///
/// A submission is reserved before it enters the worker inbox so concurrent requests cannot
/// advance the same engine choice twice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingSubmissionState {
    AwaitingSubmission,
    Reserved,
}

/// Active subscriber receiving real-time server messages.
pub struct Subscriber {
    pub viewer: ViewerRole,
    pub tx: mpsc::SyncSender<ServerMessage>,
}

/// Shared session state accessible across threads.
pub struct SessionShared {
    pub(crate) basis: super::draft_basis::DraftBasis,
    /// The scripts that are being sent to the game, by their seat ([`super::script`]). A refused
    /// entry stops a script: what it played before is in the game.
    pub(crate) scripts: ScriptRuns,
    /// A history change (undo, redo, ...) is running: a random bot that has picked its answer
    /// holds it back instead of returning it, so no decision lands while the rewind compares
    /// versions and stops the worker.
    pub history_pause: bool,
    /// A random bot seat is choosing (thinking or held back by `history_pause`). Its answer has
    /// not reached the game, so the position is still safe to rewind.
    pub bot_deciding: bool,
    /// The wait before each answer of a seat that the server plays, in milliseconds. The decider
    /// reads it for each decision, so it can change while the session runs.
    pub bot_delay_ms: u64,
    pub game_id: String,
    pub game_version: u64,
    pub latest_state: GameState,
    pub pending_decision: Option<PendingDecision>,
    /// Submitted choices awaiting the end of an engine step that opened another choice.
    pub in_flight_submissions: VecDeque<PendingDecision>,
    pub seat_inboxes: BTreeMap<PlayerId, mpsc::Sender<ChoiceSubmission>>,
    pub seat_tokens: BTreeMap<PlayerId, String>,
    pub player_ids: Vec<PlayerId>,
    pub seats: BTreeMap<PlayerId, SeatController>,
    /// The name of each player at the table ([`SessionConfig::nicknames`]).
    pub nicknames: BTreeMap<PlayerId, String>,
    pub seed: Option<u64>,
    pub subscribers: BTreeMap<u64, Subscriber>,
    pub next_subscriber_id: u64,
    pub decision_log: Vec<DecisionRecord>,
    pub finished: bool,
    pub stopped: bool,
    pub error: Option<String>,
    pub map_tiles: Vec<ti4_host::protocol::view::BoardTileView>,
    pub galaxy_layout: ti4_host::map::GalaxyLayout,
    /// The map of the game, for the facts of a message and for a draft. `None` in a session
    /// that was made without one: its messages have no `tactical` and no `exploration`.
    pub galaxy: Option<Arc<ti4_content::galaxy::Galaxy>>,
    pub event_log: Vec<GameEvent>,
    pub event_counter: u64,
    pub store: Option<Arc<FileGameStore>>,
    pub snapshot_decision_count: usize,
    pub redo_decisions: Vec<DecisionRecord>,
    /// The timeline is kept in `history.json` (undo, redo, batch commits, turn redo and
    /// recovered games): persisted atomically after each step, events published after the save.
    /// This is a storage mode for the whole life of the session, NOT "replaying" (see `replaying`).
    pub history_active: bool,
    /// The worker is still replaying recorded decisions (no live decision has been taken yet).
    /// Set at creation when there is something to replay, cleared as soon as the last recorded
    /// decision has been handed to the engine. Bluff holds and auto-resolve notes stay quiet
    /// while it is set: the players already saw those moments.
    pub replaying: bool,
    pub redo_events: Vec<GameEvent>,
    pub replay_complete: bool,
    /// How many of the game's rolls the worker has already shown (see `record_rolls`).
    pub rolls_seen: usize,
    pub history_generation: u64,
    pub batches: Vec<ti4_host::batch::BatchRecord>,
    /// Forced random positions of a timeline made by a turn redo, by decision index.
    pub rng_marks: ti4_host::rng_force::RngMarks,
    /// Decisions auto-resolved since the last state update, with the seat they belong to.
    /// Transient by design: not persisted, so a restart's replay never re-announces them.
    pub pending_auto_resolved: Vec<(PlayerId, ti4_host::protocol::server::AutoResolvedNote)>,
    /// Card names each seat asked never to be offered. The sets are shared with the seats'
    /// deciders, which read them as each question is asked.
    pub reaction_modes: BTreeMap<PlayerId, ti4_host::view::reaction_modes::NeverSet>,
    /// The decisions that their seat may take back ([`super::undo`]).
    pub(crate) undo: ti4_host::planning::undo::UndoTrack,
    /// The steps that the undo before this session took back, for the first snapshot of each
    /// connection of the seat ([`super::GameSession::undone_for`]), until the seat decides again.
    pub(crate) undone: BTreeMap<PlayerId, Vec<ti4_host::planning::steps::PlanStep>>,
    /// The length of the log that the undo left: a decision after it is new.
    pub(crate) undone_from: usize,
    /// What each seat has staged ([`super::staged`]): the book of the session's config, for the
    /// decider of a seat, which sends a ready draft of a secondary ([`super::ready_secondary`]).
    pub(crate) staged: super::staged::StagedBook,
    /// The newest nonces that each seat answered, from any connection or by the server. A second
    /// answer to one of them is told apart from a stale one ([`Self::was_answered`]).
    pub(crate) answered: BTreeMap<PlayerId, VecDeque<String>>,
    /// A copy of the game at the newest step boundary the worker passed, for batch checks (see
    /// `ti4_host::step_snapshot`). Only the worker sets it; it is dropped when the session stops
    /// or finishes, and a new session (every history change but appending) starts without one.
    pub step_snapshot: Option<Arc<ti4_host::step_snapshot::StepSnapshot>>,
    /// Set when the worker started from a copy of the game at this many decisions instead of
    /// replaying them (diagnostic; see `SessionConfig::resume`).
    pub resumed_from: Option<usize>,
    /// Declared bluff triggers, cooldown and stall budgets. Ephemeral: never persisted.
    pub bluff: crate::session::bluff::BluffBook,
    /// The map the preview needs to run a secondary on a scratch copy of the position (static for
    /// the life of a session; set by the worker once the game exists).
    pub preview_galaxy: Option<Arc<ti4_content::galaxy::Galaxy>>,
}

/// How many answered nonces are kept for each seat.
const ANSWERED_KEPT: usize = 16;

impl SessionShared {
    /// `seat` answered the choice of `nonce` (it is reserved for the answer).
    pub(crate) fn note_answer(&mut self, seat: &PlayerId, nonce: &str) {
        let answered = self.answered.entry(seat.clone()).or_default();
        if answered.len() >= ANSWERED_KEPT {
            answered.pop_front();
        }
        answered.push_back(nonce.to_owned());
    }

    /// The answer of `seat` to the choice of `nonce` did not reach the game.
    pub(crate) fn forget_answer(&mut self, seat: &PlayerId, nonce: &str) {
        if let Some(answered) = self.answered.get_mut(seat) {
            answered.retain(|kept| kept != nonce);
        }
    }

    /// `seat` answered the choice of `nonce` already.
    #[must_use]
    pub fn was_answered(&self, seat: &PlayerId, nonce: &str) -> bool {
        self.answered
            .get(seat)
            .is_some_and(|answered| answered.iter().any(|kept| kept == nonce))
    }

    /// Who sits in each seat, for a snapshot and an update: the name of the player and who
    /// plays the seat. `None` for a session with no table (it has no names).
    #[must_use]
    pub fn table_seats(&self) -> Option<ti4_host::setup::TableSeats> {
        use ti4_host::setup::{HUMAN_KIND, SeatFact, bot_kind};
        if self.nicknames.is_empty() {
            return None;
        }
        Some(
            self.nicknames
                .iter()
                .map(|(player, nickname)| {
                    let kind = match self.seats.get(player) {
                        Some(SeatController::Bot(kind)) => kind.id(),
                        Some(controller) if controller.is_in_process_bot() => {
                            bot_kind::RANDOM.to_owned()
                        }
                        _ => HUMAN_KIND.to_owned(),
                    };
                    (
                        player.clone(),
                        SeatFact {
                            nickname: nickname.clone(),
                            kind,
                        },
                    )
                })
                .collect(),
        )
    }

    /// Fail the session closed with `error`, unless it has been stopped on purpose.
    ///
    /// A stopped session's worker unwinds with errors (a human decider whose inbox was dropped
    /// answers "inbox channel closed"); those are the stop, not a failure of the game. Recording
    /// one would make the session, which readers keep seeing until its replacement is published,
    /// answer 503 "failed closed" (smoke run 1b, 2026-10-07).
    pub fn fail_unless_stopped(&mut self, error: String) {
        if !self.stopped {
            self.error = Some(error);
        }
    }

    /// Every seat's modes as plain data, for persistence and for a restarted worker.
    #[must_use]
    pub fn reaction_modes_snapshot(
        &self,
    ) -> BTreeMap<PlayerId, ti4_host::view::reaction_modes::Modes> {
        self.reaction_modes
            .iter()
            .map(|(seat, set)| (seat.clone(), set.lock().expect("never set lock").clone()))
            .filter(|(_, set)| !set.is_empty())
            .collect()
    }

    /// One seat's modes by key, as the protocol sends them.
    #[must_use]
    pub fn reaction_modes_for(&self, viewer: &ViewerRole) -> BTreeMap<String, ReactionMode> {
        let ViewerRole::Player(seat) = viewer else {
            return BTreeMap::new();
        };
        self.reaction_modes
            .get(seat)
            .map_or_else(BTreeMap::new, |set| {
                set.lock().expect("never set lock").clone()
            })
    }

    /// Record one seat's mode for one reaction (the printed name of an action card, or the key
    /// of the card of an ability), persist it, and tell that seat's clients. Idempotent: setting
    /// the mode a reaction already has changes nothing.
    ///
    /// # Errors
    /// A message for the client when the seat is unknown or the key names no reaction, or when
    /// the choice could not be saved (in which case it is not applied).
    pub fn set_reaction_mode(
        &mut self,
        seat: &PlayerId,
        card: &str,
        mode: ReactionMode,
    ) -> Result<(), String> {
        let Some(set) = self.reaction_modes.get(seat).cloned() else {
            return Err("no such seat".to_owned());
        };
        if !ti4_host::view::reaction_modes::is_reaction_key(card) {
            return Err(format!("unknown reaction '{card}'"));
        }
        let before = set.lock().expect("never set lock").clone();
        ti4_host::view::reaction_modes::set_mode(
            &mut set.lock().expect("never set lock"),
            card,
            mode,
        );
        if *set.lock().expect("never set lock") == before {
            return Ok(());
        }
        if let Some(store) = &self.store {
            let record = crate::storage::ReactionModesRecord {
                modes: self.reaction_modes_snapshot(),
                ..Default::default()
            };
            if let Err(error) = store.save_reaction_modes(&self.game_id, &record) {
                *set.lock().expect("never set lock") = before;
                return Err(format!("could not save the setting: {error}"));
            }
        }
        self.broadcast_state_update();
        Ok(())
    }

    /// Queue a note for the seat's next state update, merging a repeat of the same reason.
    pub fn note_auto_resolved(
        &mut self,
        seat: &PlayerId,
        prompt: &str,
        selected: &str,
        reason: &str,
    ) {
        self.note_passed_window(seat, prompt, selected, reason, &[]);
    }

    /// [`Self::note_auto_resolved`] for a reaction window that was declined because the seat
    /// set each of `cards` to never offer.
    pub fn note_passed_window(
        &mut self,
        seat: &PlayerId,
        prompt: &str,
        selected: &str,
        reason: &str,
        cards: &[String],
    ) {
        state_update::queue_auto_resolved(
            &mut self.pending_auto_resolved,
            self.decision_log.len(),
            self.game_version,
            seat,
            prompt,
            selected,
            reason,
            cards,
        );
    }

    fn persist_history(&self) -> Result<(), String> {
        if self.history_active
            && let Some(store) = &self.store
        {
            store
                .save_history(
                    &self.game_id,
                    &crate::storage::GameHistory {
                        decisions: self.decision_log.clone(),
                        redo: self.redo_decisions.clone(),
                        events: self.event_log.clone(),
                        redo_events: self.redo_events.clone(),
                        event_counter: self.event_counter,
                        generation: self.history_generation,
                        revision: self.game_version.saturating_add(1),
                        batches: self.batches.clone(),
                        rng_marks: self.rng_marks.clone(),
                    },
                )
                .map_err(|error| format!("failed to persist history: {error}"))?;
        }
        Ok(())
    }
    #[must_use]
    pub fn new(game_id: String, initial_state: GameState) -> Self {
        Self {
            basis: super::draft_basis::DraftBasis::default(),
            scripts: ScriptRuns::new(OnMismatch::Stop),
            game_id,
            game_version: 1,
            latest_state: initial_state,
            pending_decision: None,
            in_flight_submissions: VecDeque::new(),
            seat_inboxes: BTreeMap::new(),
            seat_tokens: BTreeMap::new(),
            player_ids: Vec::new(),
            seats: BTreeMap::new(),
            nicknames: BTreeMap::new(),
            seed: None,
            subscribers: BTreeMap::new(),
            next_subscriber_id: 0,
            decision_log: Vec::new(),
            event_log: Vec::new(),
            event_counter: 0,
            finished: false,
            stopped: false,
            error: None,
            map_tiles: Vec::new(),
            galaxy_layout: ti4_host::map::GalaxyLayout {
                version: 1,
                active_sources: Vec::new(),
                placements: Vec::new(),
                off_map_system_ids: Vec::new(),
            },
            galaxy: None,
            store: None,
            snapshot_decision_count: 0,
            history_pause: false,
            bot_deciding: false,
            bot_delay_ms: crate::session::random_bot::random_bot_delay_ms(),
            redo_decisions: Vec::new(),
            history_active: false,
            replaying: false,
            rolls_seen: 0,
            redo_events: Vec::new(),
            replay_complete: false,
            history_generation: 0,
            batches: Vec::new(),
            rng_marks: ti4_host::rng_force::RngMarks::new(),
            pending_auto_resolved: Vec::new(),
            reaction_modes: BTreeMap::new(),
            undo: ti4_host::planning::undo::UndoTrack::new(
                ti4_host::planning::undo::UndoRules::Table,
            ),
            undone: BTreeMap::new(),
            undone_from: 0,
            staged: super::staged::StagedBook::default(),
            answered: BTreeMap::new(),
            step_snapshot: None,
            resumed_from: None,
            bluff: crate::session::bluff::BluffBook::default(),
            preview_galaxy: None,
        }
    }

    /// Whether the game waits for `seat` with nothing on its way: no answer that is sent and
    /// not yet in the log, and no script. Only then may the seat take a decision back.
    fn waits_for(&self, seat: &PlayerId) -> bool {
        !self.stopped
            && !self.finished
            && self.error.is_none()
            && self.replay_complete
            && self.in_flight_submissions.is_empty()
            && !self.scripts.running(seat)
            && self.decision_log.len() == self.undo.count()
            && self.pending_decision.as_ref().is_some_and(|pending| {
                &pending.seat == seat
                    && pending.submission_state == PendingSubmissionState::AwaitingSubmission
            })
    }

    /// A seat that decided after the undo has nothing taken back to stage again.
    fn forget_undone_of_deciders(&mut self) {
        if self.undone.is_empty() {
            return;
        }
        let from = self.undone_from.min(self.decision_log.len());
        let decided: std::collections::BTreeSet<PlayerId> = self.decision_log[from..]
            .iter()
            .map(|record| record.player.clone())
            .collect();
        self.undone.retain(|seat, _| !decided.contains(seat));
    }

    fn never_of(&self, seat: &PlayerId) -> ti4_host::view::reaction_modes::Modes {
        self.reaction_modes
            .get(seat)
            .map(|set| set.lock().expect("never set lock").clone())
            .unwrap_or_default()
    }

    /// Whether `viewer` may take back its last decision now ([`super::undo`]).
    ///
    /// The game must wait for the seat. The rule would allow an undo while another seat is
    /// asked and has not answered; but that the seat is asked can tell what it holds (a window
    /// of a card), so the undo ends where the game asks another seat.
    #[must_use]
    pub fn can_undo(&self, viewer: &ViewerRole) -> bool {
        let ViewerRole::Player(seat) = viewer else {
            return false;
        };
        self.waits_for(seat) && self.undo.can_undo(seat, &self.never_of(seat))
    }

    /// What an undo of `seat` does ([`ti4_host::planning::undo::UndoTrack::plan`]), when the game is at
    /// `expected_version` and waits for the seat.
    pub(crate) fn undo_plan(
        &self,
        seat: &PlayerId,
        expected_version: u64,
    ) -> Result<
        (usize, Vec<ti4_host::planning::steps::PlanStep>),
        ti4_host::protocol::status::RejectionReason,
    > {
        use ti4_host::protocol::status::RejectionReason;
        if self.game_version != expected_version {
            return Err(RejectionReason::StaleVersion {
                expected: expected_version,
                current: self.game_version,
            });
        }
        if !self.waits_for(seat) {
            return Err(RejectionReason::NoPendingChoice);
        }
        self.undo
            .plan(seat, &self.never_of(seat))
            .map_err(|barrier| match barrier {
                Some(barrier) => RejectionReason::UndoBarrier { barrier },
                None => RejectionReason::NoPendingChoice,
            })
    }

    /// A live decision at log position `index` (0-based) was just accepted while a redo stack
    /// exists.
    ///
    /// Redo replays the recorded timeline. A decision equal to the head of the redo list (a
    /// random bot re-deciding the same position: bots are a pure function of seed, seat and
    /// index) is that recorded entry coming back, so it is consumed together with its recorded
    /// events and the rest of the redo list stays. Any other decision forks history: the redo
    /// list, its events and everything recorded past `index` are dropped.
    fn consume_or_fork_redo(&mut self, record: &DecisionRecord, index: usize) {
        if journal::consume_or_fork_redo(&mut self.redo_decisions, &mut self.redo_events, record)
            == RedoOutcome::Forked
        {
            self.batches.retain(|b| b.end_cursor <= index);
            self.rng_marks.truncate_to(index);
        }
    }

    fn publish(&mut self, message: impl Fn(&ViewerRole) -> ServerMessage) {
        self.subscribers
            .retain(|_, subscriber| subscriber.tx.try_send(message(&subscriber.viewer)).is_ok());
    }

    /// Records an authoritative event and broadcasts it to all subscribers.
    pub fn record_and_broadcast_event(
        &mut self,
        visibility: EventVisibility,
        event: GameEventKind,
        version: Option<u64>,
        decision_count: usize,
        facts: ti4_host::protocol::server::DecisionFacts,
    ) -> Result<(), String> {
        match journal::reconcile_with_redo(
            self.history_active,
            !self.redo_decisions.is_empty(),
            &self.event_log,
            &self.redo_events,
            &event,
            decision_count,
        ) {
            Reconciled::Duplicate => return Ok(()),
            Reconciled::Reattach(index) => {
                self.event_log.push(self.redo_events.remove(index));
                return Ok(());
            }
            Reconciled::Fresh => {}
        }
        self.event_counter += 1;
        let entry = journal::build_event(
            &SystemClock,
            &self.game_id,
            self.event_counter,
            &self.decision_log,
            EventDraft {
                visibility,
                event,
                version,
                decision_count,
                facts,
            },
        );
        self.push_event(entry)
    }

    /// Record and broadcast `entry`: persisted in the append-only log unless the history file
    /// carries the timeline, kept in memory, and sent to every subscriber that may see it.
    fn push_event(&mut self, entry: GameEvent) -> Result<(), String> {
        if !self.history_active
            && let Some(store) = &self.store
        {
            store
                .append_event(&self.game_id, &entry)
                .map_err(|error| format!("failed to persist event: {error}"))?;
        }

        self.event_log.push(entry.clone());

        if self.history_active {
            // Publish only after the entire authoritative timeline is atomically saved.
            return Ok(());
        }

        self.subscribers.retain(|_, subscriber| {
            entry
                .for_viewer(&subscriber.viewer)
                .is_none_or(|projected| {
                    subscriber
                        .tx
                        .try_send(ServerMessage::Event(
                            ti4_host::protocol::server::GameEventMsg {
                                protocol_version: PROTOCOL_VERSION,
                                game_id: self.game_id.clone(),
                                entry: projected,
                            },
                        ))
                        .is_ok()
                })
        });
        Ok(())
    }

    /// Publish die rolls as public events, each once: a roll whose history index already has an
    /// event (a replay after a restart, a batch that recorded its own) is skipped.
    ///
    /// Reads the engine's roll history only; nothing here can reach back into the game.
    pub fn record_rolls(
        &mut self,
        rolls: Vec<ti4_host::protocol::server::PublicRoll>,
        version: Option<u64>,
    ) -> Result<(), String> {
        for roll in rolls {
            if journal::roll_is_known(&self.event_log, &roll) {
                continue;
            }
            // The cursor the history file validates: decisions resolved so far.
            let cursor = journal::roll_cursor(&self.event_log);
            self.event_counter += 1;
            let entry = journal::build_roll_event(
                &SystemClock,
                &self.game_id,
                self.event_counter,
                &self.decision_log,
                cursor,
                self.latest_state.round,
                self.latest_state.phase,
                version,
                roll,
            );
            self.push_event(entry)?;
        }
        Ok(())
    }

    fn publish_history_events_since(&mut self, index: usize) {
        if !self.history_active {
            return;
        }
        for entry in &self.event_log[index..] {
            self.subscribers.retain(|_, subscriber| {
                entry
                    .for_viewer(&subscriber.viewer)
                    .is_none_or(|projected| {
                        subscriber
                            .tx
                            .try_send(ServerMessage::Event(
                                ti4_host::protocol::server::GameEventMsg {
                                    protocol_version: PROTOCOL_VERSION,
                                    game_id: self.game_id.clone(),
                                    entry: projected,
                                },
                            ))
                            .is_ok()
                    })
            });
        }
    }

    /// Broadcasts a newly raised decision to all subscribers.
    ///
    /// The actor receives the full `PendingChoiceMsg` with legal options.
    /// Opponents and spectators receive a redacted `TurnStatusMsg::WaitingForDecision`.
    pub fn broadcast_pending_decision(&mut self, choice: &Choice, nonce: &str) {
        let status = project_turn_status(&self.latest_state, Some(choice));
        let game_id = self.game_id.clone();
        let game_version = self.game_version;
        let state = self.latest_state.clone();
        let galaxy_layout = self.galaxy_layout.clone();

        self.publish(|viewer| {
            if viewer.is_actor(&choice.player) {
                ServerMessage::PendingChoice(PendingChoiceMsg {
                    protocol_version: PROTOCOL_VERSION,
                    game_id: game_id.clone(),
                    game_version,
                    nonce: nonce.to_owned(),
                    choice: choice.clone(),
                    state: ti4_host::projection::redacted_state(&state, viewer),
                    galaxy_layout: galaxy_layout.clone(),
                })
            } else {
                ServerMessage::TurnStatus(TurnStatusMsg {
                    protocol_version: PROTOCOL_VERSION,
                    game_id: game_id.clone(),
                    game_version,
                    status: status.clone(),
                })
            }
        });
    }

    /// Tells every subscriber who the game waits for while a seat that the server plays takes its
    /// time over `choice`: the status that the other seats get when a person is asked. A
    /// reaction window names no seat.
    pub fn broadcast_waiting_for(&mut self, choice: &Choice) {
        let message = ServerMessage::TurnStatus(TurnStatusMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: self.game_id.clone(),
            game_version: self.game_version,
            status: project_turn_status(&self.latest_state, Some(choice)),
        });
        self.publish(|_| message.clone());
    }

    /// Broadcasts a state update to all subscribers.
    pub fn broadcast_state_update(&mut self) {
        let pending = self
            .pending_decision
            .as_ref()
            .map(|p| (p.choice.clone(), p.nonce.clone()));
        let state = self.latest_state.clone();
        let auto_resolved = std::mem::take(&mut self.pending_auto_resolved);
        let modes_by_seat: BTreeMap<PlayerId, BTreeMap<String, ReactionMode>> = self
            .reaction_modes
            .keys()
            .map(|seat| {
                (
                    seat.clone(),
                    self.reaction_modes_for(&ViewerRole::Player(seat.clone())),
                )
            })
            .collect();
        // At most one seat may undo: the one that the game waits for.
        let may_undo = self
            .pending_decision
            .as_ref()
            .map(|pending| ViewerRole::Player(pending.seat.clone()))
            .filter(|viewer| self.can_undo(viewer));
        let table_seats = self.table_seats();
        let inputs = StateUpdateInputs {
            game_id: &self.game_id,
            version: self.game_version,
            state: &state,
            pending: pending
                .as_ref()
                .map(|(choice, nonce)| (choice, nonce.as_str())),
            map_tiles: &self.map_tiles,
            galaxy_layout: &self.galaxy_layout,
            galaxy: self.galaxy.as_deref(),
            watch: &self.basis.watch,
            decision_count: self.decision_log.len(),
            redo_count: self.redo_decisions.len(),
            history_generation: self.history_generation,
            decision_log: &self.decision_log,
            auto_resolved: &auto_resolved,
            reaction_modes: &modes_by_seat,
            may_undo: may_undo.as_ref(),
            seats: table_seats.as_ref(),
        };
        let path = inputs.path();
        let messages: Vec<_> = self
            .subscribers
            .iter()
            .map(|(id, subscriber)| (*id, state_update_for(&inputs, &path, &subscriber.viewer)))
            .collect();
        self.subscribers.retain(|id, subscriber| {
            messages
                .iter()
                .find(|(key, _)| key == id)
                .is_some_and(|(_, message)| subscriber.tx.try_send(message.clone()).is_ok())
        });
    }

    /// Broadcasts terminal game over to all subscribers.
    pub fn broadcast_game_over(
        &mut self,
        winner: Option<PlayerId>,
        final_scores: BTreeMap<PlayerId, u32>,
    ) {
        let msg = ServerMessage::GameOver(GameOverMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: self.game_id.clone(),
            game_version: self.game_version,
            winner,
            final_scores,
        });

        self.publish(|_| msg.clone());
    }

    /// Records and broadcasts every terminal-game consequence through one path.
    fn finish_session(&mut self, game: &Game) -> Result<(), String> {
        self.finished = true;
        let winner = ti4_engine::objectives::leader(&game.state);
        let final_scores = game
            .state
            .players
            .iter()
            .map(|player| {
                (
                    player.id.clone(),
                    player.victory_points.max(0).cast_unsigned(),
                )
            })
            .collect();
        let cursor = self.decision_log.len();
        self.record_and_broadcast_event(
            EventVisibility::Public,
            GameEventKind::GameFinished {
                winner: winner.clone(),
            },
            Some(self.game_version),
            cursor,
            ti4_host::protocol::server::DecisionFacts::default(),
        )?;
        if !self.history_active {
            self.broadcast_game_over(winner, final_scores);
        }
        Ok(())
    }

    /// Returns canonical hashes of all recorded decisions.
    #[must_use]
    pub fn decision_hashes(&self) -> Vec<CanonicalHash> {
        self.decision_log
            .iter()
            .map(|record| decision_hash(CanonicalHashVersion::V1, record))
            .collect()
    }
}

/// The bot of a seat that the replay gives each recorded decision of the seat to
/// (`ti4_host::bots::Bot::follow`), with what its decisions are drawn from.
struct Follow {
    bot: crate::session::random_bot::SharedBot,
    game_seed: u64,
    place: usize,
}

struct ReplayingDecider {
    prior_queue: Arc<Mutex<VecDeque<DecisionRecord>>>,
    inner: Box<dyn Decider>,
    follow: Option<Follow>,
    shared: Arc<Mutex<SessionShared>>,
    has_boundary_state: bool,
    /// Forced random positions for the replayed decisions (a timeline made by a turn redo).
    force: Option<ti4_host::rng_force::RngForce>,
    total: usize,
    announce_tail: bool,
}

struct CloseBasisOnExit(Arc<Mutex<SessionShared>>);

impl Drop for CloseBasisOnExit {
    fn drop(&mut self) {
        self.0.lock().expect("shared lock").basis.close();
    }
}

/// The seat of the opening of a scenario: the opening answers until it stops, then the seat is
/// asked. Its answers are decisions of the seat like any other.
struct OpeningDecider {
    inner: Box<dyn Decider>,
    run: OpeningRun,
}

impl Decider for OpeningDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        match self.run.answer(choice) {
            Some(option) => Ok(option),
            None => self.inner.choose(choice),
        }
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        match self.run.answer(choice) {
            Some(option) => Ok(option),
            None => self.inner.choose_seeing(choice, seen),
        }
    }
}

/// Capture the actual offer for decisions made by either a human or a bot. Replay
/// decisions bypass this wrapper and already have their historical events.
struct ObservedDecider {
    inner: Box<dyn Decider>,
    selected: Arc<Mutex<VecDeque<(Choice, ChoiceOption)>>>,
}

impl Decider for ObservedDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.inner.choose(choice)?;
        self.selected
            .lock()
            .expect("selected options lock")
            .push_back((choice.clone(), option.clone()));
        Ok(option)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let option = self.inner.choose_seeing(choice, seen)?;
        self.selected
            .lock()
            .expect("selected options lock")
            .push_back((choice.clone(), option.clone()));
        Ok(option)
    }
}

fn selected_for(
    selected: &mut VecDeque<(Choice, ChoiceOption)>,
    record: &DecisionRecord,
    remove: bool,
) -> Option<ChoiceOption> {
    let index = selected.iter().position(|(choice, option)| {
        choice.player == record.player
            && choice.prompt == record.prompt
            && option.id == record.chosen
    })?;
    if remove {
        selected.remove(index).map(|(_, option)| option)
    } else {
        selected.get(index).map(|(_, option)| option.clone())
    }
}

impl ReplayingDecider {
    /// [`Self::try_replay`], and the replayed answer given to the seat's bot.
    fn replay_seen(
        &self,
        choice: &Choice,
        seen: Option<&SeatObservation<'_>>,
    ) -> Option<Result<ChoiceOption, IllegalChoice>> {
        let (replayed, index) = self.try_replay(choice)?;
        if let (Ok(option), Some(follow)) = (&replayed, &self.follow) {
            follow.bot.lock().expect("bot lock").follow(
                choice,
                seen,
                option,
                follow.game_seed,
                follow.place,
                index,
            );
        }
        Some(replayed)
    }

    /// The next recorded answer, if any, and the number of its decision in the game.
    fn try_replay(&self, choice: &Choice) -> Option<(Result<ChoiceOption, IllegalChoice>, usize)> {
        let (next_prior, index) = {
            let mut lock = self.prior_queue.lock().expect("prior queue lock");
            let index = self.total.saturating_sub(lock.len());
            (lock.pop_front(), index)
        };
        if next_prior.is_some()
            && let Some(force) = &self.force
        {
            force.before_answer(index);
        }

        if self.announce_tail
            && next_prior.is_some()
            && self
                .prior_queue
                .lock()
                .expect("prior queue lock")
                .is_empty()
        {
            self.shared.lock().expect("shared lock").replaying = false;
        }
        next_prior.map(|record| {
            let replayed = if let Some(opt) = choice.options.iter().find(|o| o.id == record.chosen)
            {
                Ok(opt.clone())
            } else {
                Err(IllegalChoice::ScriptDiverged {
                    player: choice.player.clone(),
                    wanted: record.chosen,
                    offered: choice.options.iter().map(|o| o.id.clone()).collect(),
                })
            };
            (replayed, index)
        })
    }
}

impl Decider for ReplayingDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        if let Some(res) = self.replay_seen(choice, None) {
            return res;
        }
        // A single engine step can consume the last replayed decision and ask the
        // next human before returning (for example, fleet-limit enforcement).
        {
            let mut lock = self.shared.lock().expect("shared lock");
            lock.replaying = false;
            if self.has_boundary_state {
                lock.replay_complete = true;
            }
        }
        if let Some(force) = &self.force {
            force.go_live();
        }
        self.inner.choose(choice)
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if let Some(res) = self.replay_seen(choice, Some(seen)) {
            return res;
        }
        {
            let mut lock = self.shared.lock().expect("shared lock");
            lock.replaying = false;
            if self.has_boundary_state {
                lock.replay_complete = true;
            }
        }
        if let Some(force) = &self.force {
            force.go_live();
        }
        self.inner.choose_seeing(choice, seen)
    }
}

/// Spawns the dedicated session worker thread for an active game session.
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn spawn_session_worker(
    mut config: SessionConfig,
) -> (Arc<Mutex<SessionShared>>, JoinHandle<()>) {
    ti4_engine::exploration::mark_legacy_device_ids(&mut config.state, &config.prior_decisions);
    let prior_count = config.prior_decisions.len();
    let prior_records = config.prior_decisions.clone();
    let prior_queue = Arc::new(Mutex::new(VecDeque::from(config.prior_decisions.clone())));
    // A copy of this same history at an earlier step boundary replaces replaying that part. Not
    // for a table with an MLP seat: its bot is given every recorded decision of its seat
    // (`ti4_host::bots::Bot::follow`), which a copy would skip.
    let copies = config.step_snapshots && !config.seats.values().any(SeatController::is_mlp);
    let mut resume = config
        .resume
        .clone()
        .filter(|copy| copies && copy.is_prefix_of(&prior_records));
    let boundary_state = if config.replay_boundary_state.is_some() {
        config.replay_boundary_state.clone()
    } else if prior_count > 0 {
        // One private replay gives the position at the first live question. With a supplied
        // copy it replays only what the copy does not cover; without one it also copies the game
        // at a step boundary near the end of the prefix. Either way the worker forks the copy
        // instead of replaying the whole prefix a second time.
        let generation = (copies && resume.is_none()).then_some(config.history_generation);
        ti4_host::batch::replay_boundary(
            &config,
            &config.prior_decisions,
            resume.as_deref(),
            generation,
        )
        .ok()
        .map(|(state, copy)| {
            if let Some(copy) = copy.filter(|copy| copy.is_prefix_of(&prior_records)) {
                resume = Some(Arc::new(copy));
            }
            state
        })
    } else {
        None
    };
    if let Some(copy) = &resume {
        prior_queue
            .lock()
            .expect("prior queue lock")
            .drain(..copy.log_len);
    }
    let selected_options = Arc::new(Mutex::new(VecDeque::new()));
    let published_count = Arc::new(Mutex::new(prior_count));

    let mut initial_shared = SessionShared::new(config.game_id.clone(), config.state.clone());
    if let Some(state) = &boundary_state {
        initial_shared.latest_state = state.clone();
    }
    initial_shared.seat_tokens.clone_from(&config.seat_tokens);
    initial_shared.player_ids.clone_from(&config.player_ids);
    initial_shared.seats.clone_from(&config.seats);
    initial_shared.nicknames.clone_from(&config.nicknames);
    initial_shared.seed = config.seed;
    if let Some(delay) = config.random_bot_delay_ms {
        initial_shared.bot_delay_ms = delay;
    }
    initial_shared.map_tiles.clone_from(&config.map_tiles);
    initial_shared
        .galaxy_layout
        .clone_from(&config.galaxy_layout);
    initial_shared.galaxy = config.galaxy.clone().map(Arc::new);
    initial_shared.store.clone_from(&config.store);
    initial_shared.game_version = config.initial_version.max(1);
    initial_shared
        .redo_decisions
        .clone_from(&config.redo_decisions);
    initial_shared.history_active = config.history_active;
    initial_shared.replaying = !prior_queue.lock().expect("prior queue lock").is_empty();
    initial_shared.redo_events.clone_from(&config.redo_events);
    initial_shared.event_counter = config.event_counter;
    initial_shared.history_generation = config.history_generation;
    initial_shared.batches.clone_from(&config.batches);
    initial_shared.undone.clone_from(&config.undone);
    initial_shared.undone_from = config.prior_decisions.len();
    initial_shared.staged = config.staged.clone();
    initial_shared.rng_marks.clone_from(&config.rng_marks);
    let rng_force = ti4_host::rng_force::RngForce::new(&config.rng_marks);
    for seat in config.seats.keys() {
        let never = config.reaction_modes.get(seat).cloned().unwrap_or_default();
        initial_shared
            .reaction_modes
            .insert(seat.clone(), Arc::new(Mutex::new(never)));
    }

    if !config.prior_events.is_empty() {
        initial_shared.event_counter = initial_shared
            .event_counter
            .max(config.prior_events.len() as u64);
        let last_version = config
            .prior_events
            .iter()
            .filter_map(|e| e.version)
            .max()
            .unwrap_or(1);
        initial_shared.game_version = initial_shared.game_version.max(last_version);
        initial_shared.event_log.clone_from(&config.prior_events);
    }
    if !config.prior_decisions.is_empty() {
        initial_shared
            .decision_log
            .clone_from(&config.prior_decisions);
    }

    initial_shared.resumed_from = resume.as_ref().map(|copy| copy.log_len);
    let shared = Arc::new(Mutex::new(initial_shared));

    let worker_shared = shared.clone();
    // Step-boundary copies only pay off when somebody can submit a batch: a human seat.
    let wants_step_snapshots = config.step_snapshots
        && config
            .seats
            .values()
            .any(|controller| matches!(controller, SeatController::Human));
    let snapshot_generation = config.history_generation;
    // Decisions answered so far in the whole game, including the ones a fork already holds:
    // the index a random bot seeds its pick from (see `random_bot`).
    let decision_counter: crate::session::random_bot::DecisionCounter = Arc::new(
        std::sync::atomic::AtomicUsize::new(resume.as_ref().map_or(0, |copy| copy.log_len)),
    );
    let handle = thread::spawn(move || {
        let _basis_cleanup = CloseBasisOnExit(worker_shared.clone());
        let mut table = Table::new();

        // A bot seat decides by its place in the seat order of the game, not by its id.
        let order: Vec<PlayerId> = config
            .state
            .players
            .iter()
            .map(|player| player.id.clone())
            .collect();
        let place_of = |seat: &PlayerId| {
            order
                .iter()
                .position(|player| player == seat)
                .expect("a seat of the session is a seat of the game")
        };

        // Configure table deciders
        for (seat, controller) in config.seats {
            // A bot of `ti4_host::bots`, shared with the replay below.
            let bot = match &controller {
                SeatController::Bot(kind) => Some(Arc::new(Mutex::new(ti4_host::bots::Bot::new(
                    kind,
                    &config.state,
                    config.galaxy.as_ref(),
                    &seat,
                )))),
                _ => None,
            };
            let inner_human = matches!(controller, SeatController::Human);
            let never_set = worker_shared
                .lock()
                .expect("shared lock")
                .reaction_modes
                .get(&seat)
                .cloned();
            let inner: Box<dyn Decider> = match controller {
                SeatController::Human => {
                    let (inbox_tx, inbox_rx) = mpsc::channel();
                    {
                        let mut lock = worker_shared.lock().expect("shared lock");
                        lock.seat_inboxes.insert(seat.clone(), inbox_tx);
                    }
                    let decider = RemoteHumanDecider::new(
                        seat.clone(),
                        config.game_id.clone(),
                        worker_shared.clone(),
                        inbox_rx,
                    );
                    Box::new(decider)
                }
                SeatController::BotFirstOption => Box::new(FirstOption),
                SeatController::BotAlwaysDecline => Box::new(AlwaysDecline),
                SeatController::BotScripted(script) => Box::new(Scripted::new(script)),
                SeatController::Bot(_) => Box::new(
                    crate::session::random_bot::RandomBotDecider::new(
                        place_of(&seat),
                        config.seed.unwrap_or_default(),
                        decision_counter.clone(),
                        worker_shared.clone(),
                    )
                    .playing(bot.clone().expect("a bot seat has a bot")),
                ),
            };

            // A human seat's "never offer" choices are applied here, below the replay layer: a
            // declined window is an ordinary journaled decision and replay never reaches this.
            let inner: Box<dyn Decider> = match (inner_human, never_set) {
                (true, Some(never)) => {
                    let note_shared = worker_shared.clone();
                    Box::new(NeverOffer::new(inner, never).on_skip(move |choice, cards| {
                        let mut lock = note_shared.lock().expect("shared lock");
                        if lock.stopped {
                            return;
                        }
                        let names = cards.join(", ");
                        lock.note_passed_window(
                            &choice.player,
                            &format!("Reaction window ({names})"),
                            "Pass",
                            &format!("you set {names} to never offer"),
                            cards,
                        );
                    }))
                }
                _ => inner,
            };

            // The opening of a scenario answers its seat in a new game, before the seat is asked.
            let inner: Box<dyn Decider> = match &config.opening {
                Some(opening) if inner_human && prior_count == 0 && opening.seat == seat => {
                    Box::new(OpeningDecider {
                        inner,
                        run: OpeningRun::new(Some(opening.clone())),
                    })
                }
                _ => inner,
            };

            let inner: Box<dyn Decider> = Box::new(ObservedDecider {
                inner,
                selected: selected_options.clone(),
            });
            let outer: Box<dyn Decider> = if prior_count > 0 {
                Box::new(ReplayingDecider {
                    prior_queue: prior_queue.clone(),
                    inner,
                    follow: bot.map(|bot| Follow {
                        bot,
                        game_seed: config.seed.unwrap_or_default(),
                        place: place_of(&seat),
                    }),
                    shared: worker_shared.clone(),
                    has_boundary_state: boundary_state.is_some(),
                    force: rng_force.clone(),
                    total: prior_count,
                    announce_tail: config.announce_tail,
                })
            } else {
                inner
            };
            // Outside the replay: the undo of a seat is told every answer, a replayed one too.
            table.seat(
                seat,
                Box::new(super::undo::Watched {
                    inner: Box::new(crate::session::random_bot::CountingDecider {
                        inner: outer,
                        counter: decision_counter.clone(),
                    }),
                    shared: worker_shared.clone(),
                    human: inner_human,
                }),
            );
        }

        // Nested combat windows can ask another human before `game.step()` returns. Publish
        // the decisions already settled by the table at that boundary, with the actual board
        // position seen by the next offer, rather than waiting for the entire step to unwind.
        let offer_shared = worker_shared.clone();
        let offer_selected = selected_options.clone();
        let offer_published = published_count.clone();
        let note_shared = worker_shared.clone();
        let note_prior = prior_queue.clone();
        table.on_auto_resolved(move |note| {
            // While earlier decisions are being replayed after a restart the player has already
            // been told; stay quiet until the replay catches up.
            if !note_prior.lock().expect("prior queue lock").is_empty() {
                return;
            }
            let mut lock = note_shared.lock().expect("shared lock");
            if lock.stopped || lock.replaying {
                return;
            }
            lock.note_auto_resolved(&note.player, &note.prompt, &note.label, &note.reason);
        });
        // A reaction window of a kind this seat declared opened without asking it anything: the
        // live game waits a moment (see `session::bluff`). Never during a replay.
        let hold_shared = worker_shared.clone();
        let hold_prior = prior_queue.clone();
        table.on_window_skipped(Box::new(
            move |seat, event_type, relation, state, publish| {
                let replaying = !hold_prior.lock().expect("prior queue lock").is_empty();
                crate::session::bluff_hold::maybe_hold(
                    &hold_shared,
                    replaying,
                    seat,
                    event_type,
                    relation,
                    state,
                    publish,
                );
            },
        ));
        table.on_observed_offer(move |records, state| {
            let mut lock = offer_shared.lock().expect("shared lock");
            lock.undo.observe(records.len(), state);
            let mut published = offer_published.lock().expect("published count lock");
            if lock.stopped || records.len() <= *published {
                return;
            }
            let start = *published;
            let event_start = lock.event_log.len();
            lock.latest_state = state.clone();
            let mut replies = Vec::new();
            for (index, record) in records[start..].iter().enumerate() {
                let selected = selected_for(
                    &mut offer_selected.lock().expect("selected options lock"),
                    record,
                    false,
                );
                // Another card may be played *inside* this card's timing window. Its outer
                // choice has settled, but the announcement has not happened yet. Leave that
                // choice for the next boundary instead of publishing an unverified play.
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
                let selected = selected_for(
                    &mut offer_selected.lock().expect("selected options lock"),
                    record,
                    true,
                );
                lock.decision_log = records[..start + index + 1].to_vec();
                let version = lock.game_version;
                let facts = ti4_host::protocol::server::verified_event_facts(
                    record,
                    selected.as_ref(),
                    state.active_system.as_ref().map(|id| id.as_str()),
                    state,
                );
                if let Some(store) = &lock.store
                    && !lock.history_active
                {
                    if let Err(error) = store.append_decision(&lock.game_id, record) {
                        lock.error = Some(format!("failed to persist accepted decision: {error}"));
                        return;
                    }
                }
                if let Err(error) = lock.record_and_broadcast_event(
                    EventVisibility::Public,
                    GameEventKind::DecisionResolved,
                    Some(version),
                    start + index + 1,
                    facts,
                ) {
                    lock.error = Some(error);
                    return;
                }
                *published += 1;
                lock.consume_or_fork_redo(record, start + index);
                let in_flight = lock.in_flight_submissions.front().is_some_and(|pending| {
                    pending.seat == record.player
                        && pending.submitted_option_id.as_deref() == Some(&record.chosen)
                });
                let current = lock.pending_decision.as_ref().is_some_and(|pending| {
                    pending.seat == record.player
                        && pending.submitted_option_id.as_deref() == Some(&record.chosen)
                });
                let pending = if in_flight {
                    lock.in_flight_submissions.pop_front()
                } else if current {
                    lock.pending_decision.take()
                } else {
                    None
                };
                if let Some(mut pending) = pending
                    && let Some(reply) = pending.reply_tx.take()
                {
                    replies.push((
                        reply,
                        ti4_host::protocol::server::ActionAcceptedMsg {
                            protocol_version: PROTOCOL_VERSION,
                            game_id: lock.game_id.clone(),
                            game_version: version,
                            option_id: pending.submitted_option_id.expect("reserved option"),
                        },
                    ));
                }
            }
            if let Err(error) = lock.persist_history() {
                lock.error = Some(error);
                return;
            }
            lock.publish_history_events_since(event_start);
            for (reply, accepted) in replies {
                let _ = reply.send(Ok(accepted));
            }
        });

        let mut game = if let Some(copy) = &resume {
            // Positioned where the copy was taken, with the log recorded so far; the rest of
            // the prior decisions are replayed through the seats' deciders below.
            let mut game = copy.fork(&prior_records);
            game.install_table(table);
            game
        } else {
            let mut game = Game::with_table(config.state, ContentStore::embedded(), table);
            if let Some(galaxy) = config.galaxy {
                game = game.with_galaxy(galaxy);
            }
            game
        };
        if let Some(force) = &rng_force {
            force.attach(&mut game);
        }
        // The undo of a seat reads what the engine latches: a die, a shuffle, a hidden card.
        game.bind_observation(
            worker_shared
                .lock()
                .expect("shared lock")
                .undo
                .observation(),
        );
        // The watch of the strategic action follows the card from the step that resolves its
        // primary. It holds no session lock: the step holds none while its primary is asked.
        {
            let mut lock = worker_shared.lock().expect("shared lock");
            lock.basis.watch.bind(&mut game);
            lock.basis.completed_step(&game);
        }
        // The secondary preview (`session::preview`) runs on a scratch copy of the position and
        // needs the map; it never touches this game.
        worker_shared.lock().expect("shared lock").preview_galaxy =
            game.galaxy().cloned().map(Arc::new);

        if prior_count == 0 && config.prior_events.is_empty() {
            // Emit initial game initialization event
            let mut lock = worker_shared.lock().expect("shared lock");
            let round = game.state.round;
            let speaker = &game.state.speaker;
            let version = lock.game_version;
            if let Err(error) = lock.record_and_broadcast_event(
                EventVisibility::Public,
                GameEventKind::GameInitialized {
                    round,
                    phase: game.state.phase,
                    speaker: speaker.clone(),
                },
                Some(version),
                0,
                ti4_host::protocol::server::DecisionFacts::default(),
            ) {
                lock.error = Some(error);
                return;
            }
        } else if prior_count > 0 {
            // Replay prior decisions to reach current state
            while game.table.log.records.len() < prior_count {
                let result = game.step();
                if let Some(err) = result.error {
                    let mut lock = worker_shared.lock().expect("shared lock");
                    lock.fail_unless_stopped(format!("recovery replay failed: {err}"));
                    return;
                }
                if result.finished && game.table.log.records.len() < prior_count {
                    let mut lock = worker_shared.lock().expect("shared lock");
                    lock.fail_unless_stopped(
                        "recovery replay ended before all decisions".to_owned(),
                    );
                    return;
                }
                let mut lock = worker_shared.lock().expect("shared lock");
                lock.undo.observe(game.table.log.records.len(), &game.state);
                lock.basis.completed_step(&game);
            }
            if game.table.log.records.get(..prior_count) != Some(prior_records.as_slice()) {
                worker_shared.lock().expect("shared lock").error =
                    Some("recovery replay records diverged".to_owned());
                return;
            }
        }

        let mut prev_round = game.state.round;
        let mut prev_phase = game.state.phase;
        // A nested engine step can offer (and accept) live decisions before the
        // replay step returns. They still need to be published and persisted.
        let mut prev_decision_count = prior_count;
        // How much of the game's roll history is already public: whatever a replay or a resumed
        // copy rolled before this point happened before the players' current view of the game.
        let mut rolls_seen = game.rolls().len();

        'worker: loop {
            // Check stop signal
            {
                let lock = worker_shared.lock().expect("shared lock");
                if lock.stopped {
                    break;
                }
            }

            // Sync state to shared cache
            let snapshot_wanted = {
                let mut lock = worker_shared.lock().expect("shared lock");
                lock.latest_state = game.state.clone();
                lock.decision_log.clone_from(&game.table.log.records);
                lock.replay_complete = true;
                // One copy at a time. A boundary that recorded nothing since the last copy
                // adds nothing: a fork of the older copy reaches the same game by stepping.
                wants_step_snapshots
                    && !game.state.finished
                    && lock
                        .step_snapshot
                        .as_ref()
                        .is_none_or(|copy| copy.log_len < game.table.log.records.len())
            };

            // Between steps (the top of the loop, before `game.step()`) the game can be copied:
            // the control flow of a step lives on this thread's stack and cannot. A batch
            // check forks the copy instead of replaying the whole game.
            if snapshot_wanted
                && let Some(copy) =
                    ti4_host::step_snapshot::StepSnapshot::capture(&mut game, snapshot_generation)
            {
                let mut lock = worker_shared.lock().expect("shared lock");
                if !lock.stopped {
                    lock.step_snapshot = Some(Arc::new(copy));
                }
            }

            // Check if finished
            if game.state.finished {
                let mut lock = worker_shared.lock().expect("shared lock");
                lock.step_snapshot = None;
                if lock
                    .event_log
                    .last()
                    .is_some_and(|event| matches!(event.event, GameEventKind::GameFinished { .. }))
                {
                    lock.finished = true;
                    break;
                }
                if let Err(error) = lock.finish_session(&game) {
                    lock.error = Some(error);
                    break;
                }
                break;
            }

            // Step the engine (will block if human decision is required)
            let events_before = game.events.len();
            let result = game.step();

            // Record outcome
            {
                let mut lock = worker_shared.lock().expect("shared lock");
                prev_decision_count =
                    prev_decision_count.max(*published_count.lock().expect("published count lock"));
                // A rewind stops this worker at its last published decision. A step
                // interrupted while waiting for a human may still let bots act before
                // returning; those results belong to the discarded timeline.
                if lock.stopped {
                    break 'worker;
                }
                lock.undo.observe(game.table.log.records.len(), &game.state);
                let mut accepted_replies = Vec::new();
                let event_start = lock.event_log.len();
                lock.latest_state = game.state.clone();
                lock.decision_log.clone_from(&game.table.log.records);
                lock.forget_undone_of_deciders();

                if let Some(err) = result.error {
                    lock.error = Some(err.to_string());
                    let mut replies: Vec<_> = lock
                        .in_flight_submissions
                        .drain(..)
                        .filter_map(|mut pending| pending.reply_tx.take())
                        .collect();
                    replies.extend(
                        lock.pending_decision
                            .as_mut()
                            .and_then(|pending| pending.reply_tx.take()),
                    );
                    for reply_tx in replies {
                        let _ = reply_tx.send(Err(
                            ti4_host::protocol::status::RejectionReason::ValidationFailed {
                                message: "engine transition failed".to_owned(),
                            },
                        ));
                    }
                    break 'worker;
                }

                // Any newly resolved decisions:
                if game.table.log.records.len() > prev_decision_count {
                    for (offset, record) in game.table.log.records[prev_decision_count..]
                        .iter()
                        .enumerate()
                    {
                        if !lock.history_active
                            && let Some(store) = &lock.store
                            && let Err(err) = store.append_decision(&lock.game_id, record)
                        {
                            lock.error =
                                Some(format!("failed to persist accepted decision: {err}"));
                            if let Some(reply_tx) = lock
                                .pending_decision
                                .as_mut()
                                .and_then(|pending| pending.reply_tx.take())
                            {
                                let _ = reply_tx.send(Err(
                                    ti4_host::protocol::status::RejectionReason::ValidationFailed {
                                        message: "decision could not be persisted".to_owned(),
                                    },
                                ));
                            }
                            break 'worker;
                        }
                        let version = lock.game_version;
                        let selected = selected_for(
                            &mut selected_options.lock().expect("selected options lock"),
                            record,
                            true,
                        );
                        let offered = selected.as_ref();
                        let facts = ti4_host::protocol::server::verified_event_facts(
                            record,
                            offered,
                            lock.latest_state
                                .active_system
                                .as_ref()
                                .map(|id| id.as_str()),
                            &lock.latest_state,
                        );
                        let event_error = lock
                            .record_and_broadcast_event(
                                EventVisibility::Public,
                                GameEventKind::DecisionResolved,
                                Some(version),
                                prev_decision_count + offset + 1,
                                facts,
                            )
                            .err();
                        if let Some(error) = event_error {
                            lock.error = Some(error);
                            if let Some(reply_tx) = lock
                                .pending_decision
                                .as_mut()
                                .and_then(|pending| pending.reply_tx.take())
                            {
                                let _ = reply_tx.send(Err(
                                    ti4_host::protocol::status::RejectionReason::ValidationFailed {
                                        message: "event could not be persisted".to_owned(),
                                    },
                                ));
                            }
                            break 'worker;
                        }
                        lock.consume_or_fork_redo(record, prev_decision_count + offset);
                        let in_flight = lock.in_flight_submissions.front().is_some_and(|pending| {
                            pending.seat == record.player
                                && pending.submitted_option_id.as_deref() == Some(&record.chosen)
                        });
                        let current = lock.pending_decision.as_ref().is_some_and(|pending| {
                            pending.seat == record.player
                                && pending.submitted_option_id.as_deref() == Some(&record.chosen)
                        });
                        let pending = if in_flight {
                            lock.in_flight_submissions.pop_front()
                        } else if current {
                            lock.pending_decision.take()
                        } else {
                            None
                        };
                        if let Some(pending) = pending
                            && let Some(reply_tx) = pending.reply_tx
                        {
                            accepted_replies.push((
                                reply_tx,
                                ti4_host::protocol::server::ActionAcceptedMsg {
                                    protocol_version: PROTOCOL_VERSION,
                                    game_id: lock.game_id.clone(),
                                    game_version: version,
                                    option_id: pending
                                        .submitted_option_id
                                        .expect("reserved option"),
                                },
                            ));
                        }
                    }
                    prev_decision_count = game.table.log.records.len();
                    *published_count.lock().expect("published count lock") = prev_decision_count;
                }
                // Dice rolled by this step are public. Read from the engine's history after the
                // fact; a replay of recorded decisions (restart, rewind) rolled them before.
                let rolled = game.rolls().len();
                rolls_seen = rolls_seen.min(rolled);
                lock.rolls_seen = rolls_seen;
                if rolled > rolls_seen {
                    let replaying = !prior_queue.lock().expect("prior queue lock").is_empty();
                    if !replaying {
                        let active = game.state.active_system.as_ref().map(|id| id.as_str());
                        let fresh: Vec<_> = game.rolls()[rolls_seen..]
                            .iter()
                            .enumerate()
                            .filter_map(|(offset, roll)| {
                                ti4_host::protocol::server::public_roll(
                                    rolls_seen + offset,
                                    roll,
                                    active,
                                )
                            })
                            .collect();
                        let version = lock.game_version;
                        if let Err(error) = lock.record_rolls(fresh, Some(version)) {
                            lock.error = Some(error);
                            break 'worker;
                        }
                    }
                    rolls_seen = rolled;
                    lock.rolls_seen = rolled;
                }
                if !lock.history_active
                    && game.table.log.records.len() - lock.snapshot_decision_count >= 32
                {
                    if let Some(store) = &lock.store
                        && let Err(error) = store.save_snapshot(
                            &lock.game_id,
                            game.table.log.records.len(),
                            &game.state,
                        )
                    {
                        lock.error = Some(format!("failed to persist snapshot: {error}"));
                        for (reply_tx, _) in accepted_replies {
                            let _ = reply_tx.send(Err(
                                ti4_host::protocol::status::RejectionReason::ValidationFailed {
                                    message: "snapshot could not be persisted".to_owned(),
                                },
                            ));
                        }
                        break 'worker;
                    }
                    lock.snapshot_decision_count = game.table.log.records.len();
                }

                // Phase transition:
                if game.state.phase != prev_phase || game.state.round != prev_round {
                    let round = game.state.round;
                    let version = lock.game_version;
                    let cursor = lock.decision_log.len();
                    if let Err(error) = lock.record_and_broadcast_event(
                        EventVisibility::Public,
                        GameEventKind::PhaseTransition {
                            phase: game.state.phase,
                            round,
                        },
                        Some(version),
                        cursor,
                        ti4_host::protocol::server::DecisionFacts::default(),
                    ) {
                        lock.error = Some(error);
                        break 'worker;
                    }
                    prev_phase = game.state.phase;
                    prev_round = game.state.round;
                }

                if result.finished {
                    if let Err(error) = lock.finish_session(&game) {
                        lock.error = Some(error);
                        break;
                    }
                    if let Err(error) = lock.persist_history() {
                        lock.error = Some(error);
                        for (reply_tx, _) in accepted_replies {
                            let _ = reply_tx.send(Err(
                                ti4_host::protocol::status::RejectionReason::ValidationFailed {
                                    message: "history could not be persisted".to_owned(),
                                },
                            ));
                        }
                        break;
                    }
                    lock.publish_history_events_since(event_start);
                    let winner = ti4_engine::objectives::leader(&game.state);
                    let scores = game
                        .state
                        .players
                        .iter()
                        .map(|p| (p.id.clone(), p.victory_points.max(0).cast_unsigned()))
                        .collect();
                    if lock.history_active {
                        lock.broadcast_game_over(winner, scores);
                    }
                    for (reply_tx, accepted) in accepted_replies {
                        let _ = reply_tx.send(Ok(accepted));
                    }
                    break;
                }

                if let Err(error) = lock.persist_history() {
                    lock.error = Some(error);
                    for (reply_tx, _) in accepted_replies {
                        let _ = reply_tx.send(Err(
                            ti4_host::protocol::status::RejectionReason::ValidationFailed {
                                message: "history could not be persisted".to_owned(),
                            },
                        ));
                    }
                    break 'worker;
                }
                lock.publish_history_events_since(event_start);

                lock.basis.completed_step(&game);
                let action_over = ti4_host::planning::script::tactical_complete(
                    game.events.get(events_before..).unwrap_or_default(),
                );
                lock.scripts_after_step(action_over);
                for (reply_tx, accepted) in accepted_replies {
                    let _ = reply_tx.send(Ok(accepted));
                }

                lock.game_version += 1;
                lock.broadcast_state_update();
            }
        }
    });

    (shared, handle)
}

#[cfg(test)]
mod fail_unless_stopped_tests {
    use super::*;

    fn shared() -> SessionShared {
        let ids = [PlayerId::new("a"), PlayerId::new("b")];
        SessionShared::new(
            "g".to_owned(),
            GameState::new(&ids, &[], BTreeMap::new(), None, 1),
        )
    }

    #[test]
    fn a_live_session_records_its_failure() {
        let mut live = shared();
        live.fail_unless_stopped("recovery replay failed: boom".to_owned());
        assert_eq!(live.error.as_deref(), Some("recovery replay failed: boom"));
    }

    #[test]
    fn a_stopped_session_does_not_fail_closed_on_its_unwinding() {
        let mut stopped = shared();
        stopped.stopped = true;
        stopped.fail_unless_stopped(
            "recovery replay failed: decider failed while answering \"assign a hit\": inbox channel closed"
                .to_owned(),
        );
        assert!(stopped.error.is_none());
    }
}

#[cfg(test)]
mod auto_resolved_tests {
    use super::*;

    fn shared() -> SessionShared {
        let ids = [PlayerId::new("a"), PlayerId::new("b")];
        SessionShared::new(
            "g".to_owned(),
            GameState::new(&ids, &[], BTreeMap::new(), None, 1),
        )
    }

    fn subscribe(
        shared: &mut SessionShared,
        id: u64,
        viewer: ViewerRole,
    ) -> mpsc::Receiver<ServerMessage> {
        let (tx, rx) = mpsc::sync_channel(8);
        shared.subscribers.insert(id, Subscriber { viewer, tx });
        rx
    }

    fn notes(
        rx: &mpsc::Receiver<ServerMessage>,
    ) -> Vec<ti4_host::protocol::server::AutoResolvedNote> {
        match rx.try_recv().expect("a state update") {
            ServerMessage::StateUpdate(update) => update.auto_resolved,
            other => panic!("expected a state update, got {other:?}"),
        }
    }

    #[test]
    fn an_auto_resolved_note_reaches_only_its_seat_once_and_leaves_the_journal_alone() {
        let mut shared = shared();
        let a = subscribe(&mut shared, 1, ViewerRole::Player(PlayerId::new("a")));
        let b = subscribe(&mut shared, 2, ViewerRole::Player(PlayerId::new("b")));
        let spectator = subscribe(&mut shared, 3, ViewerRole::Spectator);
        let before = shared.decision_log.len();

        shared.note_auto_resolved(
            &PlayerId::new("a"),
            "pay 1 more resources",
            "trade goods",
            "only way",
        );
        shared.broadcast_state_update();

        let mine = notes(&a);
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].selected, "trade goods");
        assert_eq!(mine[0].reason, "only way");
        assert!(notes(&b).is_empty());
        assert!(notes(&spectator).is_empty());
        assert_eq!(shared.decision_log.len(), before, "journal untouched");

        shared.broadcast_state_update();
        assert!(notes(&a).is_empty(), "a note is delivered once");
    }

    #[test]
    fn repeats_of_one_reason_collapse_into_a_count() {
        let mut shared = shared();
        let seat = PlayerId::new("a");
        shared.note_auto_resolved(&seat, "pay 2 more resources", "trade goods", "only way");
        shared.note_auto_resolved(&seat, "pay 1 more resources", "trade goods", "only way");
        assert_eq!(shared.pending_auto_resolved.len(), 1);
        assert_eq!(shared.pending_auto_resolved[0].1.count, 2);
    }
}

/// The worker's two game-end paths name the tie-break winner, not the first seat with most VP.
///
/// `finish_session` records `GameFinished`; with history active the step loop broadcasts
/// `GameOver` itself. Both call `objectives::leader` (fac47ea), and neither had a test.
#[cfg(test)]
mod tie_break_tests {
    use super::*;
    use crate::session::{GameSession, SessionConfig};
    use std::time::Duration;
    use ti4_model::id::StrategyCardId;
    use ti4_model::state::Phase;

    /// Three seats tied on VP in the last action turn of a round whose status phase cannot
    /// reveal an objective. Seating order is p1, p2, p3; initiative order is p2, p3, p1. A tie
    /// broken by seating names p1, one broken by the last maximum (`max_by_key`) names p3, and the
    /// rules name p2.
    fn tied_last_turn(game_id: &str, history_active: bool) -> SessionConfig {
        let players: Vec<PlayerId> = ["p1", "p2", "p3"].into_iter().map(PlayerId::new).collect();
        let (mut state, galaxy) =
            ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
                .unwrap();
        let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
        let mut by_initiative: Vec<(StrategyCardId, i32)> = state
            .card_initiative
            .iter()
            .map(|(card, initiative)| (card.clone(), *initiative))
            .collect();
        by_initiative.sort_by_key(|(_, initiative)| *initiative);
        let cards = [
            by_initiative.last().unwrap().0.clone(), // p1: the highest number
            by_initiative[0].0.clone(),              // p2: the lowest number
            by_initiative[4].0.clone(),              // p3
        ];
        state.phase = Phase::Action;
        state.round = 4;
        state.objective_deck.clear();
        state.seating_order.clone_from(&players);
        state
            .unclaimed_strategy_cards
            .retain(|card| !cards.contains(card));
        for (seat, card) in players.iter().zip(cards.iter()) {
            let player = state.player_mut(seat).unwrap();
            player.victory_points = 6;
            player.strategy_cards = vec![card.clone()];
            player.exhausted_strategy_cards = std::iter::once(card.clone()).collect();
            player.passed = seat != &players[0];
        }
        state.active = Some(players[0].clone());
        assert_eq!(
            state.initiative_order(),
            vec![players[1].clone(), players[2].clone(), players[0].clone()]
        );
        let mut config = SessionConfig::new(game_id, state)
            .with_seed(42)
            .with_player_ids(players.clone())
            .with_galaxy(galaxy, tiles);
        for seat in &players {
            config = config.with_seat(seat.clone(), SeatController::Human);
        }
        config.history_active = history_active;
        config
    }

    /// Pass every remaining turn until the game ends; returns the `GameOver` winners seen.
    fn play_out(session: &GameSession) -> Vec<Option<PlayerId>> {
        let subscription = session.subscribe(ViewerRole::Spectator);
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !session.is_finished() {
            assert!(
                std::time::Instant::now() < deadline,
                "{:?}",
                session.error()
            );
            assert!(session.error().is_none(), "{:?}", session.error());
            let Some((seat, nonce, version)) = session.current_pending_decision() else {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            };
            let choice = session
                .get_snapshot(&ViewerRole::Player(seat.clone()))
                .pending_choice
                .unwrap()
                .choice;
            let option = choice
                .options
                .iter()
                .find(|o| o.id.contains("pass") || o.id == "decline" || o.id == "end_turn")
                .unwrap_or(&choice.options[0]);
            let _ = session.submit_choice(&seat, &nonce, version, &option.id);
        }
        let mut winners = Vec::new();
        while let Ok(message) = subscription.try_recv() {
            if let ServerMessage::GameOver(over) = message {
                winners.push(over.winner);
            }
        }
        winners
    }

    fn finished_winner(session: &GameSession) -> Option<PlayerId> {
        session
            .event_log()
            .iter()
            .find_map(|event| match &event.event {
                GameEventKind::GameFinished { winner } => Some(winner.clone()),
                _ => None,
            })
            .expect("a GameFinished event")
    }

    fn assert_still_tied(session: &GameSession) {
        let state = session.current_state();
        assert!(state.finished);
        assert!(
            state.players.iter().all(|p| p.victory_points == 6),
            "the tie must survive to the end for this test to mean anything: {:?}",
            state
                .players
                .iter()
                .map(|p| (&p.id, p.victory_points))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn a_three_way_tie_finishing_in_the_worker_goes_to_the_first_in_initiative() {
        let session = GameSession::start(tied_last_turn("tie_live", false));
        let over = play_out(&session);
        assert_still_tied(&session);
        assert_eq!(finished_winner(&session), Some(PlayerId::new("p2")));
        assert_eq!(over, vec![Some(PlayerId::new("p2"))]);
        session.stop();
    }

    #[test]
    fn a_three_way_tie_finishing_with_history_active_broadcasts_the_initiative_winner() {
        let session = GameSession::start(tied_last_turn("tie_history", true));
        let over = play_out(&session);
        assert_still_tied(&session);
        assert_eq!(finished_winner(&session), Some(PlayerId::new("p2")));
        assert_eq!(over, vec![Some(PlayerId::new("p2"))]);
        session.stop();
    }
}
