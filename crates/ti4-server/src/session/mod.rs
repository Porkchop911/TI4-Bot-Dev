pub mod bluff;
pub mod bluff_hold;
pub mod decider;
mod planning;
pub mod preview;
pub mod random_bot;
pub mod reach;
pub mod reaction_modes;
pub(crate) mod ready_secondary;
pub mod registry;
pub(crate) mod script;
pub mod splice;
pub mod staged;
pub mod table;
pub mod transport;
pub mod turn_redo;
pub(crate) mod undo;
pub mod worker;

/// The rule bot, under the paths it had before it moved to its own crate (`ti4-rule-bot`).
pub use ti4_rule_bot as smart_bot;
pub use ti4_rule_bot::faction_plans as smart_bot_faction_plans;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, Weak, mpsc};
use std::thread::JoinHandle;

use ti4_engine::choice::DecisionRecord;
use ti4_engine::fingerprint::CanonicalHash;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::protocol::server::{ActionAcceptedMsg, InitialSnapshotMsg, ServerMessage};
use crate::protocol::status::{RejectionReason, ViewerRole};
use crate::session::decider::ChoiceSubmission;
use crate::session::worker::{
    PendingSubmissionState, SessionShared, Subscriber, spawn_session_worker,
};

/// Player, controller, credential and seed details for a running lobby.
pub type LobbyDetails = (
    Vec<PlayerId>,
    BTreeMap<PlayerId, SeatController>,
    BTreeMap<PlayerId, String>,
    Option<u64>,
);

/// Transport-neutral bounded subscription that unregisters itself when dropped.
pub struct SessionSubscription {
    id: u64,
    receiver: mpsc::Receiver<ServerMessage>,
    shared: Weak<Mutex<SessionShared>>,
}

impl SessionSubscription {
    /// Receives the next update for this subscription.
    pub fn recv(&self) -> Result<ServerMessage, mpsc::RecvError> {
        self.receiver.recv()
    }

    /// Attempts to receive an update without blocking.
    pub fn try_recv(&self) -> Result<ServerMessage, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for SessionSubscription {
    fn drop(&mut self) {
        if let Some(shared) = self.shared.upgrade() {
            shared
                .lock()
                .expect("shared lock")
                .subscribers
                .remove(&self.id);
        }
    }
}

use serde::{Deserialize, Serialize};

pub use decider::RemoteHumanDecider;
pub use planning::PlanningError;
pub use random_bot::{RANDOM_BOT_DELAY_ENV, random_bot_delay_ms};
pub use registry::{BotServiceConfig, GameRegistry};
pub use ti4_host::replay::{ReplayError, ReplayReport, replay_session, replay_session_forced};
pub use ti4_host::rng_force::{RngForce, RngMarks};
pub use ti4_host::{batch, replay, rng_force, step_snapshot};
pub use transport::MockClient;

/// Configuration for the controller occupying a table seat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeatController {
    /// Remote human seat routed through channels to network/mock clients.
    Human,
    /// Automated bot always taking the first offered option.
    BotFirstOption,
    /// Automated bot declining when possible, otherwise taking first option.
    BotAlwaysDecline,
    /// Automated bot following a scripted sequence of option IDs.
    BotScripted(Vec<String>),
    /// In-process bot choosing uniformly among the legal options, reproducibly from
    /// (game seed, seat, decision index) with a think delay between decisions. See
    /// [`random_bot`].
    BotRandom,
    /// In-process bot like [`SeatController::BotRandom`] that answers the board questions it has
    /// an opinion on with simple heuristics (claim nearby planets, fight battles it expects to
    /// win, build when short of units, take favourable trades) and the rest at random. See
    /// [`smart_bot`].
    BotSmart,
}

impl SeatController {
    /// Whether the seat is answered inside the game worker (a random or rule bot): no
    /// credential, no connection, never taken over.
    #[must_use]
    pub const fn is_in_process_bot(&self) -> bool {
        matches!(self, Self::BotRandom | Self::BotSmart)
    }
}

/// Configuration used to spawn an authoritative game session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    /// In-memory drafts carried across session replacement (undo/redo).
    pub plans: BTreeMap<PlayerId, crate::planning::runner::PlayerPlan>,
    pub game_id: String,
    pub state: GameState,
    pub seats: BTreeMap<PlayerId, SeatController>,
    /// The name of each player at the table. With it every snapshot and update says who sits in
    /// each seat ([`ti4_host::setup::SeatFact`]); a session with no table has none.
    pub nicknames: BTreeMap<PlayerId, String>,
    pub seat_tokens: BTreeMap<PlayerId, String>,
    pub galaxy: Option<ti4_content::galaxy::Galaxy>,
    pub map_tiles: Vec<crate::protocol::view::BoardTileView>,
    pub galaxy_layout: crate::map::GalaxyLayout,
    pub seed: Option<u64>,
    pub player_ids: Vec<PlayerId>,
    pub store: Option<Arc<crate::storage::FileGameStore>>,
    pub prior_decisions: Vec<DecisionRecord>,
    pub prior_events: Vec<crate::protocol::server::GameEvent>,
    pub initial_version: u64,
    pub redo_decisions: Vec<DecisionRecord>,
    pub history_active: bool,
    pub redo_events: Vec<crate::protocol::server::GameEvent>,
    pub event_counter: u64,
    pub history_generation: u64,
    pub batches: Vec<crate::storage::BatchRecord>,
    /// Stream positions forced while replaying `prior_decisions` and `redo_decisions`
    /// (set only on a timeline made by a turn redo).
    pub rng_marks: RngMarks,
    /// State at the first unplanned choice, computed by private replay for a committed batch.
    pub replay_boundary_state: Option<GameState>,
    /// The worker stops being `replaying` when the last recorded decision is handed to the engine
    /// (a batch commit: whatever the engine settles after the batch is new to the players).
    /// Otherwise it stops at the first live question: the stretch between the last recorded
    /// decision and that question was already shown when the position was first live (a
    /// recovery, an undo, a redo), so its auto-resolve notes and bluff holds are not repeated.
    pub announce_tail: bool,
    /// The mode of each reaction that a seat set (see `reaction_modes`).
    pub reaction_modes: BTreeMap<PlayerId, reaction_modes::Modes>,
    /// What each seat has staged (see `staged`). Shared, not copied, by the sessions that
    /// replace each other.
    pub staged: staged::StagedBook,
    /// The steps that the undo of a seat took back, when this session is what that undo left.
    /// Every first snapshot of a connection of the seat has them, until the seat decides again.
    pub undone: BTreeMap<PlayerId, Vec<ti4_view::planning::steps::PlanStep>>,
    /// Whether the worker keeps a step-boundary copy of the game for batch checks (see
    /// `step_snapshot`). Off only in tests that compare against the full-replay path.
    pub step_snapshots: bool,
    /// Think delay of the `BotRandom` seats in milliseconds; `None` reads
    /// `TI4_RANDOM_BOT_DELAY_MS` (default 600). Tests and soaks set `Some(0)`.
    pub random_bot_delay_ms: Option<u64>,
    /// A copy of the game at an earlier step boundary of this same history: the worker forks it
    /// and replays only the decisions after it, instead of replaying the whole game. Used when a
    /// committed batch replaces a session. Checked against `prior_decisions` (prefix property)
    /// and ignored when it does not fit.
    pub resume: Option<Arc<step_snapshot::StepSnapshot>>,
}

impl batch::ReplayBasis for SessionConfig {
    fn state(&self) -> &GameState {
        &self.state
    }

    fn galaxy(&self) -> Option<&ti4_content::galaxy::Galaxy> {
        self.galaxy.as_ref()
    }

    fn rng_marks(&self) -> &RngMarks {
        &self.rng_marks
    }

    fn history_generation(&self) -> u64 {
        self.history_generation
    }

    fn clock(&self) -> &dyn ti4_host::clock::Clock {
        &ti4_host::clock::SystemClock
    }
}

impl SessionConfig {
    #[must_use]
    pub fn new(game_id: impl Into<String>, state: GameState) -> Self {
        Self {
            plans: BTreeMap::new(),
            game_id: game_id.into(),
            state,
            seats: BTreeMap::new(),
            nicknames: BTreeMap::new(),
            seat_tokens: BTreeMap::new(),
            galaxy: None,
            map_tiles: Vec::new(),
            galaxy_layout: crate::map::GalaxyLayout {
                version: 1,
                active_sources: Vec::new(),
                placements: Vec::new(),
                off_map_system_ids: Vec::new(),
            },
            seed: None,
            player_ids: Vec::new(),
            store: None,
            prior_decisions: Vec::new(),
            prior_events: Vec::new(),
            initial_version: 1,
            redo_decisions: Vec::new(),
            history_active: false,
            redo_events: Vec::new(),
            event_counter: 0,
            history_generation: 0,
            batches: Vec::new(),
            rng_marks: RngMarks::new(),
            replay_boundary_state: None,
            announce_tail: false,
            reaction_modes: BTreeMap::new(),
            staged: staged::StagedBook::default(),
            undone: BTreeMap::new(),
            step_snapshots: true,
            random_bot_delay_ms: None,
            resume: None,
        }
    }

    #[must_use]
    pub fn with_reaction_modes(mut self, modes: BTreeMap<PlayerId, reaction_modes::Modes>) -> Self {
        self.reaction_modes = modes;
        self
    }

    #[must_use]
    pub fn with_seat(mut self, seat: PlayerId, controller: SeatController) -> Self {
        self.seat_tokens
            .entry(seat.clone())
            .or_insert_with(|| format!("{:032x}", rand::random::<u128>()));
        self.seats.insert(seat, controller);
        self
    }

    #[must_use]
    pub fn with_galaxy(
        mut self,
        galaxy: ti4_content::galaxy::Galaxy,
        map_tiles: Vec<crate::protocol::view::BoardTileView>,
    ) -> Self {
        self.galaxy = Some(galaxy);
        self.map_tiles = map_tiles;
        self.galaxy_layout = crate::map::GalaxyLayout::from_galaxy(
            self.galaxy.as_ref().expect("galaxy just assigned"),
            ti4_model::content_types::POK,
        );
        self
    }

    #[must_use]
    pub fn with_random_bot_delay_ms(mut self, delay_ms: u64) -> Self {
        self.random_bot_delay_ms = Some(delay_ms);
        self
    }

    #[must_use]
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    #[must_use]
    pub fn with_player_ids(mut self, player_ids: Vec<PlayerId>) -> Self {
        self.player_ids = player_ids;
        self
    }

    #[must_use]
    pub fn with_store(mut self, store: Arc<crate::storage::FileGameStore>) -> Self {
        self.store = Some(store);
        self
    }

    #[must_use]
    pub fn with_prior_history(
        mut self,
        decisions: Vec<DecisionRecord>,
        events: Vec<crate::protocol::server::GameEvent>,
    ) -> Self {
        self.replay_boundary_state = None;
        self.prior_decisions = decisions;
        self.prior_events = events;
        self
    }
}

/// Handle to an active authoritative game session.
pub struct GameSession {
    game_id: String,
    shared: Arc<Mutex<SessionShared>>,
    worker_handle: Mutex<Option<JoinHandle<()>>>,
    initial_config: SessionConfig,
}

impl GameSession {
    /// Starts a new session worker thread with the given configuration.
    #[must_use]
    pub fn start(config: SessionConfig) -> Self {
        let game_id = config.game_id.clone();
        let initial_config = config.clone();
        let (shared, handle) = spawn_session_worker(config);

        Self {
            game_id,
            shared,
            worker_handle: Mutex::new(Some(handle)),
            initial_config,
        }
    }

    /// Starts a recovered game session with prior decision and event histories.
    #[must_use]
    pub fn start_recovered(
        mut config: SessionConfig,
        prior_decisions: Vec<DecisionRecord>,
        prior_events: Vec<crate::protocol::server::GameEvent>,
    ) -> Self {
        let game_id = config.game_id.clone();
        if config.prior_decisions != prior_decisions {
            config.replay_boundary_state = None;
        }
        config.prior_decisions = prior_decisions;
        config.prior_events = prior_events;
        let initial_config = config.clone();
        let (shared, handle) = spawn_session_worker(config);

        Self {
            game_id,
            shared,
            worker_handle: Mutex::new(Some(handle)),
            initial_config,
        }
    }

    /// Returns the session's unique game ID.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.game_id
    }

    /// Submits a choice for an active pending decision.
    ///
    /// Validates seat authentication, routes to that seat's inbox, and awaits validation.
    ///
    /// # Errors
    ///
    /// Returns [`RejectionReason`] if:
    /// - No choice is pending (`NoPendingChoice`).
    /// - The submitting seat does not match the active actor (`UnauthorizedSeat`).
    /// - The expected game version does not match (`StaleVersion`).
    /// - The nonce does not match (`StaleNonce`).
    /// - The option ID is not in the offered set (`UnknownOption`).
    pub fn submit_choice(
        &self,
        seat: &PlayerId,
        nonce: &str,
        expected_version: u64,
        option_id: &str,
    ) -> Result<ActionAcceptedMsg, RejectionReason> {
        let rx = self.reserve_choice(seat, nonce, expected_version, option_id)?;
        rx.recv().unwrap_or(Err(RejectionReason::NoPendingChoice))
    }

    pub(crate) fn reserve_choice(
        &self,
        seat: &PlayerId,
        nonce: &str,
        expected_version: u64,
        option_id: &str,
    ) -> Result<mpsc::Receiver<Result<ActionAcceptedMsg, RejectionReason>>, RejectionReason> {
        let (tx, rx) = mpsc::channel();
        let submission = ChoiceSubmission {
            seat: seat.clone(),
            nonce: nonce.to_owned(),
            expected_version,
            option_id: option_id.to_owned(),
            reply_tx: tx,
        };

        let target_inbox = {
            let mut lock = self.shared.lock().expect("shared lock");
            let Some(pending) = &mut lock.pending_decision else {
                return Err(RejectionReason::NoPendingChoice);
            };

            if &pending.seat != seat {
                return Err(RejectionReason::UnauthorizedSeat {
                    seat: Some(seat.clone()),
                });
            }

            if pending.submission_state == PendingSubmissionState::Reserved {
                return Err(RejectionReason::NoPendingChoice);
            }
            if pending.nonce != nonce {
                return Err(RejectionReason::StaleNonce);
            }
            if pending.game_version != expected_version {
                return Err(RejectionReason::StaleVersion {
                    expected: expected_version,
                    current: pending.game_version,
                });
            }
            if pending.choice.answer_for(option_id).is_none() {
                return Err(RejectionReason::UnknownOption {
                    option_id: option_id.to_owned(),
                });
            }
            pending.submission_state = PendingSubmissionState::Reserved;
            lock.note_answer(seat, nonce);

            lock.seat_inboxes.get(seat).cloned()
        };

        let Some(inbox) = target_inbox else {
            return Err(RejectionReason::UnauthorizedSeat {
                seat: Some(seat.clone()),
            });
        };

        if inbox.send(submission).is_err() {
            let mut lock = self.shared.lock().expect("shared lock");
            lock.forget_answer(seat, nonce);
            if lock
                .pending_decision
                .as_ref()
                .is_some_and(|pending| pending.seat == *seat && pending.nonce == nonce)
            {
                lock.pending_decision
                    .as_mut()
                    .expect("pending decision")
                    .submission_state = PendingSubmissionState::AwaitingSubmission;
            }
            return Err(RejectionReason::NoPendingChoice);
        }

        Ok(rx)
    }

    /// Subscribes a viewer role to receive live server messages.
    ///
    /// Slow subscribers are disconnected when their bounded queue fills.
    #[must_use]
    pub fn subscribe(&self, viewer: ViewerRole) -> SessionSubscription {
        const SUBSCRIBER_QUEUE_CAPACITY: usize = 128;
        let (tx, receiver) = mpsc::sync_channel(SUBSCRIBER_QUEUE_CAPACITY);
        let mut lock = self.shared.lock().expect("shared lock");
        lock.next_subscriber_id += 1;
        let id = lock.next_subscriber_id;
        lock.subscribers.insert(id, Subscriber { viewer, tx });
        SessionSubscription {
            id,
            receiver,
            shared: Arc::downgrade(&self.shared),
        }
    }

    /// Fetches an initial snapshot for the given viewer role.
    #[must_use]
    pub fn get_snapshot(&self, viewer: &ViewerRole) -> InitialSnapshotMsg {
        let lock = self.shared.lock().expect("shared lock");
        let pending = lock
            .pending_decision
            .as_ref()
            .map(|p| (&p.choice, p.nonce.as_str()));

        let mut snapshot = crate::projection::project_initial_snapshot_with_map(
            &self.game_id,
            lock.game_version,
            &lock.latest_state,
            viewer,
            pending,
            &lock.map_tiles,
            &lock.galaxy_layout,
            lock.galaxy.as_deref(),
            Some(&lock.planning.watch),
            &lock.event_log,
        )
        .with_history(
            lock.decision_log.len(),
            lock.redo_decisions.len(),
            lock.history_generation,
        );
        snapshot.history.can_undo = lock.can_undo(viewer);
        snapshot.seats = lock.table_seats();
        snapshot.reaction_modes = lock.reaction_modes_for(viewer);
        snapshot.current_path = crate::protocol::server::current_log_path(
            &lock.latest_state,
            pending.map(|(choice, _)| choice),
            &lock.decision_log,
        );
        snapshot
    }

    /// Resolves an unguessable seat capability to its authorized viewer role.
    #[must_use]
    pub fn viewer_for_seat_token(&self, token: &str) -> Option<ViewerRole> {
        let lock = self.shared.lock().expect("shared lock");
        lock.seat_tokens.iter().find_map(|(seat, candidate)| {
            // A bot seat has no usable credential, whatever its record holds.
            (candidate == token
                && !lock
                    .seats
                    .get(seat)
                    .is_some_and(SeatController::is_in_process_bot))
            .then(|| ViewerRole::Player(seat.clone()))
        })
    }

    /// Returns the seat capabilities created for this session.
    #[must_use]
    pub fn seat_tokens(&self) -> BTreeMap<PlayerId, String> {
        self.shared.lock().expect("shared lock").seat_tokens.clone()
    }

    /// Synchronize the running session's internal credential view after durable rotation.
    pub(crate) fn replace_player_session(&self, player: &PlayerId, credential: &str) {
        self.shared
            .lock()
            .expect("shared lock")
            .seat_tokens
            .insert(player.clone(), credential.to_owned());
    }

    /// Returns immutable lifecycle metadata needed to represent a running session as a lobby.
    #[must_use]
    pub fn lobby_details(&self) -> LobbyDetails {
        let lock = self.shared.lock().expect("shared lock");
        (
            lock.player_ids.clone(),
            lock.seats.clone(),
            lock.seat_tokens.clone(),
            lock.seed,
        )
    }

    /// Returns static board tiles for the game session.
    #[must_use]
    pub fn map_tiles(&self) -> Vec<crate::protocol::view::BoardTileView> {
        let lock = self.shared.lock().expect("shared lock");
        lock.map_tiles.clone()
    }

    /// Returns the currently pending decision details: `(seat, nonce, version)`.
    #[must_use]
    pub fn current_pending_decision(&self) -> Option<(PlayerId, String, u64)> {
        let lock = self.shared.lock().expect("shared lock");
        lock.pending_decision
            .as_ref()
            .map(|p| (p.seat.clone(), p.nonce.clone(), p.game_version))
    }

    /// Returns a clone of the authoritative latest `GameState`.
    #[must_use]
    pub fn current_state(&self) -> GameState {
        self.shared
            .lock()
            .expect("shared lock")
            .latest_state
            .clone()
    }

    /// Returns whether the game has finished.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.shared.lock().expect("shared lock").finished
    }

    /// Returns the terminal engine or persistence error, if the session failed closed.
    #[must_use]
    pub fn error(&self) -> Option<String> {
        self.shared.lock().expect("shared lock").error.clone()
    }

    /// Returns a copy of the accumulated decision log records.
    #[must_use]
    pub fn decision_log(&self) -> Vec<DecisionRecord> {
        self.shared
            .lock()
            .expect("shared lock")
            .decision_log
            .clone()
    }

    /// The decision log together with the worker's newest step-boundary copy of the game, read
    /// under one lock so the two belong to the same moment. The copy is only a hint: a batch
    /// check uses it when `StepSnapshot::is_valid_for` accepts it and replays in full otherwise.
    #[must_use]
    pub fn batch_basis(
        &self,
    ) -> (
        Vec<DecisionRecord>,
        Option<Arc<step_snapshot::StepSnapshot>>,
    ) {
        let lock = self.shared.lock().expect("shared lock");
        (lock.decision_log.clone(), lock.step_snapshot.clone())
    }

    /// How many of the game's dice the worker has published (the next one's history index).
    #[must_use]
    pub fn rolls_seen(&self) -> usize {
        self.shared.lock().expect("shared lock").rolls_seen
    }

    /// Diagnostic: the number of decisions this session did not have to replay because it
    /// started from a copy of the game (`None`: it replayed from the opening state).
    #[must_use]
    pub fn resumed_from(&self) -> Option<usize> {
        self.shared.lock().expect("shared lock").resumed_from
    }

    /// Diagnostic: the decision count of the worker's newest step-boundary copy of the game.
    #[must_use]
    pub fn step_snapshot_len(&self) -> Option<usize> {
        self.shared
            .lock()
            .expect("shared lock")
            .step_snapshot
            .as_ref()
            .map(|copy| copy.log_len)
    }

    /// Returns the canonical hashes of all recorded decisions.
    #[must_use]
    pub fn decision_hashes(&self) -> Vec<CanonicalHash> {
        self.shared.lock().expect("shared lock").decision_hashes()
    }

    /// Returns the accumulated authoritative game event log.
    #[must_use]
    pub fn event_log(&self) -> Vec<crate::protocol::server::GameEvent> {
        self.shared.lock().expect("shared lock").event_log.clone()
    }

    /// Public decision cursor and the number of decisions available for redo.
    pub fn history_status(&self) -> crate::protocol::server::HistoryStatus {
        let lock = self.shared.lock().expect("shared lock");
        crate::protocol::server::HistoryStatus {
            cursor: lock.decision_log.len(),
            redo_count: lock.redo_decisions.len(),
            generation: lock.history_generation,
            can_undo: false,
        }
    }

    /// What an undo of `seat` does now: how many decisions the game keeps, and the steps of a
    /// plan that are taken back ([`undo`]). The game is not changed.
    ///
    /// # Errors
    ///
    /// [`RejectionReason::UndoBarrier`] when the last decision lies behind a barrier for the
    /// seat; as [`Self::submit_choice`] when the game is not at `expected_version` or does not
    /// wait for the seat.
    pub fn undo_plan(
        &self,
        seat: &PlayerId,
        expected_version: u64,
    ) -> Result<(usize, Vec<ti4_view::planning::steps::PlanStep>), RejectionReason> {
        self.shared
            .lock()
            .expect("shared lock")
            .undo_plan(seat, expected_version)
    }

    /// The steps that the undo before this session took back for `seat`. Every connection of
    /// the seat is told them with its first snapshot of this session, as a local game tells them
    /// with one update; they are forgotten when the seat decides again.
    #[must_use]
    pub fn undone_for(&self, seat: &PlayerId) -> Vec<ti4_view::planning::steps::PlanStep> {
        self.shared
            .lock()
            .expect("shared lock")
            .undone
            .get(seat)
            .cloned()
            .unwrap_or_default()
    }

    /// What `seat` has staged (see `staged`).
    #[must_use]
    pub fn seat_staged(&self, seat: &PlayerId) -> staged::SeatStaged {
        self.initial_config
            .staged
            .lock()
            .get(seat)
            .cloned()
            .unwrap_or_default()
    }

    /// Write one slot of what `seat` has staged, by compare-and-set on its revision
    /// ([`staged::SeatStaged::put`]), save it, and send it to every connection of the seat with
    /// `writer` as the writer. A stale or unchanged put writes and sends nothing.
    ///
    /// # Errors
    /// A message for the client when the write could not be saved (it is then not applied).
    pub fn put_staged(
        &self,
        seat: &PlayerId,
        slot: staged::StagedSlot,
        base_revision: u64,
        value: Option<serde_json::Value>,
        writer: u64,
    ) -> Result<staged::Put, String> {
        staged::put_and_tell(
            &self.shared,
            &self.initial_config.staged,
            seat,
            slot,
            base_revision,
            value,
            Some(writer),
        )
    }

    /// `seat` answered the choice of `nonce` already: from a connection, or the server for it
    /// (a ready draft of a secondary). Only the newest answers of a seat are kept.
    #[must_use]
    pub fn was_answered(&self, seat: &PlayerId, nonce: &str) -> bool {
        self.shared
            .lock()
            .expect("shared lock")
            .was_answered(seat, nonce)
    }

    /// Sends the ready draft of the secondary of `seat` when the game asks the seat that
    /// secondary now ([`ready_secondary`]): after the seat marked it ready. A draft that the
    /// game no longer plays whole is made not ready.
    pub fn send_ready_secondary(&self, seat: &PlayerId) {
        let asked = {
            let lock = self.shared.lock().expect("shared lock");
            lock.pending_decision
                .as_ref()
                .filter(|pending| {
                    pending.seat == *seat
                        && pending.submission_state == PendingSubmissionState::AwaitingSubmission
                        && ready_secondary::may_send(&lock, seat, &pending.choice)
                })
                .map(|pending| {
                    (
                        pending.nonce.clone(),
                        pending.game_version,
                        pending.choice.clone(),
                        lock.latest_state.round,
                    )
                })
        };
        let Some((nonce, version, choice, round)) = asked else {
            return;
        };
        let book = &self.initial_config.staged;
        let Some(ready) = ready_secondary::ready_draft(book, seat, &choice, round) else {
            return;
        };
        match ready_secondary::check(&self.shared, seat, &ready.draft) {
            ready_secondary::Check::Fits(script) => {
                match self.apply_draft(seat, &nonce, version, &script) {
                    Ok(None) => ready_secondary::spent(&self.shared, book, seat, ready.revision),
                    Ok(Some(_)) => ready_secondary::unready(&self.shared, book, seat, ready),
                    // The seat answered meanwhile, or the game went on.
                    Err(_) => {}
                }
            }
            ready_secondary::Check::Unfit => {
                ready_secondary::unready(&self.shared, book, seat, ready);
            }
            ready_secondary::Check::Unknown => {}
        }
    }

    /// Set one seat's handling of one action card, by printed name.
    ///
    /// # Errors
    /// A client-facing message when the seat or card is unknown or the setting cannot be saved.
    pub fn set_reaction_mode(
        &self,
        seat: &PlayerId,
        card: &str,
        mode: ti4_model::state::ReactionMode,
    ) -> Result<(), String> {
        self.shared
            .lock()
            .expect("shared lock")
            .set_reaction_mode(seat, card, mode)
    }

    /// Declare which reaction windows this seat bluffs about (ephemeral; see `session::bluff`).
    ///
    /// # Errors
    /// A client-facing message when the declaration is refused.
    pub fn set_reaction_intent(&self, seat: &PlayerId, triggers: &[String]) -> Result<(), String> {
        self.shared
            .lock()
            .expect("shared lock")
            .set_reaction_intent(seat, triggers)
    }

    /// What `seat` would be asked for the strategy-card secondary in progress if its window
    /// opened now (see `session::preview`). Read only: copies the newest published position
    /// under the lock and computes on the copy.
    #[must_use]
    pub fn preview_secondary(
        &self,
        seat: &PlayerId,
        request: &preview::PreviewRequest,
    ) -> crate::protocol::server::SecondaryPreviewMsg {
        preview::answer(&self.shared, &self.game_id, seat, request)
    }

    /// Which ships could move into `system_id` if it were activated now (see `session::reach`).
    /// Read only: copies the newest published position under the lock and asks the engine.
    #[must_use]
    pub fn reach_into(
        &self,
        request_id: u64,
        system_id: &str,
    ) -> crate::protocol::reach::ReachIntoMsg {
        reach::answer(&self.shared, &self.game_id, request_id, system_id)
    }

    /// Let the seat end its running bluff hold early; the seat's current bluff state is
    /// returned so a refused request can still re-sync the client.
    pub fn pass_reaction_hold(&self, seat: &PlayerId) {
        self.shared
            .lock()
            .expect("shared lock")
            .pass_reaction_hold(seat);
    }

    /// The seat's own bluff state, as the protocol sends it to that seat only.
    #[must_use]
    pub fn reaction_intent_state(
        &self,
        seat: &PlayerId,
    ) -> crate::protocol::server::ReactionIntentStateMsg {
        self.shared
            .lock()
            .expect("shared lock")
            .reaction_intent_state(seat)
    }

    /// Replace the bluff limits (tests use short durations; a running hold keeps its own).
    pub fn set_bluff_policy(&self, policy: bluff::BluffPolicy) {
        self.shared.lock().expect("shared lock").bluff.policy = policy;
    }

    /// The seat whose bluff hold is running right now, if any.
    #[must_use]
    pub fn bluff_holding(&self) -> Option<PlayerId> {
        self.shared
            .lock()
            .expect("shared lock")
            .bluff
            .holding()
            .cloned()
    }

    pub fn game_version(&self) -> u64 {
        self.shared.lock().expect("shared lock").game_version
    }

    /// Starts an inactive human seat's draft from the last completed live step.
    /// This transport-neutral API takes a seat already authorized by its caller,
    /// just like submit_choice. It does not alter the live pending decision.
    pub fn start_planning(&self, player: &PlayerId) -> Result<(), PlanningError> {
        let mut lock = self.shared.lock().expect("shared lock");
        if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
            return Err(PlanningError::Unavailable);
        }
        if lock.seats.get(player) != Some(&SeatController::Human) {
            return Err(PlanningError::UnknownSeat);
        }
        if lock.latest_state.active.as_ref() == Some(player) {
            return Err(PlanningError::ActivePlayer);
        }
        lock.planning.start(player)
    }

    pub fn submit_planning_choice(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
        option_id: &str,
    ) -> Result<(), PlanningError> {
        self.submit_planning_choice_with_request_id(player, identity, option_id, None)
    }

    pub fn submit_planning_choice_with_request_id(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
        option_id: &str,
        request_id: Option<&str>,
    ) -> Result<(), PlanningError> {
        let lock = self.shared.lock().expect("shared lock");
        if lock.stopped || lock.finished || lock.error.is_some() {
            return Err(PlanningError::Unavailable);
        }
        if lock.planning.has_application(player) {
            return Err(PlanningError::Submission(
                crate::planning::runner::SubmissionError::NotWaiting,
            ));
        }
        lock.planning
            .runners
            .get(player)
            .ok_or(PlanningError::NotStarted)?
            .submit_with_request_id(identity, option_id, request_id)
            .map_err(PlanningError::Submission)
    }

    pub fn reset_planning(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
    ) -> Result<(), PlanningError> {
        let mut lock = self.shared.lock().expect("shared lock");
        if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
            return Err(PlanningError::Unavailable);
        }
        lock.planning.reset(player, identity)
    }

    pub fn edit_planning_movement(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
    ) -> Result<(), PlanningError> {
        let mut lock = self.shared.lock().expect("shared lock");
        if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
            return Err(PlanningError::Unavailable);
        }
        lock.planning.edit_movement(player, identity)
    }

    /// Confirm the current validated draft at a real tactical action opportunity.
    /// Execution then stays with the live decider, independent of the connection.
    pub fn apply_planning(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
        nonce: &str,
        expected_version: u64,
    ) -> Result<(), PlanningError> {
        let mut lock = self.shared.lock().expect("shared lock");
        if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
            return Err(PlanningError::Unavailable);
        }
        let pending = lock
            .pending_decision
            .as_ref()
            .ok_or(PlanningError::NoActionOpportunity)?;
        if pending.submission_state != PendingSubmissionState::AwaitingSubmission
            || pending.nonce != nonce
            || pending.game_version != expected_version
        {
            return Err(PlanningError::Submission(
                crate::planning::runner::SubmissionError::Retired,
            ));
        }
        let plan = lock
            .planning
            .executable_plan(player, identity, &pending.choice)?;
        let inbox = lock
            .seat_inboxes
            .get(player)
            .ok_or(PlanningError::UnknownSeat)?
            .clone();
        let (reply_tx, _reply_rx) = mpsc::channel();
        lock.pending_decision
            .as_mut()
            .expect("pending decision")
            .submission_state = PendingSubmissionState::Reserved;
        // Queue the first answer under the same reservation lock used by live input.
        // No browser-supplied choices or hypothetical state enter the live game.
        if inbox
            .send(ChoiceSubmission {
                seat: player.clone(),
                nonce: nonce.to_owned(),
                expected_version,
                option_id: "tactical".into(),
                reply_tx,
            })
            .is_err()
        {
            lock.pending_decision
                .as_mut()
                .expect("pending decision")
                .submission_state = PendingSubmissionState::AwaitingSubmission;
            return Err(PlanningError::Unavailable);
        }
        let activation_seq = lock.latest_state.activation_seq;
        lock.planning
            .begin_application(player, plan, activation_seq);
        Ok(())
    }

    /// Plays `script` as the next turn of `player`, or as its secondary (`scope`), on a copy of
    /// the game at its last completed step, as a local game does (`ti4_view::planning::draft::run_draft`). The game is not
    /// changed and nothing of the script is kept. Returns the version of the game that the
    /// script was played on.
    ///
    /// A session that cannot be copied (stopped, finished, failed, or still in its replay)
    /// answers with a failed run on the position as the seat sees it.
    pub fn run_draft(
        &self,
        player: &PlayerId,
        scope: &ti4_view::planning::audit::PlanScope,
        script: &[ti4_view::planning::draft::DraftStep],
    ) -> (u64, ti4_view::planning::draft::DraftOutcome) {
        use ti4_view::planning::draft::{DraftOutcome, DraftStop};
        let (version, outcome) = run_draft_on(&self.shared, player, scope, script);
        if let Some(outcome) = outcome {
            return (version, outcome);
        }
        let (tiles, galaxy, state) = {
            let lock = self.shared.lock().expect("shared lock");
            (
                lock.map_tiles.clone(),
                lock.galaxy.clone(),
                lock.latest_state.clone(),
            )
        };
        let update = crate::projection::project_session_update(
            &state,
            &ViewerRole::Player(player.clone()),
            None,
            &tiles,
            &crate::projection::facts_context(galaxy.as_deref()),
        );
        let outcome = DraftOutcome {
            update,
            movement: None,
            invasion: None,
            production: None,
            consumed: 0,
            stop: DraftStop::Failed {
                reason: "The game cannot be drafted on now.".to_owned(),
            },
        };
        (version, outcome)
    }

    /// Sends `script` to the live game: the pending choice of `player` and the choices after
    /// it are answered from it, as a local game answers them (`script_answer` of `ti4-view`).
    ///
    /// The script is first played on a copy of the game. When the game would refuse an entry,
    /// nothing is sent and the refusal is returned: the game is as before. Otherwise the first
    /// answer is given here and `Ok(None)` is returned; where the script stops, or that it was
    /// played whole, is told to the seat later (`ServerMessage::DraftApplied`). The copy cannot
    /// tell what another seat decides, so a script can still stop after such a decision, with
    /// its earlier entries in the game.
    ///
    /// # Errors
    ///
    /// Returns [`RejectionReason`] as [`Self::submit_choice`] does when the choice of the
    /// request is not the one that the game waits in.
    pub fn apply_draft(
        &self,
        player: &PlayerId,
        nonce: &str,
        expected_version: u64,
        script: &[ti4_view::planning::draft::DraftStep],
    ) -> Result<Option<crate::protocol::server::ScriptStopped>, RejectionReason> {
        use ti4_view::planning::draft::{ScriptAnswer, script_answer};
        // The choice is reserved while the copy plays, so no other answer can take it.
        let (choice, copy, tail, never) = {
            let mut lock = self.shared.lock().expect("shared lock");
            if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
                return Err(RejectionReason::NoPendingChoice);
            }
            let Some(pending) = &lock.pending_decision else {
                return Err(RejectionReason::NoPendingChoice);
            };
            if &pending.seat != player {
                return Err(RejectionReason::UnauthorizedSeat {
                    seat: Some(player.clone()),
                });
            }
            if pending.submission_state == PendingSubmissionState::Reserved
                || lock.planning.has_application(player)
            {
                return Err(RejectionReason::NoPendingChoice);
            }
            if pending.nonce != nonce {
                return Err(RejectionReason::StaleNonce);
            }
            if pending.game_version != expected_version {
                return Err(RejectionReason::StaleVersion {
                    expected: expected_version,
                    current: pending.game_version,
                });
            }
            let choice = pending.choice.clone();
            let copy = lock.planning.fork();
            let tail = copy
                .as_ref()
                .and_then(|(_, decisions)| lock.decision_log.get(*decisions..))
                .map(<[DecisionRecord]>::to_vec);
            lock.pending_decision
                .as_mut()
                .expect("pending decision")
                .submission_state = PendingSubmissionState::Reserved;
            lock.note_answer(player, nonce);
            let never = lock
                .reaction_modes
                .get(player)
                .map(|set| set.lock().expect("never set lock").clone())
                .unwrap_or_default();
            (choice, copy.map(|(game, _)| game), tail, never)
        };
        let release = |lock: &mut SessionShared| {
            lock.forget_answer(player, nonce);
            if let Some(pending) = lock
                .pending_decision
                .as_mut()
                .filter(|pending| pending.seat == *player && pending.nonce == nonce)
            {
                pending.submission_state = PendingSubmissionState::AwaitingSubmission;
            }
        };
        let first = script_answer(script, 0, &choice, player);
        let refusal = match &first {
            ScriptAnswer::Answer { .. } => match copy.zip(tail) {
                Some((game, tail)) => {
                    match script::check_script(game, tail, &choice, script, player, never) {
                        script::Check::Refused { reason, .. } => Some(reason),
                        script::Check::Plays | script::Check::Unknown => None,
                    }
                }
                None => None,
            },
            ScriptAnswer::Done => Some("The script has no answer for this choice.".to_owned()),
            ScriptAnswer::Interrupted { reason } | ScriptAnswer::Mismatch { reason, .. } => {
                Some(reason.clone())
            }
        };
        let mut lock = self.shared.lock().expect("shared lock");
        if let Some(reason) = refusal {
            release(&mut lock);
            return Ok(Some(script::refused(script, reason)));
        }
        let ScriptAnswer::Answer {
            option,
            next,
            script: reordered,
        } = first
        else {
            unreachable!("every other answer is a refusal");
        };
        let Some(inbox) = lock.seat_inboxes.get(player).cloned() else {
            release(&mut lock);
            return Err(RejectionReason::NoPendingChoice);
        };
        let (reply_tx, _reply_rx) = mpsc::channel();
        if inbox
            .send(ChoiceSubmission {
                seat: player.clone(),
                nonce: nonce.to_owned(),
                expected_version,
                option_id: option.id,
                reply_tx,
            })
            .is_err()
        {
            release(&mut lock);
            return Err(RejectionReason::NoPendingChoice);
        }
        lock.begin_script(
            player,
            nonce,
            reordered.unwrap_or_else(|| script.to_vec()),
            next,
        );
        Ok(None)
    }

    /// One operation on a follower's secondary draft. Unlike a tactical draft it
    /// is open to every follower at once, whoever the active player is, and
    /// never touches the live pending decision.
    pub fn secondary_planning(
        &self,
        player: &PlayerId,
        request: &crate::protocol::client::SecondaryPlanningRequest,
    ) -> Result<(), PlanningError> {
        use crate::protocol::client::SecondaryPlanningRequest as Request;
        let mut lock = self.shared.lock().expect("shared lock");
        if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
            return Err(PlanningError::Unavailable);
        }
        if lock.seats.get(player) != Some(&SeatController::Human) {
            return Err(PlanningError::UnknownSeat);
        }
        match request {
            Request::Start {} => lock.planning.start_secondary(player),
            Request::Reset { identity } => lock.planning.reset_secondary(player, *identity),
            Request::Answer {
                identity,
                option_id,
            } => lock.planning.submit_secondary(player, *identity, option_id),
            Request::SetReady { identity, ready } => {
                lock.planning.set_secondary_ready(player, *identity, *ready)
            }
        }
    }

    pub fn planning_status(&self, player: &PlayerId) -> crate::protocol::server::PlanningStatusMsg {
        let lock = self.shared.lock().expect("shared lock");
        let available = !lock.stopped
            && !lock.finished
            && lock.error.is_none()
            && lock.replay_complete
            && lock.planning.available;
        let can_apply = available
            && lock.pending_decision.as_ref().is_some_and(|pending| {
                pending.submission_state == PendingSubmissionState::AwaitingSubmission
                    && lock.planning.runners.get(player).is_some_and(|runner| {
                        lock.planning
                            .executable_plan(player, runner.identity(), &pending.choice)
                            .is_ok()
                    })
            });
        crate::protocol::server::PlanningStatusMsg {
            protocol_version: crate::protocol::PROTOCOL_VERSION,
            game_id: self.game_id.clone(),
            checkpoint_id: lock.planning.checkpoint_id,
            available,
            can_start: available
                && lock.seats.get(player) == Some(&SeatController::Human)
                && lock.latest_state.active.as_ref() != Some(player),
            has_draft: lock.planning.has_draft(player),
            identity: lock
                .planning
                .runners
                .get(player)
                .map(|runner| runner.identity()),
            can_apply,
            application: lock.planning.application(player),
            secondary: (!lock.stopped
                && !lock.finished
                && lock.error.is_none()
                && lock.replay_complete)
                .then(|| {
                    lock.planning.secondary_status(
                        player,
                        lock.seats.get(player) == Some(&SeatController::Human),
                    )
                })
                .flatten(),
        }
    }

    pub(crate) fn subscribe_secondary_planning(
        &self,
        player: &PlayerId,
    ) -> Option<mpsc::Receiver<crate::planning::runner::PlanningEnvelope>> {
        let lock = self.shared.lock().expect("shared lock");
        if lock.stopped || !lock.replay_complete {
            return None;
        }
        lock.planning
            .secondary_runner(player)
            .map(|runner| runner.subscribe())
    }

    pub(crate) fn secondary_attempt_is_current(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
    ) -> bool {
        let lock = self.shared.lock().expect("shared lock");
        !lock.stopped
            && lock
                .planning
                .secondary_runner(player)
                .is_some_and(|runner| runner.is_current_attempt(identity))
    }

    /// As [`Self::recv_planning_timeout`], for the seat's secondary draft.
    pub fn recv_secondary_planning_timeout(
        &self,
        player: &PlayerId,
        timeout: std::time::Duration,
    ) -> Result<crate::planning::runner::PlanningEnvelope, mpsc::RecvTimeoutError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let lock = self.shared.lock().expect("shared lock");
                if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
                    return Err(mpsc::RecvTimeoutError::Disconnected);
                }
                let runner = lock
                    .planning
                    .secondary_runner(player)
                    .ok_or(mpsc::RecvTimeoutError::Disconnected)?;
                match runner.try_recv() {
                    Ok(envelope) => return Ok(envelope),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(mpsc::RecvTimeoutError::Disconnected);
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
            std::thread::sleep(remaining.min(std::time::Duration::from_millis(5)));
        }
    }

    /// Private stream independent of the captured runner output used by tests.
    pub(crate) fn subscribe_planning(
        &self,
        player: &PlayerId,
    ) -> Option<mpsc::Receiver<crate::planning::runner::PlanningEnvelope>> {
        let lock = self.shared.lock().expect("shared lock");
        if lock.stopped || !lock.replay_complete {
            return None;
        }
        lock.planning
            .runners
            .get(player)
            .map(|runner| runner.subscribe())
    }

    pub(crate) fn planning_attempt_is_current(
        &self,
        player: &PlayerId,
        identity: crate::planning::runner::AttemptIdentity,
    ) -> bool {
        let lock = self.shared.lock().expect("shared lock");
        !lock.stopped
            && lock
                .planning
                .runners
                .get(player)
                .is_some_and(|runner| runner.is_current_attempt(identity))
    }

    /// Wait without holding the session lock: the live worker must remain free
    /// to refresh or cancel the runner whose output we are waiting for.
    pub fn recv_planning_timeout(
        &self,
        player: &PlayerId,
        timeout: std::time::Duration,
    ) -> Result<crate::planning::runner::PlanningEnvelope, mpsc::RecvTimeoutError> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            {
                let lock = self.shared.lock().expect("shared lock");
                if lock.stopped || lock.finished || lock.error.is_some() || !lock.replay_complete {
                    return Err(mpsc::RecvTimeoutError::Disconnected);
                }
                let runner = lock
                    .planning
                    .runners
                    .get(player)
                    .ok_or(mpsc::RecvTimeoutError::Disconnected)?;
                match runner.try_recv() {
                    Ok(envelope) => return Ok(envelope),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(mpsc::RecvTimeoutError::Disconnected);
                    }
                    Err(mpsc::TryRecvError::Empty) => {}
                }
            }
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return Err(mpsc::RecvTimeoutError::Timeout);
            }
            std::thread::sleep(remaining.min(std::time::Duration::from_millis(5)));
        }
    }

    /// Scripts survive stopped attempts and session replacement.
    pub fn plans(&self) -> BTreeMap<PlayerId, crate::planning::runner::PlayerPlan> {
        self.shared.lock().expect("shared lock").planning.plans()
    }

    /// The live history exactly as `history.json` would hold it right now (unsaved batches
    /// included), with the game's seed and seats, read under one lock so the pieces agree.
    #[must_use]
    pub fn replay_export(&self) -> (crate::storage::GameHistory, Option<u64>, Vec<PlayerId>) {
        let lock = self.shared.lock().expect("shared lock");
        (
            crate::storage::GameHistory {
                decisions: lock.decision_log.clone(),
                redo: lock.redo_decisions.clone(),
                events: lock.event_log.clone(),
                redo_events: lock.redo_events.clone(),
                event_counter: lock.event_counter,
                revision: lock.game_version.saturating_add(1),
                generation: lock.history_generation,
                batches: lock.batches.clone(),
                rng_marks: lock.rng_marks.clone(),
            },
            lock.seed,
            lock.player_ids.clone(),
        )
    }

    pub fn restart_config(&self) -> SessionConfig {
        let mut config = self.initial_config.clone();
        config.plans = self.plans();
        // Callers can replace the decision prefix (batch commit, undo, redo).
        // The old speculative view must never be reused for a different cursor.
        config.replay_boundary_state = None;
        // What an undo took back is told by the session that the undo left, and by no other.
        config.undone = BTreeMap::new();
        config.announce_tail = false;
        // A copy of the game belongs to one history; whatever replaces this one gets its own.
        config.resume = None;
        // A rewind or a batch starts a new worker; what each seat asked for survives it.
        {
            let lock = self.shared.lock().expect("shared lock");
            config.reaction_modes = lock.reaction_modes_snapshot();
            // A timeline made by a turn redo replays with forced random positions.
            config.rng_marks.clone_from(&lock.rng_marks);
            // The wait of the table as it is now, not as it was when this worker started.
            config.random_bot_delay_ms = Some(lock.bot_delay_ms);
            // And its names: a seat that was taken over has the name of who took it.
            config.nicknames.clone_from(&lock.nicknames);
        }
        config
    }

    /// The wait before each answer of a seat that the server plays, in milliseconds.
    #[must_use]
    pub fn bot_delay_ms(&self) -> u64 {
        self.shared.lock().expect("shared lock").bot_delay_ms
    }

    /// Changes the wait of the seats that the server plays. A seat that is waiting now finishes
    /// its wait; its next decision has the new one.
    pub fn set_bot_delay_ms(&self, delay_ms: u64) {
        self.shared.lock().expect("shared lock").bot_delay_ms =
            delay_ms.min(table::MAX_BOT_DELAY_MS);
    }

    /// Gives the players of the table their names: every snapshot and update after this says
    /// them. The registry calls it for a recovered game and when a seat is taken over.
    pub fn set_nicknames(&self, nicknames: impl IntoIterator<Item = (PlayerId, String)>) {
        self.shared
            .lock()
            .expect("shared lock")
            .nicknames
            .extend(nicknames);
    }

    /// The faction of each player of the game, as the game seated them.
    #[must_use]
    pub fn factions(&self) -> BTreeMap<PlayerId, String> {
        let lock = self.shared.lock().expect("shared lock");
        lock.latest_state
            .players
            .iter()
            .map(|player| (player.id.clone(), player.faction.as_str().to_owned()))
            .collect()
    }

    /// The forced random positions of the live timeline (empty unless a turn redo made it).
    #[must_use]
    pub fn rng_marks(&self) -> RngMarks {
        self.shared.lock().expect("shared lock").rng_marks.clone()
    }

    /// The decision the engine is waiting on, as `(seat, prompt)`, if any.
    #[must_use]
    pub fn pending_prompt(&self) -> Option<(PlayerId, String)> {
        let lock = self.shared.lock().expect("shared lock");
        lock.pending_decision
            .as_ref()
            .map(|p| (p.seat.clone(), p.choice.prompt.clone()))
    }

    /// The worker is still replaying recorded decisions (see `SessionShared::replaying`).
    #[must_use]
    pub fn is_replaying(&self) -> bool {
        self.shared.lock().expect("shared lock").replaying
    }

    pub fn history_ready(&self) -> bool {
        let lock = self.shared.lock().expect("shared lock");
        lock.replay_complete
            && lock.error.is_none()
            && !lock.stopped
            // A nested reaction can still have submitted decisions waiting for the
            // outer engine step to return. Rewinding stops that step and discards
            // those unpublished decisions; the new offer must itself be unreserved.
            && (lock.finished
                || lock.bot_deciding
                || lock.pending_decision.as_ref().is_some_and(|pending| {
                    pending.submission_state == PendingSubmissionState::AwaitingSubmission
                }))
    }

    /// Hold back random bot answers until the returned guard drops. A history change takes it
    /// before it compares versions so a bot cannot act during the rewind.
    #[must_use]
    pub fn pause_bots(self: &Arc<Self>) -> BotPause {
        self.shared.lock().expect("shared lock").history_pause = true;
        BotPause(self.clone())
    }

    pub(crate) fn choice_in_flight(&self) -> bool {
        let lock = self.shared.lock().expect("shared lock");
        !lock.in_flight_submissions.is_empty()
            || lock
                .pending_decision
                .as_ref()
                .is_some_and(|pending| pending.submission_state == PendingSubmissionState::Reserved)
    }

    pub fn wait_replayed(&self) -> Result<(), String> {
        let deadline = std::time::Instant::now()
            + std::time::Duration::from_secs(15)
            + batch::replay_budget(self.initial_config.prior_decisions.len());
        loop {
            let lock = self.shared.lock().expect("shared lock");
            if let Some(error) = &lock.error {
                return Err(error.clone());
            }
            if lock.replay_complete && (lock.finished || lock.pending_decision.is_some()) {
                return Ok(());
            }
            drop(lock);
            if std::time::Instant::now() >= deadline {
                return Err("session replay timed out".to_owned());
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    pub fn redo_decisions(&self) -> Vec<DecisionRecord> {
        self.shared
            .lock()
            .expect("shared lock")
            .redo_decisions
            .clone()
    }

    pub fn history_events(&self) -> (Vec<crate::protocol::server::GameEvent>, u64) {
        let lock = self.shared.lock().expect("shared lock");
        (lock.redo_events.clone(), lock.event_counter)
    }

    pub fn batches(&self) -> Vec<crate::storage::BatchRecord> {
        self.shared.lock().expect("shared lock").batches.clone()
    }

    /// Stops the worker thread cleanly.
    pub fn stop(&self) {
        {
            let mut lock = self.shared.lock().expect("shared lock");
            lock.stopped = true;
            lock.planning.close();
            // A running bluff hold ends at once instead of at its next poll.
            lock.bluff.wake();
            // Dropping inboxes unblocks any waiting RemoteHumanDecider
            lock.seat_inboxes.clear();
        }

        let mut handle_lock = self.worker_handle.lock().expect("worker handle lock");
        if let Some(handle) = handle_lock.take() {
            let _ = handle.join();
        }
        let mut lock = self.shared.lock().expect("shared lock");
        lock.pending_decision = None;
        lock.in_flight_submissions.clear();
        lock.scripts.clear();
        lock.step_snapshot = None;
    }
}

/// Guard from [`GameSession::pause_bots`]; bots resume when it drops.
pub struct BotPause(Arc<GameSession>);

impl Drop for BotPause {
    fn drop(&mut self) {
        self.0.shared.lock().expect("shared lock").history_pause = false;
    }
}

impl Drop for GameSession {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A run of a draft ([`GameSession::run_draft`]) on the game of `shared`: the version of the
/// game, and what the game made of the script. `None` when the game cannot be copied now
/// (stopped, finished, failed, or still in its replay).
pub(crate) fn run_draft_on(
    shared: &Mutex<SessionShared>,
    player: &PlayerId,
    scope: &ti4_view::planning::audit::PlanScope,
    script: &[ti4_view::planning::draft::DraftStep],
) -> (u64, Option<ti4_view::planning::draft::DraftOutcome>) {
    use ti4_view::planning::draft::{DraftBase, run_draft};
    let (copy, version, tiles, galaxy, watch) = {
        let mut lock = shared.lock().expect("shared lock");
        let playable = !lock.stopped
            && !lock.finished
            && lock.error.is_none()
            && lock.replay_complete
            && lock.seats.contains_key(player);
        let copy = playable
            .then(|| lock.planning.fork())
            .flatten()
            .map(|(game, _)| game);
        (
            copy,
            lock.game_version,
            lock.map_tiles.clone(),
            lock.galaxy.clone(),
            lock.planning.watch.clone(),
        )
    };
    let Some(game) = copy else {
        return (version, None);
    };
    let base = DraftBase {
        game: &game,
        content: ti4_content::ContentStore::embedded(),
        sources: ti4_model::content_types::POK,
        tiles: &tiles,
        galaxy: galaxy.as_deref(),
        secondary: Some(&watch),
    };
    (version, Some(run_draft(&base, player, scope, script)))
}
