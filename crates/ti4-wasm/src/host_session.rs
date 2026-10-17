//! `HostSession`: what the server's `SessionShared` (and the thread of `worker.rs`) does around the
//! engine for one game in one page, as plain data with no threads and no sockets. The page plays
//! one seat or several (a mask that can change during the game: [`LocalCommand::SwitchSeat`]). It sits on `ti4-host` (the event journal, the state update, the reach answer) and
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
//! | `UndoTrack` (table rules) / registry undo    | `UndoTrack` (local rules), checkpoints: [`HostSession::mark`], [`HostSession::took_back`] |
//! | `SessionShared::*` commands                  | [`HostSession::command`]               |
//!
//! Differences from the server, all on purpose:
//!
//! - One `state_update` per question of the person (and one at the end), not one per engine step:
//!   nobody can read the steps in between, and a state of a game is large. `game_version` counts
//!   those updates.
//! - The pending choice travels inside the `state_update` / `initial_snapshot` only; the separate
//!   `pending_choice` message is not sent.
//! - Undo is the engine checkpoint of this crate. The track of the answers is the server's
//!   `UndoTrack` under `UndoRules::Local`: only a change of the page's seats cuts it, so an undo
//!   may go back over what other seats decided and over a die or a shuffle (a checkpoint restores
//!   the dice, so an undo cannot be used to re-roll).
//! - Redo and rewind are by replay: the session says what to replay ([`Restart`]); the host starts
//!   the game again with those answers. So is an undo that goes back further than the
//!   checkpoints.
//! - A seat that the page does not play may wait for the page before it decides (stepping, or
//!   the wait of a table): the session then sends an update with no pending choice, and the
//!   page lets the seat go on ([`LocalCommand::Step`]).
//! - The staged work of each seat (`ti4_host::staged`) is kept in the session and in its save.
//!   The page is one connection: it is told the slots of the seat that it is shown as, with
//!   `connection`, after each snapshot and when that seat changes. Revisions count up for the
//!   whole page, so a write that was meant for another seat is stale. A ready draft of a
//!   secondary is sent by the session when the game asks the seat, and when the seat marks it
//!   ready while the game asks it (`ti4_host::staged::ready`).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::{Choice, ChoiceOption, DecisionRecord};
use ti4_engine::game::Game;
use ti4_host::PROTOCOL_VERSION;
use ti4_host::batch::{LiveBatch, LiveStep, MovementPlan};
use ti4_host::clock::Clock;
use ti4_host::journal::{self, EventDraft};
use ti4_host::map::GalaxyLayout;
use ti4_host::planning::audit::PlanScope;
use ti4_host::planning::draft::{DraftOutcome, DraftStep};
use ti4_host::planning::script::{
    Check, Ended, First, OnMismatch, ScriptRuns, check_script, first_answer, refused,
    tactical_complete,
};
use ti4_host::planning::steps::PlanStep;
use ti4_host::planning::undo::{UndoEntry, UndoRules, UndoTrack};
use ti4_host::preview::{Basis, Position, PreviewRequest, busy, eligibility};
use ti4_host::projection::{project_initial_snapshot_with_map, project_turn_status};
use ti4_host::protocol::check_submission;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::error::ErrorKind;
use ti4_host::protocol::reach::ReachIntoMsg;
use ti4_host::protocol::server::{
    ActionAcceptedMsg, ActionRejectedMsg, AutoResolvedNote, DraftAppliedMsg, EventVisibility,
    GameEvent, GameEventKind, GameEventMsg, GameOverMsg, MovementFact, PongMsg, ProtocolErrorMsg,
    SeatDecisionDetail, SecondaryPreviewMsg, ServerMessage, StateUpdateMsg, current_log_path,
    public_roll, verified_decision_facts,
};
use ti4_host::protocol::staged::StagedSlot;
use ti4_host::protocol::status::{RejectionReason, ViewerRole};
use ti4_host::protocol::view::BoardTileView;
use ti4_host::setup::TableSeats;
use ti4_host::staged::ready::{self, Check as ReadyCheck};
use ti4_host::staged::{Put, SeatStaged};
use ti4_host::state_update::{
    StateUpdateInputs, answer_reach, queue_auto_resolved, state_update_for,
};
use ti4_host::view::reaction_modes::{Modes, NeverSet};
use ti4_host::view::scenario::{Opening, OpeningRun};
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::{GameState, Phase, ReactionMode};

/// The most checkpoints (and so session marks) kept; see `CHECKPOINT_LIMIT` in `lib.rs`.
pub const MARK_LIMIT: usize = 32;

/// The id of the page as a connection: the `writer` of its writes of the staged work.
pub const PAGE_CONNECTION: u64 = 1;

/// What the host does with the answer of [`HostSession::command`] for the parked game.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resume {
    /// Answer the pending choice with the option at this place.
    Answer(usize),
    /// Take back the last answer (the engine's `UNDO`).
    Undo,
    /// End the run (the engine's `LEAVE`).
    Leave,
    /// Let the seat that the game waits for decide (the engine's `GO`).
    Go,
    /// The seats of the page changed: the decision is asked again (the engine's `SWITCH`).
    Switch,
}

/// From the decision with the number `decision` on, the page plays the seats of `humans` (bit 0
/// is seat `a`). An undo does not go back before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SeatSwitch {
    pub decision: u64,
    pub humans: u32,
}

/// What the session asks of the play loop that drives the game.
pub trait PlayLoop: Send {
    /// How many decisions the seats made. While the game waits, the number of the decision it
    /// waits for.
    fn decided(&self) -> u64;
    /// Whether an undo through the checkpoints reaches the answer that had the number `asked`.
    fn reaches(&self, asked: u64) -> bool;
    /// A copy of the game at its last finished step and how many decisions the log had there:
    /// the check of a script before it is sent ([`check_script`]).
    fn script_copy(&self) -> Option<(Game<'static>, usize)>;
    /// `script` of `seat` played on a copy of the game at its last finished step, in `scope`:
    /// the check of a ready draft of a secondary. `None` when there is no copy.
    fn draft(
        &self,
        seat: &PlayerId,
        scope: &PlanScope,
        script: &[DraftStep],
    ) -> Option<DraftOutcome>;
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
    /// Every decision of the game to give again, of every seat, for an undo: no seat decides
    /// these again. Empty for a change of the history, which gives `answers`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub log: Vec<String>,
    /// The changes of the seats of the page.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub switches: Vec<SeatSwitch>,
    /// The steps of a movement, the landings or a production that the undo took back: told
    /// with the first snapshot of the new run.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub undone: Vec<PlanStep>,
    /// The rule bot version the game is played with (`host_setup::Setup::bot_version`): the new
    /// run plays the same bot even when its request repeats a setup that leaves it out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_version: Option<u32>,
    /// The rules version the game is played with (`GameState::rules_version`); left out for
    /// version 0, the legacy rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_version: Option<u32>,
    /// What each seat has staged: a history change keeps it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub staged: BTreeMap<PlayerId, SeatStaged>,
}

/// The result of a command, besides the messages (those are in [`HostSession::drain`]).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Handled {
    pub resume: Option<Resume>,
    pub restart: Option<Restart>,
    /// Set by `save_state`.
    pub save: Option<serde_json::Value>,
    /// Set by `switch_seat`: the play loop is told before it is resumed.
    pub switch: Option<SeatSwitch>,
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
    /// Let the seat that the game waits for decide.
    Step {},
    /// Play `seat` from now on in place of the seat that the game is shown as: that seat
    /// decides as a seat that the page does not play, from its open decision on. Nothing
    /// happens when the page plays `seat` already.
    SwitchSeat { seat: String },
    /// Whether the game waits before a decision of a seat that the page does not play: 0 (no),
    /// 1 (before each) or 2 (before each that takes the wait of a table, not a reaction window
    /// that the seat only declines). The seats decide the same in every mode. Only a flag is
    /// set: it can be sent at any time.
    Stepping { mode: u32 },
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
    /// Go back to the start of the shown seat's last `turns` turns (1 or 2): the turn redo of
    /// the server, without its auto-play (the bots simply play on from there).
    UndoTurn { turns: u8 },
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

/// A choice that a seat answered, with what the session keeps of it.
#[derive(Debug, Clone)]
struct Selected {
    choice: Choice,
    option: ChoiceOption,
    /// The number of the answer when the seat is played by the page.
    own: Option<u64>,
    /// The game declined the window for the Never cards of the page.
    passed: bool,
}

/// An undo that goes back over several answers, through the checkpoints one at a time.
#[derive(Debug, Clone)]
struct Undoing {
    /// How many answers of the page stay.
    keep: usize,
    /// The steps that it takes back, told with the snapshot when it is done.
    steps: Vec<PlanStep>,
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
    /// 0-based place of the person: the first seat that the page plays.
    pub human_seat: usize,
    /// The seats that the page plays at the start (bit 0 is seat `a`).
    #[serde(default)]
    pub humans: u32,
    /// The game as the host was given it, with the choices that were left open filled in (card
    /// set, map, factions...): a save replays with it.
    pub game: crate::host_setup::GameRequest,
    /// The table of `ti4_host::setup` the game was made from; none for a game of a fixed setup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<ti4_host::setup::TableSetup>,
    /// The scenario the game starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// Who sits in each seat, told with every update: the game of a table.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seats: Option<TableSeats>,
    /// The rule bot version the game is played with. A `table` or `local` game has no `setup`
    /// to record it in, so the session carries it ([`crate::host_setup::Start::settle_bot_version`]).
    pub bot_version: u32,
    /// The rules version of the game (`GameState::rules_version`); 0 for a game recorded before
    /// rules versions existed. A resume gives it back, so the game replays under its own rules.
    pub rules_version: u32,
}

/// The session of one game.
pub struct HostSession {
    identity: Identity,
    clock: Box<dyn Clock>,
    play_loop: Box<dyn PlayLoop>,
    /// The seat the game is shown as: the seat asked last.
    viewer: PlayerId,
    switches: Vec<SeatSwitch>,
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
    selected: VecDeque<Selected>,
    /// The page's answers (ids), in order.
    answers: Vec<String>,
    /// What an undo reads of each answer of the page (local rules: only a change of the page's
    /// seats cuts).
    undo: UndoTrack,
    /// The next decision of the page is a window that was declined for its Never cards.
    declined: bool,
    redo: Vec<String>,
    marks: VecDeque<Mark>,
    // The map: it does not change during a game.
    tiles: Vec<BoardTileView>,
    layout: GalaxyLayout,
    galaxy: Option<Galaxy>,
    // The position the person sees.
    state: GameState,
    pending: Option<(Choice, String)>,
    /// The game waits before this decision of a seat that the page does not play.
    waiting: Option<Choice>,
    can_undo: bool,
    undoing: Option<Undoing>,
    /// The play loop went back to a checkpoint and goes on to the choice that it opens again:
    /// no seat waits on the way.
    resuming: bool,
    /// The option the person submitted, until its decision is recorded.
    awaiting: Option<String>,
    /// The next question starts with a full `initial_snapshot` (a new run, an undo).
    fresh: bool,
    /// Notes of the game's auto-resolved single-option steps, for the next update.
    auto_resolved: Vec<(PlayerId, AutoResolvedNote)>,
    undone: Vec<PlanStep>,
    /// The scripts that are being sent to the game. A refused entry takes the whole script back.
    scripts: ScriptRuns,
    /// The opening of the scenario that a new game starts from: it answers its seat until it
    /// stops (`ti4_host::view::scenario::OpeningRun`).
    opening: OpeningRun,
    /// What each seat has staged and not sent.
    staged: BTreeMap<PlayerId, SeatStaged>,
    /// The page takes part in the staged work (`Start::staged`, or `subscribe` with `staged`).
    staged_shared: bool,
    /// The seat whose slots the page was told last.
    staged_told: Option<PlayerId>,
    batch: Option<BatchRun>,
    /// How the last batch ended, for the host (taken by [`HostSession::take_batch_result`]).
    batch_result: Option<serde_json::Value>,
    // Replay of a save.
    /// The person's answers that are replayed from a save: no redo bookkeeping, no notes.
    quiet: usize,
    /// How many decisions of the game a save gives again (`Resume::log`).
    saved_log: usize,
    saved_events: Vec<(String, Option<u64>)>,
    // The end.
    finished: bool,
    error: Option<String>,
    /// The messages for the host, oldest first.
    out: Vec<ServerMessage>,
}

impl HostSession {
    /// A session for the game that begins at `state`, played again from `resume`.
    #[must_use]
    pub fn new(
        identity: Identity,
        clock: Box<dyn Clock>,
        play_loop: Box<dyn PlayLoop>,
        never: NeverSet,
        state: &GameState,
        (tiles, galaxy): (Vec<BoardTileView>, Option<Galaxy>),
        resume: &crate::host_setup::Resume,
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
            play_loop,
            viewer,
            switches: resume.switches.clone(),
            never,
            version: resume.version.max(1),
            generation: resume.generation,
            decision_log: Vec::new(),
            events: Vec::new(),
            event_counter: 0,
            rolls_seen: 0,
            prev_phase: state.phase,
            prev_round: state.round,
            selected: VecDeque::new(),
            answers: Vec::new(),
            undo: {
                let mut undo = UndoTrack::new(UndoRules::Local);
                if let Some(switch) = resume.switches.last() {
                    undo.seats_changed(usize::try_from(switch.decision).unwrap_or(usize::MAX));
                }
                undo
            },
            declined: false,
            redo: resume.redo.clone(),
            marks: VecDeque::new(),
            tiles,
            layout,
            galaxy,
            state: state.clone(),
            pending: None,
            waiting: None,
            can_undo: false,
            undoing: None,
            resuming: false,
            awaiting: None,
            fresh: true,
            auto_resolved: Vec::new(),
            undone: resume.undone.clone(),
            scripts: ScriptRuns::new(OnMismatch::TakeBack),
            opening: OpeningRun::default(),
            staged: resume.staged.clone(),
            staged_shared: false,
            staged_told: None,
            batch: None,
            batch_result: None,
            quiet: if resume.log.is_empty() {
                resume.answers.len()
            } else {
                0
            },
            saved_log: resume.log.len(),
            saved_events: resume
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

    /// The position the page sees (the newest published one).
    #[must_use]
    pub fn state(&self) -> &GameState {
        &self.state
    }

    /// The map of the game, if it has one.
    #[must_use]
    pub fn galaxy(&self) -> Option<&Galaxy> {
        self.galaxy.as_ref()
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
        self.quiet > 0 || self.decision_log.len() < self.saved_log
    }

    /// The seats that the page plays now (bit 0 is seat `a`).
    #[must_use]
    pub fn humans(&self) -> u32 {
        self.switches
            .last()
            .map_or(self.identity.humans, |switch| switch.humans)
    }

    #[must_use]
    pub fn switches(&self) -> &[SeatSwitch] {
        &self.switches
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
    /// event. `own` is the number of the answer when the page plays the seat.
    pub fn selected(&mut self, choice: &Choice, option: &ChoiceOption, own: Option<u64>) {
        let passed = own.is_some() && std::mem::take(&mut self.declined);
        self.selected.push_back(Selected {
            choice: choice.clone(),
            option: option.clone(),
            own,
            passed,
        });
    }

    /// The game declined a window for the Never cards of the page: its decision comes next.
    pub fn declined_for_never(&mut self) {
        self.declined = true;
    }

    fn selected_for(&mut self, record: &DecisionRecord, remove: bool) -> Option<Selected> {
        let index = self.selected.iter().position(|selected| {
            selected.choice.player == record.player
                && selected.choice.prompt == record.prompt
                && selected.option.id == record.chosen
        })?;
        if remove {
            self.selected.remove(index)
        } else {
            self.selected.get(index).cloned()
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
                let selected = self
                    .selected_for(record, false)
                    .map(|selected| selected.option);
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
            let option = selected.as_ref().map(|selected| &selected.option);
            let (detail, movement, seat_detail) = verified_decision_facts(
                record,
                option,
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
            let own = selected
                .as_ref()
                .map_or(record.player == self.viewer, |selected| {
                    selected.own.is_some()
                });
            if own {
                self.note_answer(record, index, selected.as_ref());
            }
        }
    }

    /// A decision of the page is in the log, at `index`.
    fn note_answer(&mut self, record: &DecisionRecord, index: usize, selected: Option<&Selected>) {
        self.answers.push(record.chosen.clone());
        let entry = match selected {
            Some(selected) => UndoEntry::new(&selected.choice, &selected.option, index)
                .asked(selected.own.unwrap_or(0))
                .passed(selected.passed),
            None => UndoEntry::recorded(
                record.player.clone(),
                index,
                record
                    .context
                    .as_ref()
                    .map(|context| context.subtype.clone())
                    .unwrap_or_default(),
            ),
        };
        self.undo.push(entry, true);
        // A redo list survives only while the person plays it again (as the server's does). An
        // answer that is played again from a save or after an undo is no new decision.
        if self.quiet > 0 {
            self.quiet -= 1;
        } else if index < self.saved_log {
            // Given again from the log of a save.
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
        if self.replaying() || seat != &self.viewer {
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
        let tactical_over = tactical_complete(game.events.get(events_before..).unwrap_or_default());
        let ended = self
            .scripts
            .after_step(tactical_over, game.state.active.as_ref());
        self.tell_scripts(ended);
    }

    fn finish(&mut self, state: &GameState) {
        if self.finished {
            return;
        }
        self.finished = true;
        self.pending = None;
        self.waiting = None;
        // The game is played again without the last answer.
        self.can_undo = self.undo_start().is_some();
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
        let auto_resolved = std::mem::take(&mut self.auto_resolved);
        let pending = self.shown_choice();
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
            seats: self.identity.seats.as_ref(),
        };
        let path = inputs.path();
        state_update_for(&inputs, &path, &viewer)
    }

    /// The choice that the update is made with: the pending one of the page, or the one of the
    /// seat that the game waits for (with no nonce: it is not the viewer's).
    fn shown_choice(&self) -> Option<(&Choice, &str)> {
        self.pending
            .as_ref()
            .map(|(choice, nonce)| (choice, nonce.as_str()))
            .or_else(|| self.waiting.as_ref().map(|choice| (choice, "")))
    }

    /// The full state of the game for the person, as a new connection gets it.
    fn snapshot(&mut self) -> ServerMessage {
        let viewer = self.viewer();
        let pending = self.shown_choice();
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
        snapshot.seats.clone_from(&self.identity.seats);
        snapshot.current_path = current_log_path(
            &self.state,
            pending.map(|(choice, _)| choice),
            &self.decision_log,
        );
        snapshot.undone = std::mem::take(&mut self.undone);
        ServerMessage::InitialSnapshot(snapshot)
    }

    /// The game waits for the page in `choice`, numbered `nonce`, at `state`. Returns the answer
    /// to give the play loop without asking the page: one more step of an undo.
    pub fn ask(&mut self, choice: &Choice, nonce: &str, state: &GameState) -> Option<Resume> {
        self.resuming = false;
        if let Some(undoing) = &self.undoing {
            let entries = self.undo.entries();
            let last = entries.iter().rev().find(|entry| !entry.passed);
            if entries.len() > undoing.keep
                && last.is_some_and(|entry| self.play_loop.reaches(entry.asked))
            {
                return Some(Resume::Undo);
            }
            self.undone = self
                .undoing
                .take()
                .map(|undoing| undoing.steps)
                .unwrap_or_default();
        }
        self.state.clone_from(state);
        self.version += 1;
        self.viewer.clone_from(&choice.player);
        self.pending = Some((choice.clone(), nonce.to_owned()));
        self.waiting = None;
        // An answer whose decision is still held back (a card declared in a reaction window of
        // rules version 3 is announced later) was accepted all the same: say so before the
        // next question, as the server answers a submit directly.
        if let Some(option_id) = self.awaiting.take() {
            self.out
                .push(ServerMessage::ActionAccepted(ActionAcceptedMsg {
                    protocol_version: PROTOCOL_VERSION,
                    game_id: self.identity.game_id.clone(),
                    game_version: self.version.saturating_sub(1),
                    option_id,
                }));
        }
        // A ready draft of the secondary that the seat is asked is sent: the seat is not asked.
        if !self.replaying()
            && let Some(index) = self.send_ready(choice, nonce)
        {
            return Some(Resume::Answer(index));
        }
        self.can_undo = self.undo_start().is_some();
        self.show();
        None
    }

    /// The game waits before `choice` of a seat that the page does not play, shown as `viewer`.
    /// Returns the answer to give the play loop without asking the page: an undo or a rollback
    /// goes on to the choice that it opens again.
    pub fn pause(
        &mut self,
        choice: &Choice,
        viewer: Option<&PlayerId>,
        state: &GameState,
    ) -> Option<Resume> {
        if self.resuming || self.undoing.is_some() {
            return Some(Resume::Go);
        }
        self.state.clone_from(state);
        self.version += 1;
        if let Some(viewer) = viewer {
            self.viewer.clone_from(viewer);
        }
        self.pending = None;
        self.waiting = Some(choice.clone());
        self.can_undo = self.undo_start().is_some();
        self.show();
        None
    }

    /// The seat that the game waits for, while it waits before a decision of another seat.
    #[must_use]
    pub fn waiting_for(&self) -> Option<&PlayerId> {
        self.waiting.as_ref().map(|choice| &choice.player)
    }

    fn show(&mut self) {
        if self.fresh {
            self.fresh = false;
            let snapshot = self.snapshot();
            self.out.push(snapshot);
            self.tell_staged();
            // A snapshot has no notes: the windows that were passed before it come with an
            // update of the same version.
            if self.auto_resolved.is_empty() {
                return;
            }
        }
        let update = self.state_update();
        self.out.push(update);
        if self.staged_told.as_ref() != Some(&self.viewer) {
            self.tell_staged();
        }
    }

    /// The page answered: nothing is pending until the next `ask`.
    pub fn answered(&mut self) {
        self.pending = None;
        self.waiting = None;
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
        let undone_steps = self.undo.take_back(mark.answers);
        if !rollback {
            if self.undoing.is_none() {
                self.undone = undone_steps
                    .into_iter()
                    .filter_map(|entry| entry.step)
                    .collect();
            }
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
        self.saved_log = self.saved_log.min(mark.decisions);
        self.declined = false;
        self.pending = None;
        self.waiting = None;
        self.resuming = true;
        self.awaiting = None;
        self.scripts.clear();
        self.auto_resolved.clear();
        self.batch = None;
        self.fresh = true;
    }

    // --- scripts --------------------------------------------------------------------------

    /// The answer of a running script to `choice` of the page, when it has one. A step that the
    /// game does not offer takes the whole script back: nothing of it stays in the game.
    fn script_answer(&mut self, choice: &Choice) -> Option<Scripted> {
        let (option, ended) = self.scripts.answer(choice);
        // The script began at the choice that its nonce numbers.
        let back = ended
            .iter()
            .find(|end| end.taken_back)
            .map(|end| end.nonce.parse().unwrap_or(0));
        self.tell_scripts(ended);
        match back {
            Some(start) => Some(Scripted::Rollback(start)),
            None => option.map(Scripted::Answer),
        }
    }

    /// Tells the page what became of its scripts.
    fn tell_scripts(&mut self, ended: Vec<Ended>) {
        for end in ended {
            self.out.push(ServerMessage::DraftApplied(DraftAppliedMsg {
                protocol_version: PROTOCOL_VERSION,
                game_id: self.identity.game_id.clone(),
                nonce: end.nonce,
                stopped: end.stopped,
            }));
        }
    }

    // --- commands -------------------------------------------------------------------------

    /// A command of the host. Messages are queued for [`HostSession::drain`].
    pub fn client_message(&mut self, message: ClientMessage) -> Handled {
        let mut handled = Handled::default();
        match message {
            ClientMessage::Subscribe { staged, .. } => {
                let snapshot = self.snapshot();
                self.out.push(snapshot);
                if staged {
                    self.share_staged();
                    self.tell_staged();
                }
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
                handled.resume = self.apply_script(&nonce, expected_version, script);
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
                    self.undo(&mut handled);
                }
            }
            ClientMessage::SetReactionMode { card, mode, .. } => {
                if !ti4_host::view::reaction_modes::is_reaction_key(&card) {
                    self.protocol_error(
                        ErrorKind::MalformedMessage,
                        format!("unknown reaction '{card}'"),
                    );
                } else {
                    {
                        let mut never = self.never.lock().expect("never set lock");
                        ti4_host::view::reaction_modes::set_mode(&mut never, &card, mode);
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
            ClientMessage::PreviewSecondary {
                request_id,
                card,
                primary,
                answers,
                ..
            } => {
                let reply = self.preview(&PreviewRequest {
                    request_id,
                    card,
                    primary,
                    answers,
                });
                self.out.push(ServerMessage::SecondaryPreview(reply));
            }
            // A bluff hold needs a second person; there is none.
            ClientMessage::SetReactionIntent { .. } | ClientMessage::PassReactionHold { .. } => {}
            ClientMessage::PutStaged {
                slot,
                base_revision,
                value,
                ..
            } => handled.resume = self.page_put(slot, base_revision, value),
            ClientMessage::RunDraft { .. } => {
                // `RunDraft` is answered by the caller (it needs the copy of the game).
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

    /// The answer of `ClientMessage::PreviewSecondary` for the seat the page is shown as
    /// (`ti4_host::preview`). The game is live while it waits for the page (a question or a
    /// step); the position is the one the page sees.
    #[must_use]
    pub fn preview(&self, request: &PreviewRequest) -> SecondaryPreviewMsg {
        let seat = &self.viewer;
        let live = !self.finished
            && self.error.is_none()
            && (self.pending.is_some() || self.waiting.is_some());
        let basis = if live {
            let position = Position {
                state: &self.state,
                decision_log: &self.decision_log,
                asked: self
                    .pending
                    .as_ref()
                    .map(|(choice, _)| (&choice.player, choice)),
            };
            eligibility(
                &position,
                seat,
                &StrategyCardId::new(request.card.clone()),
                &PlayerId::new(request.primary.clone()),
            )
            .map(|()| Basis {
                state: &self.state,
                galaxy: self.galaxy.as_ref(),
                decisions: self.decision_log.len(),
            })
        } else {
            Err(busy())
        };
        ti4_host::preview::answer(&self.identity.game_id, seat, request, self.version, basis)
    }

    /// Checks a request against the pending choice ([`check_submission`]). On success the place
    /// of `option_id` among its options (0 when none is asked for).
    fn check_pending(
        &mut self,
        nonce: &str,
        expected_version: u64,
        option_id: Option<&str>,
    ) -> Option<usize> {
        let checked = check_submission(
            self.pending
                .as_ref()
                .map(|(choice, pending_nonce)| (choice, pending_nonce.as_str())),
            nonce,
            expected_version,
            self.version,
            option_id,
        );
        if let Err(reason) = checked {
            self.reject(reason);
            return None;
        }
        let Some(option_id) = option_id else {
            return Some(0);
        };
        let place = self.pending.as_ref().and_then(|(choice, _)| {
            choice
                .options
                .iter()
                .position(|option| option.id == option_id)
        });
        if place.is_none() {
            // A free-form answer (a deal) has no place among the options, and the engine is
            // answered by place.
            self.reject(RejectionReason::ValidationFailed {
                message: "free-form answers are not available in an offline game".to_owned(),
            });
        }
        place
    }

    /// Sends `script` to the game: the pending choice and the choices after it are answered
    /// from it. A script that the game refuses on a copy ([`check_script`]) is refused before
    /// anything of it is in the game. A step that the game does not offer all the same (the
    /// copy cannot see past a decision of another seat) takes the script back whole
    /// ([`Scripted::Rollback`]), so a refused script leaves the game as it was.
    fn apply_script(
        &mut self,
        nonce: &str,
        expected_version: u64,
        script: Vec<DraftStep>,
    ) -> Option<Resume> {
        self.check_pending(nonce, expected_version, None)?;
        let (choice, _) = self.pending.clone()?;
        let first = first_answer(&script, &choice, &choice.player).and_then(|first| {
            match self.precheck(&choice, &script) {
                Check::Refused { reason, .. } => Err(reason),
                Check::Plays | Check::Unknown => Ok(first),
            }
        });
        let First {
            option,
            next,
            script: ordered,
        } = match first {
            Ok(first) => first,
            Err(reason) => {
                self.out.push(ServerMessage::DraftApplied(DraftAppliedMsg {
                    protocol_version: PROTOCOL_VERSION,
                    game_id: self.identity.game_id.clone(),
                    nonce: nonce.to_owned(),
                    stopped: Some(refused(&script, reason)),
                }));
                return None;
            }
        };
        let index = choice.options.iter().position(|o| o.id == option.id)?;
        let active = self.state.active.clone();
        let ended = self
            .scripts
            .begin(&choice.player, nonce, script, ordered, next, active);
        self.tell_scripts(ended);
        self.awaiting = Some(option.id);
        Some(Resume::Answer(index))
    }

    /// Plays `script` of the seat of `choice` on a copy of the game at its last finished step,
    /// with the decisions since then played again ([`check_script`]). `Unknown` when there is
    /// no copy, or the log of the session does not reach the step yet.
    fn precheck(&self, choice: &Choice, script: &[DraftStep]) -> Check {
        let Some((game, decisions)) = self.play_loop.script_copy() else {
            return Check::Unknown;
        };
        let Some(tail) = self.decision_log.get(decisions..) else {
            return Check::Unknown;
        };
        let never = self.never.lock().expect("never set lock").clone();
        check_script(game, tail.to_vec(), choice, script, &choice.player, never)
    }

    /// The place in the entries of the track of the first answer that one undo takes back: the
    /// last answer of the page, or a movement, the landings or a purchase whole. An answer from
    /// before the last change of the seats stays. A window that was declined for the Never cards
    /// is marked when it is answered, so no cards are read here.
    fn undo_start(&self) -> Option<usize> {
        self.undo.start(&self.viewer, &Modes::new())
    }

    /// One undo of the page: through the checkpoints when they reach back far enough, else by
    /// playing the game again from its log without the answers it takes back.
    fn undo(&mut self, handled: &mut Handled) {
        let parked = self.pending.is_some() || self.waiting.is_some();
        let Some(keep) = self.undo_start().filter(|_| parked || self.finished) else {
            self.reject(RejectionReason::NoPendingChoice);
            return;
        };
        let first = self.undo.entries()[keep].clone();
        let steps = self.undo.steps_from(keep);
        if parked && !self.finished && self.play_loop.reaches(first.asked) {
            self.undoing = Some(Undoing { keep, steps });
            handled.resume = Some(Resume::Undo);
            return;
        }
        let log = self.decision_log[..first.decision]
            .iter()
            .map(|record| record.chosen.clone())
            .collect();
        let mut redo = self.answers[keep..].to_vec();
        redo.extend(self.redo.iter().cloned());
        handled.restart = Some(Restart {
            answers: self.answers[..keep].to_vec(),
            redo,
            generation: self.generation + 1,
            version: self.version,
            log,
            switches: self.switches.clone(),
            undone: steps,
            bot_version: self.bot_version(),
            rules_version: self.rules_version(),
            staged: self.staged.clone(),
        });
        handled.resume = Some(Resume::Leave);
    }

    /// Plays `seat` from now on in place of the seat that the game is shown as.
    fn switch_seat(&mut self, seat: &str, handled: &mut Handled) {
        let bit = |player: &str| {
            crate::SEATS
                .iter()
                .take(self.identity.players)
                .position(|id| *id == player)
                .map(|index| 1_u32 << index)
        };
        let parked = self.pending.is_some() || self.waiting.is_some();
        let Some(wanted) = bit(seat).filter(|_| parked && !self.finished) else {
            self.reject(RejectionReason::NoPendingChoice);
            return;
        };
        let humans = self.humans();
        if humans & wanted != 0 {
            return;
        }
        let humans = (humans & !bit(self.viewer.as_str()).unwrap_or(0)) | wanted;
        // The seat that is given up drops what it staged.
        let left = self.viewer.clone();
        self.drop_staged(&left);
        let switch = SeatSwitch {
            decision: self.play_loop.decided(),
            humans,
        };
        self.switches.push(switch);
        self.undo
            .seats_changed(usize::try_from(switch.decision).unwrap_or(usize::MAX));
        self.undoing = None;
        handled.switch = Some(switch);
        handled.resume = Some(Resume::Switch);
    }

    /// A command that is not a `ClientMessage`.
    pub fn local(&mut self, command: LocalCommand, engine: serde_json::Value) -> Handled {
        let mut handled = Handled::default();
        match command {
            LocalCommand::SaveState {} => handled.save = Some(self.save_state(engine)),
            LocalCommand::Leave {} => handled.resume = Some(Resume::Leave),
            LocalCommand::Step {} => {
                if self.waiting.is_some() {
                    handled.resume = Some(Resume::Go);
                }
            }
            LocalCommand::SwitchSeat { seat } => self.switch_seat(&seat, &mut handled),
            // Set by the caller: it is a flag of the play loop.
            LocalCommand::Stepping { .. } => {}
            LocalCommand::SubmitBatch {
                nonce,
                expected_version,
                plan,
            } => handled.resume = self.submit_batch(&nonce, expected_version, &plan),
            LocalCommand::History { action } => match action {
                HistoryAction::Undo | HistoryAction::UndoBatch | HistoryAction::UndoPipeline => {
                    self.undo(&mut handled);
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
                            log: Vec::new(),
                            switches: self.switches.clone(),
                            undone: Vec::new(),
                            bot_version: self.bot_version(),
                            rules_version: self.rules_version(),
                            staged: self.staged.clone(),
                        });
                        handled.resume = Some(Resume::Leave);
                    }
                }
                HistoryAction::RestoreCursor { cursor } => {
                    self.restore(cursor, &mut handled);
                }
                HistoryAction::UndoTurn { turns } => self.undo_turn(turns, &mut handled),
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

    /// Go back to the start of the last `turns` turns of the seat that the game is shown as. The
    /// answers from there on are kept for a redo; the bots decide again from the same game, so
    /// the replay is the one a save of the shorter game gives.
    fn undo_turn(&mut self, turns: u8, handled: &mut Handled) {
        let parked = self.pending.is_some() || self.waiting.is_some();
        if !(parked || self.finished) {
            self.reject(RejectionReason::NoPendingChoice);
            return;
        }
        match ti4_host::history::turn_redo::redo_window(&self.decision_log, &self.viewer, turns) {
            Ok(window) => self.restore(window.start, handled),
            Err(error) => self.reject(RejectionReason::ValidationFailed {
                message: error.to_string(),
            }),
        }
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
        let keep = self
            .undo
            .entries()
            .partition_point(|entry| entry.decision < cursor);
        let mut redo = self.answers[keep..].to_vec();
        redo.extend(self.redo.iter().cloned());
        handled.restart = Some(Restart {
            answers: self.answers[..keep].to_vec(),
            redo,
            generation: self.generation + 1,
            version: self.version,
            log: Vec::new(),
            switches: self.switches.clone(),
            undone: Vec::new(),
            bot_version: self.bot_version(),
            rules_version: self.rules_version(),
            staged: self.staged.clone(),
        });
        handled.resume = Some(Resume::Leave);
    }

    /// The rule bot version of this game, as its setup records it.
    fn bot_version(&self) -> Option<u32> {
        Some(self.identity.bot_version)
    }

    /// The rules version of this game as a save or restart records it (left out for 0, so a
    /// legacy game is saved as it always was).
    fn rules_version(&self) -> Option<u32> {
        (self.identity.rules_version != 0).then_some(self.identity.rules_version)
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

    /// The game is new and starts from a scenario with this opening: its seat is not asked until
    /// the opening stops. A game that is played again from a save has no opening: its answers
    /// are in the save.
    pub fn open(&mut self, opening: Option<Opening>) {
        self.opening = OpeningRun::new(opening);
    }

    /// What the opening of a scenario, a running script or a batch answers to `choice` of the
    /// person.
    pub fn scripted(&mut self, choice: &Choice) -> Scripted {
        if let Some(option) = self.opening.answer(choice) {
            return Scripted::Answer(option);
        }
        if let Some(answer) = self.script_answer(choice) {
            return answer;
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

    // --- staged work -----------------------------------------------------------------------

    /// The page takes part in the staged work of its seats: it is told their slots.
    pub fn share_staged(&mut self) {
        self.staged_shared = true;
    }

    /// What `seat` has staged.
    #[must_use]
    pub fn seat_staged(&self, seat: &PlayerId) -> SeatStaged {
        self.staged.get(seat).cloned().unwrap_or_default()
    }

    /// Tells the page the slots of the seat that it is shown as, as the message after a
    /// snapshot (with `connection`): they replace what the page holds.
    fn tell_staged(&mut self) {
        if !self.staged_shared {
            return;
        }
        self.staged_told = Some(self.viewer.clone());
        let mut message = self
            .seat_staged(&self.viewer)
            .message(&self.identity.game_id);
        message.connection = Some(PAGE_CONNECTION);
        self.out.push(ServerMessage::SeatStaged(message));
    }

    /// Writes one slot of `seat` ([`SeatStaged::put`]). Revisions count up for the whole page.
    /// A write is told to the page when it holds the slots of `seat`, with `writer`: the page,
    /// or none for a write of the session.
    fn put_staged(
        &mut self,
        seat: &PlayerId,
        slot: StagedSlot,
        base_revision: u64,
        value: Option<serde_json::Value>,
        writer: Option<u64>,
    ) -> Put {
        let top = self
            .staged
            .values()
            .map(|seat| seat.revision)
            .max()
            .unwrap_or(0);
        let mut entry = self.seat_staged(seat);
        entry.revision = top;
        let put = entry.put(slot, base_revision, value);
        if put != Put::Written {
            return put;
        }
        self.staged.insert(seat.clone(), entry.clone());
        if self.staged_shared && self.staged_told.as_ref() == Some(seat) {
            let mut message = entry.message(&self.identity.game_id);
            message.writer = writer;
            self.out.push(ServerMessage::SeatStaged(message));
        }
        put
    }

    /// A write of the page, for the seat that it is shown as. A stale write is answered with
    /// what the seat has. A draft of a secondary that is marked ready while the game asks the
    /// seat for it is sent now: the answer for the parked game.
    fn page_put(
        &mut self,
        slot: StagedSlot,
        base_revision: u64,
        value: Option<serde_json::Value>,
    ) -> Option<Resume> {
        let seat = self.viewer.clone();
        match self.put_staged(&seat, slot, base_revision, value, Some(PAGE_CONNECTION)) {
            Put::Stale => {
                let message = self.seat_staged(&seat).message(&self.identity.game_id);
                self.out.push(ServerMessage::SeatStaged(message));
                None
            }
            Put::Written if slot == StagedSlot::Secondary && !self.replaying() => {
                let (choice, nonce) = self.pending.clone()?;
                if choice.player != seat || self.finished {
                    return None;
                }
                self.send_ready(&choice, &nonce).map(Resume::Answer)
            }
            Put::Written | Put::Unchanged => None,
        }
    }

    /// Clears every slot of `seat` (a write of the session).
    fn drop_staged(&mut self, seat: &PlayerId) {
        let slots: Vec<(StagedSlot, u64)> = self
            .staged
            .get(seat)
            .map(|staged| {
                staged
                    .slots
                    .iter()
                    .filter(|(_, value)| value.value.is_some())
                    .map(|(slot, value)| (*slot, value.revision))
                    .collect()
            })
            .unwrap_or_default();
        for (slot, revision) in slots {
            self.put_staged(seat, slot, revision, None, None);
        }
    }

    /// The ready draft of the secondary that `choice` asks its seat, numbered `nonce`: when the
    /// game plays it whole on a copy, it begins as a script, its slot is cleared, and the place
    /// of its first answer is returned. A draft that no longer fits is written back not ready,
    /// and the seat is asked.
    fn send_ready(&mut self, choice: &Choice, nonce: &str) -> Option<usize> {
        let seat = &choice.player;
        let ready = ready::ready_draft(&self.staged, seat, choice, self.state.round)?;
        let script = ready.draft.script();
        let check = self
            .play_loop
            .draft(seat, &ready.scope(), &script)
            .map_or(ReadyCheck::Unknown, |outcome| {
                ready::judge(&outcome, script.len())
            });
        let first = match check {
            ReadyCheck::Unknown => return None,
            ReadyCheck::Unfit => None,
            ReadyCheck::Fits => first_answer(&script, choice, seat).ok(),
        };
        let place = first.as_ref().and_then(|first| {
            choice
                .options
                .iter()
                .position(|option| option.id == first.option.id)
        });
        let (Some(first), Some(place)) = (first, place) else {
            self.put_staged(
                seat,
                StagedSlot::Secondary,
                ready.revision,
                Some(ready::unready_value(ready.value)),
                None,
            );
            return None;
        };
        let active = self.state.active.clone();
        let ended = self
            .scripts
            .begin(seat, nonce, script, first.script, first.next, active);
        self.tell_scripts(ended);
        self.put_staged(seat, StagedSlot::Secondary, ready.revision, None, None);
        Some(place)
    }

    /// The save of the game as it stands ([`HostSave`]), as JSON.
    #[must_use]
    pub fn save_state(&self, extra: serde_json::Value) -> serde_json::Value {
        let save = HostSave {
            format: crate::host_setup::SAVE_FORMAT,
            seed: self.identity.seed,
            players: self.identity.players,
            game: self.identity.game.clone(),
            table: self.identity.table.clone(),
            scenario: self.identity.scenario.clone(),
            bot_version: self.bot_version(),
            rules_version: self.rules_version(),
            humans: self.identity.humans,
            decisions: self
                .decision_log
                .iter()
                .map(|record| record.chosen.clone())
                .collect(),
            answers: self.answers.clone(),
            switches: self.switches.clone(),
            redo: self.redo.clone(),
            staged: self.staged.clone(),
            never: self
                .never
                .lock()
                .expect("never set lock")
                .iter()
                .filter(|(_, mode)| **mode == ReactionMode::Never)
                .map(|(card, _)| card.clone())
                .collect(),
            event_times: self
                .events
                .iter()
                .map(|event| event.timestamp.clone())
                .collect(),
            event_versions: self.events.iter().map(|event| event.version).collect(),
            version: self.version,
            generation: self.generation,
            decisions_hash: decisions_hash(&self.decision_log),
            finished: self.finished,
            round: self.state.round,
            phase: self.state.phase,
            engine: extra,
        };
        serde_json::to_value(save).expect("a save is JSON")
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

/// The save of a game ([`HostSession::save_state`]), format [`crate::host_setup::SAVE_FORMAT`]:
/// the game, and every decision of every seat in order, so that a resume gives each of them again
/// and no seat decides again. An undo cuts `decisions` at the answer that it takes back.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct HostSave {
    /// [`crate::host_setup::SAVE_FORMAT`]: a save of another format is not read.
    pub format: u64,
    pub seed: u64,
    pub players: usize,
    /// The game as the host was given it, with the choices that were left open filled in (card
    /// set, map, factions...): a save replays with it.
    pub game: crate::host_setup::GameRequest,
    /// The table of `ti4_host::setup` the game was made from; none for a game of a fixed setup.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub table: Option<ti4_host::setup::TableSetup>,
    /// The scenario the game starts from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The rule bot version the game was played with (left out: version 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bot_version: Option<u32>,
    /// The rules version the game was played with (`GameState::rules_version`; left out:
    /// version 0, the rules of a save made before the field existed).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules_version: Option<u32>,
    /// The seats that the page plays at the start (bit 0 is seat `a`).
    pub humans: u32,
    /// The option id of every decision of every seat, in order.
    pub decisions: Vec<String>,
    /// The page's answers among them (ids), in order.
    pub answers: Vec<String>,
    /// The changes of the seats that the page plays, in order.
    pub switches: Vec<SeatSwitch>,
    /// The answers that a redo can bring back, next first.
    pub redo: Vec<String>,
    /// What each seat has staged and not sent (`ti4_host::staged`).
    pub staged: BTreeMap<PlayerId, SeatStaged>,
    /// The cards that are never offered, by printed name; an own-turn card is not kept.
    pub never: BTreeSet<String>,
    /// The time of each event of the game, in order.
    pub event_times: Vec<String>,
    /// The game version of each event, in order.
    pub event_versions: Vec<Option<u64>>,
    pub version: u64,
    /// The history generation of the run.
    pub generation: u64,
    /// [`decisions_hash`] of the decisions: a replay tells that the engine still decides the same.
    pub decisions_hash: String,
    pub finished: bool,
    pub round: u32,
    pub phase: Phase,
    /// The engine that saved: its build and the digest of its content.
    #[cfg_attr(feature = "ts", ts(type = "{ build: string, content_digest: string }"))]
    pub engine: serde_json::Value,
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
