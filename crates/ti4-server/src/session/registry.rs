//! Registry for pre-game lobbies and active game sessions.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use ti4_content::ContentStore;
use ti4_model::id::PlayerId;

use crate::protocol::server::{ActionAcceptedMsg, InitialSnapshotMsg};
use crate::protocol::status::{RejectionReason, ViewerRole};
use crate::session::batch::{BatchFailure, BatchRequest, simulate};
use crate::session::{GameSession, SeatController, SessionConfig};
use crate::storage::{
    BatchRecord, GameHistory, GameInitRecord, LobbyRecord, PersistedLobbyPhase, PersistedLobbySeat,
    StorageError,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum HistoryAction {
    Undo,
    UndoBatch,
    UndoPipeline,
    Redo,
    RedoBatch,
    RedoPipeline,
    Restore { event_id: String },
    RestoreCursor { cursor: usize },
}

#[cfg(test)]
mod committed_worker_tests {
    use super::*;
    use crate::session::batch::{BatchKind, MovementPlan, MovementStep};
    use crate::storage::FileGameStore;

    fn pending(session: &GameSession) -> (PlayerId, String, u64, String) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some((seat, nonce, version)) = session.current_pending_decision() {
                let option = session
                    .get_snapshot(&ViewerRole::Player(seat.clone()))
                    .pending_choice
                    .unwrap()
                    .choice
                    .options[0]
                    .id
                    .clone();
                return (seat, nonce, version, option);
            }
            assert!(Instant::now() < deadline, "worker: {:?}", session.error());
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn committed_batch_recovers_from_first_replacement_replay_failure() {
        let path = std::env::temp_dir().join(format!("ti4_replay_{:032x}", rand::random::<u128>()));
        let store = Arc::new(FileGameStore::new(&path).unwrap());
        let registry = GameRegistry::new().with_store(store.clone());
        let host = PlayerId::new("p1");
        let guest = PlayerId::new("p2");
        let players = vec![host.clone(), guest.clone()];
        let (state, galaxy) =
            crate::map::create_game_with_map(ContentStore::embedded(), &players, 42).unwrap();
        let tiles = crate::map::build_board_tiles(ContentStore::embedded(), &galaxy);
        let config = SessionConfig::new("replay_retry", state)
            .with_seed(42)
            .with_player_ids(players)
            .with_galaxy(galaxy, tiles)
            .with_seat(host.clone(), SeatController::Human)
            .with_seat(guest, SeatController::Human);
        let session = registry.create_game(config).unwrap();
        let token = session.seat_tokens()[&host].clone();
        for _ in 0..4 {
            let (seat, nonce, version, _) = pending(&session);
            let choice = session
                .get_snapshot(&ViewerRole::Player(seat.clone()))
                .pending_choice
                .unwrap()
                .choice;
            let option = choice
                .options
                .iter()
                .find(|o| o.id == "tactical" || o.id == "22")
                .unwrap_or(&choice.options[0]);
            session
                .submit_choice(&seat, &nonce, version, &option.id)
                .unwrap();
        }
        let (_, nonce, version, _) = pending(&session);
        let start_cursor = session.decision_log().len();
        let request = BatchRequest {
            request_id: "replay_failure".into(),
            expected_version: version,
            nonce,
            plan: MovementPlan {
                kind: BatchKind::TacticalMovement,
                destination: session.current_state().active_system.unwrap().to_string(),
                steps: vec![MovementStep::DoneMoving],
            },
        };
        let mut attempts = 0;
        let result = registry
            .submit_batch_with_worker("replay_retry", &token, request.clone(), |mut config| {
                attempts += 1;
                if attempts == 1 {
                    // Only the first worker receives a bad replay input. The durable
                    // history has already been written with the correct decision.
                    config.prior_decisions[0].chosen = "unoffered_option".into();
                }
                GameSession::start(config)
            })
            .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(result.start_cursor, start_cursor);
        assert_eq!(result.end_cursor, start_cursor + 1);
        let live = registry.get_game("replay_retry").unwrap();
        let _ = pending(&live);
        assert!(live.error().is_none());
        assert_eq!(live.history_status().cursor, start_cursor + 1);
        let history = store.load_history("replay_retry").unwrap().unwrap();
        assert_eq!(history.decisions, live.decision_log());
        assert_eq!(history.batches.len(), 1);
        assert_eq!(history.batches[0].batch_id, result.batch_id);
        assert_eq!(history.events, live.event_log());
        let duplicate = registry
            .submit_batch("replay_retry", &token, request)
            .unwrap();
        assert_eq!(duplicate.batch_id, result.batch_id);
        assert_eq!(live.history_status().cursor, start_cursor + 1);

        let recovered = store.recover_session("replay_retry").unwrap();
        recovered.wait_replayed().unwrap();
        assert_eq!(recovered.decision_log(), live.decision_log());
        assert_eq!(recovered.decision_hashes(), live.decision_hashes());
        assert_eq!(recovered.current_state(), live.current_state());
        assert_eq!(recovered.batches().len(), 1);
        recovered.stop();

        registry
            .change_history(
                "replay_retry",
                &token,
                live.game_version(),
                HistoryAction::UndoBatch,
            )
            .unwrap();
        let undone = registry.get_game("replay_retry").unwrap();
        let (_, nonce, version, _) = pending(&undone);
        let failed_request = BatchRequest {
            request_id: "persistent_replay_failure".into(),
            expected_version: version,
            nonce,
            plan: MovementPlan {
                kind: BatchKind::TacticalMovement,
                destination: undone.current_state().active_system.unwrap().to_string(),
                steps: vec![MovementStep::DoneMoving],
            },
        };
        let mut failed_attempts = 0;
        let failed = registry
            .submit_batch_with_worker(
                "replay_retry",
                &token,
                failed_request.clone(),
                |mut config| {
                    failed_attempts += 1;
                    config.prior_decisions[0].chosen = "unoffered_option".into();
                    GameSession::start(config)
                },
            )
            .unwrap_err();
        assert_eq!(failed_attempts, 2);
        assert!(failed.reason.contains("batch committed"), "{failed:?}");
        let committed_history = store.load_history("replay_retry").unwrap().unwrap();
        assert_eq!(committed_history.batches.len(), 1);
        assert_eq!(
            committed_history.batches[0].request_id,
            failed_request.request_id
        );
        assert_eq!(committed_history.decisions.len(), start_cursor + 1);
        assert!(
            registry
                .submit_batch("replay_retry", &token, failed_request)
                .unwrap_err()
                .reason
                .contains("batch committed")
        );
        let restarted = store.recover_session("replay_retry").unwrap();
        restarted.wait_replayed().unwrap();
        assert_eq!(restarted.decision_log(), committed_history.decisions);
        assert_eq!(restarted.event_log(), committed_history.events);
        assert_eq!(restarted.batches().len(), 1);
        let _ = pending(&restarted);
        restarted.stop();
        drop(registry);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    NotFound,
    Forbidden,
    Conflict(String),
    InvalidTarget,
    Storage(String),
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub request_id: String,
    pub batch_id: String,
    pub start_cursor: usize,
    pub end_cursor: usize,
    pub active: bool,
    pub snapshot: InitialSnapshotMsg,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchError {
    pub failed_step: usize,
    pub reason: String,
    pub expected: String,
    pub offered_summary: Vec<String>,
}

impl From<BatchFailure> for BatchError {
    fn from(value: BatchFailure) -> Self {
        Self {
            failed_step: value.failed_step,
            reason: value.reason,
            expected: value.expected,
            offered_summary: value.offered_summary,
        }
    }
}

impl BatchError {
    fn simple(reason: &str) -> Self {
        Self {
            failed_step: 0,
            reason: reason.into(),
            expected: String::new(),
            offered_summary: Vec::new(),
        }
    }
}

use crate::storage::{
    LobbySlotId, PLAYER_RECORD_VERSION, PlayerGameInitRecord, PlayerLobbyMember, PlayerLobbyRecord,
    PlayerLobbySlot, PlayerSession, PlayerSessionsRecord, generate_player_id,
};

/// Public projection; deliberately cannot serialize the private persistence record.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerLobbyView {
    pub game_id: String,
    pub phase: LobbyPhase,
    pub host_player_id: PlayerId,
    pub slots: Vec<PlayerSlotView>,
    pub lobby_version: u64,
    pub bot_service_enabled: bool,
}

/// Configuration for running bot agents on this server.
#[derive(Debug, Clone)]
pub struct BotServiceConfig {
    pub password: String,
    pub advisor_url: String,
    pub bot_agent_bin: std::path::PathBuf,
    pub server_port: u16,
    pub max_active_bots: usize,
}

pub struct BotChild {
    pub child: tokio::process::Child,
    pub nickname: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSlotView {
    pub slot_id: LobbySlotId,
    pub position: usize,
    pub occupant: Option<PlayerId>,
    pub nickname: Option<String>,
    pub ready: bool,
    pub connected: bool,
    pub can_take_over: bool,
}

impl PlayerLobbyRecord {
    /// Build a lobby with the creator in the first slot, leaving the rest open.
    pub fn create(
        game_id: String,
        slot_count: usize,
        seed: u64,
        nickname: &str,
    ) -> Result<(Self, PlayerId, PlayerSession), StorageError> {
        crate::storage::validate_nickname(nickname)?;
        crate::storage::validate_game_id(&game_id)?;
        if !(1..=8).contains(&slot_count) {
            return Err(StorageError::InvalidPlayerRecord("slot count"));
        }
        let mut players = BTreeMap::new();
        let host_player_id = generate_player_id(&players);
        let session = PlayerSession::generate();
        players.insert(
            host_player_id.clone(),
            PlayerLobbyMember {
                ready: false,
                session: session.clone(),
                nickname: nickname.to_owned(),
            },
        );
        let slots = (0..slot_count)
            .map(|index| PlayerLobbySlot {
                slot_id: LobbySlotId(format!("slot_{}", index + 1)),
                occupant: (index == 0).then(|| host_player_id.clone()),
            })
            .collect();
        let lobby = Self {
            schema_version: PLAYER_RECORD_VERSION,
            game_id,
            phase: PersistedLobbyPhase::Lobby,
            host_player_id: host_player_id.clone(),
            slots,
            players,
            seed,
            lobby_version: 1,
        };
        lobby.validate()?;
        Ok((lobby, host_player_id, session))
    }

    /// Only this explicit projection is suitable for public HTTP/WS output.
    #[must_use]
    pub fn public_view(&self) -> PlayerLobbyView {
        PlayerLobbyView {
            game_id: self.game_id.clone(),
            phase: match self.phase {
                PersistedLobbyPhase::Lobby => LobbyPhase::Lobby,
                PersistedLobbyPhase::Running => LobbyPhase::Running,
            },
            host_player_id: self.host_player_id.clone(),
            slots: self
                .slots
                .iter()
                .enumerate()
                .map(|(index, slot)| PlayerSlotView {
                    slot_id: slot.slot_id.clone(),
                    position: index + 1,
                    occupant: slot.occupant.clone(),
                    nickname: slot
                        .occupant
                        .as_ref()
                        .map(|id| self.players[id].nickname.clone()),
                    ready: slot
                        .occupant
                        .as_ref()
                        .is_some_and(|id| self.players[id].ready),
                    connected: false,
                    can_take_over: false,
                })
                .collect(),
            lobby_version: self.lobby_version,
            bot_service_enabled: false,
        }
    }
}

/// Summary of an active, completed, or unstarted game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameSummary {
    pub game_id: String,
    pub is_finished: bool,
    pub pending_seat: Option<PlayerId>,
}

/// Lifecycle phase exposed by the lobby API.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LobbyPhase {
    Lobby,
    Running,
}

/// Server-owned lobby configuration.
#[derive(Debug, Clone)]
pub struct LobbyState {
    pub game_id: String,
    pub phase: LobbyPhase,
    pub host_seat: PlayerId,
    pub player_ids: Vec<PlayerId>,
    pub seats: BTreeMap<PlayerId, LobbySeat>,
    pub seed: u64,
    pub lobby_version: u64,
}

/// Lobby state for a configured seat. This type is never serialized directly.
#[derive(Clone)]
pub struct LobbySeat {
    pub controller: SeatController,
    pub ready: bool,
    pub seat_token: Option<String>,
    pub lease_expires_at_ms: Option<u64>,
}

impl std::fmt::Debug for LobbySeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LobbySeat")
            .field("controller", &self.controller)
            .field("ready", &self.ready)
            .field("seat_token", &"[redacted]")
            .field("lease_expires_at_ms", &self.lease_expires_at_ms)
            .finish()
    }
}

/// Input used to create a durable pre-game lobby.
#[derive(Debug, Clone)]
pub struct LobbyConfig {
    pub game_id: String,
    pub host_seat: PlayerId,
    pub player_ids: Vec<PlayerId>,
    pub seats: BTreeMap<PlayerId, SeatController>,
    pub seed: u64,
}

/// Result returned only at creation time, when handoff capabilities are allowed.
#[derive(Clone)]
pub struct CreatedLobby {
    pub lobby: LobbyState,
    pub creator_token: String,
    /// Internal creation result; HTTP deliberately returns only `creator_token`.
    pub seat_tokens: BTreeMap<PlayerId, String>,
}

impl std::fmt::Debug for CreatedLobby {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreatedLobby")
            .field("lobby", &self.lobby)
            .field("creator_token", &"[redacted]")
            .field("seat_tokens", &"[redacted]")
            .finish()
    }
}

/// Snapshot of a lobby and the caller authenticated by a capability, if any.
#[derive(Debug, Clone)]
pub struct LobbyStatus {
    pub lobby: LobbyState,
    pub viewer: Option<PlayerId>,
}

/// A deterministic lifecycle or authorization failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LobbyError {
    NotFound,
    NotInLobby,
    AlreadyRunning,
    InvalidCapability,
    HumanSeatRequired,
    HostRequired,
    HumansNotReady,
    SeatUnavailable,
    TakeoverUnavailable,
    InvalidPlayerId,
    InvalidSlotOrder,
    InvalidNickname,
    Map(String),
    Storage(String),
}

impl LobbyError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "Game not found".to_owned(),
            Self::NotInLobby => "Game has no lobby".to_owned(),
            Self::AlreadyRunning => "Game has already started".to_owned(),
            Self::InvalidCapability => "Invalid seat capability".to_owned(),
            Self::HumanSeatRequired => "Only human seats may update readiness".to_owned(),
            Self::HostRequired => "Only the lobby host may perform this action".to_owned(),
            Self::HumansNotReady => "Every human seat must be ready before starting".to_owned(),
            Self::SeatUnavailable => "Seat is unavailable".to_owned(),
            Self::TakeoverUnavailable => "Takeover is not available yet".to_owned(),
            Self::InvalidPlayerId => "Invalid player ID".to_owned(),
            Self::InvalidSlotOrder => "Slot order must be a complete permutation".to_owned(),
            Self::InvalidNickname => "Nickname must be trimmed, nonempty Unicode text of at most 64 UTF-8 bytes without control or formatting characters".to_owned(),
            Self::Map(error) => format!("Failed to start game with map: {error}"),
            Self::Storage(error) => format!("Failed to persist lobby lifecycle: {error}"),
        }
    }
}

fn check_nickname(nickname: &str) -> Result<(), LobbyError> {
    crate::storage::validate_nickname(nickname).map_err(|_| LobbyError::InvalidNickname)
}

#[derive(Default)]
struct RegistryState {
    sessions: BTreeMap<String, Arc<GameSession>>,
    lobbies: BTreeMap<String, LobbyState>,
    player_lobbies: BTreeMap<String, PlayerLobbyRecord>,
    presence: BTreeMap<(String, PlayerId), PlayerPresence>,
    next_connection_id: u64,
}

/// A failed save is left on disk unchanged; other games can still be recovered.
#[derive(Debug)]
pub struct RecoveryFailure {
    pub game_id: String,
    pub stage: &'static str,
    pub error: StorageError,
}

#[derive(Debug, Default)]
pub struct RecoveryReport {
    pub recovered: Vec<String>,
    pub failed: Vec<RecoveryFailure>,
}

struct PlayerPresence {
    last_heartbeat: Option<Instant>,
    connections: BTreeMap<u64, Instant>,
    admitted_at: Instant,
}

impl Default for PlayerPresence {
    fn default() -> Self {
        Self {
            last_heartbeat: None,
            connections: BTreeMap::new(),
            admitted_at: Instant::now(),
        }
    }
}

/// Heartbeats every 10 seconds; disconnect is displayed after 30 seconds without one.
pub const PRESENCE_GRACE: Duration = Duration::from_secs(30);

/// Thread-safe registry that serializes each lobby's transition into an active session.
pub struct GameRegistry {
    state: Mutex<RegistryState>,
    /// Serializes timeline mutations for one game without blocking unrelated games.
    game_gates: Mutex<BTreeMap<String, Arc<Mutex<()>>>>,
    store: Option<Arc<crate::storage::FileGameStore>>,
    lease_duration: Duration,
    started_at: Instant,
    presence_grace: Duration,
    bot_config: Option<BotServiceConfig>,
    active_bots: Mutex<BTreeMap<String, Vec<BotChild>>>,
}

impl Default for GameRegistry {
    fn default() -> Self {
        Self {
            state: Mutex::new(RegistryState::default()),
            game_gates: Mutex::new(BTreeMap::new()),
            store: None,
            lease_duration: Duration::from_secs(30),
            started_at: Instant::now(),
            presence_grace: PRESENCE_GRACE,
            bot_config: None,
            active_bots: Mutex::new(BTreeMap::new()),
        }
    }
}

// The history is already durable here. Do not publish an unusable first worker:
// a fresh replay can recover from a transient worker-start failure without
// re-executing the batch or assigning it a second request ID. If both attempts
// fail, publish the committed history with its error so a restart can recover it.
fn start_committed_worker(
    config: SessionConfig,
    start: &mut impl FnMut(SessionConfig) -> GameSession,
) -> (Arc<GameSession>, Result<(), String>) {
    let first = Arc::new(start(config.clone()));
    if first.wait_replayed().is_ok() {
        return (first, Ok(()));
    }
    first.stop();
    let retry = Arc::new(start(config));
    let replay = retry.wait_replayed();
    (retry, replay)
}

impl GameRegistry {
    fn game_gate(&self, game_id: &str) -> Arc<Mutex<()>> {
        self.game_gates
            .lock()
            .expect("game gates lock")
            .entry(game_id.to_owned())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Replay a staged movement privately, then durably replace the entire timeline.
    pub fn submit_batch(
        &self,
        game_id: &str,
        credential: &str,
        request: BatchRequest,
    ) -> Result<BatchResult, BatchError> {
        self.submit_batch_with_worker(game_id, credential, request, GameSession::start)
    }

    fn submit_batch_with_worker(
        &self,
        game_id: &str,
        credential: &str,
        request: BatchRequest,
        mut start_worker: impl FnMut(SessionConfig) -> GameSession,
    ) -> Result<BatchResult, BatchError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let actor = if let Some(lobby) = state.player_lobbies.get(game_id) {
            authenticate_player(lobby, credential)
                .map_err(|_| BatchError::simple("unauthorized"))?
        } else {
            authenticated_seat(
                state
                    .lobbies
                    .get(game_id)
                    .ok_or_else(|| BatchError::simple("game not found"))?,
                credential,
            )
            .map_err(|_| BatchError::simple("unauthorized"))?
        };
        let session = state
            .sessions
            .get(game_id)
            .ok_or_else(|| BatchError::simple("game not found"))?
            .clone();
        if request.request_id.is_empty()
            || request.request_id.len() > 128
            || !request
                .request_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(BatchError::simple("invalid request_id"));
        }
        if let Some(batch) = session
            .batches()
            .iter()
            .find(|b| b.request_id == request.request_id)
        {
            if batch.actor != actor {
                return Err(BatchError::simple("request_id already used"));
            }
            if !session.history_ready() {
                return Err(BatchError::simple(
                    "batch committed, replacement session unavailable",
                ));
            }
            return Ok(BatchResult {
                request_id: request.request_id,
                batch_id: batch.batch_id.clone(),
                start_cursor: batch.start_cursor,
                end_cursor: batch.end_cursor,
                active: batch.end_cursor <= session.decision_log().len(),
                snapshot: session.get_snapshot(&ViewerRole::Player(actor)),
            });
        }
        if !session.history_ready()
            || session.game_version() != request.expected_version
            || !session
                .current_pending_decision()
                .is_some_and(|(seat, nonce, version)| {
                    seat == actor && nonce == request.nonce && version == request.expected_version
                })
        {
            return Err(BatchError::simple("stale decision boundary"));
        }
        if request.plan.kind == crate::session::batch::BatchKind::TacticalMovement
            && session
                .current_state()
                .active_system
                .as_ref()
                .map(|id| id.as_str())
                != Some(request.plan.destination.as_str())
        {
            return Err(BatchError::simple("movement destination changed"));
        }
        let config = session.restart_config();
        let prior = session.decision_log();
        drop(state);
        let simulation = simulate(&config, &prior, &actor, &request.plan)?;
        let boundary_state = simulation.boundary_state.clone();
        let decisions = simulation.decisions;
        let start_cursor = prior.len();
        let end_cursor = start_cursor + decisions.len();
        let batch_id = format!("batch_{:032x}", rand::random::<u128>());
        let all_decisions: Vec<_> = prior.iter().chain(decisions.iter()).cloned().collect();
        let mut events = session.event_log();
        let (_, mut counter) = session.history_events();
        counter = counter.max(events.len() as u64);
        for (i, decision) in decisions.iter().enumerate() {
            let offered = &simulation.selected[i];
            let action_id = if decision
                .context
                .as_ref()
                .is_some_and(|c| c.phase == ti4_model::state::Phase::Action)
            {
                crate::protocol::server::action_id_for(&all_decisions, start_cursor + i + 1)
            } else {
                None
            };
            let action_start_cursor = action_id
                .as_ref()
                .and_then(|id| id.strip_prefix("action_"))
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));
            let (detail, movement, seat_detail) = crate::protocol::server::verified_decision_facts(
                decision,
                Some(offered),
                (request.plan.kind == crate::session::batch::BatchKind::TacticalMovement)
                    .then_some(request.plan.destination.as_str()),
                &boundary_state,
            );
            let grouping = crate::protocol::server::decision_grouping(
                decision,
                Some(offered),
                &all_decisions,
                start_cursor + i + 1,
            );
            counter += 1;
            events.push(crate::protocol::server::GameEvent {
                id: format!("{game_id}-{counter}"),
                timestamp: String::new(),
                version: Some(request.expected_version),
                visibility: crate::protocol::server::EventVisibility::Public,
                event: crate::protocol::server::GameEventKind::DecisionResolved,
                decision_count: Some(start_cursor + i + 1),
                batch_id: Some(batch_id.clone()),
                batch_start_cursor: Some(start_cursor),
                batch_end_cursor: Some(end_cursor),
                action_id,
                action_start_cursor,
                actor: grouping.0,
                round: grouping.1,
                phase: grouping.2,
                action_type: grouping.3,
                action_actor: grouping.4,
                stage: grouping.5,
                movement,
                detail,
                seat_detail,
                private_detail: None,
            });
            for (_, phase, round) in simulation
                .transitions
                .iter()
                .filter(|(cursor, _, _)| *cursor == start_cursor + i + 1)
            {
                counter += 1;
                events.push(crate::protocol::server::GameEvent {
                    id: format!("{game_id}-{counter}"),
                    timestamp: String::new(),
                    version: Some(request.expected_version),
                    visibility: crate::protocol::server::EventVisibility::Public,
                    event: crate::protocol::server::GameEventKind::PhaseTransition {
                        phase: *phase,
                        round: *round,
                    },
                    decision_count: Some(start_cursor + i + 1),
                    batch_id: None,
                    batch_start_cursor: None,
                    batch_end_cursor: None,
                    action_id: None,
                    action_start_cursor: None,
                    actor: None,
                    round: None,
                    phase: None,
                    action_type: None,
                    action_actor: None,
                    stage: None,
                    detail: None,
                    movement: None,
                    seat_detail: None,
                    private_detail: None,
                });
            }
        }
        if let Some(winner) = simulation.winner {
            counter += 1;
            events.push(crate::protocol::server::GameEvent {
                id: format!("{game_id}-{counter}"),
                timestamp: String::new(),
                version: Some(request.expected_version),
                visibility: crate::protocol::server::EventVisibility::Public,
                event: crate::protocol::server::GameEventKind::GameFinished { winner },
                decision_count: Some(end_cursor),
                batch_id: None,
                batch_start_cursor: None,
                batch_end_cursor: None,
                action_id: None,
                action_start_cursor: None,
                actor: None,
                round: None,
                phase: None,
                action_type: None,
                action_actor: None,
                stage: None,
                detail: None,
                movement: None,
                seat_detail: None,
                private_detail: None,
            });
        }
        let mut batches = session.batches();
        batches.retain(|b| b.end_cursor <= start_cursor);
        batches.push(BatchRecord {
            request_id: request.request_id.clone(),
            batch_id: batch_id.clone(),
            actor: actor.clone(),
            start_cursor,
            end_cursor,
        });
        let revision = request
            .expected_version
            .checked_add(1)
            .ok_or_else(|| BatchError::simple("version exhausted"))?;
        let history = GameHistory {
            decisions: prior.into_iter().chain(decisions).collect(),
            redo: Vec::new(),
            events,
            redo_events: Vec::new(),
            event_counter: counter,
            revision,
            generation: session.history_status().generation.saturating_add(1),
            batches,
        };
        let mut state = self.state.lock().expect("registry lock");
        if !state
            .sessions
            .get(game_id)
            .is_some_and(|live| Arc::ptr_eq(live, &session))
            || session.game_version() != request.expected_version
            || !session.history_ready()
        {
            return Err(BatchError::simple("game advanced during batch"));
        }
        session.stop();
        if session.decision_log().len() != start_cursor {
            let replacement = Arc::new(GameSession::start_recovered(
                config.clone(),
                session.decision_log(),
                session.event_log(),
            ));
            state.sessions.insert(game_id.to_owned(), replacement);
            return Err(BatchError::simple("game advanced during batch"));
        }
        if let Some(store) = &config.store
            && let Err(error) = store.save_history(game_id, &history)
        {
            let replacement = Arc::new(GameSession::start_recovered(
                config.clone(),
                session.decision_log(),
                session.event_log(),
            ));
            state.sessions.insert(game_id.to_owned(), replacement);
            return Err(BatchError::simple(&format!("storage error: {error}")));
        }
        let mut next = config;
        next.prior_decisions = history.decisions;
        next.prior_events = history.events;
        next.redo_decisions = history.redo;
        next.redo_events = history.redo_events;
        next.event_counter = history.event_counter;
        next.initial_version = revision;
        next.history_generation = history.generation;
        next.history_active = true;
        next.batches = history.batches;
        next.replay_boundary_state = Some(boundary_state);
        let (replacement, replay) = start_committed_worker(next, &mut start_worker);
        state
            .sessions
            .insert(game_id.to_owned(), replacement.clone());
        replay.map_err(|error| {
            BatchError::simple(&format!(
                "batch committed, replacement session failed to replay: {error}"
            ))
        })?;
        Ok(BatchResult {
            request_id: request.request_id,
            batch_id,
            start_cursor,
            end_cursor,
            active: true,
            snapshot: replacement.get_snapshot(&ViewerRole::Player(actor)),
        })
    }
    /// Serializes a host rewind against credential rotation and human submissions.
    #[expect(
        clippy::too_many_lines,
        reason = "one serialized authorization, replay and publication boundary"
    )]
    pub fn change_history(
        &self,
        game_id: &str,
        credential: &str,
        expected_version: u64,
        action: HistoryAction,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let host = if let Some(lobby) = state.player_lobbies.get(game_id) {
            let actor =
                authenticate_player(lobby, credential).map_err(|_| HistoryError::Forbidden)?;
            if actor != lobby.host_player_id {
                return Err(HistoryError::Forbidden);
            }
            actor
        } else if let Some(lobby) = state.lobbies.get(game_id) {
            let actor =
                authenticated_seat(lobby, credential).map_err(|_| HistoryError::Forbidden)?;
            if actor != lobby.host_seat {
                return Err(HistoryError::Forbidden);
            }
            actor
        } else {
            return Err(HistoryError::NotFound);
        };
        let session = state
            .sessions
            .get(game_id)
            .ok_or(HistoryError::NotFound)?
            .clone();
        if session.game_version() != expected_version || !session.history_ready() {
            return Err(HistoryError::Conflict(
                "Game advanced or a decision is in flight".to_owned(),
            ));
        }
        let current = session.decision_log();
        let redo = session.redo_decisions();
        let events = session.event_log();
        let (redo_events, event_counter) = session.history_events();
        let original_count = current.len();
        let total = original_count + redo.len();
        let target = match action {
            HistoryAction::Undo => current
                .len()
                .checked_sub(1)
                .ok_or(HistoryError::InvalidTarget)?,
            HistoryAction::UndoBatch => session
                .batches()
                .iter()
                .rev()
                .find(|b| b.end_cursor <= current.len())
                .map(|b| b.start_cursor)
                .ok_or(HistoryError::InvalidTarget)?,
            HistoryAction::UndoPipeline => {
                let last = current
                    .len()
                    .checked_sub(1)
                    .ok_or(HistoryError::InvalidTarget)?;
                if let Some(id) = events
                    .iter()
                    .rev()
                    .find(|event| {
                        matches!(
                            event.event,
                            crate::protocol::server::GameEventKind::DecisionResolved
                        ) && event.decision_count == Some(last + 1)
                    })
                    .and_then(|event| event.action_id.as_ref())
                {
                    events
                        .iter()
                        .find(|event| event.action_id.as_ref() == Some(id))
                        .and_then(|event| event.decision_count)
                        .and_then(|cursor| cursor.checked_sub(1))
                        .ok_or(HistoryError::InvalidTarget)?
                } else {
                    let phase = current[..=last]
                        .iter()
                        .rev()
                        .find_map(|record| record.context.as_ref());
                    current[..=last]
                        .iter()
                        .rposition(|record| {
                            record.prompt == "action phase"
                                && phase.is_some_and(|end| {
                                    end.phase == ti4_model::state::Phase::Action
                                        && record
                                            .context
                                            .as_ref()
                                            .is_none_or(|start| start.round == end.round)
                                })
                        })
                        .unwrap_or(last)
                }
            }
            HistoryAction::Redo => {
                if redo.is_empty() {
                    return Err(HistoryError::InvalidTarget);
                }
                current.len() + 1
            }
            HistoryAction::RedoBatch => session
                .batches()
                .iter()
                .find(|b| b.start_cursor == current.len())
                .map(|b| b.end_cursor)
                .ok_or(HistoryError::InvalidTarget)?,
            HistoryAction::RedoPipeline => {
                if redo.is_empty() {
                    return Err(HistoryError::InvalidTarget);
                }
                if let Some(id) = redo_events
                    .iter()
                    .find(|event| {
                        matches!(
                            event.event,
                            crate::protocol::server::GameEventKind::DecisionResolved
                        ) && event.decision_count == Some(current.len() + 1)
                    })
                    .and_then(|event| event.action_id.as_ref())
                {
                    redo_events
                        .iter()
                        .filter(|event| event.action_id.as_ref() == Some(id))
                        .filter_map(|event| event.decision_count)
                        .max()
                        .ok_or(HistoryError::InvalidTarget)?
                } else {
                    // Older histories lack an action boundary. Continue until the next
                    // action-phase offer after the first redone decision, or the end.
                    let offset = redo.iter().skip(1).position(|record| {
                        record.prompt == "action phase"
                            && record.context.as_ref().is_none_or(|context| {
                                context.phase == ti4_model::state::Phase::Action
                            })
                    });
                    current.len() + offset.map_or(redo.len(), |index| index + 1)
                }
            }
            HistoryAction::Restore { event_id } => {
                if event_id.len() > 128 {
                    return Err(HistoryError::InvalidTarget);
                }
                let mut count = 0;
                let event = events
                    .iter()
                    .find(|event| {
                        if matches!(
                            event.event,
                            crate::protocol::server::GameEventKind::DecisionResolved
                        ) {
                            count += 1;
                        }
                        event.id == event_id
                            && event.visibility.permits(&ViewerRole::Player(host.clone()))
                    })
                    .ok_or(HistoryError::InvalidTarget)?;
                let target = event.decision_count.unwrap_or(count);
                if target >= current.len() {
                    return Err(HistoryError::InvalidTarget);
                }
                target
            }
            HistoryAction::RestoreCursor { cursor } => {
                if cursor >= current.len() {
                    return Err(HistoryError::InvalidTarget);
                }
                cursor
            }
        };
        if target > total {
            return Err(HistoryError::InvalidTarget);
        }
        let all: Vec<_> = current.into_iter().chain(redo).collect();
        let config = session.restart_config();
        drop(state);
        let report = crate::session::replay::replay_session(
            &config.state,
            config.galaxy.as_ref(),
            &all[..target],
        )
        .map_err(|e| HistoryError::Conflict(format!("Replay failed: {e}")))?;
        if !report.hashes_match || report.decision_count != target {
            return Err(HistoryError::Conflict("Replay diverged".to_owned()));
        }
        // This stable boundary excludes all events after the selected decision. Legacy
        // events have no cursor, so their decision_resolved ordering supplies one.
        let all_events: Vec<_> = events.into_iter().chain(redo_events).collect();
        let mut count = 0;
        let mut split = 0;
        for event in &all_events {
            if matches!(
                event.event,
                crate::protocol::server::GameEventKind::DecisionResolved
            ) {
                count += 1;
            }
            if event.decision_count.unwrap_or(count) > target {
                break;
            }
            split += 1;
        }
        let revision = expected_version
            .checked_add(1)
            .ok_or(HistoryError::InvalidTarget)?;
        let history = GameHistory {
            decisions: all[..target].to_vec(),
            redo: all[target..].to_vec(),
            events: all_events[..split].to_vec(),
            redo_events: all_events[split..].to_vec(),
            event_counter: event_counter.max(all_events.len() as u64),
            revision,
            generation: session.history_status().generation.saturating_add(1),
            batches: session.batches(),
        };
        // A live worker must be quiescent before publishing the new authoritative branch.
        let mut state = self.state.lock().expect("registry lock");
        if !state
            .sessions
            .get(game_id)
            .is_some_and(|live| Arc::ptr_eq(live, &session))
            || session.game_version() != expected_version
            || !session.history_ready()
        {
            return Err(HistoryError::Conflict(
                "Game advanced during rewind".to_owned(),
            ));
        }
        session.stop();
        if session.decision_log().len() != original_count {
            // This branch is only reachable for an autonomous bot decision; keep the
            // original timeline alive instead of committing an outdated cursor.
            let replacement = Arc::new(GameSession::start_recovered(
                config.clone(),
                session.decision_log(),
                session.event_log(),
            ));
            state.sessions.insert(game_id.to_owned(), replacement);
            return Err(HistoryError::Conflict(
                "Game advanced during rewind".to_owned(),
            ));
        }
        if let Some(store) = &config.store
            && let Err(error) = store.save_history(game_id, &history)
        {
            let replacement = Arc::new(GameSession::start_recovered(
                config.clone(),
                session.decision_log(),
                session.event_log(),
            ));
            state.sessions.insert(game_id.to_owned(), replacement);
            return Err(HistoryError::Storage(error.to_string()));
        }
        let mut next = config;
        next.prior_decisions.clone_from(&history.decisions);
        next.prior_events.clone_from(&history.events);
        next.redo_decisions = history.redo;
        next.redo_events = history.redo_events;
        next.event_counter = history.event_counter;
        next.initial_version = revision;
        next.history_active = true;
        next.history_generation = history.generation;
        next.batches = history.batches;
        let replacement = Arc::new(GameSession::start(next));
        let replay = replacement.wait_replayed();
        state
            .sessions
            .insert(game_id.to_owned(), replacement.clone());
        replay.map_err(|error| {
            HistoryError::Conflict(format!(
                "history committed, replacement session failed to replay: {error}"
            ))
        })?;
        Ok(replacement.get_snapshot(&ViewerRole::Player(host)))
    }
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_store(mut self, store: Arc<crate::storage::FileGameStore>) -> Self {
        self.store = Some(store);
        self
    }

    #[must_use]
    pub fn with_lease_duration(mut self, lease_duration: Duration) -> Self {
        self.lease_duration =
            lease_duration.clamp(Duration::from_secs(1), Duration::from_secs(300));
        self
    }

    /// Override the display grace for bounded tests. Credentials never expire with presence.
    #[must_use]
    pub fn with_presence_grace(mut self, grace: Duration) -> Self {
        self.presence_grace = grace;
        self
    }

    #[must_use]
    pub fn with_bot_service(mut self, config: BotServiceConfig) -> Self {
        self.bot_config = Some(config);
        self
    }

    pub fn bot_service_enabled(&self) -> bool {
        self.bot_config.is_some()
    }

    pub fn verify_bot_password(&self, password: &str) -> bool {
        use sha2::{Digest, Sha256};
        if let Some(ref config) = self.bot_config {
            let input_hash = Sha256::digest(password.as_bytes());
            let expected_hash = Sha256::digest(config.password.as_bytes());
            input_hash == expected_hash
        } else {
            false
        }
    }

    fn prune_dead_bots(&self) {
        if let Ok(mut bots) = self.active_bots.lock() {
            for bot_list in bots.values_mut() {
                bot_list.retain_mut(|b| {
                    match b.child.try_wait() {
                        Ok(Some(_)) => false, // exited
                        Ok(None) => true,     // still running
                        Err(_) => false,
                    }
                });
            }
            bots.retain(|_, v| !v.is_empty());
        }
    }

    fn player_view(&self, state: &RegistryState, record: &PlayerLobbyRecord) -> PlayerLobbyView {
        let mut view = record.public_view();
        view.bot_service_enabled = self.bot_config.is_some();
        for slot in &mut view.slots {
            if let Some(player) = &slot.occupant {
                slot.connected = state
                    .presence
                    .get(&(record.game_id.clone(), player.clone()))
                    .is_some_and(|presence| {
                        presence
                            .last_heartbeat
                            .is_some_and(|at| at.elapsed() < self.presence_grace)
                            || presence
                                .connections
                                .values()
                                .any(|at| at.elapsed() < self.presence_grace)
                    });
                slot.can_take_over =
                    !slot.connected && self.disconnected_since(state, &record.game_id, player);
            }
        }
        view
    }

    /// Whether a disconnected player has passed the takeover display grace.
    #[must_use]
    pub fn player_disconnected(&self, game_id: &str, player: &PlayerId) -> bool {
        let state = self.state.lock().expect("registry lock");
        if !state
            .player_lobbies
            .get(game_id)
            .is_some_and(|l| l.players.contains_key(player))
        {
            return false;
        }
        self.disconnected_since(&state, game_id, player)
    }

    fn disconnected_since(&self, state: &RegistryState, game_id: &str, player: &PlayerId) -> bool {
        let presence = state.presence.get(&(game_id.to_owned(), player.clone()));
        let last = presence.and_then(|p| {
            p.last_heartbeat
                .into_iter()
                .chain(p.connections.values().copied())
                .max()
        });
        last.unwrap_or(presence.map_or(self.started_at, |p| p.admitted_at))
            .elapsed()
            >= self.presence_grace
    }

    /// Authenticates without changing credential validity or persisted state.
    pub fn authenticate_player_session(
        &self,
        game_id: &str,
        credential: &str,
    ) -> Result<PlayerId, LobbyError> {
        let state = self.state.lock().expect("registry lock");
        if let Some(lobby) = state.player_lobbies.get(game_id) {
            return authenticate_player(lobby, credential);
        }
        state
            .lobbies
            .get(game_id)
            .and_then(|l| authenticated_seat(l, credential).ok())
            .ok_or(LobbyError::InvalidCapability)
    }

    /// Records a credential-authenticated HTTP heartbeat without touching the lobby version.
    pub fn player_heartbeat(
        &self,
        game_id: &str,
        credential: &str,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get(game_id)
            .ok_or(LobbyError::NotFound)?;
        let player = authenticate_player(lobby, credential)?;
        state
            .presence
            .entry((game_id.to_owned(), player))
            .or_default()
            .last_heartbeat = Some(Instant::now());
        Ok(self.player_view(
            &state,
            state
                .player_lobbies
                .get(game_id)
                .expect("authenticated lobby"),
        ))
    }

    /// Register a WS connection; only application pings refresh it.
    pub fn connect_player(
        &self,
        game_id: &str,
        credential: &str,
    ) -> Result<(PlayerId, u64), LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let player = if let Some(lobby) = state.player_lobbies.get(game_id) {
            authenticate_player(lobby, credential)?
        } else {
            authenticated_seat(
                state.lobbies.get(game_id).ok_or(LobbyError::NotFound)?,
                credential,
            )?
        };
        state.next_connection_id += 1;
        let connection = state.next_connection_id;
        state
            .presence
            .entry((game_id.to_owned(), player.clone()))
            .or_default()
            .connections
            .insert(connection, Instant::now());
        Ok((player, connection))
    }

    pub fn ping_player(
        &self,
        game_id: &str,
        credential: &str,
        connection: u64,
    ) -> Result<(), LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let player = if let Some(lobby) = state.player_lobbies.get(game_id) {
            authenticate_player(lobby, credential)?
        } else {
            authenticated_seat(
                state.lobbies.get(game_id).ok_or(LobbyError::NotFound)?,
                credential,
            )?
        };
        let at = state
            .presence
            .get_mut(&(game_id.to_owned(), player))
            .and_then(|p| p.connections.get_mut(&connection))
            .ok_or(LobbyError::InvalidCapability)?;
        *at = Instant::now();
        Ok(())
    }

    pub fn disconnect_player(&self, game_id: &str, player: &PlayerId, connection: u64) {
        let mut state = self.state.lock().expect("registry lock");
        if let Some(presence) = state
            .presence
            .get_mut(&(game_id.to_owned(), player.clone()))
        {
            if presence.connections.remove(&connection).is_some() {
                presence.last_heartbeat = Some(Instant::now());
            }
        }
    }

    /// Returns the optional underlying game store.
    #[must_use]
    pub fn store(&self) -> Option<Arc<crate::storage::FileGameStore>> {
        self.store.clone()
    }

    /// Recovers both unstarted lobbies and started sessions from durable storage.
    pub fn recover_all_games(&self) -> Result<Vec<String>, StorageError> {
        let mut report = self.recover_all_games_report()?;
        if !report.failed.is_empty() {
            return Err(report.failed.remove(0).error);
        }
        Ok(report.recovered)
    }

    /// Recover each saved game independently, reporting invalid saves without loading them.
    pub fn recover_all_games_report(&self) -> Result<RecoveryReport, StorageError> {
        let Some(store) = &self.store else {
            return Ok(RecoveryReport::default());
        };
        let game_ids: BTreeSet<_> = store.list_saved_games()?.into_iter().collect();
        let lobby_ids: BTreeSet<_> = store.list_saved_lobbies()?.into_iter().collect();
        let mut state = self.state.lock().expect("registry lock");
        let mut report = RecoveryReport::default();

        for game_id in game_ids.union(&lobby_ids) {
            if state.lobbies.contains_key(game_id)
                || state.sessions.contains_key(game_id)
                || state.player_lobbies.contains_key(game_id)
            {
                continue;
            }
            match Self::recover_one_game(
                store,
                &mut state,
                game_id,
                lobby_ids.contains(game_id),
                game_ids.contains(game_id),
            ) {
                Ok(()) => report.recovered.push(game_id.clone()),
                Err((stage, error)) => report.failed.push(RecoveryFailure {
                    game_id: game_id.clone(),
                    stage,
                    error,
                }),
            }
        }
        Ok(report)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "all recovery branches share one atomic registry insertion boundary"
    )]
    fn recover_one_game(
        store: &Arc<crate::storage::FileGameStore>,
        state: &mut RegistryState,
        game_id: &str,
        has_lobby: bool,
        has_init: bool,
    ) -> Result<(), (&'static str, StorageError)> {
        if has_lobby {
            // A versioned player lobby is private; never deserialize it as a v1 seat lobby.
            let path = store
                .game_dir(game_id)
                .map_err(|e| ("lobby.json", e))?
                .join("lobby.json");
            if std::fs::metadata(&path)
                .map_err(|e| ("lobby.json", e.into()))?
                .len()
                > 64 * 1024
            {
                return Err((
                    "lobby.json",
                    StorageError::Oversized {
                        path,
                        limit: 64 * 1024,
                    },
                ));
            }
            let marker: serde_json::Value = serde_json::from_slice(
                &std::fs::read(&path).map_err(|e| ("lobby.json", e.into()))?,
            )
            .map_err(|e| ("lobby.json", e.into()))?;
            if marker
                .get("payload")
                .and_then(|v| v.get("schema_version"))
                .is_some()
            {
                let mut record = store
                    .load_player_lobby(game_id)
                    .map_err(|e| ("lobby.json", e))?
                    .expect("listed lobby exists");
                let session = if has_init {
                    let init = store
                        .load_player_init(game_id)
                        .map_err(|e| ("init.json", e))?;
                    let credentials = store
                        .load_player_sessions(game_id)
                        .map_err(|e| ("player_sessions.json", e))?;
                    if init.player_ids
                        != record
                            .slots
                            .iter()
                            .filter_map(|slot| slot.occupant.clone())
                            .collect::<Vec<_>>()
                        || credentials.sessions.len() != record.players.len()
                        || credentials.sessions.keys().ne(record.players.keys())
                    {
                        return Err((
                            "player_sessions.json",
                            StorageError::InvalidPlayerRecord("started roster"),
                        ));
                    }
                    // The session record is authoritative after start; the lobby file
                    // may predate a successful running-game credential rotation.
                    for (id, session) in credentials.sessions {
                        let member = record.players.get_mut(&id).expect("validated roster");
                        member.session = session;
                        member.nickname.clone_from(&credentials.nicknames[&id]);
                    }
                    let session = Arc::new(
                        store
                            .recover_player_session(game_id)
                            .map_err(|e| ("session replay", e))?,
                    );
                    record.phase = PersistedLobbyPhase::Running;
                    Some(session)
                } else {
                    record.phase = PersistedLobbyPhase::Lobby;
                    None
                };
                if let Some(session) = session {
                    let legacy_lobby = running_lobby_from_session(&session);
                    state.lobbies.insert(game_id.to_owned(), legacy_lobby);
                    state.sessions.insert(game_id.to_owned(), session);
                }
                state.player_lobbies.insert(game_id.to_owned(), record);
                return Ok(());
            }
            let record = store
                .load_lobby(game_id)
                .map_err(|e| ("lobby.json", e))?
                .expect("listed lobby exists");
            let mut lobby = lobby_from_record(record);
            let session = if has_init {
                let session = Arc::new(
                    store
                        .recover_session(game_id)
                        .map_err(|e| ("session replay", e))?,
                );
                lobby.phase = LobbyPhase::Running;
                Some(session)
            } else {
                // A crash between marking the lobby running and writing init.json remains a lobby.
                lobby.phase = LobbyPhase::Lobby;
                None
            };
            if let Some(session) = session {
                state.sessions.insert(game_id.to_owned(), session);
            }
            state.lobbies.insert(game_id.to_owned(), lobby);
            return Ok(());
        }

        let init = store.load_init(game_id).map_err(|e| ("init.json", e))?;
        let session = Arc::new(
            store
                .recover_session(game_id)
                .map_err(|e| ("session replay", e))?,
        );
        state
            .lobbies
            .insert(game_id.to_owned(), legacy_running_lobby(&init));
        state.sessions.insert(game_id.to_owned(), session);
        Ok(())
    }

    /// Creates the private v2 lobby before delivering its first credential.
    pub fn create_player_lobby(
        &self,
        game_id: String,
        count: usize,
        seed: u64,
        nickname: &str,
    ) -> Result<(PlayerLobbyView, PlayerId, PlayerSession), LobbyError> {
        check_nickname(nickname)?;
        let mut state = self.state.lock().expect("registry lock");
        if state.lobbies.contains_key(&game_id)
            || state.player_lobbies.contains_key(&game_id)
            || state.sessions.contains_key(&game_id)
            || self
                .store
                .as_ref()
                .is_some_and(|store| store.game_dir(&game_id).is_ok_and(|dir| dir.exists()))
        {
            return Err(LobbyError::SeatUnavailable);
        }
        let (record, player, credential) =
            PlayerLobbyRecord::create(game_id.clone(), count, seed, nickname)
                .map_err(|error| LobbyError::Storage(error.to_string()))?;
        self.save_player_lobby(&record)?;
        let view = record.public_view();
        state
            .presence
            .insert((game_id.clone(), player.clone()), PlayerPresence::default());
        state.player_lobbies.insert(game_id, record);
        Ok((view, player, credential))
    }

    fn save_player_lobby(&self, record: &PlayerLobbyRecord) -> Result<(), LobbyError> {
        if let Some(store) = &self.store {
            store
                .save_player_lobby(record)
                .map_err(|e| LobbyError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    /// Read-only spectator projection; an invalid supplied credential fails closed.
    pub fn player_lobby_status(
        &self,
        game_id: &str,
        credential: Option<&str>,
    ) -> Result<(PlayerLobbyView, Option<PlayerId>), LobbyError> {
        let state = self.state.lock().expect("registry lock");
        let record = state
            .player_lobbies
            .get(game_id)
            .ok_or(LobbyError::NotFound)?;
        let viewer = credential
            .map(|value| authenticate_player(record, value))
            .transpose()?;
        Ok((self.player_view(&state, record), viewer))
    }

    /// Serializes first-open admission and credential-bearing reconnects.
    pub fn join_player_lobby(
        &self,
        game_id: &str,
        credential: Option<&str>,
        nickname: Option<&str>,
    ) -> Result<(PlayerLobbyView, PlayerId, Option<PlayerSession>), LobbyError> {
        if credential.is_some() && nickname.is_some() {
            return Err(LobbyError::InvalidNickname);
        }
        let nickname = if credential.is_none() {
            let name = nickname.ok_or(LobbyError::InvalidNickname)?;
            check_nickname(name)?;
            Some(name)
        } else {
            None
        };
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if let Some(credential) = credential {
            let player = authenticate_player(lobby, credential)?;
            return Ok((
                self.player_view(&state, &state.player_lobbies[game_id]),
                player,
                None,
            ));
        }
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        let slot = lobby
            .slots
            .iter()
            .position(|slot| slot.occupant.is_none())
            .ok_or(LobbyError::SeatUnavailable)?;
        let mut updated = lobby.clone();
        let player = generate_player_id(&updated.players);
        let session = loop {
            let candidate = PlayerSession::generate();
            if updated
                .players
                .values()
                .all(|member| member.session != candidate)
            {
                break candidate;
            }
        };
        updated.slots[slot].occupant = Some(player.clone());
        updated.players.insert(
            player.clone(),
            PlayerLobbyMember {
                ready: false,
                session: session.clone(),
                nickname: nickname.expect("new admission has nickname").to_owned(),
            },
        );
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        state.presence.insert(
            (game_id.to_owned(), player.clone()),
            PlayerPresence::default(),
        );
        Ok((
            self.player_view(&state, &state.player_lobbies[game_id]),
            player,
            Some(session),
        ))
    }

    /// Replace an existing disconnected player's credential under the registry lock.
    /// The running session record is the sole durable credential authority after start.
    pub fn take_over_player(
        &self,
        game_id: &str,
        player: &PlayerId,
        nickname: &str,
    ) -> Result<(PlayerLobbyView, PlayerSession), LobbyError> {
        check_nickname(nickname)?;
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !lobby.players.contains_key(player) {
            return Err(LobbyError::InvalidPlayerId);
        }
        if !self.disconnected_since(&state, game_id, player) {
            return Err(LobbyError::TakeoverUnavailable);
        }
        if state
            .sessions
            .get(game_id)
            .is_some_and(|session| session.choice_in_flight())
        {
            return Err(LobbyError::TakeoverUnavailable);
        }
        let mut updated = lobby.clone();
        let replacement = loop {
            let candidate = PlayerSession::generate();
            if updated
                .players
                .values()
                .all(|member| member.session != candidate)
            {
                break candidate;
            }
        };
        let member = updated.players.get_mut(player).expect("validated player");
        member.session = replacement.clone();
        member.nickname = nickname.to_owned();
        updated.lobby_version += 1;
        if matches!(updated.phase, PersistedLobbyPhase::Running) {
            if !state.sessions.contains_key(game_id) {
                return Err(LobbyError::NotFound);
            }
            if let Some(store) = &self.store {
                store
                    .save_player_sessions(&PlayerSessionsRecord {
                        schema_version: PLAYER_RECORD_VERSION,
                        game_id: game_id.to_owned(),
                        sessions: updated
                            .players
                            .iter()
                            .map(|(id, member)| (id.clone(), member.session.clone()))
                            .collect(),
                        nicknames: updated
                            .players
                            .iter()
                            .map(|(id, member)| (id.clone(), member.nickname.clone()))
                            .collect(),
                    })
                    .map_err(|e| LobbyError::Storage(e.to_string()))?;
            }
        } else {
            self.save_player_lobby(&updated)?;
        }
        if let Some(session) = state.sessions.get(game_id) {
            session.replace_player_session(player, replacement.as_str());
        }
        state.player_lobbies.insert(game_id.to_owned(), updated);
        // Discard all old connection IDs and heartbeat timestamps. The new holder
        // receives a fresh absence grace even before its first connection.
        state.presence.insert(
            (game_id.to_owned(), player.clone()),
            PlayerPresence::default(),
        );
        Ok((
            self.player_view(&state, &state.player_lobbies[game_id]),
            replacement,
        ))
    }

    /// Serialize the full choice submission with takeover so a choice authorized
    /// before rotation cannot be committed after it.
    pub fn submit_player_choice(
        &self,
        game_id: &str,
        credential: &str,
        player: &PlayerId,
        session: &GameSession,
        nonce: &str,
        version: u64,
        option: &str,
    ) -> Result<ActionAcceptedMsg, RejectionReason> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        let state = self.state.lock().expect("registry lock");
        let authenticated = if let Some(lobby) = state.player_lobbies.get(game_id) {
            authenticate_player(lobby, credential).ok()
        } else {
            state
                .lobbies
                .get(game_id)
                .and_then(|lobby| authenticated_seat(lobby, credential).ok())
        };
        if authenticated.as_ref() != Some(player) {
            return Err(RejectionReason::UnauthorizedSeat {
                seat: Some(player.clone()),
            });
        }
        // Reserve under the gate. A nested reaction can be offered before the
        // worker returns from this step, so its next choice must be able to reserve too.
        if !state.sessions.get(game_id).is_some_and(|current| {
            std::ptr::eq(Arc::as_ptr(current), session as *const GameSession)
        }) {
            return Err(RejectionReason::NoPendingChoice);
        }
        drop(state);
        let reply = session.reserve_choice(player, nonce, version, option)?;
        drop(_reservation);
        reply
            .recv()
            .unwrap_or(Err(RejectionReason::NoPendingChoice))
    }

    /// Bind a private HTTP snapshot to the current credential under the same
    /// lock used to commit a takeover.
    pub fn player_snapshot(
        &self,
        game_id: &str,
        credential: Option<&str>,
        _session: &GameSession,
    ) -> Result<InitialSnapshotMsg, LobbyError> {
        let state = self.state.lock().expect("registry lock");
        let role = match credential {
            Some(token) => {
                let player = if let Some(lobby) = state.player_lobbies.get(game_id) {
                    authenticate_player(lobby, token)?
                } else {
                    authenticated_seat(
                        state.lobbies.get(game_id).ok_or(LobbyError::NotFound)?,
                        token,
                    )?
                };
                ViewerRole::Player(player)
            }
            None => ViewerRole::Spectator,
        };
        Ok(state
            .sessions
            .get(game_id)
            .ok_or(LobbyError::NotFound)?
            .get_snapshot(&role))
    }

    /// A leaving player retires their identity and credential, never the host's.
    pub fn leave_player_lobby(
        &self,
        game_id: &str,
        credential: &str,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        let player = authenticate_player(lobby, credential)?;
        if player == lobby.host_player_id {
            return Err(LobbyError::HostRequired);
        }
        let mut updated = lobby.clone();
        updated.players.remove(&player);
        updated
            .slots
            .iter_mut()
            .find(|slot| slot.occupant.as_ref() == Some(&player))
            .expect("validated occupant")
            .occupant = None;
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        state.presence.remove(&(game_id.to_owned(), player));
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Reorder entire slots, including open ones, without changing participants.
    /// Admission and start use the same registry lock, so the next open slot is
    /// always selected from one complete, persisted order.
    pub fn reorder_player_lobby(
        &self,
        game_id: &str,
        credential: &str,
        slot_ids: &[LobbySlotId],
    ) -> Result<PlayerLobbyView, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        if authenticate_player(lobby, credential)? != lobby.host_player_id {
            return Err(LobbyError::HostRequired);
        }
        let by_id: BTreeMap<_, _> = lobby
            .slots
            .iter()
            .map(|slot| (slot.slot_id.clone(), slot.clone()))
            .collect();
        if slot_ids.len() != lobby.slots.len()
            || slot_ids.iter().collect::<BTreeSet<_>>().len() != lobby.slots.len()
            || slot_ids.iter().any(|id| !by_id.contains_key(id))
        {
            return Err(LobbyError::InvalidSlotOrder);
        }
        let mut updated = lobby.clone();
        updated.slots = slot_ids.iter().map(|id| by_id[id].clone()).collect();
        if updated
            .slots
            .iter()
            .map(|s| &s.slot_id)
            .ne(lobby.slots.iter().map(|s| &s.slot_id))
        {
            updated.lobby_version += 1;
            self.save_player_lobby(&updated)?;
            *lobby = updated;
        }
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    pub fn set_player_ready(
        &self,
        game_id: &str,
        credential: &str,
        ready: bool,
    ) -> Result<(PlayerLobbyView, PlayerId), LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        let player = authenticate_player(lobby, credential)?;
        if lobby.players[&player].ready != ready {
            let mut updated = lobby.clone();
            updated
                .players
                .get_mut(&player)
                .expect("authenticated player")
                .ready = ready;
            updated.lobby_version += 1;
            self.save_player_lobby(&updated)?;
            *lobby = updated;
        }
        Ok((
            self.player_view(&state, &state.player_lobbies[game_id]),
            player,
        ))
    }

    pub fn start_player_lobby(
        &self,
        game_id: &str,
        credential: &str,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        if authenticate_player(lobby, credential)? != lobby.host_player_id {
            return Err(LobbyError::HostRequired);
        }
        if lobby.slots.iter().any(|slot| slot.occupant.is_none())
            || lobby.players.values().any(|member| !member.ready)
        {
            return Err(LobbyError::HumansNotReady);
        }
        let players: Vec<_> = lobby
            .slots
            .iter()
            .map(|slot| slot.occupant.clone().expect("full lobby"))
            .collect();
        let content = ContentStore::embedded();
        let (initial_state, galaxy) =
            crate::map::create_game_with_map(content, &players, lobby.seed)
                .map_err(|e| LobbyError::Map(e.to_string()))?;
        let map_tiles = crate::map::build_board_tiles(content, &galaxy);
        let mut config = SessionConfig::new(game_id, initial_state.clone())
            .with_seed(lobby.seed)
            .with_player_ids(players.clone())
            .with_galaxy(galaxy, map_tiles.clone());
        config.seats = players
            .iter()
            .map(|p| (p.clone(), SeatController::Human))
            .collect();
        config.seat_tokens = lobby
            .players
            .iter()
            .map(|(p, m)| (p.clone(), m.session.as_str().to_owned()))
            .collect();
        config.store.clone_from(&self.store);
        let mut running = lobby.clone();
        running.phase = PersistedLobbyPhase::Running;
        running.lobby_version += 1;
        if let Some(store) = &self.store {
            store
                .save_player_sessions(&PlayerSessionsRecord {
                    schema_version: PLAYER_RECORD_VERSION,
                    game_id: game_id.to_owned(),
                    sessions: lobby
                        .players
                        .iter()
                        .map(|(p, m)| (p.clone(), m.session.clone()))
                        .collect(),
                    nicknames: lobby
                        .players
                        .iter()
                        .map(|(id, member)| (id.clone(), member.nickname.clone()))
                        .collect(),
                })
                .map_err(|e| LobbyError::Storage(e.to_string()))?;
            self.save_player_lobby(&running)?;
            if let Err(e) = store.save_player_init(&PlayerGameInitRecord {
                schema_version: PLAYER_RECORD_VERSION,
                game_id: game_id.to_owned(),
                seed: lobby.seed,
                player_ids: players,
                initial_state,
                map_tiles,
                seats: Some(config.seats.clone()),
            }) {
                // No valid init: recovery treats the lobby as unstarted.
                let _ = self.save_player_lobby(lobby);
                return Err(LobbyError::Storage(e.to_string()));
            }
        }
        *lobby = running;
        let view = self.player_view(&state, &state.player_lobbies[game_id]);
        state
            .sessions
            .insert(game_id.to_owned(), Arc::new(GameSession::start(config)));
        Ok(view)
    }

    pub async fn spawn_bot_for_lobby(
        &self,
        game_id: &str,
        host_credential: &str,
        nickname: Option<String>,
        temperature: Option<f64>,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let config = self.bot_config.as_ref().ok_or_else(|| {
            LobbyError::Storage("Bot service is not enabled on this server".to_owned())
        })?;

        let (server_port, default_nick) = {
            let state = self.state.lock().expect("registry lock");
            let lobby = state
                .player_lobbies
                .get(game_id)
                .ok_or(LobbyError::NotFound)?;
            if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
                return Err(LobbyError::AlreadyRunning);
            }
            if authenticate_player(lobby, host_credential)? != lobby.host_player_id {
                return Err(LobbyError::HostRequired);
            }
            let position = lobby
                .slots
                .iter()
                .position(|s| s.occupant.is_none())
                .map(|idx| idx + 1)
                .ok_or(LobbyError::SeatUnavailable)?;
            (config.server_port, format!("Bot {position}"))
        };

        self.prune_dead_bots();
        {
            let bots_guard = self.active_bots.lock().expect("active bots lock");
            let total_active: usize = bots_guard.values().map(Vec::len).sum();
            if total_active >= config.max_active_bots {
                return Err(LobbyError::SeatUnavailable);
            }
        }

        let chosen_nick = nickname.unwrap_or(default_nick);
        check_nickname(&chosen_nick)?;

        let server_url = format!("ws://127.0.0.1:{server_port}");
        let temp_val = temperature.unwrap_or(0.25);
        if !temp_val.is_finite() || temp_val <= 0.0 {
            return Err(LobbyError::InvalidNickname);
        }

        let child = tokio::process::Command::new(&config.bot_agent_bin)
            .arg("--server")
            .arg(&server_url)
            .arg("--game")
            .arg(game_id)
            .arg("--advisor")
            .arg(&config.advisor_url)
            .arg("--nickname")
            .arg(&chosen_nick)
            .arg("--temperature")
            .arg(temp_val.to_string())
            .spawn()
            .map_err(|e| LobbyError::Storage(format!("Cannot spawn bot agent: {e}")))?;

        {
            let mut bots_guard = self.active_bots.lock().expect("active bots lock");
            bots_guard
                .entry(game_id.to_owned())
                .or_default()
                .push(BotChild {
                    child,
                    nickname: chosen_nick.clone(),
                });
        }

        let deadline = Instant::now() + Duration::from_millis(1500);
        while Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
            let state = self.state.lock().expect("registry lock");
            if let Some(lobby) = state.player_lobbies.get(game_id) {
                if lobby.slots.iter().any(|s| {
                    s.occupant.as_ref().is_some_and(|id| {
                        lobby.players.get(id).map(|p| &p.nickname) == Some(&chosen_nick)
                    })
                }) {
                    return Ok(self.player_view(&state, lobby));
                }
            }
        }

        let state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get(game_id)
            .ok_or(LobbyError::NotFound)?;
        Ok(self.player_view(&state, lobby))
    }

    pub fn remove_bot_or_player_from_lobby(
        &self,
        game_id: &str,
        host_credential: &str,
        player_id: &PlayerId,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        if authenticate_player(lobby, host_credential)? != lobby.host_player_id {
            return Err(LobbyError::HostRequired);
        }
        if player_id == &lobby.host_player_id {
            return Err(LobbyError::HostRequired);
        }
        let nickname = lobby.players.get(player_id).map(|p| p.nickname.clone());

        let mut updated = lobby.clone();
        updated.players.remove(player_id);
        if let Some(slot) = updated
            .slots
            .iter_mut()
            .find(|slot| slot.occupant.as_ref() == Some(player_id))
        {
            slot.occupant = None;
        } else {
            return Err(LobbyError::NotFound);
        }
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        state
            .presence
            .remove(&(game_id.to_owned(), player_id.clone()));

        if let Some(nick) = nickname {
            let mut bots_guard = self.active_bots.lock().expect("active bots lock");
            if let Some(bot_list) = bots_guard.get_mut(game_id) {
                if let Some(pos) = bot_list.iter().position(|b| b.nickname == nick) {
                    let mut b = bot_list.remove(pos);
                    let _ = b.child.start_kill();
                }
            }
        }

        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Creates a pre-game lobby and persists it before returning capabilities.
    pub fn create_lobby(&self, config: LobbyConfig) -> Result<CreatedLobby, String> {
        crate::storage::validate_game_id(&config.game_id).map_err(|error| error.to_string())?;
        let mut state = self.state.lock().expect("registry lock");
        if state.sessions.contains_key(&config.game_id)
            || state.lobbies.contains_key(&config.game_id)
        {
            return Err(format!("Game session '{}' already exists", config.game_id));
        }

        let mut seats = BTreeMap::new();
        let creator_token = new_token();
        let lease_expires_at_ms = Some(self.lease_expiry_ms());
        for player_id in &config.player_ids {
            let controller = config
                .seats
                .get(player_id)
                .expect("validated lobby seat")
                .clone();
            let seat_token = (player_id == &config.host_seat).then(|| creator_token.clone());
            seats.insert(
                player_id.clone(),
                LobbySeat {
                    controller,
                    ready: false,
                    seat_token,
                    lease_expires_at_ms: if player_id == &config.host_seat {
                        lease_expires_at_ms
                    } else {
                        None
                    },
                },
            );
        }
        let lobby = LobbyState {
            game_id: config.game_id.clone(),
            phase: LobbyPhase::Lobby,
            host_seat: config.host_seat.clone(),
            player_ids: config.player_ids,
            seats,
            seed: config.seed,
            lobby_version: 1,
        };
        if let Some(store) = &self.store {
            store
                .save_lobby(&lobby_to_record(&lobby))
                .map_err(|error| format!("Failed to save lobby: {error}"))?;
        }
        state.lobbies.insert(lobby.game_id.clone(), lobby.clone());
        Ok(CreatedLobby {
            lobby,
            creator_token: creator_token.clone(),
            seat_tokens: BTreeMap::from([(config.host_seat, creator_token)]),
        })
    }

    /// Returns public lobby state and authenticates an optional capability.
    pub fn lobby_status(
        &self,
        game_id: &str,
        token: Option<&str>,
    ) -> Result<LobbyStatus, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state.lobbies.get_mut(game_id).ok_or(LobbyError::NotFound)?;
        self.expire_claims(lobby)?;
        let viewer = match token {
            Some(token) => lobby
                .seats
                .iter()
                .find_map(|(seat, lobby_seat)| {
                    (lobby_seat.seat_token.as_deref() == Some(token)).then(|| seat.clone())
                })
                .ok_or(LobbyError::InvalidCapability)?,
            None => {
                return Ok(LobbyStatus {
                    lobby: lobby.clone(),
                    viewer: None,
                });
            }
        };
        Ok(LobbyStatus {
            lobby: lobby.clone(),
            viewer: Some(viewer),
        })
    }

    /// Atomically claims an available human seat and returns its new bearer credential.
    pub fn claim_seat(
        &self,
        game_id: &str,
        requested_seat: &str,
    ) -> Result<(LobbyStatus, String), LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state.lobbies.get_mut(game_id).ok_or(LobbyError::NotFound)?;
        self.expire_claims(lobby)?;
        let seat_id = PlayerId::new(requested_seat);
        let seat = lobby
            .seats
            .get(&seat_id)
            .ok_or(LobbyError::SeatUnavailable)?;
        if seat.controller != SeatController::Human || seat.seat_token.is_some() {
            return Err(LobbyError::SeatUnavailable);
        }
        let token = new_token();
        let mut updated = lobby.clone();
        let claimed = updated.seats.get_mut(&seat_id).expect("validated seat");
        claimed.seat_token = Some(token.clone());
        claimed.lease_expires_at_ms = Some(self.lease_expiry_ms());
        claimed.ready = false;
        updated.lobby_version += 1;
        self.save_lobby(&updated)?;
        *lobby = updated.clone();
        Ok((
            LobbyStatus {
                lobby: updated,
                viewer: Some(seat_id),
            },
            token,
        ))
    }

    /// Authenticates and renews a current claim. All HTTP and WebSocket authorization enters here.
    pub fn authenticate_and_renew(
        &self,
        game_id: &str,
        token: &str,
    ) -> Result<PlayerId, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        if let Some(lobby) = state.player_lobbies.get(game_id) {
            // Until PIL-03 replaces the WebSocket field, the WS authentication
            // boundary still calls this method. Never renew or expire v2 credentials.
            return authenticate_player(lobby, token);
        }
        let lobby = state.lobbies.get_mut(game_id).ok_or(LobbyError::NotFound)?;
        self.expire_claims(lobby)?;
        let viewer = authenticated_seat(lobby, token)?;
        let mut updated = lobby.clone();
        updated
            .seats
            .get_mut(&viewer)
            .expect("authenticated seat")
            .lease_expires_at_ms = Some(self.lease_expiry_ms());
        self.save_lobby(&updated)?;
        *lobby = updated;
        Ok(viewer)
    }

    /// Updates only the authenticated human seat's readiness while still in the lobby.
    pub fn set_ready(
        &self,
        game_id: &str,
        token: &str,
        ready: bool,
    ) -> Result<LobbyStatus, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state.lobbies.get_mut(game_id).ok_or(LobbyError::NotFound)?;
        self.expire_claims(lobby)?;
        if lobby.phase != LobbyPhase::Lobby {
            return Err(LobbyError::AlreadyRunning);
        }
        let viewer = authenticated_seat(lobby, token)?;
        let seat = lobby.seats.get(&viewer).expect("authenticated seat exists");
        if seat.controller != SeatController::Human {
            return Err(LobbyError::HumanSeatRequired);
        }
        if seat.ready != ready {
            let mut updated = lobby.clone();
            updated
                .seats
                .get_mut(&viewer)
                .expect("authenticated seat exists")
                .ready = ready;
            updated.lobby_version += 1;
            self.save_lobby(&updated)?;
            *lobby = updated;
        }
        Ok(LobbyStatus {
            lobby: lobby.clone(),
            viewer: Some(viewer),
        })
    }

    /// Starts exactly one game after the authenticated host has all human seats ready.
    pub fn start_lobby(&self, game_id: &str, token: &str) -> Result<Arc<GameSession>, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state.lobbies.get_mut(game_id).ok_or(LobbyError::NotFound)?;
        self.expire_claims(lobby)?;
        if lobby.phase != LobbyPhase::Lobby {
            return Err(LobbyError::AlreadyRunning);
        }
        let viewer = authenticated_seat(lobby, token)?;
        if viewer != lobby.host_seat {
            return Err(LobbyError::HostRequired);
        }
        if lobby
            .seats
            .values()
            .any(|seat| seat.controller == SeatController::Human && !seat.ready)
        {
            return Err(LobbyError::HumansNotReady);
        }

        let content = ContentStore::embedded();
        let (initial_state, galaxy) =
            crate::map::create_game_with_map(content, &lobby.player_ids, lobby.seed)
                .map_err(|error| LobbyError::Map(error.to_string()))?;
        let map_tiles = crate::map::build_board_tiles(content, &galaxy);
        let mut config = SessionConfig::new(&lobby.game_id, initial_state.clone())
            .with_seed(lobby.seed)
            .with_player_ids(lobby.player_ids.clone())
            .with_galaxy(galaxy, map_tiles);
        config.seats = lobby
            .seats
            .iter()
            .map(|(seat, lobby_seat)| (seat.clone(), lobby_seat.controller.clone()))
            .collect();
        config.seat_tokens = lobby
            .seats
            .iter()
            .filter_map(|(seat, lobby_seat)| {
                lobby_seat
                    .seat_token
                    .as_ref()
                    .map(|token| (seat.clone(), token.clone()))
            })
            .collect();
        config.store.clone_from(&self.store);

        // Record Running before init; recovery treats a Running record without init as Lobby.
        lobby.phase = LobbyPhase::Running;
        lobby.lobby_version += 1;
        if let Some(store) = &self.store {
            if let Err(error) = store.save_lobby(&lobby_to_record(lobby)) {
                lobby.phase = LobbyPhase::Lobby;
                lobby.lobby_version -= 1;
                return Err(LobbyError::Storage(error.to_string()));
            }
            let init = GameInitRecord {
                game_id: config.game_id.clone(),
                seed: config.seed,
                player_ids: config.player_ids.clone(),
                initial_state,
                seats: config.seats.clone(),
                seat_tokens: config.seat_tokens.clone(),
                map_tiles: config.map_tiles.clone(),
            };
            if let Err(error) = store.save_init(&init) {
                lobby.phase = LobbyPhase::Lobby;
                lobby.lobby_version -= 1;
                let _ = store.save_lobby(&lobby_to_record(lobby));
                return Err(LobbyError::Storage(error.to_string()));
            }
        }
        let session = Arc::new(GameSession::start(config));
        state.sessions.insert(game_id.to_owned(), session.clone());
        Ok(session)
    }

    fn lease_expiry_ms(&self) -> u64 {
        now_ms().saturating_add(
            self.lease_duration
                .as_millis()
                .try_into()
                .unwrap_or(u64::MAX),
        )
    }

    fn save_lobby(&self, lobby: &LobbyState) -> Result<(), LobbyError> {
        if let Some(store) = &self.store {
            store
                .save_lobby(&lobby_to_record(lobby))
                .map_err(|error| LobbyError::Storage(error.to_string()))?;
        }
        Ok(())
    }

    fn expire_claims(&self, lobby: &mut LobbyState) -> Result<(), LobbyError> {
        let now = now_ms();
        let mut updated = lobby.clone();
        let mut changed = false;
        for seat in updated.seats.values_mut() {
            if seat.controller == SeatController::Human
                && seat.lease_expires_at_ms.is_some_and(|expiry| expiry <= now)
            {
                seat.seat_token = None;
                seat.lease_expires_at_ms = None;
                seat.ready = false;
                changed = true;
            }
        }
        if changed {
            updated.lobby_version += 1;
            self.save_lobby(&updated)?;
            *lobby = updated;
        }
        Ok(())
    }

    /// Creates and starts a legacy active session. New HTTP games use [`Self::create_lobby`].
    pub fn create_game(&self, mut config: SessionConfig) -> Result<Arc<GameSession>, String> {
        crate::storage::validate_game_id(&config.game_id).map_err(|error| error.to_string())?;
        let mut state = self.state.lock().expect("registry lock");
        if state.sessions.contains_key(&config.game_id)
            || state.lobbies.contains_key(&config.game_id)
        {
            return Err(format!("Game session '{}' already exists", config.game_id));
        }
        if config.store.is_none() {
            config.store.clone_from(&self.store);
        }
        if let Some(store) = &config.store {
            if config.player_ids.is_empty() {
                return Err("durable sessions require an explicit player order".to_owned());
            }
            store
                .save_init(&GameInitRecord {
                    game_id: config.game_id.clone(),
                    seed: config.seed,
                    player_ids: config.player_ids.clone(),
                    initial_state: config.state.clone(),
                    seats: config.seats.clone(),
                    seat_tokens: config.seat_tokens.clone(),
                    map_tiles: config.map_tiles.clone(),
                })
                .map_err(|error| format!("Failed to save initial game configuration: {error}"))?;
        }
        let lobby = running_lobby_from_session_config(&config);
        let session = Arc::new(GameSession::start(config));
        state.lobbies.insert(lobby.game_id.clone(), lobby);
        state
            .sessions
            .insert(session.id().to_owned(), session.clone());
        Ok(session)
    }

    /// Creates and starts an authoritative dev scenario session with both player and legacy lobby registered.
    ///
    /// When persistence is enabled via `self.store`, all three required player records
    /// (`player_sessions.json`, `init.json`, and `lobby.json`) are durably written before
    /// starting the session or registering it in memory.
    ///
    /// # Failure and retry semantics
    /// - If the target directory already exists, returns an error immediately to prevent overwriting
    ///   or deleting an existing game's files on ID collision.
    /// - Writes proceed in order:
    ///   1. `player_sessions.json` (authoritative credentials)
    ///   2. `init.json` (immutable engine initialization and seat controllers)
    ///   3. `lobby.json` (running lobby record)
    /// - If any write fails, all artifacts created by this launch are deleted and the session is
    ///   neither registered in memory nor advertised. Retrying will generate a fresh game ID.
    pub fn launch_dev_scenario(
        &self,
        mut config: SessionConfig,
        lobby_record: PlayerLobbyRecord,
    ) -> Result<Arc<GameSession>, String> {
        lobby_record.validate().map_err(|error| error.to_string())?;
        crate::storage::validate_game_id(&config.game_id).map_err(|error| error.to_string())?;
        let mut state = self.state.lock().expect("registry lock");
        if state.sessions.contains_key(&config.game_id)
            || state.lobbies.contains_key(&config.game_id)
            || state.player_lobbies.contains_key(&config.game_id)
        {
            return Err(format!("Game session '{}' already exists", config.game_id));
        }

        let running_lobby = if let Some(store) = &self.store {
            let dir = store.game_dir(&config.game_id).map_err(|e| e.to_string())?;
            if dir.exists() {
                return Err(format!(
                    "Game directory '{}' already exists",
                    config.game_id
                ));
            }
            std::fs::create_dir_all(&dir)
                .map_err(|e| format!("Failed to create scenario directory: {e}"))?;

            config.store.clone_from(&self.store);

            let seed = config.seed.unwrap_or(lobby_record.seed);
            let sessions_record = PlayerSessionsRecord {
                schema_version: PLAYER_RECORD_VERSION,
                game_id: config.game_id.clone(),
                sessions: lobby_record
                    .players
                    .iter()
                    .map(|(p, m)| (p.clone(), m.session.clone()))
                    .collect(),
                nicknames: lobby_record
                    .players
                    .iter()
                    .map(|(id, member)| (id.clone(), member.nickname.clone()))
                    .collect(),
            };

            let init_record = PlayerGameInitRecord {
                schema_version: PLAYER_RECORD_VERSION,
                game_id: config.game_id.clone(),
                seed,
                player_ids: config.player_ids.clone(),
                initial_state: config.state.clone(),
                map_tiles: config.map_tiles.clone(),
                seats: Some(config.seats.clone()),
            };

            let mut running_lobby = lobby_record.clone();
            running_lobby.game_id.clone_from(&config.game_id);
            running_lobby.phase = PersistedLobbyPhase::Running;
            running_lobby.seed = seed;
            running_lobby.slots = config
                .player_ids
                .iter()
                .enumerate()
                .map(|(i, pid)| PlayerLobbySlot {
                    slot_id: LobbySlotId(format!("slot_{}", i + 1)),
                    occupant: Some(pid.clone()),
                })
                .collect();

            let cleanup = || {
                let _ = std::fs::remove_file(dir.join("lobby.json"));
                let _ = std::fs::remove_file(dir.join("init.json"));
                let _ = std::fs::remove_file(dir.join("player_sessions.json"));
                let _ = std::fs::remove_dir(&dir);
            };

            if let Err(e) = store.save_player_sessions(&sessions_record) {
                cleanup();
                return Err(format!("Failed to save player sessions: {e}"));
            }

            if let Err(e) = store.save_player_init(&init_record) {
                cleanup();
                return Err(format!("Failed to save player init: {e}"));
            }

            if let Err(e) = store.save_player_lobby(&running_lobby) {
                cleanup();
                return Err(format!("Failed to save player lobby: {e}"));
            }

            running_lobby
        } else {
            lobby_record
        };

        let legacy_lobby = running_lobby_from_session_config(&config);
        let session = Arc::new(GameSession::start(config));
        state
            .lobbies
            .insert(legacy_lobby.game_id.clone(), legacy_lobby);
        state
            .player_lobbies
            .insert(running_lobby.game_id.clone(), running_lobby);
        state
            .sessions
            .insert(session.id().to_owned(), session.clone());
        Ok(session)
    }

    pub fn register_recovered(&self, session: Arc<GameSession>) -> Result<(), String> {
        let mut state = self.state.lock().expect("registry lock");
        if state.sessions.contains_key(session.id()) || state.lobbies.contains_key(session.id()) {
            return Err(format!("Game session '{}' already exists", session.id()));
        }
        let lobby = running_lobby_from_session(&session);
        state.lobbies.insert(lobby.game_id.clone(), lobby);
        state.sessions.insert(session.id().to_owned(), session);
        Ok(())
    }

    #[must_use]
    pub fn get_game(&self, game_id: &str) -> Option<Arc<GameSession>> {
        self.state
            .lock()
            .expect("registry lock")
            .sessions
            .get(game_id)
            .cloned()
    }

    #[must_use]
    pub fn contains_game(&self, game_id: &str) -> bool {
        let state = self.state.lock().expect("registry lock");
        state.sessions.contains_key(game_id)
            || state.lobbies.contains_key(game_id)
            || state.player_lobbies.contains_key(game_id)
    }

    #[must_use]
    pub fn list_games(&self) -> Vec<GameSummary> {
        let state = self.state.lock().expect("registry lock");
        let mut games: Vec<_> = state
            .sessions
            .values()
            .map(|session| GameSummary {
                game_id: session.id().to_owned(),
                is_finished: session.is_finished(),
                pending_seat: session.current_pending_decision().map(|(seat, _, _)| seat),
            })
            .collect();
        games.extend(
            state
                .lobbies
                .values()
                .filter(|lobby| lobby.phase == LobbyPhase::Lobby)
                .map(|lobby| GameSummary {
                    game_id: lobby.game_id.clone(),
                    is_finished: false,
                    pending_seat: None,
                }),
        );
        games.extend(
            state
                .player_lobbies
                .values()
                .filter(|lobby| matches!(lobby.phase, PersistedLobbyPhase::Lobby))
                .map(|lobby| GameSummary {
                    game_id: lobby.game_id.clone(),
                    is_finished: false,
                    pending_seat: None,
                }),
        );
        games.sort_by(|left, right| left.game_id.cmp(&right.game_id));
        games
    }

    pub fn remove_game(&self, game_id: &str) -> Option<Arc<GameSession>> {
        let mut state = self.state.lock().expect("registry lock");
        state.lobbies.remove(game_id);
        state.player_lobbies.remove(game_id);
        let session = state.sessions.remove(game_id);
        if let Some(session) = &session {
            session.stop();
        }
        drop(state);
        let mut bots_guard = self.active_bots.lock().expect("active bots lock");
        if let Some(mut bot_list) = bots_guard.remove(game_id) {
            for b in &mut bot_list {
                let _ = b.child.start_kill();
            }
        }
        session
    }
}

impl Drop for GameRegistry {
    fn drop(&mut self) {
        if let Ok(mut bots_guard) = self.active_bots.lock() {
            for bot_list in bots_guard.values_mut() {
                for b in bot_list {
                    let _ = b.child.start_kill();
                }
            }
        }
    }
}

fn authenticate_player(
    lobby: &PlayerLobbyRecord,
    credential: &str,
) -> Result<PlayerId, LobbyError> {
    lobby
        .players
        .iter()
        .find_map(|(id, member)| (member.session.as_str() == credential).then(|| id.clone()))
        .ok_or(LobbyError::InvalidCapability)
}

fn running_lobby_from_session_config(config: &SessionConfig) -> LobbyState {
    let host_seat = config
        .player_ids
        .first()
        .cloned()
        .unwrap_or_else(|| PlayerId::new("host"));
    LobbyState {
        game_id: config.game_id.clone(),
        phase: LobbyPhase::Running,
        host_seat,
        player_ids: config.player_ids.clone(),
        seed: config.seed.unwrap_or_default(),
        lobby_version: 1,
        seats: config
            .seats
            .iter()
            .map(|(seat, controller)| {
                (
                    seat.clone(),
                    LobbySeat {
                        controller: controller.clone(),
                        ready: true,
                        seat_token: config.seat_tokens.get(seat).cloned(),
                        lease_expires_at_ms: None,
                    },
                )
            })
            .collect(),
    }
}

fn running_lobby_from_session(session: &GameSession) -> LobbyState {
    let (player_ids, seats, seat_tokens, seed) = session.lobby_details();
    running_lobby_from_session_config(&SessionConfig {
        game_id: session.id().to_owned(),
        state: session.current_state(),
        seats,
        seat_tokens,
        galaxy: None,
        map_tiles: Vec::new(),
        galaxy_layout: crate::map::GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        seed,
        player_ids,
        store: None,
        prior_decisions: Vec::new(),
        prior_events: Vec::new(),
        initial_version: 1,
        redo_decisions: Vec::new(),
        redo_events: Vec::new(),
        event_counter: 0,
        history_active: false,
        history_generation: 0,
        batches: Vec::new(),
        replay_boundary_state: None,
    })
}

fn authenticated_seat(lobby: &LobbyState, token: &str) -> Result<PlayerId, LobbyError> {
    lobby
        .seats
        .iter()
        .find_map(|(seat, lobby_seat)| {
            (lobby_seat.seat_token.as_deref() == Some(token)).then(|| seat.clone())
        })
        .ok_or(LobbyError::InvalidCapability)
}

fn lobby_to_record(lobby: &LobbyState) -> LobbyRecord {
    LobbyRecord {
        game_id: lobby.game_id.clone(),
        phase: match lobby.phase {
            LobbyPhase::Lobby => PersistedLobbyPhase::Lobby,
            LobbyPhase::Running => PersistedLobbyPhase::Running,
        },
        host_seat: lobby.host_seat.clone(),
        player_ids: lobby.player_ids.clone(),
        seed: lobby.seed,
        lobby_version: lobby.lobby_version,
        seats: lobby
            .seats
            .iter()
            .map(|(seat, lobby_seat)| {
                (
                    seat.clone(),
                    PersistedLobbySeat {
                        controller: lobby_seat.controller.clone(),
                        ready: lobby_seat.ready,
                        seat_token: lobby_seat.seat_token.clone(),
                        lease_expires_at_ms: lobby_seat.lease_expires_at_ms,
                    },
                )
            })
            .collect(),
    }
}

fn lobby_from_record(record: LobbyRecord) -> LobbyState {
    LobbyState {
        game_id: record.game_id,
        phase: match record.phase {
            PersistedLobbyPhase::Lobby => LobbyPhase::Lobby,
            PersistedLobbyPhase::Running => LobbyPhase::Running,
        },
        host_seat: record.host_seat,
        player_ids: record.player_ids,
        seed: record.seed,
        lobby_version: record.lobby_version,
        seats: record
            .seats
            .into_iter()
            .map(|(seat, lobby_seat)| {
                (
                    seat,
                    LobbySeat {
                        controller: lobby_seat.controller,
                        ready: lobby_seat.ready,
                        // Pre-lease records have no proof of a live claim, so do not resurrect credentials.
                        seat_token: lobby_seat.lease_expires_at_ms.and(lobby_seat.seat_token),
                        lease_expires_at_ms: lobby_seat.lease_expires_at_ms,
                    },
                )
            })
            .collect(),
    }
}

fn new_token() -> String {
    format!("{:032x}", rand::random::<u128>())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn legacy_running_lobby(init: &GameInitRecord) -> LobbyState {
    running_lobby_from_session_config(&SessionConfig {
        game_id: init.game_id.clone(),
        state: init.initial_state.clone(),
        seed: init.seed,
        galaxy: None,
        map_tiles: init.map_tiles.clone(),
        galaxy_layout: crate::map::GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        seats: init.seats.clone(),
        seat_tokens: init.seat_tokens.clone(),
        store: None,
        player_ids: init.player_ids.clone(),
        prior_decisions: Vec::new(),
        prior_events: Vec::new(),
        initial_version: 1,
        redo_decisions: Vec::new(),
        redo_events: Vec::new(),
        event_counter: 0,
        history_active: false,
        history_generation: 0,
        batches: Vec::new(),
        replay_boundary_state: None,
    })
}
