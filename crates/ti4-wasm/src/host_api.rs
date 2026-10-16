//! The hosted game: a whole game for one person in a browser, behind the C ABI.
//!
//! The host (a Web Worker) talks to the engine in JSON, in the shapes the server speaks:
//! `ClientMessage` in, `ServerMessage` out, wrapped in an [`Envelope`] that also carries what the
//! HTTP calls of the server carry (history, batch, save). The exports are:
//!
//! | Export | Use |
//! |---|---|
//! | `ti4_host_in(len)` | Reserve the input buffer; returns where to write `len` bytes. |
//! | `ti4_host_out_ptr()`, `ti4_host_out_len()` | The last output (re-read after every call). |
//! | `ti4_host_set_time(unix_ms, monotonic_ms)` | Tell the engine the time (events carry it). |
//! | `ti4_host_query(len)` | Stateless: `describe`, `map_preview`, `catalog`. |
//! | `ti4_host_start(len)` | Play a game (JSPI `promising`): returns when it ends or is left. |
//! | `ti4_host_command(len)` | A command while the game waits for the person. |
//!
//! The one import is `host.ask() -> i32`, as for `ti4_play`. At each call the output buffer holds
//! an [`Envelope`] of kind `ask` (the person's turn: send its messages to the page, then wait for
//! a command whose envelope has `resume`) or `progress` (a saved game is being played again:
//! answer [`CONTINUE`] at once). See `README.md` for the whole flow.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::game::Game;
use ti4_host::PROTOCOL_VERSION;
use ti4_host::clock::Clock;
use ti4_host::maps::catalog::MapChoice;
use ti4_host::parse_client_message;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::error::ErrorKind;
use ti4_host::protocol::server::{DraftOutcomeMsg, ProtocolErrorMsg, ServerMessage};
use ti4_model::state::{GameState, ReactionMode};
use ti4_view::planning::draft::DraftStop;

use crate::host_session::{
    Handled, HostSession, Identity, LocalCommand, Restart, Resume, Scripted, decisions_hash,
};
use crate::host_setup::{Resume as SavedGame, Start};

/// The answer of the host to a `progress` envelope: go on.
pub const CONTINUE: i32 = -5;

static SESSION: Mutex<Option<HostSession>> = Mutex::new(None);
static HOSTED: AtomicBool = AtomicBool::new(false);
static IN: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static OUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static NOW_UNIX_MS: AtomicU64 = AtomicU64::new(0);
static NOW_MONOTONIC_MS: AtomicU64 = AtomicU64::new(0);

/// What the replay of a save is checked against when it has been played.
struct Replay {
    total: u64,
    decisions: Option<usize>,
    hash: Option<String>,
    never: Vec<String>,
    checked: bool,
}

static REPLAY: Mutex<Option<Replay>> = Mutex::new(None);
/// What the last call of `ti4_host_start` leaves in the output when it ends.
static CHECK: LazyLock<Mutex<Option<serde_json::Value>>> = LazyLock::new(|| Mutex::new(None));

/// The time the host last told ([`ti4_host_set_time`]); on a native run the system clock.
struct HostClock;

impl Clock for HostClock {
    fn monotonic_ms(&self) -> u64 {
        NOW_MONOTONIC_MS.load(Ordering::Relaxed)
    }

    fn unix_secs(&self) -> u64 {
        let told = NOW_UNIX_MS.load(Ordering::Relaxed);
        if told != 0 {
            return told / 1000;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            ti4_host::clock::SystemClock.unix_secs()
        }
        #[cfg(target_arch = "wasm32")]
        {
            0
        }
    }
}

/// Tell the engine the time: the host calls this before it starts a game and each time it
/// resumes one. Events of the game carry the time of day of the last call.
pub fn set_time(unix_ms: u64, monotonic_ms: u64) {
    NOW_UNIX_MS.store(unix_ms, Ordering::Relaxed);
    NOW_MONOTONIC_MS.store(monotonic_ms, Ordering::Relaxed);
}

/// Whether the game that is played is a hosted one (messages, not the update buffer).
pub fn hosted() -> bool {
    HOSTED.load(Ordering::Relaxed)
}

fn session() -> std::sync::MutexGuard<'static, Option<HostSession>> {
    SESSION.lock().expect("session lock")
}

/// Run `f` on the session of the hosted game, if there is one.
pub fn with<R>(f: impl FnOnce(&mut HostSession) -> R) -> Option<R> {
    session().as_mut().map(f)
}

// --- the output ----------------------------------------------------------------------------

/// What the host reads after a call.
#[derive(Serialize)]
pub struct Envelope<'a> {
    /// `ask`, `progress`, `end`, `command` or `query`.
    kind: &'a str,
    messages: &'a [ServerMessage],
    /// Set for `progress`.
    #[serde(skip_serializing_if = "Option::is_none")]
    progress: Option<Progress>,
    /// What the host does with the parked game: the place of the option to answer with, or
    /// `-2` (undo), `-1` (leave).
    #[serde(skip_serializing_if = "Option::is_none")]
    resume: Option<i32>,
    /// A history change that needs a new run with these answers (after `resume` of `-1`).
    #[serde(skip_serializing_if = "Option::is_none")]
    restart: Option<&'a Restart>,
    /// How the last batch ended (see `HostSession::scripted`).
    #[serde(skip_serializing_if = "Option::is_none")]
    batch: Option<&'a serde_json::Value>,
    /// The save of the game, for `save_state`.
    #[serde(skip_serializing_if = "Option::is_none")]
    save: Option<&'a serde_json::Value>,
    /// Whether a saved game that was played again reached the position it was saved at.
    #[serde(skip_serializing_if = "Option::is_none")]
    resume_check: Option<&'a serde_json::Value>,
    /// Set for `end`: why the run ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    end: Option<&'a serde_json::Value>,
}

#[derive(Serialize)]
struct Progress {
    done: u64,
    total: u64,
}

impl<'a> Envelope<'a> {
    const fn new(kind: &'a str, messages: &'a [ServerMessage]) -> Self {
        Self {
            kind,
            messages,
            progress: None,
            resume: None,
            restart: None,
            batch: None,
            save: None,
            resume_check: None,
            end: None,
        }
    }
}

fn write_out(envelope: &Envelope<'_>) -> usize {
    let mut out = OUT.lock().expect("out lock");
    out.clear();
    serde_json::to_writer(&mut *out, envelope).expect("an envelope serializes");
    out.len()
}

/// The text of the output, for native callers.
#[must_use]
pub fn out_text() -> String {
    String::from_utf8_lossy(&OUT.lock().expect("out lock")).into_owned()
}

/// The output as it is now: where it is and how long.
pub fn out_ptr() -> *const u8 {
    OUT.lock().expect("out lock").as_ptr()
}

pub fn out_len() -> u32 {
    u32::try_from(OUT.lock().expect("out lock").len()).expect("an output fits 4 GiB")
}

/// The run could not start or failed in a way that has no `end` envelope: the error text is the
/// output. Returns the negated length.
pub fn fail(text: impl fmt::Display) -> i32 {
    HOSTED.store(false, Ordering::Relaxed);
    write_error(text)
}

fn write_error(text: impl fmt::Display) -> i32 {
    let mut out = OUT.lock().expect("out lock");
    out.clear();
    out.extend_from_slice(text.to_string().as_bytes());
    -i32::try_from(out.len()).expect("an error text is short")
}

fn length(len: usize) -> i32 {
    i32::try_from(len).expect("an output is smaller than 2 GiB")
}

// --- the input -----------------------------------------------------------------------------

/// Reserve `len` bytes of input and return where the host writes them.
pub fn reserve_input(len: usize) -> *mut u8 {
    let mut input = IN.lock().expect("in lock");
    input.clear();
    input.resize(len, 0);
    input.as_mut_ptr()
}

fn input(len: usize) -> Result<String, String> {
    let input = IN.lock().expect("in lock");
    let bytes = input
        .get(..len)
        .ok_or_else(|| format!("the input is {} bytes, not {len}", input.len()))?;
    String::from_utf8(bytes.to_vec()).map_err(|error| format!("the input is not text: {error}"))
}

/// Put `text` in the input buffer (a native caller's `ti4_host_in` plus the write).
pub fn put_input(text: &str) -> usize {
    let mut input = IN.lock().expect("in lock");
    input.clear();
    input.extend_from_slice(text.as_bytes());
    text.len()
}

// --- queries -------------------------------------------------------------------------------

/// Stateless questions of the setup form. The input is `{"query": "...", ...}`:
///
/// - `{"query":"describe"}`: players, card sets, factions, bot kinds, maps per table size,
///   presets, scenarios (`host_setup::describe`).
/// - `{"query":"map_preview","choice":{"kind":"template","alias":"..."}|{"kind":"random"},
///   "player_count":4,"variant":0}`: a pictured map (`MapPreview` of the server).
/// - `{"query":"catalog"}`: the printed cards (as `ti4_catalog`).
pub fn query(len: usize) -> i32 {
    #[derive(serde::Deserialize)]
    #[serde(tag = "query", rename_all = "snake_case")]
    enum Query {
        Describe,
        MapPreview {
            choice: MapChoice,
            player_count: usize,
            #[serde(default)]
            variant: u32,
        },
        Catalog,
    }
    let text = match input(len) {
        Ok(text) => text,
        Err(error) => return write_error(error),
    };
    let query: Query = match serde_json::from_str(&text) {
        Ok(query) => query,
        Err(error) => return write_error(format_args!("query: {error}")),
    };
    let value = match query {
        Query::Describe => Ok(crate::host_setup::describe()),
        Query::MapPreview {
            choice,
            player_count,
            variant,
        } => crate::host_setup::map_preview(&choice, player_count, variant),
        Query::Catalog => serde_json::to_value(ti4_view::components::catalog(
            ti4_content::ContentStore::embedded(),
            ti4_model::content_types::POK,
        ))
        .map_err(|error| error.to_string()),
    };
    match value {
        Ok(value) => {
            let mut out = OUT.lock().expect("out lock");
            out.clear();
            serde_json::to_writer(&mut *out, &value).expect("a value serializes");
            length(out.len())
        }
        Err(error) => write_error(error),
    }
}

// --- starting a game -------------------------------------------------------------------------

/// What `ti4_host_start` makes ready: the game to drive, and what the play loop needs.
pub struct Prepared {
    pub game: Game<'static>,
    pub seed: u64,
    pub players: usize,
    pub human_mask: u32,
    pub rule_seats: u32,
    /// The person's answers to play again, given to the host's seat without asking.
    pub answers: VecDeque<String>,
}

/// Read the request of `ti4_host_start` and make the session and the game.
pub fn prepare(len: usize) -> Result<Prepared, String> {
    let text = input(len)?;
    let start: Start = serde_json::from_str(&text).map_err(|error| format!("start: {error}"))?;
    let built = start.setup.build()?;
    let resume: SavedGame = start.resume.unwrap_or_default();
    let identity = Identity {
        game_id: built.setup.game_id.clone(),
        seed: built.seed,
        players: built.players,
        human_seat: built.human,
        setup: serde_json::to_value(&built.setup).map_err(|error| error.to_string())?,
    };
    let tiles = built.galaxy.as_ref().map_or_else(Vec::new, |galaxy| {
        ti4_view::map::build_board_tiles(ti4_content::ContentStore::embedded(), galaxy)
    });
    let never = crate::NEVER.clone();
    {
        // The cards of the person's Never set apply to the game that is played live: a replay
        // asks every window whole, and the set is applied when the replay ends.
        let mut set = never.lock().expect("never set lock");
        set.clear();
        if resume.answers.is_empty() {
            set.extend(
                resume
                    .never
                    .iter()
                    .map(|card| (card.clone(), ReactionMode::Never)),
            );
        }
    }
    let session = HostSession::new(
        identity,
        Box::new(HostClock),
        never,
        &built.game.state,
        tiles,
        built.galaxy,
        resume.answers.len(),
        resume.redo.clone(),
        resume.generation,
        resume.version,
        resume
            .event_times
            .iter()
            .enumerate()
            .map(|(index, time)| {
                (
                    time.clone(),
                    resume.event_versions.get(index).copied().flatten(),
                )
            })
            .collect(),
    );
    *session_lock() = Some(session);
    *REPLAY.lock().expect("replay lock") = Some(Replay {
        total: resume.answers.len() as u64,
        decisions: resume.decisions,
        hash: resume.decisions_hash.clone(),
        never: resume.never.clone(),
        checked: resume.answers.is_empty(),
    });
    *CHECK.lock().expect("check lock") = None;
    HOSTED.store(true, Ordering::Relaxed);
    Ok(Prepared {
        game: built.game,
        seed: built.seed,
        players: built.players,
        human_mask: 1 << built.human,
        rule_seats: built.rule_seats,
        answers: resume.answers.into(),
    })
}

fn session_lock() -> std::sync::MutexGuard<'static, Option<HostSession>> {
    session()
}

// --- hooks of the play loop ---------------------------------------------------------------------

/// The person is asked `choice`, numbered `nonce`, on `state`. The messages for the page are
/// written to the output.
pub fn ask(choice: &Choice, nonce: &str, state: &GameState, can_undo: bool) {
    let mut guard = session();
    let Some(session) = guard.as_mut() else {
        return;
    };
    session.ask(choice, nonce, state, can_undo);
    let check = replay_check(session);
    let messages = session.drain();
    let batch = session.take_batch_result();
    let mut envelope = Envelope::new("ask", &messages);
    envelope.batch = batch.as_ref();
    envelope.resume_check = check.as_ref();
    write_out(&envelope);
}

/// After the first question that follows a replayed save: did the game come back to where it
/// was saved?
fn replay_check(session: &HostSession) -> Option<serde_json::Value> {
    let mut replay = REPLAY.lock().expect("replay lock");
    let replay = replay.as_mut()?;
    if replay.checked {
        return None;
    }
    replay.checked = true;
    let (Some(expected), Some(hash)) = (replay.decisions, replay.hash.as_ref()) else {
        return None;
    };
    let log = session.decision_log();
    let found = log.get(..expected).map(decisions_hash);
    let ok = found.as_ref() == Some(hash);
    Some(serde_json::json!({
        "ok": ok,
        "decisions": expected,
        "expected_hash": hash,
        "found_hash": found,
        "found_decisions": log.len(),
    }))
}

/// The page answered: nothing is pending until the next question.
pub fn answered() {
    with(HostSession::answered);
}

pub fn offer(records: &[ti4_engine::choice::DecisionRecord], state: &GameState) {
    with(|session| session.offer(records, state));
}

pub fn selected(choice: &Choice, option: &ChoiceOption) {
    with(|session| session.selected(choice, option));
}

pub fn after_step(game: &Game<'_>, events_before: usize) {
    with(|session| session.after_step(game, events_before));
}

pub fn mark(asked: u64, decisions: usize) {
    with(|session| session.mark(asked, decisions));
}

pub fn took_back(asked: u64, rollback: bool, reanswers: usize) {
    with(|session| {
        session.took_back(asked, rollback);
        session.replaying_again(reanswers);
    });
}

pub fn scripted(choice: &Choice) -> Scripted {
    with(|session| session.scripted(choice)).unwrap_or(Scripted::Ask)
}

pub fn auto_resolved(seat: &ti4_model::id::PlayerId, prompt: &str, selected: &str, reason: &str) {
    with(|session| session.auto_resolved(seat, prompt, selected, reason));
}

pub fn passed_window(seat: &ti4_model::id::PlayerId, cards: &[String]) {
    let names = cards.join(", ");
    with(|session| {
        session.passed_window(
            seat,
            &format!("Reaction window ({names})"),
            "Pass",
            &format!("you set {names} to never offer"),
            cards,
        );
    });
}

/// How far a replay is: `(played, total)`, `None` when no save is being played again.
pub fn replay_progress(left: u64) -> Option<(u64, u64)> {
    let replay = REPLAY.lock().expect("replay lock");
    let total = replay
        .as_ref()
        .map(|replay| replay.total)
        .filter(|total| *total > 0)?;
    Some((total - left.min(total), total))
}

/// The replay of the save has been played: the person's Never set applies from here on.
pub fn replay_finished() {
    let never = REPLAY
        .lock()
        .expect("replay lock")
        .as_ref()
        .map(|replay| replay.never.clone())
        .unwrap_or_default();
    crate::NEVER
        .lock()
        .expect("never set lock")
        .extend(never.into_iter().map(|card| (card, ReactionMode::Never)));
}

/// Write a `progress` envelope; the caller then asks the host (which answers [`CONTINUE`]).
pub fn write_progress(done: u64, total: u64) {
    let mut envelope = Envelope::new("progress", &[]);
    envelope.progress = Some(Progress { done, total });
    write_out(&envelope);
}

/// The run ended: `error` is the engine's, when it failed; `left` that the host left. The final
/// output is an `end` envelope; its length is returned.
pub fn finish_run(error: Option<&str>, left: bool) -> usize {
    HOSTED.store(false, Ordering::Relaxed);
    let mut guard = session();
    let Some(session) = guard.as_mut() else {
        return 0;
    };
    if let Some(error) = error {
        session.failed(error);
    }
    let messages = session.drain();
    let end = serde_json::json!({
        "finished": session.finished(),
        "left": left,
        "error": error,
        "decisions": session.decision_log().len(),
        "answers": session.answers().len(),
    });
    let mut envelope = Envelope::new("end", &messages);
    envelope.end = Some(&end);
    write_out(&envelope)
}

// --- commands --------------------------------------------------------------------------------

fn resume_code(resume: &Resume) -> i32 {
    match resume {
        Resume::Answer(index) => {
            i32::try_from(*index).expect("a choice has fewer than 2^31 options")
        }
        Resume::Undo => crate::UNDO,
        Resume::Leave => crate::LEAVE,
    }
}

/// A command while the game waits for the person (or after it ended): `ClientMessage` JSON, or
/// one of the local commands (`history`, `save_state`, `leave`, `submit_batch`). The output is
/// a `command` envelope; when it has `resume` the host answers the parked `ask` with it.
pub fn command(len: usize) -> i32 {
    let text = match input(len) {
        Ok(text) => text,
        Err(error) => return write_error(error),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(error) => return write_error(format_args!("command: {error}")),
    };
    let local = matches!(
        value.get("type").and_then(serde_json::Value::as_str),
        Some("history" | "save_state" | "leave" | "submit_batch")
    );
    let mut guard = session();
    let Some(session) = guard.as_mut() else {
        return write_error("no game: start one first");
    };
    let handled = if local {
        match serde_json::from_value::<LocalCommand>(value) {
            Ok(command) => session.local(command, engine_info()),
            Err(error) => return write_error(format_args!("command: {error}")),
        }
    } else {
        match parse_client_message(&text) {
            Ok(ClientMessage::RunDraft {
                request_id,
                script,
                scope,
                ..
            }) => {
                run_draft(session, &request_id, &script, &scope);
                Handled::default()
            }
            Ok(message) => {
                session.client_message(message, &mut |script| match crate::draft_outcome(
                    script,
                    &ti4_view::planning::audit::PlanScope::default(),
                ) {
                    Ok(outcome) => match outcome.stop {
                        DraftStop::Refused { reason, .. } => Some(reason),
                        _ => None,
                    },
                    Err(error) => Some(error.to_string()),
                })
            }
            Err(error) => {
                let message = ServerMessage::Error(ProtocolErrorMsg {
                    protocol_version: PROTOCOL_VERSION,
                    kind: ErrorKind::MalformedMessage,
                    message: error.to_string(),
                });
                return write_envelope_of(session, "command", vec![message], &Handled::default());
            }
        }
    };
    write_envelope_of(session, "command", Vec::new(), &handled)
}

fn run_draft(
    session: &mut HostSession,
    request_id: &str,
    script: &[ti4_view::planning::draft::DraftStep],
    scope: &ti4_view::planning::audit::PlanScope,
) {
    let message = match crate::draft_outcome(script, scope) {
        Ok(outcome) => ServerMessage::DraftOutcome(DraftOutcomeMsg {
            protocol_version: PROTOCOL_VERSION,
            game_id: session.game_id().to_owned(),
            request_id: request_id.to_owned(),
            base_version: session.version(),
            outcome: Box::new(outcome),
        }),
        Err(error) => ServerMessage::Error(ProtocolErrorMsg {
            protocol_version: PROTOCOL_VERSION,
            kind: ErrorKind::EngineError,
            message: error.to_string(),
        }),
    };
    session.push_message(message);
}

fn engine_info() -> serde_json::Value {
    serde_json::json!({
        "build": env!("CARGO_PKG_VERSION"),
        "content_digest": ti4_content::provenance::embedded_digest().corpus,
    })
}

fn write_envelope_of(
    session: &mut HostSession,
    kind: &str,
    extra: Vec<ServerMessage>,
    handled: &Handled,
) -> i32 {
    let mut messages = session.drain();
    messages.extend(extra);
    let batch = session.take_batch_result();
    let mut envelope = Envelope::new(kind, &messages);
    envelope.resume = handled.resume.as_ref().map(resume_code);
    envelope.restart = handled.restart.as_ref();
    envelope.save = handled.save.as_ref();
    envelope.batch = batch.as_ref();
    length(write_envelope(&envelope))
}

fn write_envelope(envelope: &Envelope<'_>) -> usize {
    write_out(envelope)
}
