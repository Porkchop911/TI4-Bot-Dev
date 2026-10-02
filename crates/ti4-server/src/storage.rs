//! Local filesystem storage for durable game persistence, append-only decision logging,
//! and crash recovery.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use ti4_content::ContentStore;
use ti4_engine::choice::DecisionRecord;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use crate::protocol::server::GameEvent;
use crate::protocol::view::BoardTileView;
use crate::session::replay::replay_session;
use crate::session::{GameSession, SeatController, SessionConfig};

/// Errors encountered during game persistence or recovery.
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Replay error during recovery of game '{game_id}': {source}")]
    Replay {
        game_id: String,
        source: crate::session::replay::ReplayError,
    },
    #[error("Game '{0}' not found in storage")]
    NotFound(String),
    #[error("Seating/map error: {0}")]
    Map(String),
    #[error("Invalid game ID '{0}'")]
    InvalidGameId(String),
    #[error("Corrupt JSONL record in {path} at line {line}: {message}")]
    CorruptLog {
        path: PathBuf,
        line: usize,
        message: String,
    },
    #[error("Recovery replay canonical hashes differ for game '{0}'")]
    HashMismatch(String),
    #[error("Persistence record in {path} exceeds the {limit}-byte limit")]
    Oversized { path: PathBuf, limit: usize },
    #[error("Unsupported persistence format version {0}")]
    UnsupportedFormat(u16),
    #[error("Persistence identity mismatch for {field}")]
    IdentityMismatch { field: &'static str },
    #[error("Persistence checksum mismatch")]
    ChecksumMismatch,
    #[error("Snapshot state does not match replay for game '{0}'")]
    SnapshotMismatch(String),
    #[error("Invalid player persistence record: {0}")]
    InvalidPlayerRecord(&'static str),
}

const PERSISTENCE_FORMAT_VERSION: u16 = 1;
const MAX_INIT_BYTES: usize = 4 * 1024 * 1024;
const MAX_LOG_BYTES: usize = 64 * 1024 * 1024;
const MAX_LOG_RECORD_BYTES: usize = 64 * 1024;
const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;
pub const PLAYER_RECORD_VERSION: u16 = 3;
const MAX_PLAYER_LOBBY_BYTES: usize = 64 * 1024;
const MAX_PLAYER_SESSIONS_BYTES: usize = 16 * 1024;
const MAX_LOBBY_SLOTS: usize = 8;

/// Stable identifier of a physical lobby slot; it never identifies its occupant.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LobbySlotId(pub String);

/// A bearer credential. It can only be serialized into private persistence records.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PlayerSession(String);

impl std::fmt::Debug for PlayerSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlayerSession([redacted])")
    }
}

impl PlayerSession {
    #[must_use]
    pub fn generate() -> Self {
        Self(hex_random_256("session_"))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn validate(&self) -> Result<(), StorageError> {
        if valid_random_id(&self.0, "session_") {
            Ok(())
        } else {
            Err(StorageError::InvalidPlayerRecord("player session"))
        }
    }
}

/// Generate a fresh stable player identity, retrying if it collides with this lobby.
#[must_use]
pub fn generate_player_id(existing: &BTreeMap<PlayerId, PlayerLobbyMember>) -> PlayerId {
    generate_player_id_with(existing, || PlayerId::new(hex_random_256("player_")))
}

fn generate_player_id_with(
    existing: &BTreeMap<PlayerId, PlayerLobbyMember>,
    mut candidate: impl FnMut() -> PlayerId,
) -> PlayerId {
    loop {
        let id = candidate();
        if !existing.contains_key(&id) {
            return id;
        }
    }
}

fn hex_random_256(prefix: &str) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(prefix.len() + 64);
    out.push_str(prefix);
    for byte in rand::random::<[u8; 32]>() {
        write!(out, "{byte:02x}").expect("write to String");
    }
    out
}

fn valid_random_id(value: &str, prefix: &str) -> bool {
    value.len() == prefix.len() + 64
        && value.starts_with(prefix)
        && value[prefix.len()..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerLobbySlot {
    pub slot_id: LobbySlotId,
    pub occupant: Option<PlayerId>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerLobbyMember {
    pub ready: bool,
    pub session: PlayerSession,
    pub nickname: String,
}

impl std::fmt::Debug for PlayerLobbyMember {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerLobbyMember")
            .field("ready", &self.ready)
            .field("nickname", &self.nickname)
            .field("session", &self.session)
            .finish()
    }
}

/// Private versioned lobby record. Never return this type from a public API.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerLobbyRecord {
    pub schema_version: u16,
    pub game_id: String,
    pub phase: PersistedLobbyPhase,
    pub host_player_id: PlayerId,
    pub slots: Vec<PlayerLobbySlot>,
    pub players: BTreeMap<PlayerId, PlayerLobbyMember>,
    pub seed: u64,
    pub lobby_version: u64,
}

impl PlayerLobbyRecord {
    pub fn validate(&self) -> Result<(), StorageError> {
        check_player_schema(self.schema_version)?;
        validate_game_id(&self.game_id)?;
        if !(1..=MAX_LOBBY_SLOTS).contains(&self.slots.len())
            || self.players.len() > self.slots.len()
            || !self.players.contains_key(&self.host_player_id)
        {
            return Err(StorageError::InvalidPlayerRecord("lobby roster"));
        }
        let mut slots = std::collections::BTreeSet::new();
        let mut occupants = std::collections::BTreeSet::new();
        let mut credentials = std::collections::BTreeSet::new();
        for slot in &self.slots {
            if slot.slot_id.0.len() > 32
                || slot.slot_id.0.is_empty()
                || !slot
                    .slot_id
                    .0
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
                || !slots.insert(&slot.slot_id)
            {
                return Err(StorageError::InvalidPlayerRecord("slot IDs"));
            }
            if let Some(id) = &slot.occupant
                && (!self.players.contains_key(id) || !occupants.insert(id))
            {
                return Err(StorageError::InvalidPlayerRecord("slot occupants"));
            }
        }
        if occupants.len() != self.players.len() {
            return Err(StorageError::InvalidPlayerRecord("unseated player"));
        }
        for (id, player) in &self.players {
            if !valid_random_id(id.as_str(), "player_") {
                return Err(StorageError::InvalidPlayerRecord("player ID"));
            }
            player.session.validate()?;
            validate_nickname(&player.nickname)?;
            if !credentials.insert(player.session.as_str()) {
                return Err(StorageError::InvalidPlayerRecord("duplicate session"));
            }
        }
        Ok(())
    }
}

/// Immutable engine initialization: credentials live only in the current-session record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerGameInitRecord {
    pub schema_version: u16,
    pub game_id: String,
    pub seed: u64,
    pub player_ids: Vec<PlayerId>,
    pub initial_state: GameState,
    pub map_tiles: Vec<BoardTileView>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<BTreeMap<PlayerId, SeatController>>,
}

/// Authoritative running-session credential mapping, atomically replaced on rotation.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSessionsRecord {
    pub schema_version: u16,
    pub game_id: String,
    pub sessions: BTreeMap<PlayerId, PlayerSession>,
    pub nicknames: BTreeMap<PlayerId, String>,
}

impl std::fmt::Debug for PlayerSessionsRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlayerSessionsRecord")
            .field("schema_version", &self.schema_version)
            .field("game_id", &self.game_id)
            .field("sessions", &"[redacted]")
            .field("nicknames", &self.nicknames)
            .finish()
    }
}

fn check_player_schema(version: u16) -> Result<(), StorageError> {
    if version == PLAYER_RECORD_VERSION {
        Ok(())
    } else {
        Err(StorageError::UnsupportedFormat(version))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedEnvelope<T> {
    format_version: u16,
    content_identity: String,
    rules_identity: String,
    payload: T,
    checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SessionSnapshot {
    decision_count: usize,
    state: GameState,
}

/// Atomic authoritative timeline once a game has been rewound. The original append-only
/// files are retained for historical audit but are no longer the active branch.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameHistory {
    pub decisions: Vec<DecisionRecord>,
    pub redo: Vec<DecisionRecord>,
    pub events: Vec<GameEvent>,
    #[serde(default)]
    pub redo_events: Vec<GameEvent>,
    #[serde(default)]
    pub event_counter: u64,
    pub revision: u64,
    #[serde(default)]
    pub generation: u64,
    #[serde(default)]
    pub batches: Vec<BatchRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BatchRecord {
    pub request_id: String,
    pub batch_id: String,
    pub actor: PlayerId,
    pub start_cursor: usize,
    pub end_cursor: usize,
}

/// Initial configuration record saved atomically to `init.json`.
#[derive(Clone, Serialize, Deserialize)]
pub struct GameInitRecord {
    pub game_id: String,
    pub seed: Option<u64>,
    pub player_ids: Vec<PlayerId>,
    pub initial_state: GameState,
    pub seats: BTreeMap<PlayerId, SeatController>,
    #[serde(default)]
    pub seat_tokens: BTreeMap<PlayerId, String>,
    #[serde(default)]
    pub map_tiles: Vec<BoardTileView>,
}

impl std::fmt::Debug for GameInitRecord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GameInitRecord")
            .field("game_id", &self.game_id)
            .field("seat_tokens", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// Persisted lifecycle state for a game that has not yet entered the engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersistedLobbyPhase {
    Lobby,
    Running,
}

/// Persisted configuration for one lobby seat.
#[derive(Clone, Serialize, Deserialize)]
pub struct PersistedLobbySeat {
    pub controller: SeatController,
    pub ready: bool,
    pub seat_token: Option<String>,
    #[serde(default)]
    pub lease_expires_at_ms: Option<u64>,
}

impl std::fmt::Debug for PersistedLobbySeat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PersistedLobbySeat")
            .field("controller", &self.controller)
            .field("ready", &self.ready)
            .field("seat_token", &"[redacted]")
            .finish_non_exhaustive()
    }
}

/// Durable pre-game metadata. Engine state is intentionally absent until start succeeds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyRecord {
    pub game_id: String,
    pub phase: PersistedLobbyPhase,
    pub host_seat: PlayerId,
    pub player_ids: Vec<PlayerId>,
    pub seats: BTreeMap<PlayerId, PersistedLobbySeat>,
    pub seed: u64,
    pub lobby_version: u64,
}

/// Durable file-based game storage manager.
#[derive(Debug, Clone)]
pub struct FileGameStore {
    base_dir: PathBuf,
}

impl FileGameStore {
    pub fn save_history(&self, game_id: &str, history: &GameHistory) -> Result<(), StorageError> {
        let dir = self.game_dir(game_id)?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("history.json");
        if serde_json::to_vec_pretty(&persist(history)?)?.len() + 1 > MAX_LOG_BYTES {
            return Err(StorageError::Oversized {
                path,
                limit: MAX_LOG_BYTES,
            });
        }
        atomic_write_json(&path, history)?;
        Ok(())
    }

    pub fn load_history(&self, game_id: &str) -> Result<Option<GameHistory>, StorageError> {
        let path = self.game_dir(game_id)?.join("history.json");
        if !path.exists() {
            return Ok(None);
        }
        let history: GameHistory = read_json_file(&path, MAX_LOG_BYTES)?;
        if history.revision == 0 || history.decisions.len() + history.redo.len() > 1_000_000 {
            return Err(StorageError::InvalidPlayerRecord("history cursor"));
        }
        let mut cursor = 0;
        let mut ids = std::collections::BTreeSet::new();
        for event in history.events.iter().chain(&history.redo_events) {
            if matches!(
                event.event,
                crate::protocol::server::GameEventKind::DecisionResolved
            ) {
                cursor += 1;
            }
            if !ids.insert(&event.id) || event.decision_count.is_some_and(|count| count != cursor) {
                return Err(StorageError::InvalidPlayerRecord("history event cursor"));
            }
        }
        let mut requests = std::collections::BTreeSet::new();
        let mut batch_ids = std::collections::BTreeSet::new();
        let total = history.decisions.len() + history.redo.len();
        if cursor != total {
            return Err(StorageError::InvalidPlayerRecord(
                "history decision event count",
            ));
        }
        for batch in &history.batches {
            if batch.start_cursor >= batch.end_cursor
                || batch.end_cursor > total
                || !requests.insert(&batch.request_id)
                || !batch_ids.insert(&batch.batch_id)
                || !history
                    .decisions
                    .iter()
                    .chain(&history.redo)
                    .skip(batch.start_cursor)
                    .take(batch.end_cursor - batch.start_cursor)
                    .all(|d| d.player == batch.actor)
            {
                return Err(StorageError::InvalidPlayerRecord("history batch"));
            }
            for decision_cursor in batch.start_cursor + 1..=batch.end_cursor {
                if !history
                    .events
                    .iter()
                    .chain(&history.redo_events)
                    .any(|event| {
                        event.decision_count == Some(decision_cursor)
                            && event.batch_id.as_deref() == Some(&batch.batch_id)
                            && matches!(
                                event.event,
                                crate::protocol::server::GameEventKind::DecisionResolved
                            )
                    })
                {
                    return Err(StorageError::InvalidPlayerRecord(
                        "history missing batch event",
                    ));
                }
            }
        }
        for event in history.events.iter().chain(&history.redo_events) {
            if let Some(id) = &event.action_id {
                let start = id
                    .strip_prefix("action_")
                    .and_then(|value| value.parse::<usize>().ok());
                if !matches!(
                    event.event,
                    crate::protocol::server::GameEventKind::DecisionResolved
                ) || !start.is_some_and(|start| {
                    start > 0
                        && start <= event.decision_count.unwrap_or(0)
                        && history
                            .decisions
                            .iter()
                            .chain(&history.redo)
                            .nth(start - 1)
                            .is_some_and(|record| record.prompt == "action phase")
                }) {
                    return Err(StorageError::InvalidPlayerRecord("history action event"));
                }
            }
            if let Some(id) = &event.batch_id
                && !history.batches.iter().any(|batch| {
                    &batch.batch_id == id
                        && matches!(
                            event.event,
                            crate::protocol::server::GameEventKind::DecisionResolved
                        )
                        && event.decision_count.is_some_and(|cursor| {
                            cursor > batch.start_cursor && cursor <= batch.end_cursor
                        })
                })
            {
                return Err(StorageError::InvalidPlayerRecord("history batch event"));
            }
        }
        Ok(Some(history))
    }
    /// Store the versioned lobby as a single atomic private record.
    pub fn save_player_lobby(&self, record: &PlayerLobbyRecord) -> Result<(), StorageError> {
        record.validate()?;
        let dir = self.game_dir(&record.game_id)?;
        fs::create_dir_all(&dir)?;
        atomic_write_player_record(&dir.join("lobby.json"), record, MAX_PLAYER_LOBBY_BYTES)?;
        Ok(())
    }

    /// Reject legacy or corrupted lobby records rather than reinterpreting seat IDs.
    pub fn load_player_lobby(
        &self,
        game_id: &str,
    ) -> Result<Option<PlayerLobbyRecord>, StorageError> {
        let path = self.game_dir(game_id)?.join("lobby.json");
        if !path.exists() {
            return Ok(None);
        }
        let record: PlayerLobbyRecord = read_player_record(&path, MAX_PLAYER_LOBBY_BYTES)?;
        if record.game_id != game_id {
            return Err(StorageError::IdentityMismatch { field: "game_id" });
        }
        record.validate()?;
        Ok(Some(record))
    }

    /// Save immutable init without any bearer credentials.
    pub fn save_player_init(&self, record: &PlayerGameInitRecord) -> Result<(), StorageError> {
        validate_player_init(record)?;
        let dir = self.game_dir(&record.game_id)?;
        fs::create_dir_all(&dir)?;
        atomic_write_player_record(&dir.join("init.json"), record, MAX_INIT_BYTES)?;
        Ok(())
    }

    pub fn load_player_init(&self, game_id: &str) -> Result<PlayerGameInitRecord, StorageError> {
        let path = self.game_dir(game_id)?.join("init.json");
        let record: PlayerGameInitRecord = read_player_record(&path, MAX_INIT_BYTES)?;
        if record.game_id != game_id {
            return Err(StorageError::IdentityMismatch { field: "game_id" });
        }
        validate_player_init(&record)?;
        Ok(record)
    }

    /// The sole authority for current running credentials, separate from immutable init.
    pub fn save_player_sessions(&self, record: &PlayerSessionsRecord) -> Result<(), StorageError> {
        validate_player_sessions(record)?;
        let dir = self.game_dir(&record.game_id)?;
        fs::create_dir_all(&dir)?;
        atomic_write_player_record(
            &dir.join("player_sessions.json"),
            record,
            MAX_PLAYER_SESSIONS_BYTES,
        )?;
        Ok(())
    }

    pub fn load_player_sessions(
        &self,
        game_id: &str,
    ) -> Result<PlayerSessionsRecord, StorageError> {
        let path = self.game_dir(game_id)?.join("player_sessions.json");
        let record: PlayerSessionsRecord = read_player_record(&path, MAX_PLAYER_SESSIONS_BYTES)?;
        if record.game_id != game_id {
            return Err(StorageError::IdentityMismatch { field: "game_id" });
        }
        validate_player_sessions(&record)?;
        Ok(record)
    }

    /// Creates a new store rooted at `base_dir`, ensuring the directory exists.
    ///
    /// # Errors
    ///
    /// Returns [`std::io::Error`] if directory creation fails.
    pub fn new(base_dir: impl Into<PathBuf>) -> std::io::Result<Self> {
        let base_dir = base_dir.into();
        fs::create_dir_all(&base_dir)?;
        Ok(Self { base_dir })
    }

    /// Returns the filesystem path to a game's directory.
    pub fn game_dir(&self, game_id: &str) -> Result<PathBuf, StorageError> {
        validate_game_id(game_id)?;
        Ok(self.base_dir.join(game_id))
    }

    /// Saves the initial game configuration atomically to `init.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if serialization or filesystem write fails.
    pub fn save_init(&self, record: &GameInitRecord) -> Result<(), StorageError> {
        let dir = self.game_dir(&record.game_id)?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("init.json");
        atomic_write_json(&path, record)?;
        Ok(())
    }

    /// Saves lobby metadata atomically before a game session exists.
    pub fn save_lobby(&self, record: &LobbyRecord) -> Result<(), StorageError> {
        let dir = self.game_dir(&record.game_id)?;
        fs::create_dir_all(&dir)?;
        atomic_write_json(&dir.join("lobby.json"), record)?;
        Ok(())
    }

    /// Loads durable lobby metadata when present.
    pub fn load_lobby(&self, game_id: &str) -> Result<Option<LobbyRecord>, StorageError> {
        let path = self.game_dir(game_id)?.join("lobby.json");
        if !path.exists() {
            return Ok(None);
        }
        let record: LobbyRecord = read_json_file(&path, MAX_INIT_BYTES)?;
        if record.game_id != game_id {
            return Err(StorageError::IdentityMismatch { field: "game_id" });
        }
        Ok(Some(record))
    }

    /// Appends an accepted decision record to `decisions.jsonl` with fsync.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if append or sync fails.
    pub fn append_decision(
        &self,
        game_id: &str,
        record: &DecisionRecord,
    ) -> Result<(), StorageError> {
        let dir = self.game_dir(game_id)?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("decisions.jsonl");
        append_json_line(&path, record)?;
        Ok(())
    }

    /// Appends an authoritative game event to `events.jsonl` with fsync.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if append or sync fails.
    pub fn append_event(&self, game_id: &str, event: &GameEvent) -> Result<(), StorageError> {
        let dir = self.game_dir(game_id)?;
        fs::create_dir_all(&dir)?;
        let path = dir.join("events.jsonl");
        append_json_line(&path, event)?;
        Ok(())
    }

    /// Atomically stores a bounded replay-validation snapshot.
    pub fn save_snapshot(
        &self,
        game_id: &str,
        decision_count: usize,
        state: &GameState,
    ) -> Result<(), StorageError> {
        let dir = self.game_dir(game_id)?;
        fs::create_dir_all(&dir)?;
        atomic_write_json(
            &dir.join("snapshot.json"),
            &SessionSnapshot {
                decision_count,
                state: state.clone(),
            },
        )?;
        Ok(())
    }

    /// Loads the initial configuration for a game from `init.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if reading or parsing fails.
    pub fn load_init(&self, game_id: &str) -> Result<GameInitRecord, StorageError> {
        let path = self.game_dir(game_id)?.join("init.json");
        if !path.exists() {
            return Err(StorageError::NotFound(game_id.to_owned()));
        }
        let record: GameInitRecord = read_json_file(&path, MAX_INIT_BYTES)?;
        if record.game_id != game_id {
            return Err(StorageError::IdentityMismatch { field: "game_id" });
        }
        Ok(record)
    }

    /// Loads all recorded decisions from `decisions.jsonl`.
    ///
    /// Tolerant to an incomplete trailing write (e.g. abrupt power cut).
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on I/O error.
    pub fn load_decisions(&self, game_id: &str) -> Result<Vec<DecisionRecord>, StorageError> {
        let path = self.game_dir(game_id)?.join("decisions.jsonl");
        read_json_lines(&path)
    }

    /// Loads all recorded events from `events.jsonl`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on I/O error.
    pub fn load_events(&self, game_id: &str) -> Result<Vec<GameEvent>, StorageError> {
        let path = self.game_dir(game_id)?.join("events.jsonl");
        read_json_lines(&path)
    }

    fn load_snapshot(&self, game_id: &str) -> Result<Option<SessionSnapshot>, StorageError> {
        let path = self.game_dir(game_id)?.join("snapshot.json");
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(read_json_file(&path, MAX_SNAPSHOT_BYTES)?))
    }

    /// Lists all game IDs found in the storage directory with an `init.json`.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] on directory traversal error.
    pub fn list_saved_games(&self) -> Result<Vec<String>, StorageError> {
        let mut games = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let init_path = entry.path().join("init.json");
                if init_path.exists() {
                    let name = entry.file_name();
                    if let Some(s) = name.to_str() {
                        games.push(s.to_owned());
                    }
                }
            }
        }
        games.sort();
        Ok(games)
    }

    /// Lists lobby IDs, including lobbies that have subsequently started.
    pub fn list_saved_lobbies(&self) -> Result<Vec<String>, StorageError> {
        let mut games = Vec::new();
        for entry in fs::read_dir(&self.base_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() && entry.path().join("lobby.json").exists() {
                let name = entry.file_name();
                if let Some(s) = name.to_str() {
                    games.push(s.to_owned());
                }
            }
        }
        games.sort();
        Ok(games)
    }

    /// Recovers a game session by loading initial state and replaying recorded decisions.
    ///
    /// The recovered session is ready to accept choices and will append future decisions
    /// to the existing store.
    ///
    /// # Errors
    ///
    /// Returns [`StorageError`] if recovery fails.
    pub fn recover_session(self: &Arc<Self>, game_id: &str) -> Result<GameSession, StorageError> {
        let init_record = self.load_init(game_id)?;
        self.recover_from_init(game_id, init_record)
    }

    /// Recover using the current-session record as credential authority.
    pub fn recover_player_session(
        self: &Arc<Self>,
        game_id: &str,
    ) -> Result<GameSession, StorageError> {
        let init = self.load_player_init(game_id)?;
        let sessions = self.load_player_sessions(game_id)?;
        if init.player_ids.len() != sessions.sessions.len()
            || init
                .player_ids
                .iter()
                .any(|id| !sessions.sessions.contains_key(id))
        {
            return Err(StorageError::InvalidPlayerRecord("started sessions"));
        }
        let seats = if let Some(seats) = init.seats {
            seats
        } else {
            init.player_ids
                .iter()
                .map(|id| (id.clone(), SeatController::Human))
                .collect()
        };
        let record = GameInitRecord {
            game_id: init.game_id,
            seed: Some(init.seed),
            player_ids: init.player_ids.clone(),
            initial_state: init.initial_state,
            seats,
            seat_tokens: sessions
                .sessions
                .into_iter()
                .map(|(id, session)| (id, session.as_str().to_owned()))
                .collect(),
            map_tiles: init.map_tiles,
        };
        self.recover_from_init(game_id, record)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "validates the durable replay and snapshot before starting a worker"
    )]
    fn recover_from_init(
        self: &Arc<Self>,
        game_id: &str,
        init_record: GameInitRecord,
    ) -> Result<GameSession, StorageError> {
        let history = self.load_history(game_id)?;
        let decisions = if let Some(h) = &history {
            h.decisions.clone()
        } else {
            self.load_decisions(game_id)?
        };
        let events = if let Some(h) = &history {
            h.events.clone()
        } else {
            self.load_events(game_id)?
        };

        let content = ContentStore::embedded();
        let galaxy = if let Some(seed) = init_record.seed {
            let (_, g) = crate::map::create_game_with_map(content, &init_record.player_ids, seed)
                .map_err(|e| StorageError::Map(e.to_string()))?;
            Some(g)
        } else {
            None
        };

        let map_tiles = if init_record.map_tiles.is_empty() {
            if let Some(g) = &galaxy {
                crate::map::build_board_tiles(content, g)
            } else {
                Vec::new()
            }
        } else {
            init_record.map_tiles.clone()
        };

        // Replay all accepted decisions to reach the current recovered state
        let replay_report = replay_session(&init_record.initial_state, galaxy.as_ref(), &decisions)
            .map_err(|source| StorageError::Replay {
                game_id: game_id.to_owned(),
                source,
            })?;

        if !replay_report.hashes_match {
            return Err(StorageError::HashMismatch(game_id.to_owned()));
        }

        if let Some(history) = &history {
            let future: Vec<_> = history
                .decisions
                .iter()
                .chain(&history.redo)
                .cloned()
                .collect();
            let validated = replay_session(&init_record.initial_state, galaxy.as_ref(), &future)
                .map_err(|source| StorageError::Replay {
                    game_id: game_id.to_owned(),
                    source,
                })?;
            if !validated.hashes_match || validated.decision_count != future.len() {
                return Err(StorageError::HashMismatch(game_id.to_owned()));
            }
            let event_decisions = history
                .events
                .iter()
                .chain(&history.redo_events)
                .filter(|entry| {
                    matches!(
                        entry.event,
                        crate::protocol::server::GameEventKind::DecisionResolved
                    )
                })
                .count();
            if event_decisions != future.len()
                || history.event_counter < (history.events.len() + history.redo_events.len()) as u64
            {
                return Err(StorageError::InvalidPlayerRecord("history events"));
            }
        }

        if let Some(snapshot) = if history.is_none() {
            self.load_snapshot(game_id)?
        } else {
            None
        } {
            if snapshot.decision_count > decisions.len() {
                return Err(StorageError::SnapshotMismatch(game_id.to_owned()));
            }
            let snapshot_replay = replay_session(
                &init_record.initial_state,
                galaxy.as_ref(),
                &decisions[..snapshot.decision_count],
            )
            .map_err(|source| StorageError::Replay {
                game_id: game_id.to_owned(),
                source,
            })?;
            if state_checksum(&snapshot_replay.final_state)? != state_checksum(&snapshot.state)? {
                return Err(StorageError::SnapshotMismatch(game_id.to_owned()));
            }
        }

        // Configure session starting at initial state with prior history for live continuation
        let mut config =
            SessionConfig::new(&init_record.game_id, init_record.initial_state.clone())
                .with_store(self.clone())
                .with_prior_history(decisions.clone(), events.clone());
        if let Some(history) = history {
            config.initial_version = history.revision;
            config.redo_decisions = history.redo;
            config.redo_events = history.redo_events;
            config.event_counter = history.event_counter;
            config.history_generation = history.generation;
            config.history_active = true;
            config.batches = history.batches;
        }

        if let Some(seed) = init_record.seed {
            config = config.with_seed(seed);
        }
        config = config.with_player_ids(init_record.player_ids.clone());

        if let Some(g) = galaxy.clone() {
            config = config.with_galaxy(g, map_tiles);
        }

        for (seat, controller) in &init_record.seats {
            config = config.with_seat(seat.clone(), controller.clone());
        }
        if !init_record.seat_tokens.is_empty() {
            config.seat_tokens = init_record.seat_tokens;
        }

        let session = GameSession::start_recovered(config, decisions, events);

        Ok(session)
    }
}

fn validate_player_init(record: &PlayerGameInitRecord) -> Result<(), StorageError> {
    check_player_schema(record.schema_version)?;
    validate_game_id(&record.game_id)?;
    if !(1..=MAX_LOBBY_SLOTS).contains(&record.player_ids.len())
        || record
            .player_ids
            .iter()
            .any(|id| !valid_random_id(id.as_str(), "player_"))
        || record
            .player_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != record.player_ids.len()
    {
        return Err(StorageError::InvalidPlayerRecord("game player order"));
    }
    if let Some(seats) = &record.seats
        && (seats.len() != record.player_ids.len()
            || record.player_ids.iter().any(|id| !seats.contains_key(id)))
    {
        return Err(StorageError::InvalidPlayerRecord("seat controllers"));
    }
    Ok(())
}

fn validate_player_sessions(record: &PlayerSessionsRecord) -> Result<(), StorageError> {
    check_player_schema(record.schema_version)?;
    validate_game_id(&record.game_id)?;
    if !(1..=MAX_LOBBY_SLOTS).contains(&record.sessions.len()) {
        return Err(StorageError::InvalidPlayerRecord("session count"));
    }
    if record.sessions.keys().ne(record.nicknames.keys()) {
        return Err(StorageError::InvalidPlayerRecord("nickname roster"));
    }
    for nickname in record.nicknames.values() {
        validate_nickname(nickname)?;
    }
    let mut credentials = std::collections::BTreeSet::new();
    for (id, session) in &record.sessions {
        if !valid_random_id(id.as_str(), "player_") || !credentials.insert(session.as_str()) {
            return Err(StorageError::InvalidPlayerRecord("session identity"));
        }
        session.validate()?;
    }
    Ok(())
}

/// Nicknames are exact, trimmed Unicode text of at most 64 UTF-8 bytes.
/// Reject control and formatting code points, including bidirectional overrides.
pub fn validate_nickname(nickname: &str) -> Result<(), StorageError> {
    let invalid = nickname.is_empty()
        || nickname.len() > 64
        || nickname.trim() != nickname
        || nickname.chars().all(char::is_whitespace)
        || nickname.chars().any(|c| {
            c.is_control()
                || matches!(c as u32,
                    0x00ad | 0x0600..=0x0605 | 0x061c | 0x06dd | 0x070f | 0x0890..=0x0891
                    | 0x08e2 | 0x180e | 0x200b..=0x200f | 0x202a..=0x202e
                    | 0x2060..=0x206f | 0xfeff | 0xfff9..=0xfffb | 0x110bd
                    | 0x110cd | 0x13430..=0x1343f | 0x1bca0..=0x1bca3
                    | 0x1d173..=0x1d17a | 0xe0001 | 0xe0020..=0xe007f)
        });
    if invalid {
        Err(StorageError::InvalidPlayerRecord("nickname"))
    } else {
        Ok(())
    }
}

fn read_player_record<T: Serialize + for<'de> Deserialize<'de>>(
    path: &Path,
    limit: usize,
) -> Result<T, StorageError> {
    let bytes = read_bounded(path, limit)?;
    // Read the schema marker before decoding the payload, so old records fail clearly.
    let envelope: serde_json::Value = serde_json::from_slice(&bytes)?;
    let version = envelope
        .get("payload")
        .and_then(|payload| payload.get("schema_version"))
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    check_player_schema(u16::try_from(version).unwrap_or(0))?;
    validate_envelope(serde_json::from_slice(&bytes)?)
}

fn atomic_write_player_record<T: Serialize + Clone>(
    path: &Path,
    record: &T,
    limit: usize,
) -> Result<(), StorageError> {
    let bytes = serde_json::to_vec_pretty(&persist(record)?)?;
    if bytes.len() + 1 > limit {
        return Err(StorageError::Oversized {
            path: path.to_owned(),
            limit,
        });
    }
    atomic_write_json(path, record)?;
    Ok(())
}

fn atomic_write_json<T: Serialize + Clone>(path: &Path, value: &T) -> std::io::Result<()> {
    let tmp_path = path.with_extension("tmp");
    let file = File::create(&tmp_path)?;
    let mut writer = std::io::BufWriter::new(file);
    let envelope = persist(value)?;
    serde_json::to_writer_pretty(&mut writer, &envelope)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    fs::rename(&tmp_path, path)?;
    Ok(())
}

fn append_json_line<T: Serialize + Clone>(path: &Path, value: &T) -> std::io::Result<()> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut writer = std::io::BufWriter::new(file);
    let envelope = persist(value)?;
    serde_json::to_writer(&mut writer, &envelope)?;
    writer.write_all(b"\n")?;
    writer.flush()?;
    writer.get_ref().sync_data()?;
    Ok(())
}

/// Validates that a game ID is one bounded, portable filesystem component.
pub fn validate_game_id(game_id: &str) -> Result<(), StorageError> {
    const MAX_GAME_ID_BYTES: usize = 64;
    let valid = !game_id.is_empty()
        && game_id.len() <= MAX_GAME_ID_BYTES
        && game_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(StorageError::InvalidGameId(game_id.to_owned()))
    }
}

fn read_json_file<T: Serialize + for<'de> Deserialize<'de>>(
    path: &Path,
    limit: usize,
) -> Result<T, StorageError> {
    let bytes = read_bounded(path, limit)?;
    let envelope = serde_json::from_slice(&bytes)?;
    validate_envelope(envelope)
}

fn read_json_lines<T: Serialize + for<'de> Deserialize<'de>>(
    path: &Path,
) -> Result<Vec<T>, StorageError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let bytes = read_bounded(path, MAX_LOG_BYTES)?;
    let has_complete_final_line = bytes.last() == Some(&b'\n');
    let text = std::str::from_utf8(&bytes).map_err(|error| StorageError::CorruptLog {
        path: path.to_owned(),
        line: 1,
        message: error.to_string(),
    })?;
    let mut items = Vec::new();
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.len() > MAX_LOG_RECORD_BYTES {
            return Err(StorageError::Oversized {
                path: path.to_owned(),
                limit: MAX_LOG_RECORD_BYTES,
            });
        }
        let parsed = serde_json::from_str::<PersistedEnvelope<T>>(trimmed)
            .map_err(StorageError::from)
            .and_then(validate_envelope);
        match parsed {
            Ok(item) => items.push(item),
            Err(_error) if idx + 1 == lines.len() && !has_complete_final_line => {
                // A power loss may leave exactly the final append torn; all earlier records are durable.
                break;
            }
            Err(error) => {
                return Err(StorageError::CorruptLog {
                    path: path.to_owned(),
                    line: idx + 1,
                    message: error.to_string(),
                });
            }
        }
    }
    Ok(items)
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, StorageError> {
    if fs::metadata(path)?.len() > limit as u64 {
        return Err(StorageError::Oversized {
            path: path.to_owned(),
            limit,
        });
    }
    Ok(fs::read(path)?)
}

fn persist<T: Serialize + Clone>(payload: &T) -> Result<PersistedEnvelope<T>, serde_json::Error> {
    let mut envelope = PersistedEnvelope {
        format_version: PERSISTENCE_FORMAT_VERSION,
        content_identity: content_identity(),
        rules_identity: env!("CARGO_PKG_VERSION").to_owned(),
        payload: payload.clone(),
        checksum: String::new(),
    };
    envelope.checksum = envelope_checksum(&envelope)?;
    Ok(envelope)
}

fn validate_envelope<T: Serialize>(envelope: PersistedEnvelope<T>) -> Result<T, StorageError> {
    if envelope.format_version != PERSISTENCE_FORMAT_VERSION {
        return Err(StorageError::UnsupportedFormat(envelope.format_version));
    }
    if envelope.content_identity != content_identity() {
        return Err(StorageError::IdentityMismatch {
            field: "content_identity",
        });
    }
    if envelope.rules_identity != env!("CARGO_PKG_VERSION") {
        return Err(StorageError::IdentityMismatch {
            field: "rules_identity",
        });
    }
    if envelope_checksum(&envelope)? != envelope.checksum {
        return Err(StorageError::ChecksumMismatch);
    }
    Ok(envelope.payload)
}

fn envelope_checksum<T: Serialize>(
    envelope: &PersistedEnvelope<T>,
) -> Result<String, serde_json::Error> {
    #[derive(Serialize)]
    struct ChecksumInput<'a, T> {
        format_version: u16,
        content_identity: &'a str,
        rules_identity: &'a str,
        payload: &'a T,
    }
    let bytes = serde_json::to_vec(&ChecksumInput {
        format_version: envelope.format_version,
        content_identity: &envelope.content_identity,
        rules_identity: &envelope.rules_identity,
        payload: &envelope.payload,
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn state_checksum(state: &GameState) -> Result<String, StorageError> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(state)?)))
}

fn content_identity() -> String {
    format!(
        "{:x}",
        Sha256::digest(include_bytes!("../../ti4-content/content/CHECKSUMS.sha256"))
    )
}

#[cfg(test)]
mod player_record_tests {
    use super::*;

    fn store() -> (FileGameStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("ti4_pil01_{:032x}", rand::random::<u128>()));
        (FileGameStore::new(&dir).expect("create store"), dir)
    }

    #[test]
    fn lobby_round_trip_preserves_identity_and_hides_credentials_from_public_output() {
        let (store, dir) = store();
        let (lobby, host, session) =
            PlayerLobbyRecord::create("pil01_lobby".into(), 3, 7, "Host").unwrap();
        store.save_player_lobby(&lobby).unwrap();
        let loaded = store.load_player_lobby("pil01_lobby").unwrap().unwrap();
        assert_eq!(loaded.host_player_id, host);
        assert_eq!(loaded.slots.len(), 3);
        assert_eq!(loaded.slots[0].occupant, Some(host));
        assert!(loaded.slots[1].occupant.is_none());
        assert_eq!(loaded.players[&loaded.host_player_id].session, session);
        let public = serde_json::to_string(&loaded.public_view()).unwrap();
        assert!(!public.contains(session.as_str()));
        assert!(!format!("{loaded:?}").contains(session.as_str()));
        assert!(!format!("{session:?}").contains(session.as_str()));
        let legacy = PersistedLobbySeat {
            controller: SeatController::Human,
            ready: false,
            seat_token: Some(session.as_str().to_owned()),
            lease_expires_at_ms: None,
        };
        assert!(!format!("{legacy:?}").contains(session.as_str()));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn repeated_generated_ids_are_unique_and_collision_is_skipped() {
        let mut players = BTreeMap::new();
        for _ in 0..256 {
            let id = generate_player_id(&players);
            assert!(valid_random_id(id.as_str(), "player_"));
            assert!(
                players
                    .insert(
                        id,
                        PlayerLobbyMember {
                            ready: false,
                            session: PlayerSession::generate(),
                            nickname: "Test".into(),
                        }
                    )
                    .is_none()
            );
        }
        let fresh = generate_player_id(&players);
        assert!(!players.contains_key(&fresh));
        let occupied = players.keys().next().unwrap().clone();
        let new = PlayerId::new(hex_random_256("player_"));
        let mut attempts = [occupied, new.clone()].into_iter();
        assert_eq!(
            generate_player_id_with(&players, || attempts.next().unwrap()),
            new
        );
    }

    #[test]
    fn old_records_and_unknown_versions_are_rejected_before_payload_decode() {
        let (store, dir) = store();
        let (lobby, _, _) = PlayerLobbyRecord::create("old".into(), 2, 1, "Host").unwrap();
        let path = store.game_dir("old").unwrap().join("lobby.json");
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut old = serde_json::to_value(persist(&lobby).unwrap()).unwrap();
        old["payload"]
            .as_object_mut()
            .unwrap()
            .remove("schema_version");
        fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
        assert!(matches!(
            store.load_player_lobby("old"),
            Err(StorageError::UnsupportedFormat(0))
        ));
        let mut unsupported = lobby.clone();
        unsupported.schema_version = 2;
        atomic_write_json(&path, &unsupported).unwrap();
        assert!(matches!(
            store.load_player_lobby("old"),
            Err(StorageError::UnsupportedFormat(2))
        ));
        unsupported.schema_version = 99;
        atomic_write_json(&path, &unsupported).unwrap();
        assert!(matches!(
            store.load_player_lobby("old"),
            Err(StorageError::UnsupportedFormat(99))
        ));
        let old_init = GameInitRecord {
            game_id: "old".into(),
            seed: None,
            player_ids: vec![PlayerId::new("p1")],
            initial_state: crate::fixtures::create_sample_game(),
            seats: BTreeMap::new(),
            seat_tokens: BTreeMap::new(),
            map_tiles: Vec::new(),
        };
        store.save_init(&old_init).unwrap();
        let mut debug_init = old_init.clone();
        debug_init
            .seat_tokens
            .insert(PlayerId::new("p1"), "private-value".into());
        assert!(!format!("{debug_init:?}").contains("private-value"));
        assert!(matches!(
            store.load_player_init("old"),
            Err(StorageError::UnsupportedFormat(0))
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn bounds_checksum_and_reference_corruption_fail_closed() {
        let (store, dir) = store();
        let (mut lobby, _, _) = PlayerLobbyRecord::create("broken".into(), 2, 1, "Host").unwrap();
        lobby.slots[1].occupant = lobby.slots[0].occupant.clone();
        assert!(matches!(
            store.save_player_lobby(&lobby),
            Err(StorageError::InvalidPlayerRecord(_))
        ));
        lobby.slots[1].occupant = None;
        let id = lobby.host_player_id.clone();
        lobby.players.insert(
            PlayerId::new(hex_random_256("player_")),
            PlayerLobbyMember {
                ready: false,
                session: lobby.players[&id].session.clone(),
                nickname: "Test".into(),
            },
        );
        assert!(matches!(
            store.save_player_lobby(&lobby),
            Err(StorageError::InvalidPlayerRecord(_))
        ));
        lobby.players.retain(|player, _| player == &id);
        store.save_player_lobby(&lobby).unwrap();
        let path = store.game_dir("broken").unwrap().join("lobby.json");
        let bytes = fs::read_to_string(&path)
            .unwrap()
            .replace("\"checksum\": \"", "\"checksum\": \"0");
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            store.load_player_lobby("broken"),
            Err(StorageError::ChecksumMismatch)
        ));
        fs::write(&path, vec![b' '; MAX_PLAYER_LOBBY_BYTES + 1]).unwrap();
        assert!(matches!(
            store.load_player_lobby("broken"),
            Err(StorageError::Oversized { .. })
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn running_credentials_rotate_independently_of_immutable_init() {
        let (store, dir) = store();
        let (lobby, host, old) = PlayerLobbyRecord::create("running".into(), 1, 4, "Host").unwrap();
        let init = PlayerGameInitRecord {
            schema_version: PLAYER_RECORD_VERSION,
            game_id: lobby.game_id.clone(),
            seed: lobby.seed,
            player_ids: vec![host.clone()],
            initial_state: crate::fixtures::create_sample_game(),
            map_tiles: Vec::new(),
            seats: None,
        };
        store.save_player_init(&init).unwrap();
        let mut sessions = PlayerSessionsRecord {
            schema_version: PLAYER_RECORD_VERSION,
            game_id: lobby.game_id,
            sessions: BTreeMap::from([(host.clone(), old.clone())]),
            nicknames: BTreeMap::from([(host.clone(), "Host".into())]),
        };
        store.save_player_sessions(&sessions).unwrap();
        let replacement = PlayerSession::generate();
        sessions.sessions.insert(host.clone(), replacement.clone());
        store.save_player_sessions(&sessions).unwrap();
        let mut duplicate = sessions.clone();
        duplicate.sessions.insert(
            PlayerId::new(hex_random_256("player_")),
            replacement.clone(),
        );
        assert!(matches!(
            store.save_player_sessions(&duplicate),
            Err(StorageError::InvalidPlayerRecord(_))
        ));
        let mut missing_name = sessions.clone();
        missing_name.nicknames.clear();
        assert!(matches!(
            store.save_player_sessions(&missing_name),
            Err(StorageError::InvalidPlayerRecord("nickname roster"))
        ));
        let restarted = FileGameStore::new(&dir).unwrap();
        assert_eq!(
            restarted.load_player_sessions("running").unwrap().sessions[&host],
            replacement
        );
        assert_ne!(
            restarted.load_player_sessions("running").unwrap().sessions[&host],
            old
        );
        let init_file =
            fs::read_to_string(restarted.game_dir("running").unwrap().join("init.json")).unwrap();
        assert!(!init_file.contains(old.as_str()));
        assert!(!init_file.contains(replacement.as_str()));
        assert_eq!(
            restarted.load_player_init("running").unwrap().player_ids,
            vec![host]
        );
        let path = restarted
            .game_dir("running")
            .unwrap()
            .join("player_sessions.json");
        fs::write(&path, vec![b' '; MAX_PLAYER_SESSIONS_BYTES + 1]).unwrap();
        assert!(matches!(
            restarted.load_player_sessions("running"),
            Err(StorageError::Oversized { .. })
        ));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn nickname_validation_is_exact_bounded_and_nonunique() {
        for name in ["Élodie", "東京", "Same Name", "a-b_1", &"a".repeat(64)] {
            validate_nickname(name).unwrap();
        }
        for name in [
            "",
            " ",
            " leading",
            "trailing ",
            "new\nline",
            "a\u{200d}b",
            "a\u{202e}b",
            &"é".repeat(33),
        ] {
            assert!(matches!(
                validate_nickname(name),
                Err(StorageError::InvalidPlayerRecord("nickname"))
            ));
        }
    }
}
