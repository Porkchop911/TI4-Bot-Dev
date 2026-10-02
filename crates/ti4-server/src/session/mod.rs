pub mod batch;
pub mod decider;
pub mod registry;
pub mod replay;
pub mod transport;
pub mod worker;

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
pub use registry::{BotServiceConfig, GameRegistry};
pub use replay::{ReplayError, ReplayReport, replay_session};
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
}

/// Configuration used to spawn an authoritative game session.
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub game_id: String,
    pub state: GameState,
    pub seats: BTreeMap<PlayerId, SeatController>,
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
    /// State at the first unplanned choice, computed by private replay for a committed batch.
    pub replay_boundary_state: Option<GameState>,
}

impl SessionConfig {
    #[must_use]
    pub fn new(game_id: impl Into<String>, state: GameState) -> Self {
        Self {
            game_id: game_id.into(),
            state,
            seats: BTreeMap::new(),
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
            replay_boundary_state: None,
        }
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
            if !pending
                .choice
                .options
                .iter()
                .any(|option| option.id == option_id)
            {
                return Err(RejectionReason::UnknownOption {
                    option_id: option_id.to_owned(),
                });
            }
            pending.submission_state = PendingSubmissionState::Reserved;

            lock.seat_inboxes.get(seat).cloned()
        };

        let Some(inbox) = target_inbox else {
            return Err(RejectionReason::UnauthorizedSeat {
                seat: Some(seat.clone()),
            });
        };

        if inbox.send(submission).is_err() {
            let mut lock = self.shared.lock().expect("shared lock");
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
            &lock.event_log,
        )
        .with_history(
            lock.decision_log.len(),
            lock.redo_decisions.len(),
            lock.history_generation,
        );
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
            (candidate == token).then(|| ViewerRole::Player(seat.clone()))
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
    pub fn lobby_details(
        &self,
    ) -> (
        Vec<PlayerId>,
        BTreeMap<PlayerId, SeatController>,
        BTreeMap<PlayerId, String>,
        Option<u64>,
    ) {
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
        }
    }

    pub fn game_version(&self) -> u64 {
        self.shared.lock().expect("shared lock").game_version
    }

    pub fn restart_config(&self) -> SessionConfig {
        let mut config = self.initial_config.clone();
        // Callers can replace the decision prefix (batch commit, undo, redo).
        // The old speculative view must never be reused for a different cursor.
        config.replay_boundary_state = None;
        config
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
                || lock.pending_decision.as_ref().is_some_and(|pending| {
                    pending.submission_state == PendingSubmissionState::AwaitingSubmission
                }))
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
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
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
    }
}

impl Drop for GameSession {
    fn drop(&mut self) {
        self.stop();
    }
}
