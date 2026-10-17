//! The hosted game: a whole game in a browser page, behind the C ABI. Both pages play through it:
//! the offline game of `web/` and the local play of `web2/`.
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
//! | `ti4_host_query(len)` | Stateless: `describe`, `map_preview`, `catalog`, `space_odds`, `ground_odds`. |
//! | `ti4_host_start(len)` | Play a game (JSPI `promising`): returns when it ends or is left. |
//! | `ti4_host_command(len)` | A command while the game waits for the person. |
//! | `ti4_infer_check(len)` | The logits of a synthetic MLP model, for the test of the build. |
//!
//! The one import is `host.ask() -> i32`. At each call the output buffer holds an [`Envelope`] of
//! kind `ask` (the page's turn: send its messages to the page, then wait for a command whose
//! envelope has `resume`; with `waiting` the game waits before a decision of a seat that the page
//! does not play, and `step` lets it go on) or `progress` (a saved game is being played again:
//! answer [`CONTINUE`] at once). See `README.md` for the whole flow.

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use serde::Serialize;
use sha2::Digest;
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::game::Game;
use ti4_host::PROTOCOL_VERSION;
use ti4_host::clock::Clock;
use ti4_host::maps::catalog::MapChoice;
use ti4_host::parse_client_message;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::error::ErrorKind;
use ti4_host::protocol::server::{DraftOutcomeMsg, ProtocolErrorMsg, ServerMessage};
use ti4_model::id::PlayerId;
use ti4_model::state::{GameState, ReactionMode};

use crate::host_session::{
    Handled, HostSession, Identity, LocalCommand, PlayLoop, Restart, Resume, Scripted, SeatSwitch,
    decisions_hash,
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
    /// Every decision of the save, of every seat, given again in order (`Resume::log`).
    log: Vec<String>,
    decisions: Option<usize>,
    hash: Option<String>,
    /// The modes that apply once the replay is played: a replay of answers asks every window
    /// whole.
    modes: Vec<(String, ReactionMode)>,
    checked: bool,
}

static REPLAY: Mutex<Option<Replay>> = Mutex::new(None);
/// Why a saved game could not be played again: a decision of its log that the game does not
/// offer.
static DIVERGED: Mutex<Option<String>> = Mutex::new(None);

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

/// Whether a game is played.
pub fn hosted() -> bool {
    HOSTED.load(Ordering::Relaxed)
}

/// The play loop of `lib.rs`, as the session asks it.
struct CrateLoop;

impl PlayLoop for CrateLoop {
    fn decided(&self) -> u64 {
        crate::decided()
    }

    fn reaches(&self, asked: u64) -> bool {
        crate::undo_reaches(asked)
    }

    fn script_copy(&self) -> Option<(Game<'static>, usize)> {
        crate::script_copy()
    }

    fn draft(
        &self,
        seat: &PlayerId,
        scope: &ti4_host::planning::audit::PlanScope,
        script: &[ti4_host::planning::draft::DraftStep],
    ) -> Option<ti4_host::planning::draft::DraftOutcome> {
        crate::draft_outcome_of(seat, script, scope).ok()
    }
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
    /// Set for an `ask` with no choice of the page: the game waits before a decision of this
    /// seat, which the page does not play (`step` lets it decide).
    #[serde(skip_serializing_if = "Option::is_none")]
    waiting: Option<&'a PlayerId>,
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
            waiting: None,
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
    put_bytes(text.as_bytes())
}

/// Put `bytes` in the input buffer (a native caller's `ti4_host_in` plus the write): the file of
/// `ti4_model_load`.
pub fn put_bytes(bytes: &[u8]) -> usize {
    let mut input = IN.lock().expect("in lock");
    input.clear();
    input.extend_from_slice(bytes);
    bytes.len()
}

// --- the MLP ---------------------------------------------------------------------------------

/// What [`load_model`] answers: the model that is now in the registry.
#[derive(Serialize)]
struct LoadedModel<'a> {
    name: &'a str,
    label: &'a str,
    /// The SHA-256 of the whole file, the address of a server's copy (`/api/models/<sha256>.ti4m`).
    sha256: String,
    backend: &'static str,
}

/// Load the `.ti4m` file of the input (`len` bytes) into the registry of the engine
/// (`ti4_host::bots::register`), in place of a model of the same name: its trailer is checked,
/// and it runs on the fastest deterministic backend (`simd128` in the page). The output is
/// `{"name", "label", "sha256", "backend"}`, `sha256` being the SHA-256 of the whole file.
pub fn load_model(len: usize) -> i32 {
    let loaded = {
        let input = IN.lock().expect("in lock");
        match input.get(..len) {
            Some(bytes) => ti4_host::bots::MlpModel::from_bytes(bytes, None)
                .map(|model| (model, format!("{:x}", sha2::Sha256::digest(bytes)))),
            None => Err(format!("the input is {} bytes, not {len}", input.len())),
        }
    };
    match loaded {
        Ok((model, sha256)) => {
            ti4_host::bots::register(model.clone());
            // The file is not kept twice.
            {
                let mut input = IN.lock().expect("in lock");
                input.clear();
                input.shrink_to_fit();
            }
            write_json(&LoadedModel {
                name: model.name(),
                label: model.label(),
                sha256,
                backend: model.model().backend().name(),
            })
        }
        Err(error) => write_error(format_args!("the model is not loaded: {error}")),
    }
}

/// The logits of a synthetic model on some decisions: the input is a request of
/// `ti4_infer::synthetic::check`, the output its answer. The test of the wasm build compares them
/// bit for bit with what the native build gives (`fixtures/infer/synthetic_logits.json`).
pub fn infer_check(len: usize) -> i32 {
    match input(len).and_then(|request| ti4_infer::synthetic::check(&request)) {
        Ok(answer) => {
            let mut out = OUT.lock().expect("out lock");
            out.clear();
            out.extend_from_slice(answer.as_bytes());
            length(out.len())
        }
        Err(error) => write_error(error),
    }
}

// --- queries -------------------------------------------------------------------------------

/// Stateless questions. The input is `{"query": "...", ...}`:
///
/// - `{"query":"describe"}`: players, card sets, factions, bot kinds, maps per table size,
///   presets, scenarios (`host_setup::describe`).
/// - `{"query":"map_preview","choice":{"kind":"template","alias":"..."}|{"kind":"random"},
///   "player_count":4,"variant":0}`: a pictured map (`MapPreview` of the server).
/// - `{"query":"catalog"}`: the printed cards (as `GET /api/catalog` of the server).
/// - `{"query":"space_odds", ...SpaceOddsRequest}`: the odds of a space combat, a
///   `ti4_calc::odds::BattleResponse` (2000 rollouts of `ti4_calc` from a fixed seed).
/// - `{"query":"ground_odds", ...GroundOddsRequest}`: the odds of a ground combat, a
///   `ti4_calc::odds::GroundOddsResponse` of `ti4_calc::odds::ground_odds`.
///
/// Nothing of a game is read or changed: the host may ask while a game waits for the page, and
/// with no game at all.
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
        SpaceOdds(crate::SpaceOddsRequest),
        GroundOdds(ti4_calc::odds::GroundOddsRequest),
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
        Query::Catalog => {
            // Written through `dyn Write`, not into a `Value`: a copy into a `Value` costs about
            // 45 KB of wasm.
            let mut out = OUT.lock().expect("out lock");
            out.clear();
            let writer: &mut dyn std::io::Write = &mut *out;
            serde_json::to_writer(
                writer,
                &ti4_host::view::components::catalog(
                    ti4_content::ContentStore::embedded(),
                    ti4_model::content_types::POK,
                ),
            )
            .expect("the catalogue serializes");
            return length(out.len());
        }
        Query::SpaceOdds(request) => {
            return match crate::space_odds_of(&request) {
                Ok(odds) => write_json(&odds),
                Err(error) => write_error(format_args!("space odds: {error}")),
            };
        }
        Query::GroundOdds(request) => {
            return match ti4_calc::odds::ground_odds(
                ti4_content::ContentStore::embedded(),
                &request,
            ) {
                Ok(odds) => write_json(&odds),
                Err(error) => write_error(format_args!("ground odds: {error}")),
            };
        }
    };
    match value {
        Ok(value) => write_json(&value),
        Err(error) => write_error(error),
    }
}

/// Write `value` as the output; returns its length.
fn write_json(value: &impl Serialize) -> i32 {
    let mut out = OUT.lock().expect("out lock");
    out.clear();
    serde_json::to_writer(&mut *out, value).expect("a value serializes");
    length(out.len())
}

// --- starting a game -------------------------------------------------------------------------

/// What `ti4_host_start` makes ready: the game to drive, and what the play loop needs.
pub struct Prepared {
    pub game: Game<'static>,
    pub seed: u64,
    pub players: usize,
    pub human_mask: u32,
    /// The kind of the bot of each seat (`host_setup::Built::bots`).
    pub bots: Vec<Option<String>>,
    /// See `Host::table` in `lib.rs`.
    pub table: bool,
    pub stepping: u32,
    pub switches: Vec<SeatSwitch>,
    /// The person's answers to play again, given to the page's seats without asking.
    pub answers: VecDeque<String>,
    /// How many decisions of the game the save gives again, of every seat ([`saved_decision`]).
    pub saved: u64,
}

/// Read the request of `ti4_host_start` and make the session and the game.
pub fn prepare(len: usize) -> Result<Prepared, String> {
    let text = input(len)?;
    let start: serde_json::Value =
        serde_json::from_str(&text).map_err(|error| format!("start: {error}"))?;
    // A save of another format is not read (a `restart` has no format).
    if let Some(format) = start.pointer("/resume/format")
        && format.as_u64() != Some(crate::host_setup::SAVE_FORMAT)
    {
        return Err(format!(
            "the save has format {format}; this engine reads format {}",
            crate::host_setup::SAVE_FORMAT
        ));
    }
    let mut start: Start = serde_json::from_value(start).map_err(|error| format!("start: {error}"))?;
    let bot_version = start.settle_bot_version()?;
    let mut built = start.build()?;
    // A new game plays the current rules; a resumed save (or a restart of this host) plays the
    // rules it was made with, and one made before rules versions existed is version 0.
    let rules_version = match &start.resume {
        Some(resume) => resume.rules_version.unwrap_or(0),
        None => ti4_model::state::RULES_VERSION,
    };
    built.game.state.rules_version = rules_version;
    // The rule seats of this game play the bot version it was made with (one game per host).
    ti4_rule_bot::tuning::set(ti4_rule_bot::tuning::for_version(bot_version));
    let mut resume: SavedGame = start.resume.unwrap_or_default();
    if !resume.log.is_empty() {
        // Every decision is given again: the answers are among them.
        resume.answers.clear();
    }
    let identity = Identity {
        game_id: built.game_id.clone(),
        seed: built.seed,
        players: built.players,
        human_seat: built.humans.trailing_zeros().min(built.players as u32 - 1) as usize,
        humans: built.humans,
        game: built.request.clone(),
        table: built.setup.clone(),
        scenario: built.scenario.clone(),
        seats: built.seats.clone(),
        bot_version,
        rules_version,
    };
    let tiles = built.galaxy.as_ref().map_or_else(Vec::new, |galaxy| {
        ti4_host::map::build_board_tiles(ti4_content::ContentStore::embedded(), galaxy)
    });
    let never = crate::NEVER.clone();
    let modes: Vec<(String, ReactionMode)> = start
        .modes
        .iter()
        .map(|(card, mode)| (card.clone(), *mode))
        .chain(
            resume
                .never
                .iter()
                .map(|card| (card.clone(), ReactionMode::Never)),
        )
        .collect();
    // The modes apply to the game that is played live. A replay of answers asks every window
    // whole, so they are applied when it ends; the decisions of a log are given whatever the
    // modes are.
    let deferred = !resume.answers.is_empty();
    {
        let mut set = never.lock().expect("never set lock");
        set.clear();
        if !deferred {
            set.extend(modes.iter().cloned());
        }
    }
    let session = HostSession::new(
        identity,
        Box::new(HostClock),
        Box::new(CrateLoop),
        never,
        &built.game.state,
        (tiles, built.galaxy),
        &resume,
    );
    let mut session = session;
    // The opening of a scenario is played once, in a new game: a save and a new run after a
    // change of the history (it goes on from a version) have its answers or take them back.
    if resume.answers.is_empty() && resume.log.is_empty() && resume.version == 0 {
        session.open(built.opening);
    }
    if start.staged {
        session.share_staged();
    }
    *session_lock() = Some(session);
    let replayed = resume.answers.len().max(resume.log.len());
    let decisions = resume.decisions_hash.as_ref().map(|_| resume.log.len());
    *REPLAY.lock().expect("replay lock") = Some(Replay {
        total: replayed as u64,
        log: std::mem::take(&mut resume.log),
        decisions,
        hash: resume.decisions_hash.clone(),
        modes: if deferred { modes } else { Vec::new() },
        checked: replayed == 0,
    });
    *CHECK.lock().expect("check lock") = None;
    *DIVERGED.lock().expect("diverged lock") = None;
    HOSTED.store(true, Ordering::Relaxed);
    let saved = REPLAY
        .lock()
        .expect("replay lock")
        .as_ref()
        .map_or(0, |replay| replay.log.len() as u64);
    Ok(Prepared {
        game: built.game,
        seed: built.seed,
        players: built.players,
        human_mask: built.humans,
        bots: built.bots,
        table: built.table,
        stepping: start.stepping,
        switches: resume.switches.clone(),
        answers: resume.answers.into(),
        saved,
    })
}

/// The decision with the number `index` of the save that is played again (`Resume::log`).
pub fn saved_decision(index: u64) -> Option<String> {
    let replay = REPLAY.lock().expect("replay lock");
    let index = usize::try_from(index).ok()?;
    replay.as_ref()?.log.get(index).cloned()
}

fn session_lock() -> std::sync::MutexGuard<'static, Option<HostSession>> {
    session()
}

// --- hooks of the play loop ---------------------------------------------------------------------

/// The page is asked `choice`, numbered `nonce`, on `state`. The messages for the page are
/// written to the output. Returns the answer of the play loop when the page is not asked: one
/// more step of an undo.
pub fn ask(choice: &Choice, nonce: &str, state: &GameState) -> Option<i32> {
    let mut guard = session();
    let session = guard.as_mut()?;
    if let Some(resume) = session.ask(choice, nonce, state) {
        return Some(resume_code(&resume));
    }
    let check = replay_check(session);
    let messages = session.drain();
    let batch = session.take_batch_result();
    let mut envelope = Envelope::new("ask", &messages);
    envelope.batch = batch.as_ref();
    envelope.resume_check = check.as_ref();
    write_out(&envelope);
    None
}

/// The game waits before `choice` of a seat that the page does not play, and the page is shown
/// it as `viewer`. Returns the answer of the play loop when the page is not asked.
pub fn paused(choice: &Choice, viewer: Option<&PlayerId>, state: &GameState) -> Option<i32> {
    let mut guard = session();
    let session = guard.as_mut()?;
    if let Some(resume) = session.pause(choice, viewer, state) {
        return Some(resume_code(&resume));
    }
    let messages = session.drain();
    let mut envelope = Envelope::new("ask", &messages);
    envelope.waiting = Some(&choice.player);
    write_out(&envelope);
    None
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

pub fn selected(choice: &Choice, option: &ChoiceOption, own: Option<u64>) {
    with(|session| session.selected(choice, option, own));
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
        session.declined_for_never();
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

/// The replay of the save has been played: the modes of the page apply from here on.
pub fn replay_finished() {
    let modes = REPLAY
        .lock()
        .expect("replay lock")
        .as_mut()
        .map(|replay| std::mem::take(&mut replay.modes))
        .unwrap_or_default();
    crate::NEVER.lock().expect("never set lock").extend(modes);
}

/// The saved game does not fit this engine: `text` says where. Told with the end of the run.
pub fn diverged(text: &str) {
    *DIVERGED.lock().expect("diverged lock") = Some(text.to_owned());
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
        "diverged": DIVERGED.lock().expect("diverged lock").take(),
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
        Resume::Go => crate::GO,
        Resume::Switch => crate::SWITCH,
    }
}

/// A command while the game waits for the page (or after it ended): `ClientMessage` JSON, or one
/// of the local commands (`history`, `save_state`, `leave`, `submit_batch`, `step`,
/// `switch_seat`, `stepping`). The output is a `command` envelope; when it has `resume` the host
/// answers the parked `ask` with it.
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
        Some(
            "history"
                | "save_state"
                | "leave"
                | "submit_batch"
                | "step"
                | "switch_seat"
                | "stepping"
        )
    );
    let mut guard = session();
    let Some(session) = guard.as_mut() else {
        return write_error("no game: start one first");
    };
    let handled = if local {
        match serde_json::from_value::<LocalCommand>(value) {
            Ok(command) => {
                if let LocalCommand::Stepping { mode } = command {
                    crate::set_stepping(mode);
                }
                let handled = session.local(command, engine_info());
                if let Some(switch) = handled.switch {
                    crate::switch_seats(switch.decision, switch.humans);
                }
                handled
            }
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
            Ok(message) => session.client_message(message),
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
    script: &[ti4_host::planning::draft::DraftStep],
    scope: &ti4_host::planning::audit::PlanScope,
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
