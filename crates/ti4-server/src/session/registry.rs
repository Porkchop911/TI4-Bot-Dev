//! Registry for pre-game lobbies and active game sessions.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use ti4_content::ContentStore;
use ti4_model::id::PlayerId;

use ti4_host::protocol::server::{ActionAcceptedMsg, InitialSnapshotMsg};
use ti4_host::protocol::status::{RejectionReason, ViewerRole};

use crate::session::{GameSession, SeatController, SessionConfig};
use crate::storage::{
    GameHistory, GameInitRecord, LobbyRecord, PersistedLobbyPhase, PersistedLobbySeat, StorageError,
};
use ti4_host::batch::{BatchFailure, BatchRecord, BatchRequest, simulate};

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
    use crate::storage::FileGameStore;
    use ti4_host::batch::{BatchKind, MovementPlan, MovementStep};

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
            ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
                .unwrap();
        let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
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

    /// A two-seat game advanced to its first tactical movement boundary.
    fn game_at_movement(
        registry: &GameRegistry,
        game_id: &str,
    ) -> (Arc<GameSession>, String, BatchRequest) {
        let host = PlayerId::new("p1");
        let guest = PlayerId::new("p2");
        let players = vec![host.clone(), guest.clone()];
        let (state, galaxy) =
            ti4_host::map::create_game_with_template(ContentStore::embedded(), &players, 42, None)
                .unwrap();
        let tiles = ti4_host::map::build_board_tiles(ContentStore::embedded(), &galaxy);
        let config = SessionConfig::new(game_id, state)
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
        let request = BatchRequest {
            request_id: format!("{game_id}_batch"),
            expected_version: version,
            nonce,
            plan: MovementPlan {
                kind: BatchKind::TacticalMovement,
                destination: session.current_state().active_system.unwrap().to_string(),
                steps: vec![MovementStep::DoneMoving],
            },
        };
        (session, token, request)
    }

    fn is_hh_mm_ss(value: &str) -> bool {
        let b = value.as_bytes();
        b.len() == 8
            && b[2] == b':'
            && b[5] == b':'
            && [0, 1, 3, 4, 6, 7].iter().all(|&i| b[i].is_ascii_digit())
    }

    /// A batch's replacement replay must not hold the registry lock (P1).
    ///
    /// Runs 06 and 23 of the 2026-10-06 sweep timed out on `/snapshot` while a batch replayed a
    /// thousand decisions with the global lock held: every read of every game waited for it.
    #[test]
    #[allow(
        clippy::result_large_err,
        reason = "the closure returns the registry's own batch result"
    )]
    fn a_slow_batch_replay_does_not_block_registry_reads() {
        use std::sync::atomic::{AtomicBool, Ordering};
        const SLOW_REPLAY: Duration = Duration::from_millis(1500);

        let registry = GameRegistry::new();
        let (before, token, request) = game_at_movement(&registry, "slow_replay");
        let other = game_at_movement(&registry, "bystander").0;
        let seat = before.current_pending_decision().unwrap().0;
        let replaying = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let batch = scope.spawn(|| {
                registry.submit_batch_with_worker("slow_replay", &token, request, |config| {
                    replaying.store(true, Ordering::SeqCst);
                    std::thread::sleep(SLOW_REPLAY);
                    GameSession::start(config)
                })
            });
            let deadline = Instant::now() + Duration::from_secs(10);
            while !replaying.load(Ordering::SeqCst) {
                assert!(
                    Instant::now() < deadline,
                    "the batch never reached its replay"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
            let started = Instant::now();
            // The game being replaced still answers, from the stopped pre-batch session.
            let stale = registry.get_game("slow_replay").unwrap();
            assert!(Arc::ptr_eq(&stale, &before));
            let snapshot = registry
                .player_snapshot("slow_replay", Some(&token), &stale)
                .unwrap();
            assert_eq!(snapshot.game_id, "slow_replay");
            // So does every other game, and a spectator view.
            let bystander = registry.get_game("bystander").unwrap();
            registry
                .player_snapshot("bystander", None, &bystander)
                .unwrap();
            let elapsed = started.elapsed();
            assert!(
                elapsed < Duration::from_millis(500),
                "registry reads waited {elapsed:?} for a batch replay"
            );
            assert!(!batch.is_finished(), "the reads overlapped the replay");

            let result = batch.join().unwrap().unwrap();
            let live = registry.get_game("slow_replay").unwrap();
            assert!(!Arc::ptr_eq(&live, &before), "the replacement is published");
            assert_eq!(live.history_status().cursor, result.end_cursor);
            let _ = pending(&live);
            assert!(before.current_pending_decision().is_none());
            assert!(
                before
                    .submit_choice(&seat, "any", result.snapshot.game_version, "any")
                    .is_err(),
                "the stopped session refuses choices"
            );
            live.stop();
        });
        other.stop();
    }

    /// `remove_game` is not serialised by the game gate; a batch must not resurrect a game
    /// removed while its replacement replayed.
    #[test]
    #[allow(
        clippy::result_large_err,
        reason = "the closure returns the registry's own batch result"
    )]
    fn a_game_removed_during_the_batch_replay_stays_removed() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let registry = GameRegistry::new();
        let (_, token, request) = game_at_movement(&registry, "removed_mid_batch");
        let replaying = AtomicBool::new(false);
        let release = AtomicBool::new(false);
        std::thread::scope(|scope| {
            let batch = scope.spawn(|| {
                registry.submit_batch_with_worker("removed_mid_batch", &token, request, |config| {
                    replaying.store(true, Ordering::SeqCst);
                    while !release.load(Ordering::SeqCst) {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    GameSession::start(config)
                })
            });
            while !replaying.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(2));
            }
            assert!(registry.remove_game("removed_mid_batch").is_some());
            release.store(true, Ordering::SeqCst);
            let error = batch.join().unwrap().unwrap_err();
            assert_eq!(error.reason, "game not found");
        });
        assert!(registry.get_game("removed_mid_batch").is_none());
    }

    #[test]
    fn batch_events_carry_wall_clock_timestamps() {
        let path = std::env::temp_dir().join(format!("ti4_ts_{:032x}", rand::random::<u128>()));
        let store = Arc::new(FileGameStore::new(&path).unwrap());
        let registry = GameRegistry::new().with_store(store.clone());
        let (_, token, request) = game_at_movement(&registry, "batch_ts");
        let result = registry.submit_batch("batch_ts", &token, request).unwrap();
        let history = store.load_history("batch_ts").unwrap().unwrap();
        let batch_events: Vec<_> = history
            .events
            .iter()
            .filter(|e| e.batch_id.as_deref() == Some(result.batch_id.as_str()))
            .collect();
        assert!(!batch_events.is_empty());
        for event in &history.events {
            assert!(
                is_hh_mm_ss(&event.timestamp),
                "event {} has timestamp {:?}",
                event.id,
                event.timestamp
            );
        }
        registry.get_game("batch_ts").unwrap().stop();
        drop(registry);
        std::fs::remove_dir_all(path).unwrap();
    }
}

/// The decisions and the events of a session, with what the host can redo.
struct Timeline {
    current: Vec<ti4_engine::choice::DecisionRecord>,
    redo: Vec<ti4_engine::choice::DecisionRecord>,
    events: Vec<ti4_host::protocol::server::GameEvent>,
    redo_events: Vec<ti4_host::protocol::server::GameEvent>,
    event_counter: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HistoryError {
    NotFound,
    Forbidden(String),
    Conflict(String),
    InvalidTarget(String),
    Storage(String),
}

impl HistoryError {
    /// Human-readable explanation for the requesting client.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "Game not found".to_owned(),
            Self::Forbidden(why) => format!("History change forbidden: {why}"),
            Self::Conflict(why) => format!("History change conflicts with the game: {why}"),
            Self::InvalidTarget(why) => format!("Invalid history target: {why}"),
            Self::Storage(why) => format!("History could not be saved: {why}"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchResult {
    pub request_id: String,
    pub batch_id: String,
    pub start_cursor: usize,
    pub end_cursor: usize,
    pub active: bool,
    /// Present when the batch stopped early at a reaction window; the rest of the plan was not
    /// applied and the window is pending for its seat.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interrupted: Option<ti4_host::batch::BatchInterruption>,
    pub snapshot: InitialSnapshotMsg,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatchError {
    pub failed_step: usize,
    /// Stable machine-readable reason; HTTP status mapping keys on it.
    pub reason: String,
    pub expected: String,
    pub offered_summary: Vec<String>,
    /// Human-readable explanation, safe to show to the submitting seat.
    pub message: String,
    #[serde(skip_serializing_if = "is_zero")]
    pub planned_steps: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offered: Option<Box<ti4_host::batch::OfferedDecision>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_version: Option<u64>,
}

#[allow(
    clippy::trivially_copy_pass_by_ref,
    reason = "serde's skip_serializing_if passes the field by reference"
)]
fn is_zero(value: &usize) -> bool {
    *value == 0
}

impl From<BatchFailure> for BatchError {
    fn from(value: BatchFailure) -> Self {
        Self {
            failed_step: value.failed_step,
            reason: value.reason,
            expected: value.expected,
            offered_summary: value.offered_summary,
            message: value.message,
            planned_steps: value.planned_steps,
            offered: value.offered,
            expected_version: None,
            current_version: None,
        }
    }
}

impl BatchError {
    pub fn simple(reason: &str) -> Self {
        Self::explained(reason, reason)
    }

    /// An error whose stable `reason` is accompanied by a specific explanation.
    pub fn explained(reason: &str, message: impl Into<String>) -> Self {
        Self {
            failed_step: 0,
            reason: reason.into(),
            expected: String::new(),
            offered_summary: Vec::new(),
            message: message.into(),
            planned_steps: 0,
            offered: None,
            expected_version: None,
            current_version: None,
        }
    }

    fn with_versions(mut self, expected: u64, current: u64) -> Self {
        self.expected_version = Some(expected);
        self.current_version = Some(current);
        self
    }
}

use crate::storage::{
    LobbySlotId, PLAYER_RECORD_VERSION, PlayerGameInitRecord, PlayerLobbyMember, PlayerLobbyRecord,
    PlayerLobbySlot, PlayerSession, PlayerSessionsRecord, generate_player_id,
};

/// The in-process bot that picks uniformly among the legal options.
pub const RANDOM_BOT_KIND: &str = "random";
/// The in-process bot that plays the board heuristically and the rest at random.
pub const SMART_BOT_KIND: &str = "smart";
/// Bot kinds answered inside the game worker, in the order a lobby offers them: `random`,
/// `smart`, then `mlp:<name>` for every model the process loaded (`ti4_host::bots::models`) and
/// offers (`crate::models::is_offered`).
#[must_use]
pub fn in_process_bot_kinds() -> Vec<String> {
    [RANDOM_BOT_KIND, SMART_BOT_KIND]
        .into_iter()
        .map(str::to_owned)
        .chain(
            ti4_host::bots::models()
                .iter()
                .filter(|model| crate::models::is_offered(model.name()))
                .map(ti4_host::bots::MlpModel::kind_id),
        )
        .collect()
}

/// Whether `kind` is one of [`in_process_bot_kinds`]: an MLP model is always `mlp:<name>`.
#[must_use]
pub fn is_in_process_bot_kind(kind: &str) -> bool {
    ti4_host::bots::BotKind::parse(kind).is_ok()
}

/// Public projection; deliberately cannot serialize the private persistence record.
#[derive(Debug, Clone, Serialize)]
pub struct PlayerLobbyView {
    pub game_id: String,
    pub phase: LobbyPhase,
    pub host_player_id: PlayerId,
    pub slots: Vec<PlayerSlotView>,
    pub lobby_version: u64,
    /// The bot kinds the host can add to this lobby ([`in_process_bot_kinds`]): `random`, `smart`,
    /// and `mlp:<name>` for every loaded model.
    pub bot_kinds: Vec<String>,
    /// What the table will play on; never the seed.
    pub map: ti4_host::maps::MapChoiceView,
    /// Changes whenever the previewed board changes; refetch the preview when it does.
    pub map_revision: u64,
    /// The strategy card set this table plays (`te`, `pok`, `base_game_codex1`); `pok` for a
    /// game created before the option existed.
    pub strategy_card_set: String,
    /// Who opens as speaker: `random` (drawn from the seed) or `seat` (the first seat).
    pub speaker_order: String,
    /// The factions a seat can be given (`ti4_engine::seating::LOBBY_FACTIONS`, base game first): what
    /// every seat's faction picker offers. A seat that chose none is dealt one of the rest by the seed.
    pub factions: Vec<String>,
    /// The wait before each answer of a seat that the server plays, in milliseconds: the value
    /// of the table, or the value of the process when the host set none.
    pub bot_delay_ms: u64,
    /// The scenario that the game starts from (`ti4_host::view::scenario`); absent for round 1. With a
    /// scenario the player count, the map, the strategy cards and the factions are its own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_from: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PlayerSlotView {
    pub slot_id: LobbySlotId,
    pub position: usize,
    pub occupant: Option<PlayerId>,
    pub nickname: Option<String>,
    pub connected: bool,
    pub can_take_over: bool,
    /// The kind of bot occupying the seat (`random` or `smart`), absent for a person or an MLP bot (which
    /// connects like a person).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bot: Option<String>,
    /// The faction chosen for this seat (by the host, or by the person in it); absent for "random".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub faction: Option<String>,
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
                legacy_ready: None,
                bot: None,
                faction: None,
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
            map_template: None,
            start_preset: None,
            strategy_card_set: Some(ti4_host::setup::card_set::DEFAULT.to_owned()),
            speaker_order: None,
            map_revision: 0,
            legacy_seat_order: None,
            bot_delay_ms: None,
            start_from: None,
            lobby_version: 1,
        };
        lobby.validate()?;
        Ok((lobby, host_player_id, session))
    }

    /// The faction each seat asked for, in slot order (`None`: any, and every open seat).
    #[must_use]
    pub fn seat_factions(&self) -> Vec<Option<String>> {
        self.slots
            .iter()
            .map(|slot| {
                slot.occupant
                    .as_ref()
                    .and_then(|id| self.players[id].faction.clone())
            })
            .collect()
    }

    /// Checks that `faction` can be seated for `except` (the seat being set, if any): it must be
    /// in the pool and no other seat may hold it.
    ///
    /// # Errors
    /// [`LobbyError::InvalidFaction`] or [`LobbyError::FactionTaken`], with a message for the host.
    pub fn check_faction_free(
        &self,
        faction: &str,
        except: Option<&PlayerId>,
    ) -> Result<(), LobbyError> {
        if !ti4_engine::seating::LOBBY_FACTIONS.contains(&faction) {
            return Err(LobbyError::InvalidFaction(format!(
                "Faction '{faction}' is not available: choose one of {}",
                ti4_engine::seating::LOBBY_FACTIONS.join(", ")
            )));
        }
        if let Some((_, holder)) = self
            .players
            .iter()
            .find(|(id, member)| Some(*id) != except && member.faction.as_deref() == Some(faction))
        {
            // The message is shown to a player as it is: it names the faction as a table does.
            let name = ti4_content::factions::get(ContentStore::embedded(), faction)
                .map_or(faction, |printed| printed.short_name());
            return Err(LobbyError::FactionTaken(format!(
                "{name} is already taken by {}",
                holder.nickname
            )));
        }
        Ok(())
    }

    /// Only this explicit projection is suitable for public HTTP/WS output.
    #[must_use]
    pub fn public_view(&self) -> PlayerLobbyView {
        // A scenario is a game of its own: its seats have its factions, in the order of the join,
        // and it plays the cards of a game that names no set.
        let scenario = self.start_from.is_some();
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
                    connected: false,
                    can_take_over: false,
                    bot: slot
                        .occupant
                        .as_ref()
                        .and_then(|id| self.players[id].bot.clone()),
                    faction: if scenario {
                        slot.occupant
                            .as_ref()
                            .map(|_| ti4_engine::seating::seat_faction(index).to_owned())
                    } else {
                        slot.occupant
                            .as_ref()
                            .and_then(|id| self.players[id].faction.clone())
                    },
                })
                .collect(),
            lobby_version: self.lobby_version,
            bot_kinds: in_process_bot_kinds(),
            map: ti4_host::maps::catalog::describe(
                &ti4_host::maps::MapChoice::from_stored(self.map_template.as_deref()),
                self.slots.len(),
            ),
            map_revision: self.map_revision,
            strategy_card_set: if scenario {
                crate::storage::LEGACY_CARD_SET.to_owned()
            } else {
                crate::storage::stored_card_set(self.strategy_card_set.as_deref()).to_owned()
            },
            speaker_order: ti4_host::setup::SpeakerOrder::from_stored(
                self.speaker_order.as_deref(),
            )
            .as_str()
            .to_owned(),
            factions: ti4_engine::seating::LOBBY_FACTIONS
                .iter()
                .map(|faction| (*faction).to_owned())
                .collect(),
            bot_delay_ms: self
                .bot_delay_ms
                .unwrap_or_else(super::random_bot::random_bot_delay_ms),
            start_from: self.start_from.clone(),
        }
    }
}

/// The name of every player of a table, for the session of its game.
fn nicknames_of(record: &PlayerLobbyRecord) -> BTreeMap<PlayerId, String> {
    record
        .players
        .iter()
        .map(|(id, member)| (id.clone(), member.nickname.clone()))
        .collect()
}

/// Summary of an active, completed, or unstarted game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GameSummary {
    pub game_id: String,
    pub is_finished: bool,
    pub pending_seat: Option<PlayerId>,
}

/// One open game of a caller, as listed in the "too many games" refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OpenGameInfo {
    pub game_id: String,
    /// `lobby` or `started`.
    pub phase: &'static str,
    pub players: usize,
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
    /// The call is for a running game, and the table is still a lobby.
    NotRunning,
    InvalidCapability,
    HostRequired,
    /// A table starts when every seat is filled.
    SeatsOpen,
    /// A smaller table has no seat for a person who is at it; the text names who.
    SeatsTaken(String),
    /// A setting that the table cannot take (a value outside its range, an unknown name).
    InvalidSetting(String),
    /// The table starts from a scenario, which has its own seats, map, cards and factions.
    ScenarioTable(String),
    SeatUnavailable,
    TakeoverUnavailable,
    InvalidPlayerId,
    InvalidNickname,
    /// A faction the lobby cannot seat (unknown, outside the pool, or asked for a person's seat).
    InvalidFaction(String),
    /// A faction another seat already holds; the text names the faction and the seat.
    FactionTaken(String),
    /// A map choice the lobby cannot use (unknown, wrong size, or does not build).
    InvalidMap(String),
    Map(String),
    Storage(String),
    /// A seat of the table is played by a model that the server has not loaded; the text names it.
    ModelMissing(String),
}

impl LobbyError {
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "Game not found".to_owned(),
            Self::NotInLobby => "Game has no lobby".to_owned(),
            Self::AlreadyRunning => "Game has already started".to_owned(),
            Self::NotRunning => "Game has not started".to_owned(),
            Self::InvalidCapability => "Invalid seat capability".to_owned(),
            Self::HostRequired => "Only the lobby host may perform this action".to_owned(),
            Self::SeatsOpen => "Every seat must be filled before starting".to_owned(),
            Self::SeatsTaken(error) | Self::InvalidSetting(error) => error.clone(),
            Self::ScenarioTable(error) => format!("The table starts from a scenario: {error}"),
            Self::SeatUnavailable => "Seat is unavailable".to_owned(),
            Self::TakeoverUnavailable => "Takeover is not available yet".to_owned(),
            Self::InvalidPlayerId => "Invalid player ID".to_owned(),
            Self::InvalidNickname => "Nickname must be trimmed, nonempty Unicode text of at most 64 UTF-8 bytes without control or formatting characters".to_owned(),
            Self::InvalidFaction(error) | Self::FactionTaken(error) => error.clone(),
            Self::InvalidMap(error) => format!("Invalid map choice: {error}"),
            Self::Map(error) => format!("Failed to start game with map: {error}"),
            Self::Storage(error) => format!("Failed to persist lobby lifecycle: {error}"),
            Self::ModelMissing(error) => format!("The table cannot start: {error}"),
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
    /// Redos in flight, by game: the saved original timeline and where the redo stands.
    /// Mirrors `turn_redo.json`; loaded from the store on first use after a restart.
    turn_redos: Mutex<BTreeMap<String, ti4_host::history::turn_redo::TurnRedoRecord>>,
    /// Games ended by their host since this process started; open sockets use it to say why they close.
    terminated: Mutex<BTreeSet<String>>,
    /// Public-playtest hardening; inert by default.
    public: Arc<crate::public::PublicState>,
    /// One channel for each table that a socket watches (`ws/table.rs`): a counter that goes up
    /// when the lobby record or the presence of a player changes.
    tables: Mutex<BTreeMap<String, tokio::sync::watch::Sender<u64>>>,
    /// The models of `TI4_MODEL_DIR` with their files, for `GET /api/models` (`crate::models`).
    models: Vec<crate::models::ServedModel>,
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
            turn_redos: Mutex::new(BTreeMap::new()),
            terminated: Mutex::new(BTreeSet::new()),
            public: Arc::new(crate::public::PublicState::default()),
            tables: Mutex::new(BTreeMap::new()),
            models: Vec::new(),
        }
    }
}

mod turn_redo_ops;

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
    // The retry replays everything: it must not depend on whatever made the first attempt fail.
    let mut config = config;
    config.resume = None;
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
    #[allow(
        clippy::result_large_err,
        reason = "a batch error is built once per rejected request and serialized to the client"
    )]
    pub fn submit_batch(
        &self,
        game_id: &str,
        credential: &str,
        request: BatchRequest,
    ) -> Result<BatchResult, BatchError> {
        self.submit_batch_with_worker(game_id, credential, request, GameSession::start)
    }

    #[allow(
        clippy::result_large_err,
        reason = "a batch error is built once per rejected request and serialized to the client"
    )]
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
            return Err(BatchError::explained(
                "invalid request_id",
                "request_id must be 1 to 128 characters of letters, digits, '-' or '_'",
            ));
        }
        if let Some(batch) = session
            .batches()
            .iter()
            .find(|b| b.request_id == request.request_id)
        {
            if batch.actor != actor {
                return Err(BatchError::explained(
                    "request_id already used",
                    format!(
                        "request_id {} was already used by another seat",
                        request.request_id
                    ),
                ));
            }
            if !session.history_ready() {
                return Err(BatchError::explained(
                    "batch committed, replacement session unavailable",
                    "this batch was already committed, but the game is still reloading its history; retry shortly",
                ));
            }
            return Ok(BatchResult {
                request_id: request.request_id,
                batch_id: batch.batch_id.clone(),
                start_cursor: batch.start_cursor,
                end_cursor: batch.end_cursor,
                active: batch.end_cursor <= session.decision_log().len(),
                interrupted: batch.interrupted.clone(),
                snapshot: session.get_snapshot(&ViewerRole::Player(actor)),
            });
        }
        let current_version = session.game_version();
        let pending = session.current_pending_decision();
        let stale = if !session.history_ready() {
            Some("the game is reloading its history".to_owned())
        } else if current_version != request.expected_version {
            Some(format!(
                "the plan was built for version {} but the game is at version {current_version}",
                request.expected_version
            ))
        } else {
            match &pending {
                None => Some("no decision is pending".to_owned()),
                Some((seat, _, _)) if *seat != actor => {
                    Some("the pending decision belongs to another seat".to_owned())
                }
                Some((_, nonce, _)) if *nonce != request.nonce => Some(
                    "the pending decision was re-offered since the plan was built (nonce changed)"
                        .to_owned(),
                ),
                Some((_, _, version)) if *version != request.expected_version => Some(format!(
                    "the pending decision was offered at version {version}, not {}",
                    request.expected_version
                )),
                Some(_) => None,
            }
        };
        if let Some(why) = stale {
            return Err(BatchError::explained(
                "stale decision boundary",
                format!("stale decision boundary: {why}; refresh and plan again"),
            )
            .with_versions(request.expected_version, current_version));
        }
        if request.plan.kind == ti4_host::batch::BatchKind::TacticalMovement
            && session
                .current_state()
                .active_system
                .as_ref()
                .map(|id| id.as_str())
                != Some(request.plan.destination.as_str())
        {
            let active = session
                .current_state()
                .active_system
                .as_ref()
                .map_or_else(|| "no system".to_owned(), |id| format!("system {id}"));
            return Err(BatchError::explained(
                "movement destination changed",
                format!(
                    "the plan moves into system {} but {active} is active",
                    request.plan.destination
                ),
            ));
        }
        let config = session.restart_config();
        let (prior, step_snapshot) = session.batch_basis();
        drop(state);
        let simulation = simulate(
            &config,
            &prior,
            &actor,
            &request.plan,
            step_snapshot.as_deref(),
        )?;
        let boundary_state = simulation.boundary_state.clone();
        let decisions = simulation.decisions;
        let start_cursor = prior.len();
        let end_cursor = start_cursor + decisions.len();
        let batch_id = format!("batch_{:032x}", rand::random::<u128>());
        let all_decisions: Vec<_> = prior.iter().chain(decisions.iter()).cloned().collect();
        let mut events = session.event_log();
        let shown_rolls = session.rolls_seen();
        let (_, mut counter) = session.history_events();
        counter = counter.max(events.len() as u64);
        // One wall-clock reading for the whole batch, in the worker's HH:MM:SS format.
        let timestamp = ti4_host::clock::Clock::utc_time_string(&ti4_host::clock::SystemClock);
        for (i, decision) in decisions.iter().enumerate() {
            let offered = &simulation.selected[i];
            let action_id = if decision
                .context
                .as_ref()
                .is_some_and(|c| c.phase == ti4_model::state::Phase::Action)
            {
                ti4_host::protocol::server::action_id_for(&all_decisions, start_cursor + i + 1)
            } else {
                None
            };
            let action_start_cursor = action_id
                .as_ref()
                .and_then(|id| id.strip_prefix("action_"))
                .and_then(|n| n.parse::<usize>().ok())
                .and_then(|n| n.checked_sub(1));
            let ti4_host::protocol::server::DecisionFacts {
                detail,
                movement,
                seat_detail,
                fact,
            } = ti4_host::protocol::server::verified_event_facts(
                decision,
                Some(offered),
                (request.plan.kind == ti4_host::batch::BatchKind::TacticalMovement)
                    .then_some(request.plan.destination.as_str()),
                &boundary_state,
            );
            let grouping = ti4_host::protocol::server::decision_grouping(
                decision,
                Some(offered),
                &all_decisions,
                start_cursor + i + 1,
            );
            counter += 1;
            events.push(ti4_host::protocol::server::GameEvent {
                id: format!("{game_id}-{counter}"),
                timestamp: timestamp.clone(),
                version: Some(request.expected_version),
                visibility: ti4_host::protocol::server::EventVisibility::Public,
                event: ti4_host::protocol::server::GameEventKind::DecisionResolved,
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
                fact,
                seat_detail,
                private_detail: None,
                roll: None,
            });
            for (_, phase, round) in simulation
                .transitions
                .iter()
                .filter(|(cursor, _, _)| *cursor == start_cursor + i + 1)
            {
                counter += 1;
                events.push(ti4_host::protocol::server::GameEvent {
                    id: format!("{game_id}-{counter}"),
                    timestamp: timestamp.clone(),
                    version: Some(request.expected_version),
                    visibility: ti4_host::protocol::server::EventVisibility::Public,
                    event: ti4_host::protocol::server::GameEventKind::PhaseTransition {
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
                    fact: None,
                    seat_detail: None,
                    private_detail: None,
                    roll: None,
                });
            }
            // Dice the batch rolled that the table has not been shown, after the decision that
            // was current when their step returned.
            for (index, roll, cursor) in &simulation.rolls {
                if *index < shown_rolls
                    || (*cursor).clamp(start_cursor + 1, end_cursor) != start_cursor + i + 1
                {
                    continue;
                }
                let active = boundary_state.active_system.as_ref().map(|id| id.as_str());
                let Some(public) = ti4_host::protocol::server::public_roll(*index, roll, active)
                else {
                    continue;
                };
                let action = ti4_host::journal::roll_action(
                    &all_decisions,
                    start_cursor + i + 1,
                    boundary_state.phase,
                );
                counter += 1;
                events.push(ti4_host::protocol::server::GameEvent {
                    id: format!("{game_id}-{counter}"),
                    timestamp: timestamp.clone(),
                    version: Some(request.expected_version),
                    visibility: ti4_host::protocol::server::EventVisibility::Public,
                    event: ti4_host::protocol::server::GameEventKind::DiceRolled,
                    decision_count: Some(start_cursor + i + 1),
                    batch_id: None,
                    batch_start_cursor: None,
                    batch_end_cursor: None,
                    action_id: action.action_id,
                    action_start_cursor: action.action_start_cursor,
                    actor: public.roller.clone(),
                    round: Some(boundary_state.round),
                    phase: Some(boundary_state.phase),
                    action_type: action.action_type,
                    action_actor: action.action_actor,
                    stage: None,
                    detail: Some(public.describe()),
                    movement: None,
                    fact: None,
                    seat_detail: None,
                    private_detail: None,
                    roll: Some(public),
                });
            }
        }
        if let Some(winner) = simulation.winner {
            counter += 1;
            events.push(ti4_host::protocol::server::GameEvent {
                id: format!("{game_id}-{counter}"),
                timestamp: timestamp.clone(),
                version: Some(request.expected_version),
                visibility: ti4_host::protocol::server::EventVisibility::Public,
                event: ti4_host::protocol::server::GameEventKind::GameFinished { winner },
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
                fact: None,
                seat_detail: None,
                private_detail: None,
                roll: None,
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
            interrupted: simulation.interruption.clone(),
        });
        let revision = request.expected_version.checked_add(1).ok_or_else(|| {
            BatchError::explained("version exhausted", "the game version counter is exhausted")
        })?;
        let history = GameHistory {
            decisions: prior.into_iter().chain(decisions).collect(),
            redo: Vec::new(),
            events,
            redo_events: Vec::new(),
            event_counter: counter,
            revision,
            generation: session.history_status().generation.saturating_add(1),
            batches,
            rng_marks: {
                let mut marks = session.rng_marks();
                marks.truncate_to(start_cursor);
                marks
            },
        };
        let mut state = self.state.lock().expect("registry lock");
        if !state
            .sessions
            .get(game_id)
            .is_some_and(|live| Arc::ptr_eq(live, &session))
            || session.game_version() != request.expected_version
            || !session.history_ready()
        {
            return Err(BatchError::explained(
                "game advanced during batch",
                "the game changed while the batch was being checked; refresh and plan again",
            )
            .with_versions(request.expected_version, session.game_version()));
        }
        session.stop();
        if session.decision_log().len() != start_cursor {
            let replacement = Arc::new(GameSession::start_recovered(
                config.clone(),
                session.decision_log(),
                session.event_log(),
            ));
            state.sessions.insert(game_id.to_owned(), replacement);
            return Err(BatchError::explained(
                "game advanced during batch",
                format!(
                    "another decision was recorded while the batch was being checked (log length {} instead of {start_cursor}); refresh and plan again",
                    session.decision_log().len()
                ),
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
            return Err(BatchError::simple(&format!("storage error: {error}")));
        }
        // The batch is durable and the old worker is stopped. Replaying the whole game into the
        // replacement can take seconds, so it runs without the registry lock: every snapshot,
        // websocket tick and lobby call for every game waits on that lock. This game stays
        // consistent without it because the game gate, held since the top of this function,
        // keeps out every other timeline mutation (choices, batches, history changes, takeover)
        // until the replacement is published. Meanwhile the stopped session keeps answering
        // reads with the pre-batch position, and a choice sent to it is refused.
        drop(state);
        let mut next = config;
        next.prior_decisions = history.decisions;
        next.prior_events = history.events;
        next.redo_decisions = history.redo;
        next.redo_events = history.redo_events;
        next.event_counter = history.event_counter;
        next.initial_version = revision;
        next.history_generation = history.generation;
        next.history_active = true;
        next.announce_tail = true;
        next.batches = history.batches;
        next.rng_marks = history.rng_marks;
        next.replay_boundary_state = Some(boundary_state);
        // The replacement forks the step-boundary copy of the session it replaces and replays
        // only the decisions after it (the worker re-checks that the copy fits this history and
        // otherwise replays everything; recovery from disk always replays everything).
        next.resume.clone_from(&step_snapshot);
        let (replacement, replay) = start_committed_worker(next, &mut start_worker);
        {
            let mut state = self.state.lock().expect("registry lock");
            // Only an ungated call can have touched this slot while the lock was released:
            // `remove_game` (or a new game created under the same id after it). The stopped
            // session cannot advance, so identity is the whole check. Never publish over a
            // session this batch did not stop.
            if !state
                .sessions
                .get(game_id)
                .is_some_and(|live| Arc::ptr_eq(live, &session))
            {
                drop(state);
                replacement.stop();
                return Err(BatchError::explained(
                    "game not found",
                    "the game was removed while the batch was being committed",
                ));
            }
            state
                .sessions
                .insert(game_id.to_owned(), replacement.clone());
        }
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
            interrupted: simulation.interruption,
            snapshot: replacement.get_snapshot(&ViewerRole::Player(actor)),
        })
    }
    /// Replaces the session by one whose game has the first `target` decisions of `timeline`:
    /// the rest waits as redo of the host. The caller holds the gate of the game and has
    /// checked that `session` is at `expected_version` and may be rewound. `undone` is what
    /// the replacement tells a seat with its next snapshot; `viewer` is the seat that is shown
    /// the replacement.
    #[expect(
        clippy::too_many_arguments,
        reason = "one history change: the session, its version, the target, the timeline, what was undone and who looks"
    )]
    fn rewind(
        &self,
        game_id: &str,
        session: &Arc<GameSession>,
        expected_version: u64,
        target: usize,
        timeline: Timeline,
        undone: Option<(PlayerId, Vec<ti4_host::planning::steps::PlanStep>)>,
        viewer: PlayerId,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let Timeline {
            current,
            redo,
            events,
            redo_events,
            event_counter,
        } = timeline;
        let original_count = current.len();
        let all: Vec<_> = current.into_iter().chain(redo).collect();
        let mut config = session.restart_config();
        config.undone = undone.into_iter().collect();
        let (report, copy) = ti4_host::replay::replay_session_with_copy(
            &config.state,
            config.galaxy.as_ref(),
            &all[..target],
            &config.rng_marks,
            session.history_status().generation.saturating_add(1),
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
                ti4_host::protocol::server::GameEventKind::DecisionResolved
            ) {
                count += 1;
            }
            if event.decision_count.unwrap_or(count) > target {
                break;
            }
            split += 1;
        }
        let revision = expected_version.checked_add(1).ok_or_else(|| {
            HistoryError::Conflict("the game version counter is exhausted".to_owned())
        })?;
        let history = GameHistory {
            decisions: all[..target].to_vec(),
            redo: all[target..].to_vec(),
            events: all_events[..split].to_vec(),
            redo_events: all_events[split..].to_vec(),
            event_counter: event_counter.max(all_events.len() as u64),
            revision,
            generation: session.history_status().generation.saturating_add(1),
            batches: session.batches(),
            rng_marks: config.rng_marks.clone(),
        };
        self.publish_history(
            game_id,
            session,
            config,
            history,
            expected_version,
            original_count,
            viewer,
            copy,
        )
    }

    /// Takes back the last decision of `player` ([`crate::session::undo`]): the seat's own
    /// undo, bounded by what it learned since. The session is replaced as a change of the
    /// host's history replaces it, and the decisions that are taken back wait as its redo.
    ///
    /// # Errors
    ///
    /// [`RejectionReason::UndoBarrier`] when the decision lies behind a barrier; as
    /// [`Self::submit_player_choice`] for a credential, a version or a game that does not fit.
    pub fn undo_player(
        &self,
        game_id: &str,
        credential: &str,
        player: &PlayerId,
        session: &Arc<GameSession>,
        expected_version: u64,
    ) -> Result<(), RejectionReason> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        // Random bots hold their answers for the whole change, as for a host's rewind.
        let _bot_pause = session.pause_bots();
        self.current_player_session(game_id, credential, player, session)
            .ok_or_else(|| RejectionReason::UnauthorizedSeat {
                seat: Some(player.clone()),
            })?;
        let (target, undone) = session.undo_plan(player, expected_version)?;
        let (redo_events, event_counter) = session.history_events();
        let timeline = Timeline {
            current: session.decision_log(),
            redo: session.redo_decisions(),
            events: session.event_log(),
            redo_events,
            event_counter,
        };
        self.rewind(
            game_id,
            session,
            expected_version,
            target,
            timeline,
            Some((player.clone(), undone)),
            player.clone(),
        )
        .map(|_| ())
        .map_err(|error| RejectionReason::ValidationFailed {
            message: error.message(),
        })
    }

    /// The host's seat for `game_id`, or why `credential` may not change history.
    fn require_host(
        state: &RegistryState,
        game_id: &str,
        credential: &str,
    ) -> Result<PlayerId, HistoryError> {
        if let Some(lobby) = state.player_lobbies.get(game_id) {
            let actor = authenticate_player(lobby, credential).map_err(|_| {
                HistoryError::Forbidden(
                    "the session credential is not valid for this game".to_owned(),
                )
            })?;
            if actor != lobby.host_player_id {
                return Err(HistoryError::Forbidden(
                    "only the host may change history".to_owned(),
                ));
            }
            Ok(actor)
        } else if let Some(lobby) = state.lobbies.get(game_id) {
            let actor = authenticated_seat(lobby, credential).map_err(|_| {
                HistoryError::Forbidden(
                    "the session credential is not valid for this game".to_owned(),
                )
            })?;
            if actor != lobby.host_seat {
                return Err(HistoryError::Forbidden(
                    "only the host may change history".to_owned(),
                ));
            }
            Ok(actor)
        } else {
            Err(HistoryError::NotFound)
        }
    }

    /// Host-only, read-only dry run of removing or changing one past decision (H7 phase 1).
    ///
    /// Replays the edited history in memory from the game's fixed seed. It changes no session,
    /// no stored history and no version; the registry lock is released before the replay.
    pub fn splice_preview(
        &self,
        game_id: &str,
        credential: &str,
        edit: &ti4_host::protocol::splice::SpliceEdit,
    ) -> Result<ti4_host::protocol::splice::SplicePreview, HistoryError> {
        let (decisions, config) = {
            let state = self.state.lock().expect("registry lock");
            Self::require_host(&state, game_id, credential)?;
            let session = state
                .sessions
                .get(game_id)
                .ok_or(HistoryError::NotFound)?
                .clone();
            (session.decision_log(), session.restart_config())
        };
        ti4_host::history::splice::splice_preview(
            &config.state,
            config.galaxy.as_ref(),
            &decisions,
            edit,
        )
        .map_err(|e| HistoryError::InvalidTarget(e.to_string()))
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
        let host = Self::require_host(&state, game_id, credential)?;
        let session = state
            .sessions
            .get(game_id)
            .ok_or(HistoryError::NotFound)?
            .clone();
        // Random bots hold their answers for the whole change: nothing may act between the
        // version check and the worker stop ("Game advanced during rewind").
        let _bot_pause = session.pause_bots();
        if session.game_version() != expected_version || !session.history_ready() {
            // Clients retry on this leading phrase, so keep it stable and append details.
            return Err(HistoryError::Conflict(if session.history_ready() {
                format!(
                    "Game advanced or a decision is in flight: the request expected version {expected_version} but the game is at version {}",
                    session.game_version()
                )
            } else {
                "Game advanced or a decision is in flight: history is still reloading".to_owned()
            }));
        }
        let current = session.decision_log();
        let redo = session.redo_decisions();
        let events = session.event_log();
        let (redo_events, event_counter) = session.history_events();
        let total = current.len() + redo.len();
        let target = match action {
            HistoryAction::Undo => current.len().checked_sub(1).ok_or_else(|| {
                HistoryError::InvalidTarget("there is nothing to undo".to_owned())
            })?,
            HistoryAction::UndoBatch => session
                .batches()
                .iter()
                .rev()
                .find(|b| b.end_cursor <= current.len())
                .map(|b| b.start_cursor)
                .ok_or_else(|| {
                    HistoryError::InvalidTarget("there is no committed batch to undo".to_owned())
                })?,
            HistoryAction::UndoPipeline => {
                let last = current.len().checked_sub(1).ok_or_else(|| {
                    HistoryError::InvalidTarget("there is nothing to undo".to_owned())
                })?;
                if let Some(id) = events
                    .iter()
                    .rev()
                    .find(|event| {
                        matches!(
                            event.event,
                            ti4_host::protocol::server::GameEventKind::DecisionResolved
                        ) && event.decision_count == Some(last + 1)
                    })
                    .and_then(|event| event.action_id.as_ref())
                {
                    events
                        .iter()
                        .find(|event| event.action_id.as_ref() == Some(id))
                        .and_then(|event| event.decision_count)
                        .and_then(|cursor| cursor.checked_sub(1))
                        .ok_or_else(|| {
                            HistoryError::InvalidTarget(format!(
                                "the start of action {id} is not in the event log"
                            ))
                        })?
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
                    return Err(HistoryError::InvalidTarget(
                        "there is nothing to redo".to_owned(),
                    ));
                }
                current.len() + 1
            }
            HistoryAction::RedoBatch => session
                .batches()
                .iter()
                .find(|b| b.start_cursor == current.len())
                .map(|b| b.end_cursor)
                .ok_or_else(|| {
                    HistoryError::InvalidTarget(format!(
                        "no undone batch starts at decision {}",
                        current.len()
                    ))
                })?,
            HistoryAction::RedoPipeline => {
                if redo.is_empty() {
                    return Err(HistoryError::InvalidTarget(
                        "there is nothing to redo".to_owned(),
                    ));
                }
                if let Some(id) = redo_events
                    .iter()
                    .find(|event| {
                        matches!(
                            event.event,
                            ti4_host::protocol::server::GameEventKind::DecisionResolved
                        ) && event.decision_count == Some(current.len() + 1)
                    })
                    .and_then(|event| event.action_id.as_ref())
                {
                    redo_events
                        .iter()
                        .filter(|event| event.action_id.as_ref() == Some(id))
                        .filter_map(|event| event.decision_count)
                        .max()
                        .ok_or_else(|| {
                            HistoryError::InvalidTarget(format!(
                                "action {id} has no undone decisions"
                            ))
                        })?
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
                    return Err(HistoryError::InvalidTarget(
                        "event_id is longer than 128 characters".to_owned(),
                    ));
                }
                let mut count = 0;
                let event = events
                    .iter()
                    .find(|event| {
                        if matches!(
                            event.event,
                            ti4_host::protocol::server::GameEventKind::DecisionResolved
                        ) {
                            count += 1;
                        }
                        event.id == event_id
                            && event.visibility.permits(&ViewerRole::Player(host.clone()))
                    })
                    .ok_or_else(|| {
                        HistoryError::InvalidTarget(format!(
                            "event {event_id} is not in the host's event log"
                        ))
                    })?;
                let target = event.decision_count.unwrap_or(count);
                if target >= current.len() {
                    return Err(HistoryError::InvalidTarget(format!(
                        "event {event_id} is at decision {target}, not before the current decision {}",
                        current.len()
                    )));
                }
                target
            }
            HistoryAction::RestoreCursor { cursor } => {
                if cursor >= current.len() {
                    return Err(HistoryError::InvalidTarget(format!(
                        "cursor {cursor} is not before the current decision {}",
                        current.len()
                    )));
                }
                cursor
            }
        };
        // A rewind that would hand the first removed decision to a random bot is walked back to
        // the nearest decision of a person: the bot is deterministic, so it would give the very
        // same answer at once, making the undo a no-op and the redo stack disappear.
        let target = if target < current.len() {
            let seats = session.lobby_details().1;
            let is_bot = |index: usize| {
                seats
                    .get(&current[index].player)
                    .is_some_and(SeatController::is_in_process_bot)
            };
            if is_bot(target) {
                (0..=target)
                    .rev()
                    .find(|&index| !is_bot(index))
                    .ok_or_else(|| {
                        HistoryError::InvalidTarget("there is nothing to undo".to_owned())
                    })?
            } else {
                target
            }
        } else {
            target
        };
        if target > total {
            return Err(HistoryError::InvalidTarget(format!(
                "decision {target} is beyond the recorded history of {total} decisions"
            )));
        }
        drop(state);
        let timeline = Timeline {
            current,
            redo,
            events,
            redo_events,
            event_counter,
        };
        self.rewind(
            game_id,
            &session,
            expected_version,
            target,
            timeline,
            None,
            host,
        )
    }

    /// Replace the live session by one that replays `history`, the shared tail of every history
    /// change (rewind, redo, turn redo, restore of an original timeline).
    ///
    /// Stops the live worker once it is quiescent, saves the history, and starts a replacement
    /// that replays to the end of `history.decisions` and waits there. The game version becomes
    /// `history.revision` and the history generation is whatever `history` carries, so every
    /// websocket client resyncs.
    #[expect(
        clippy::too_many_arguments,
        reason = "one serialized publication boundary shared by every history change"
    )]
    pub(crate) fn publish_history(
        &self,
        game_id: &str,
        session: &Arc<GameSession>,
        config: SessionConfig,
        history: GameHistory,
        expected_version: u64,
        original_count: usize,
        host: PlayerId,
        copy: Option<ti4_host::step_snapshot::StepSnapshot>,
    ) -> Result<InitialSnapshotMsg, HistoryError> {
        let revision = history.revision;
        let mut next = config.clone();
        next.prior_decisions.clone_from(&history.decisions);
        next.prior_events.clone_from(&history.events);
        next.redo_decisions.clone_from(&history.redo);
        next.rng_marks.clone_from(&history.rng_marks);
        next.redo_events.clone_from(&history.redo_events);
        next.event_counter = history.event_counter;
        next.initial_version = revision;
        next.history_active = true;
        next.history_generation = history.generation;
        next.batches.clone_from(&history.batches);
        // The replacement's private replay runs here, before the registry lock is taken.
        Self::prepare_replacement(&mut next, copy);
        // A live worker must be quiescent before publishing the new authoritative branch.
        let mut state = self.state.lock().expect("registry lock");
        if !state
            .sessions
            .get(game_id)
            .is_some_and(|live| Arc::ptr_eq(live, session))
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

    /// Do the replacement worker's private replay up front: the position at its first live
    /// question and a copy of the game near the end of its decisions, which the worker forks
    /// instead of replaying them all (`spawn_session_worker` would otherwise do the same replay,
    /// with the registry locked). `copy` is a copy the caller's own replay of these decisions
    /// already took; then only what it does not cover is replayed. When the replay fails nothing
    /// is prepared and the worker's own replay reports the failure, as before.
    fn prepare_replacement(
        next: &mut SessionConfig,
        copy: Option<ti4_host::step_snapshot::StepSnapshot>,
    ) {
        next.replay_boundary_state = None;
        next.resume = None;
        if next.prior_decisions.is_empty() {
            return;
        }
        let copy =
            copy.filter(|copy| next.step_snapshots && copy.is_prefix_of(&next.prior_decisions));
        let generation = (next.step_snapshots && copy.is_none()).then_some(next.history_generation);
        match ti4_host::batch::replay_boundary(
            next,
            &next.prior_decisions,
            copy.as_ref(),
            generation,
        ) {
            Ok((boundary, taken)) => {
                next.replay_boundary_state = Some(boundary);
                next.resume = copy
                    .or_else(|| taken.filter(|taken| taken.is_prefix_of(&next.prior_decisions)))
                    .map(Arc::new);
            }
            Err(_) => next.resume = copy.map(Arc::new),
        }
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
    pub fn with_public(mut self, config: crate::public::PublicConfig) -> Self {
        self.public = Arc::new(crate::public::PublicState::new(config));
        self
    }

    /// The models whose files `GET /api/models` serves. They are registered in the process
    /// (`ti4_host::bots::register`) by the caller (`crate::models::load_from_env`).
    #[must_use]
    pub fn with_models(mut self, models: Vec<crate::models::ServedModel>) -> Self {
        self.models = models;
        self
    }

    /// The models of the server with their files.
    #[must_use]
    pub fn models(&self) -> &[crate::models::ServedModel] {
        &self.models
    }

    /// Public-playtest settings and state.
    #[must_use]
    pub fn public(&self) -> &crate::public::PublicState {
        &self.public
    }

    fn player_view(&self, state: &RegistryState, record: &PlayerLobbyRecord) -> PlayerLobbyView {
        let mut view = record.public_view();
        // The seats of a running game have the factions that the start gave them, also the
        // seats that chose none.
        let seated = state
            .sessions
            .get(&record.game_id)
            .filter(|_| matches!(record.phase, PersistedLobbyPhase::Running))
            .map(|session| session.factions());
        for slot in &mut view.slots {
            if let Some(faction) = seated
                .as_ref()
                .zip(slot.occupant.as_ref())
                .and_then(|(seated, player)| seated.get(player))
            {
                slot.faction = Some(faction.clone());
            }
            if slot.bot.is_some() {
                // An in-process bot has no connection to lose: always present, never taken over.
                slot.connected = true;
                slot.can_take_over = false;
                continue;
            }
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
            .is_some_and(|l| l.players.get(player).is_some_and(|m| m.bot.is_none()))
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
        drop(state);
        self.table_changed(game_id);
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
        drop(state);
        self.table_changed(game_id);
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
                    // The names and the wait are of the table: its record has them as they
                    // were last, also after a seat was taken over or the host changed the wait.
                    session.set_nicknames(nicknames_of(&record));
                    if let Some(delay) = record.bot_delay_ms {
                        session.set_bot_delay_ms(delay);
                    }
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
        self.create_player_lobby_with_template(game_id, count, seed, nickname, None)
    }

    /// As [`Self::create_player_lobby`], laying the board out from the named map template.
    pub fn create_player_lobby_with_template(
        &self,
        game_id: String,
        count: usize,
        seed: u64,
        nickname: &str,
        map_template: Option<String>,
    ) -> Result<(PlayerLobbyView, PlayerId, PlayerSession), LobbyError> {
        self.create_player_lobby_with_options(game_id, count, seed, nickname, map_template, None)
    }

    /// As [`Self::create_player_lobby_with_template`], also naming a start preset that prepares
    /// the opening state (see [`ti4_host::preset`]).
    pub fn create_player_lobby_with_options(
        &self,
        game_id: String,
        count: usize,
        seed: u64,
        nickname: &str,
        map_template: Option<String>,
        start_preset: Option<String>,
    ) -> Result<(PlayerLobbyView, PlayerId, PlayerSession), LobbyError> {
        self.create_player_lobby_with_card_set(
            game_id,
            count,
            seed,
            nickname,
            map_template,
            start_preset,
            ti4_host::setup::card_set::DEFAULT,
        )
    }

    /// As [`Self::create_player_lobby_with_options`], also naming the strategy card set
    /// (one of [`ti4_host::setup::card_set::OFFERED`]).
    #[allow(
        clippy::too_many_arguments,
        reason = "one argument per creation option"
    )]
    pub fn create_player_lobby_with_card_set(
        &self,
        game_id: String,
        count: usize,
        seed: u64,
        nickname: &str,
        map_template: Option<String>,
        start_preset: Option<String>,
        strategy_card_set: &str,
    ) -> Result<(PlayerLobbyView, PlayerId, PlayerSession), LobbyError> {
        self.create_player_lobby_with_speaker(
            game_id,
            count,
            seed,
            nickname,
            map_template,
            start_preset,
            strategy_card_set,
            ti4_host::setup::SpeakerOrder::default(),
        )
    }

    /// As [`Self::create_player_lobby_with_card_set`], also choosing who opens as speaker: a
    /// seat drawn from the seed (the default) or the first seat.
    #[allow(
        clippy::too_many_arguments,
        reason = "one argument per creation option"
    )]
    pub fn create_player_lobby_with_speaker(
        &self,
        game_id: String,
        count: usize,
        seed: u64,
        nickname: &str,
        map_template: Option<String>,
        start_preset: Option<String>,
        strategy_card_set: &str,
        speaker_order: ti4_host::setup::SpeakerOrder,
    ) -> Result<(PlayerLobbyView, PlayerId, PlayerSession), LobbyError> {
        ti4_host::setup::card_set::validate(strategy_card_set)
            .map_err(|error| LobbyError::Storage(error.to_string()))?;
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
        let (mut record, player, credential) =
            PlayerLobbyRecord::create(game_id.clone(), count, seed, nickname)
                .map_err(|error| LobbyError::Storage(error.to_string()))?;
        record.map_template = map_template;
        record.start_preset = start_preset;
        record.strategy_card_set = Some(strategy_card_set.to_owned());
        record.speaker_order = Some(speaker_order.as_str().to_owned());
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
        // Every change of a lobby is written here, while the registry lock is held: a socket of
        // the table reads the new record once the lock is free.
        self.table_changed(&record.game_id);
        Ok(())
    }

    /// The channel of a table, for a socket that shows it (`ws/table.rs`). The value only says
    /// that something changed: the socket reads the view again.
    #[must_use]
    pub fn watch_table(&self, game_id: &str) -> tokio::sync::watch::Receiver<u64> {
        self.tables
            .lock()
            .expect("tables lock")
            .entry(game_id.to_owned())
            .or_insert_with(|| tokio::sync::watch::channel(0).0)
            .subscribe()
    }

    /// Drops the channel of a table that no socket watches any more.
    pub fn unwatch_table(&self, game_id: &str) {
        let mut tables = self.tables.lock().expect("tables lock");
        if tables
            .get(game_id)
            .is_some_and(|table| table.receiver_count() == 0)
        {
            tables.remove(game_id);
        }
    }

    /// Tells the sockets of a table that its lobby record or the presence of a player changed.
    fn table_changed(&self, game_id: &str) {
        if let Some(table) = self.tables.lock().expect("tables lock").get(game_id) {
            table.send_modify(|version| *version += 1);
        }
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
                legacy_ready: None,
                bot: None,
                faction: None,
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
        if lobby.players[player].bot.is_some() {
            return Err(LobbyError::TakeoverUnavailable);
        }
        if !self.disconnected_since(&state, game_id, player) {
            return Err(LobbyError::TakeoverUnavailable);
        }
        // A seat that the server plays has nobody to take over from.
        if state.sessions.get(game_id).is_some_and(|session| {
            session
                .lobby_details()
                .1
                .get(player)
                .is_some_and(|controller| *controller != SeatController::Human)
        }) {
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
            // The seat has the name of who took it: the next update of the game says so.
            session.set_nicknames([(player.clone(), nickname.to_owned())]);
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

    /// A script that is sent shares the takeover/history gate with live answers
    /// ([`GameSession::apply_draft`]). The gate is held while the script is checked on a copy.
    pub fn apply_player_draft(
        &self,
        game_id: &str,
        credential: &str,
        player: &PlayerId,
        session: &GameSession,
        nonce: &str,
        version: u64,
        script: &[ti4_host::planning::draft::DraftStep],
    ) -> Result<Option<ti4_host::protocol::server::ScriptStopped>, RejectionReason> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        self.current_player_session(game_id, credential, player, session)
            .ok_or_else(|| RejectionReason::UnauthorizedSeat {
                seat: Some(player.clone()),
            })?;
        session.apply_draft(player, nonce, version, script)
    }

    /// A run of a draft for the seat of this credential ([`GameSession::run_draft`]). `None`
    /// when the credential is not of `player`, or `session` is no longer the game.
    pub fn run_player_draft(
        &self,
        game_id: &str,
        credential: &str,
        player: &PlayerId,
        session: &GameSession,
        scope: &ti4_host::planning::audit::PlanScope,
        script: &[ti4_host::planning::draft::DraftStep],
    ) -> Option<(u64, ti4_host::planning::draft::DraftOutcome)> {
        self.current_player_session(game_id, credential, player, session)?;
        Some(session.run_draft(player, scope, script))
    }

    /// Sends the ready draft of the secondary of the seat of this credential, when the game asks
    /// the seat now ([`GameSession::send_ready_secondary`]). It shares the takeover/history gate
    /// with live answers, as a script that a page sends does.
    pub fn send_ready_secondary(
        &self,
        game_id: &str,
        credential: &str,
        player: &PlayerId,
        session: &GameSession,
    ) {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        if self
            .current_player_session(game_id, credential, player, session)
            .is_some()
        {
            session.send_ready_secondary(player);
        }
    }

    /// `Some` when `credential` is of `player` and `session` is the running game.
    fn current_player_session(
        &self,
        game_id: &str,
        credential: &str,
        player: &PlayerId,
        session: &GameSession,
    ) -> Option<()> {
        let state = self.state.lock().expect("registry lock");
        let authenticated = if let Some(lobby) = state.player_lobbies.get(game_id) {
            authenticate_player(lobby, credential).ok()
        } else {
            state
                .lobbies
                .get(game_id)
                .and_then(|lobby| authenticated_seat(lobby, credential).ok())
        };
        (authenticated.as_ref() == Some(player)
            && state.sessions.get(game_id).is_some_and(|current| {
                std::ptr::eq(Arc::as_ptr(current), std::ptr::from_ref(session))
            }))
        .then_some(())
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

    /// Host-only, lobby phase only: choose the map the table will play. Every call draws a new
    /// private seed, so choosing again re-rolls the open slots (or the whole random board).
    /// The seed is never part of any response.
    ///
    /// # Errors
    /// [`LobbyError::InvalidMap`] for an unknown, wrongly sized or unbuildable template;
    /// [`LobbyError::HostRequired`]; [`LobbyError::AlreadyRunning`]. `start_preset` is the dev
    /// opening-state preset (see [`ti4_host::preset`]); it is validated here and is not public.
    pub fn choose_player_lobby_map(
        &self,
        game_id: &str,
        credential: &str,
        choice: &ti4_host::maps::MapChoice,
        start_preset: Option<&str>,
    ) -> Result<PlayerLobbyView, LobbyError> {
        if let Some(name) = start_preset
            && !name.is_empty()
            && !ti4_host::preset::is_known(name)
        {
            return Err(LobbyError::InvalidMap(format!(
                "unknown start_preset {name:?}"
            )));
        }
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
        if lobby.start_from.is_some() {
            return Err(LobbyError::ScenarioTable(
                "its map is the map of the scenario".to_owned(),
            ));
        }
        let seed = rand::random::<u64>();
        ti4_host::maps::catalog::preview(choice, lobby.slots.len(), seed)
            .map_err(LobbyError::InvalidMap)?;
        let mut updated = lobby.clone();
        updated.map_template = choice.stored();
        updated.seed = seed;
        // Dev/smoke only: `Some("")` clears the preset, `None` leaves it as it is.
        match start_preset {
            Some("") => updated.start_preset = None,
            Some(name) => updated.start_preset = Some(name.to_owned()),
            None => {}
        }
        updated.map_revision += 1;
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// The board the lobby's current choice and seed give, with a place for each seat. It names
    /// no faction: who sits where, and with which faction, is drawn at Start.
    ///
    /// # Errors
    /// [`LobbyError::NotFound`], or [`LobbyError::Map`] if the stored choice no longer builds.
    pub fn player_lobby_map_preview(
        &self,
        game_id: &str,
    ) -> Result<ti4_host::maps::MapPreview, LobbyError> {
        let setup = {
            let state = self.state.lock().expect("registry lock");
            let lobby = state
                .player_lobbies
                .get(game_id)
                .ok_or(LobbyError::NotFound)?;
            super::table::setup_of(lobby)
        };
        ti4_host::setup::preview(&setup).map_err(LobbyError::Map)
    }

    /// As [`Self::create_player_lobby_with_speaker`], with every other seat played by a random
    /// bot ([`SeatController::RANDOM`], as [`Self::add_random_bots`] seats them), and started
    /// at once: the creator is the only person of the game. The seats of the game are kept with
    /// it (`init.json`), so a recovered game has the same random seats.
    #[expect(
        clippy::too_many_arguments,
        reason = "the arguments of a lobby, as `create_player_lobby_with_speaker` takes them"
    )]
    pub fn create_player_game_with_random_seats(
        &self,
        game_id: String,
        count: usize,
        seed: u64,
        nickname: &str,
        map_template: Option<String>,
        start_preset: Option<String>,
        strategy_card_set: &'static str,
        speaker_order: ti4_host::setup::SpeakerOrder,
        bot_delay_ms: Option<u64>,
    ) -> Result<(PlayerLobbyView, PlayerId, PlayerSession), LobbyError> {
        let (_, player, credential) = self.create_player_lobby_with_speaker(
            game_id.clone(),
            count,
            seed,
            nickname,
            map_template,
            start_preset,
            strategy_card_set,
            speaker_order,
        )?;
        if let Some(delay) = bot_delay_ms {
            self.change_player_lobby_settings(
                &game_id,
                credential.as_str(),
                &super::table::TableSettings {
                    bot_delay_ms: Some(delay),
                    ..super::table::TableSettings::default()
                },
            )?;
        }
        if count > 1 {
            self.add_random_bots(&game_id, credential.as_str(), None, None)?;
        }
        let view = self.start_player_lobby(&game_id, credential.as_str())?;
        Ok((view, player, credential))
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
        if lobby.slots.iter().any(|slot| slot.occupant.is_none()) {
            return Err(LobbyError::SeatsOpen);
        }
        let content = ContentStore::embedded();
        // The setup of `ti4-host` starts the table, as it does for the table of a page.
        let setup = super::table::setup_of(lobby);
        // A scenario is the game of its own seed; every other table plays the seed of the lobby.
        let seed = setup.game_seed();
        // The players sit in the order of the slots, as the lobby shows them (decision 7 of the
        // lobby plan); the factions that nobody chose are drawn for that order.
        let players = super::table::joined_players(lobby);
        let chosen: Vec<Option<String>> = players
            .iter()
            .map(|player| lobby.players[player].faction.clone())
            .collect();
        // The drawn speaker is part of `initial_state`, which the init record stores, so replay
        // and recovery start from the same speaker without re-deriving it.
        // A start preset (a tool of this server) seats its own factions in the order of the join
        // and sets its cases on the opening state; `ti4_host::setup` does both.
        let drawn = ti4_host::setup::draw(&setup, &chosen).map_err(LobbyError::Map)?;
        let (initial_state, galaxy) =
            ti4_host::setup::create(content, &setup, &players, &drawn).map_err(LobbyError::Map)?;
        let map_tiles = ti4_host::map::build_board_tiles(content, &galaxy);
        // A table that starts from a scenario plays its opening (`ti4_host::view::scenario`).
        let opening = setup
            .scenario()
            .map_err(LobbyError::Map)?
            .and_then(|scenario| {
                ti4_host::view::scenario::opening(scenario, &initial_state, &galaxy)
            });
        let mut config = SessionConfig::new(game_id, initial_state.clone())
            .with_seed(seed)
            .with_player_ids(players.clone())
            .with_galaxy(galaxy, map_tiles.clone());
        // A table with no value waits as the process says, as before.
        config.random_bot_delay_ms = lobby.bot_delay_ms;
        config.opening = opening;
        config.seats = players
            .iter()
            .map(|p| {
                let controller = match lobby.players[p].bot.as_deref() {
                    // A kind of an earlier build that is no kind now plays at random, as before.
                    Some(kind) => SeatController::Bot(match ti4_host::bots::BotKind::parse(kind) {
                        Ok(kind) => kind,
                        Err(_) if ti4_host::bots::model_name(kind).is_none() => {
                            ti4_host::bots::BotKind::Random
                        }
                        Err(error) => return Err(LobbyError::ModelMissing(error)),
                    }),
                    None => SeatController::Human,
                };
                Ok((p.clone(), controller))
            })
            .collect::<Result<_, LobbyError>>()?;
        config.seat_tokens = lobby
            .players
            .iter()
            .filter(|(_, m)| m.bot.is_none())
            .map(|(p, m)| (p.clone(), m.session.as_str().to_owned()))
            .collect();
        config.nicknames = nicknames_of(lobby);
        config.store.clone_from(&self.store);
        let mut running = lobby.clone();
        running.phase = PersistedLobbyPhase::Running;
        running.lobby_version += 1;
        running.seed = seed;
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
                seed,
                player_ids: players,
                initial_state,
                map_tiles,
                seats: Some(config.seats.clone()),
                map_template: lobby.map_template.clone(),
                strategy_card_set: Some(
                    crate::storage::stored_card_set(lobby.strategy_card_set.as_deref()).to_owned(),
                ),
                bot_delay_ms: lobby.bot_delay_ms,
                start_from: lobby.start_from.clone(),
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
        // Without a store nothing was saved, so the sockets of the table are told here.
        self.table_changed(game_id);
        Ok(view)
    }

    /// Host-only, lobby phase only: change the settings of the table (see
    /// [`super::table::TableSettings`]). Every field is checked before anything changes.
    ///
    /// # Errors
    /// The usual lobby errors, and what [`super::table::apply_settings`] refuses.
    pub fn change_player_lobby_settings(
        &self,
        game_id: &str,
        credential: &str,
        settings: &super::table::TableSettings,
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
        let mut updated = super::table::apply_settings(lobby, settings, self.public.is_public())?;
        updated
            .validate()
            .map_err(|error| LobbyError::Storage(error.to_string()))?;
        let removed: Vec<PlayerId> = lobby
            .players
            .keys()
            .filter(|id| !updated.players.contains_key(*id))
            .cloned()
            .collect();
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        for player in removed {
            state.presence.remove(&(game_id.to_owned(), player));
        }
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Host only, for a running game: the wait of the seats that the server plays. A seat that
    /// waits now has the new wait at once; the record of the table keeps it, so a recovered
    /// game has it.
    ///
    /// # Errors
    /// The usual lobby errors, [`LobbyError::NotRunning`] before the start, and
    /// [`LobbyError::InvalidSetting`] for a wait outside its range.
    pub fn change_running_bot_delay(
        &self,
        game_id: &str,
        credential: &str,
        delay_ms: u64,
    ) -> Result<PlayerLobbyView, LobbyError> {
        if delay_ms > super::table::MAX_BOT_DELAY_MS {
            return Err(LobbyError::InvalidSetting(format!(
                "bot_delay_ms must be 0-{}",
                super::table::MAX_BOT_DELAY_MS
            )));
        }
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Running) {
            return Err(LobbyError::NotRunning);
        }
        if authenticate_player(lobby, credential)? != lobby.host_player_id {
            return Err(LobbyError::HostRequired);
        }
        let session = state.sessions.get(game_id).ok_or(LobbyError::NotFound)?;
        if lobby.bot_delay_ms != Some(delay_ms) {
            let mut updated = lobby.clone();
            updated.bot_delay_ms = Some(delay_ms);
            updated.lobby_version += 1;
            self.save_player_lobby(&updated)?;
            session.set_bot_delay_ms(delay_ms);
            state.player_lobbies.insert(game_id.to_owned(), updated);
        }
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Lobby phase only: the player of the credential takes another nickname.
    ///
    /// # Errors
    /// The usual lobby errors and [`LobbyError::InvalidNickname`].
    pub fn rename_player(
        &self,
        game_id: &str,
        credential: &str,
        nickname: &str,
    ) -> Result<PlayerLobbyView, LobbyError> {
        check_nickname(nickname)?;
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        let player = authenticate_player(lobby, credential)?;
        if lobby.players[&player].nickname != nickname {
            let mut updated = lobby.clone();
            updated
                .players
                .get_mut(&player)
                .expect("authenticated player")
                .nickname = nickname.to_owned();
            updated.lobby_version += 1;
            self.save_player_lobby(&updated)?;
            *lobby = updated;
        }
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Host-only, lobby phase only: seat `count` in-process random bots in the first open
    /// slots (`None` fills every open slot). Needs only the host's own credential: a random bot
    /// is a seat answered by the game itself, not a child process.
    ///
    /// All bots are admitted or none: the lobby changes once, under the registry lock, with one
    /// durable save. `nickname` names a single bot; several bots are always "Random bot N".
    ///
    /// # Errors
    /// [`LobbyError::NotFound`], [`LobbyError::AlreadyRunning`], [`LobbyError::InvalidCapability`],
    /// [`LobbyError::HostRequired`], [`LobbyError::InvalidNickname`] (bad nickname, or a nickname
    /// with more than one bot), [`LobbyError::SeatUnavailable`] (no open slot, or fewer than
    /// `count`).
    pub fn add_random_bots(
        &self,
        game_id: &str,
        host_credential: &str,
        nickname: Option<String>,
        count: Option<usize>,
    ) -> Result<PlayerLobbyView, LobbyError> {
        self.add_in_process_bots(
            game_id,
            host_credential,
            RANDOM_BOT_KIND,
            nickname,
            count,
            None,
        )
    }

    /// [`Self::add_random_bots`] for any in-process bot kind ([`in_process_bot_kinds`]): `random`
    /// seats are named "Random bot N", `smart` seats "Rule bot N", the seats of a model by its
    /// label and a number.
    ///
    /// # Errors
    /// As [`Self::add_random_bots`]; an unknown `kind` is [`LobbyError::InvalidCapability`]
    /// (the HTTP layer rejects it first).
    pub fn add_in_process_bots(
        &self,
        game_id: &str,
        host_credential: &str,
        kind: &str,
        nickname: Option<String>,
        count: Option<usize>,
        faction: Option<&str>,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let name_stem = match ti4_host::bots::BotKind::parse(kind) {
            Ok(ti4_host::bots::BotKind::Smart) => "Rule bot".to_owned(),
            Ok(ti4_host::bots::BotKind::Random) => "Random bot".to_owned(),
            Ok(ti4_host::bots::BotKind::Mlp(model)) => model.label().to_owned(),
            Err(_) => return Err(LobbyError::InvalidCapability),
        };
        if let Some(name) = &nickname {
            check_nickname(name)?;
        }
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
        let open: Vec<usize> = lobby
            .slots
            .iter()
            .enumerate()
            .filter(|(_, slot)| slot.occupant.is_none())
            .map(|(index, _)| index)
            .collect();
        let wanted = count.unwrap_or(open.len());
        if wanted == 0 || wanted > open.len() {
            return Err(LobbyError::SeatUnavailable);
        }
        if nickname.is_some() && wanted != 1 {
            return Err(LobbyError::InvalidNickname);
        }
        if let Some(faction) = faction {
            if wanted != 1 {
                return Err(LobbyError::InvalidFaction(
                    "A faction can be chosen for one bot at a time".to_owned(),
                ));
            }
            lobby.check_faction_free(faction, None)?;
        }
        let mut updated = lobby.clone();
        let mut next_number = 1;
        for &slot in open.iter().take(wanted) {
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
            let name = nickname.clone().unwrap_or_else(|| {
                // First unused "Random bot N", so removing and re-adding never doubles a name.
                loop {
                    let candidate = format!("{name_stem} {next_number}");
                    next_number += 1;
                    if updated.players.values().all(|m| m.nickname != candidate) {
                        break candidate;
                    }
                }
            });
            updated.slots[slot].occupant = Some(player.clone());
            updated.players.insert(
                player,
                PlayerLobbyMember {
                    legacy_ready: None,
                    bot: Some(kind.to_owned()),
                    faction: faction.map(str::to_owned),
                    session,
                    nickname: name,
                },
            );
        }
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Host only, lobby phase only: the players of the table in another order of its rows
    /// ([`super::table::shuffle_seats`]). The rows are the seats of the game.
    ///
    /// # Errors
    /// The usual lobby errors, and [`LobbyError::HostRequired`] for another player.
    pub fn shuffle_seats(
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
        let mut updated = super::table::shuffle_seats(lobby, &mut rand::rng());
        updated.lobby_version += 1;
        self.save_player_lobby(&updated)?;
        *lobby = updated;
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
    }

    /// Lobby phase only: choose (`Some`) or clear (`None`, back to random) the faction of a seat.
    /// The host may set any seat, bot or person; a person may set their own seat only. Ready state
    /// is untouched.
    ///
    /// # Errors
    /// The usual lobby errors, [`LobbyError::HostRequired`] for someone else's seat,
    /// [`LobbyError::InvalidFaction`] for an unknown faction, and [`LobbyError::FactionTaken`] when
    /// another seat holds it.
    pub fn set_seat_faction(
        &self,
        game_id: &str,
        credential: &str,
        player_id: &PlayerId,
        faction: Option<&str>,
    ) -> Result<PlayerLobbyView, LobbyError> {
        let mut state = self.state.lock().expect("registry lock");
        let lobby = state
            .player_lobbies
            .get_mut(game_id)
            .ok_or(LobbyError::NotFound)?;
        if !matches!(lobby.phase, PersistedLobbyPhase::Lobby) {
            return Err(LobbyError::AlreadyRunning);
        }
        let caller = authenticate_player(lobby, credential)?;
        let member = lobby
            .players
            .get(player_id)
            .ok_or(LobbyError::InvalidPlayerId)?;
        if caller != lobby.host_player_id && &caller != player_id {
            return Err(LobbyError::HostRequired);
        }
        if lobby.start_from.is_some() {
            return Err(LobbyError::ScenarioTable(
                "its seats have the factions of the scenario".to_owned(),
            ));
        }
        if let Some(faction) = faction {
            lobby.check_faction_free(faction, Some(player_id))?;
        }
        if member.faction.as_deref() != faction {
            let mut updated = lobby.clone();
            updated
                .players
                .get_mut(player_id)
                .expect("checked above")
                .faction = faction.map(str::to_owned);
            updated.lobby_version += 1;
            self.save_player_lobby(&updated)?;
            *lobby = updated;
        }
        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
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

        Ok(self.player_view(&state, &state.player_lobbies[game_id]))
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
                    (!lobby_seat.controller.is_in_process_bot()
                        && lobby_seat.seat_token.as_deref() == Some(token))
                    .then(|| seat.clone())
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

    /// Creates and starts a legacy active session. New HTTP games start from a player lobby.
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
                    map_template: None,
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

    /// True when the host ended `game_id` while this process was running.
    pub fn is_terminated(&self, game_id: &str) -> bool {
        self.terminated
            .lock()
            .expect("terminated lock")
            .contains(game_id)
    }

    /// Phase and seated players of each given game that is still open (unfinished), for the
    /// "too many games" answer. Unknown or finished ids are left out.
    pub fn open_game_infos(&self, ids: &[String]) -> Vec<OpenGameInfo> {
        let state = self.state.lock().expect("registry lock");
        ids.iter()
            .filter_map(|id| {
                if let Some(session) = state.sessions.get(id) {
                    if session.is_finished() {
                        return None;
                    }
                    let players = state.player_lobbies.get(id).map_or_else(
                        || state.lobbies.get(id).map_or(0, |l| l.player_ids.len()),
                        |l| l.slots.iter().filter(|s| s.occupant.is_some()).count(),
                    );
                    return Some(OpenGameInfo {
                        game_id: id.clone(),
                        phase: "started",
                        players,
                    });
                }
                if let Some(l) = state.player_lobbies.get(id) {
                    let players = l.slots.iter().filter(|s| s.occupant.is_some()).count();
                    return Some(OpenGameInfo {
                        game_id: id.clone(),
                        phase: "lobby",
                        players,
                    });
                }
                state.lobbies.get(id).map(|l| OpenGameInfo {
                    game_id: id.clone(),
                    phase: "lobby",
                    players: l.player_ids.len(),
                })
            })
            .collect()
    }

    /// Ends a game for good at the request of its host: stops the worker and bots, forgets the
    /// game and moves its files into `terminated/`. Works for lobbies and started games.
    pub fn terminate_game(&self, game_id: &str, credential: &str) -> Result<(), LobbyError> {
        let gate = self.game_gate(game_id);
        let _reservation = gate.lock().expect("game gate lock");
        {
            let state = self.state.lock().expect("registry lock");
            if let Some(lobby) = state.player_lobbies.get(game_id) {
                let actor = authenticate_player(lobby, credential)?;
                if actor != lobby.host_player_id {
                    return Err(LobbyError::HostRequired);
                }
            } else if let Some(lobby) = state.lobbies.get(game_id) {
                let actor = authenticated_seat(lobby, credential)?;
                if actor != lobby.host_seat {
                    return Err(LobbyError::HostRequired);
                }
            } else {
                return Err(LobbyError::NotFound);
            }
        }
        self.terminated
            .lock()
            .expect("terminated lock")
            .insert(game_id.to_owned());
        self.remove_game(game_id);
        self.turn_redos
            .lock()
            .expect("turn redos lock")
            .remove(game_id);
        self.state
            .lock()
            .expect("registry lock")
            .presence
            .retain(|(id, _), _| id != game_id);
        if let Some(store) = &self.store {
            store
                .terminate_game(game_id)
                .map_err(|e| LobbyError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    pub fn remove_game(&self, game_id: &str) -> Option<Arc<GameSession>> {
        let mut state = self.state.lock().expect("registry lock");
        state.lobbies.remove(game_id);
        state.player_lobbies.remove(game_id);
        let session = state.sessions.remove(game_id);
        if let Some(session) = &session {
            session.stop();
        }
        session
    }
}

fn authenticate_player(
    lobby: &PlayerLobbyRecord,
    credential: &str,
) -> Result<PlayerId, LobbyError> {
    lobby
        .players
        .iter()
        .find_map(|(id, member)| {
            // A bot seat has no usable credential, whatever its record holds.
            (member.bot.is_none() && member.session.as_str() == credential).then(|| id.clone())
        })
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
        nicknames: BTreeMap::new(),
        seat_tokens,
        galaxy: None,
        map_tiles: Vec::new(),
        galaxy_layout: ti4_host::map::GalaxyLayout {
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
        announce_tail: false,
        history_generation: 0,
        batches: Vec::new(),
        rng_marks: ti4_host::rng_force::RngMarks::new(),
        replay_boundary_state: None,
        reaction_modes: BTreeMap::new(),
        staged: crate::session::staged::StagedBook::default(),
        undone: BTreeMap::new(),
        step_snapshots: true,
        random_bot_delay_ms: None,
        resume: None,
        opening: None,
    })
}

fn authenticated_seat(lobby: &LobbyState, token: &str) -> Result<PlayerId, LobbyError> {
    lobby
        .seats
        .iter()
        .find_map(|(seat, lobby_seat)| {
            (!lobby_seat.controller.is_in_process_bot()
                && lobby_seat.seat_token.as_deref() == Some(token))
            .then(|| seat.clone())
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
        galaxy_layout: ti4_host::map::GalaxyLayout {
            version: 1,
            active_sources: Vec::new(),
            placements: Vec::new(),
            off_map_system_ids: Vec::new(),
        },
        seats: init.seats.clone(),
        nicknames: BTreeMap::new(),
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
        announce_tail: false,
        history_generation: 0,
        batches: Vec::new(),
        rng_marks: ti4_host::rng_force::RngMarks::new(),
        replay_boundary_state: None,
        reaction_modes: BTreeMap::new(),
        staged: crate::session::staged::StagedBook::default(),
        undone: BTreeMap::new(),
        step_snapshots: true,
        random_bot_delay_ms: None,
        resume: None,
        opening: None,
    })
}
