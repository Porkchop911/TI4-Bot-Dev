//! The replayer application's state, with no window in it.
//!
//! R02-007 splits in two on purpose. Everything a reviewer could get wrong — which seat answers
//! next, whether "Play from this frame" is honest, whether a click landed twice, whether closing the
//! window during a rebuild loses the project — is decided here, in plain data, where a test can
//! reach it. The egui shell in `gui.rs` only turns these answers into widgets and calls back.
//!
//! Two rules shape the whole module. **A button that does nothing must say why**: every refusal
//! carries the sentence the tooltip will show, so there is no state where the app is silent about
//! itself. **Nothing here can mutate a game**: the only path to the engine is a [`BranchHandle`],
//! which in the real app is a [`crate::live::Gate`] and in tests is a scripted stand-in, so the
//! reducer never needs a running simulation to be sure of itself.

use std::fs;
use std::io::Write as _;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ti4_model::id::PlayerId;

use crate::BranchId;
use crate::control::{
    ManualSubmission, ModeEffect, OfferId, PendingManualChoice, SeatControl, SeatMode,
    SubmitOutcome,
};
use crate::live::{AdvanceGoal, FrameTick, Gate, LiveError, LiveState, Snapshot};
use crate::project::{ReplayerProject, Verification};
use crate::rebuild::ReplayScript;

/// Where the replayer keeps its own window settings.
///
/// R01 keeps `out/reviews/reviewer-settings.json`. These are deliberately under a different
/// directory: opening a replayer must not change how the reviewer looks tomorrow, and the two
/// applications are built and shipped separately.
pub const SETTINGS_PATH: &str = "out/replays/replayer-settings.json";

/// The settings file is advisory; refusing to read a corrupt one beats guessing at it.
pub const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

/// The replayer's own settings. No field is shared with R01's `ReviewerSettings`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ReplaySettings {
    pub window_width: f32,
    pub window_height: f32,
    pub players_open: bool,
    pub decisions_open: bool,
    pub branches_open: bool,
    /// Project last opened, so a re-open after a crash is one click.
    pub last_project: Option<String>,
    /// Selected branch of that project, kept as the project's own id string.
    pub last_branch: Option<u32>,
    /// The table last played at, so that "start a table" means "the one I keep playing" and not "the
    /// example in the source tree".
    pub setup: SetupDefaults,
}

/// What the setup form starts with.
///
/// Kept beside [`ReplaySettings`] rather than flattened into it: four booleans in one struct is how
/// clippy says a struct is really two, and this group is one decision - what to offer when the window
/// opens with nothing in it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SetupDefaults {
    /// Checkpoint path as the operator typed it. Blank means nothing has been remembered.
    pub checkpoint: String,
    /// Map pool path as the operator typed it. Blank means nothing has been remembered.
    pub map_pool: String,
    /// Seed as text, because it is edited as text and a leading zero is somebody's idea of a seed.
    pub seed: String,
    pub rotation: usize,
    /// `"Learner"` or `"Accepted"`, kept as words so this file does not depend on how an enum happens
    /// to serialize.
    pub profile_table: String,
    pub temperature: f64,
    pub diplomacy: bool,
    /// The six factions last seated, in order. Empty means nothing remembered (the standard six).
    pub lineup: Vec<String>,
}

impl Default for SetupDefaults {
    fn default() -> Self {
        Self {
            checkpoint: String::new(),
            map_pool: String::new(),
            seed: "4242".to_owned(),
            rotation: 0,
            profile_table: "Learner".to_owned(),
            temperature: 0.5,
            diplomacy: false,
            lineup: Vec::new(),
        }
    }
}

impl Default for ReplaySettings {
    fn default() -> Self {
        Self {
            // Slightly wider than the reviewer: the seat chips and the choice panel share the board's
            // row rather than stacking under it.
            window_width: 1720.0,
            window_height: 980.0,
            players_open: true,
            decisions_open: true,
            branches_open: true,
            last_project: None,
            last_branch: None,
            setup: SetupDefaults::default(),
        }
    }
}

impl ReplaySettings {
    /// Read settings, treating anything unreadable, oversized or unknown as "use the defaults".
    ///
    /// Settings are not state a user would call precious, and refusing to launch over a bad window
    /// size would be worse than resetting it.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        let Ok(metadata) = fs::metadata(path) else {
            return Self::default();
        };
        if metadata.len() > MAX_SETTINGS_BYTES {
            return Self::default();
        }
        let Ok(text) = fs::read_to_string(path) else {
            return Self::default();
        };
        serde_json::from_str(&text).unwrap_or_default()
    }

    /// Write settings atomically: temp file in the same directory, then rename over the target.
    ///
    /// # Errors
    ///
    /// Returns the IO error from creating, writing or renaming the file, and if the settings cannot
    /// be serialized.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .map_err(|error| format!("creating {}: {error}", parent.display()))?;
        }
        let temp = path.with_extension("json.tmp");
        let text = serde_json::to_string_pretty(self).map_err(|error| error.to_string())?;
        {
            let mut file = fs::File::create(&temp)
                .map_err(|error| format!("writing {}: {error}", temp.display()))?;
            file.write_all(text.as_bytes())
                .map_err(|error| format!("writing {}: {error}", temp.display()))?;
        }
        fs::rename(&temp, path).map_err(|error| format!("replacing {}: {error}", path.display()))
    }
}

/// What the running branch of the current branch will answer to.
///
/// Implemented by [`Gate`] in the application and by a scripted stand-in in tests, so every
/// interaction below is reachable without a game: parked, running, paused, answered, refused.
pub trait BranchHandle {
    /// The control surface the gate would draw right now.
    fn snapshot(&self) -> Snapshot;
    /// Ask the seat for a mode; the gate decides whether a panel is released by it.
    fn set_mode(&self, seat: &PlayerId, mode: SeatMode) -> ModeEffect;
    /// Hand an answer to the gate, which validates it against what is actually pending.
    fn submit(&self, submission: &ManualSubmission) -> SubmitOutcome;
    /// Ask the branch to advance.
    ///
    /// # Errors
    ///
    /// [`LiveError::BadState`] when the branch cannot take a goal where it stands.
    fn run(&self, goal: AdvanceGoal) -> Result<(), LiveError>;
    /// Stop at the next step boundary.
    fn pause(&self);
    /// Stop the branch for good.
    fn stop(&self);
    /// Let the policy answer the parked decision, once, for a manual seat — only if `offer` is the
    /// occurrence still waiting.
    ///
    /// # Errors
    ///
    /// Whatever the branch refuses with when that offer is not the one parked.
    fn delegate_pending(&self, offer: OfferId) -> Result<PlayerId, LiveError>;
}

/// A handle behind the reference counter a window keeps it in.
///
/// The branch thread and the window share one gate, so the window's handle is an `Arc<Gate>`; without
/// this the app would have to be generic over both spellings to say the same thing.
impl<H: BranchHandle + ?Sized> BranchHandle for std::sync::Arc<H> {
    fn snapshot(&self) -> Snapshot {
        (**self).snapshot()
    }

    fn set_mode(&self, seat: &PlayerId, mode: SeatMode) -> ModeEffect {
        (**self).set_mode(seat, mode)
    }

    fn submit(&self, submission: &ManualSubmission) -> SubmitOutcome {
        (**self).submit(submission)
    }

    fn run(&self, goal: AdvanceGoal) -> Result<(), LiveError> {
        (**self).run(goal)
    }

    fn pause(&self) {
        (**self).pause();
    }

    fn stop(&self) {
        (**self).stop();
    }

    fn delegate_pending(&self, offer: OfferId) -> Result<PlayerId, LiveError> {
        (**self).delegate_pending(offer)
    }
}

impl BranchHandle for Gate {
    fn snapshot(&self) -> Snapshot {
        Gate::snapshot(self)
    }

    fn set_mode(&self, seat: &PlayerId, mode: SeatMode) -> ModeEffect {
        Gate::set_mode(self, seat, mode)
    }

    fn submit(&self, submission: &ManualSubmission) -> SubmitOutcome {
        Gate::submit(self, submission)
    }

    fn run(&self, goal: AdvanceGoal) -> Result<(), LiveError> {
        Gate::run(self, goal)
    }

    fn pause(&self) {
        Gate::pause(self);
    }

    fn stop(&self) {
        Gate::stop(self);
    }

    fn delegate_pending(&self, offer: OfferId) -> Result<PlayerId, LiveError> {
        Gate::delegate_pending(self, offer)
    }
}

/// Why "Play from this frame" is not available, in the words the tooltip shows.
///
/// There is deliberately no silent case: if the button is greyed, the reader can ask why without
/// guessing what a verified branch means.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PlayBlock {
    /// The project's inputs are not the files on disk, so nothing would reproduce.
    InputsUnverified(String),
    /// The branch has never been rebuilt, so its frames have no proven relationship to the engine.
    BranchUnverified,
    /// A rebuild is already running; two would fight over the same inputs.
    Rebuilding,
    /// A branch is still running; Play forks from recorded history, not from a half-answered step.
    BranchRunning(LiveState),
    /// The viewed frame is the last one of a finished game.
    TerminalFrame,
    /// Nothing is selected, which happens before the first frame arrives.
    NoFrame,
    /// The project is at its branch bound.
    TooManyBranches {
        /// The bound the project enforces.
        limit: u32,
    },
    /// The project has no frame budget left for a branch this long.
    FrameBudget {
        /// Frames the project already holds.
        total: u64,
        /// Frames the project may hold in total.
        limit: u64,
    },
    /// The tree refused the fork for a structural reason.
    Refused(String),
}

impl PlayBlock {
    /// The tooltip text. Written for someone who has not read this module.
    #[must_use]
    pub fn tooltip(&self) -> String {
        match self {
            Self::InputsUnverified(why) => {
                format!("Play is off because the checkpoint, map pool or content corpus on disk is not the one this project recorded: {why}")
            }
            Self::BranchUnverified => {
                "This branch has not been reproduced from its inputs yet, so a frame of it cannot be proved to be a real game state. Rebuild it once first.".to_owned()
            }
            Self::Rebuilding => "A rebuild is already running. Wait for it or cancel it.".to_owned(),
            Self::BranchRunning(state) => format!(
                "The branch is {} - pause or stop it first, then Play forks from the frame you are                  looking at.",
                state.as_str()
            ),
            Self::TerminalFrame => {
                "This is the end of the game; there is nothing after it to replay.".to_owned()
            }
            Self::NoFrame => "No frame is selected yet.".to_owned(),
            Self::TooManyBranches { limit } => format!(
                "This project is at its limit of {limit} branches. Open another project to keep exploring."
            ),
            Self::FrameBudget { total, limit } => format!(
                "This project already holds {total} of its {limit} frames, so a new branch would not fit."
            ),
            Self::Refused(why) => format!("The branch tree refused this fork: {why}"),
        }
    }
}

/// What Play is about to do, so the caller can run the rebuild on its own thread and the tests can
/// see what would have been asked of it.
#[derive(Clone, Debug)]
pub struct PlayPlan {
    /// The new child branch, already allocated in the project.
    pub child: BranchId,
    /// The branch it was forked from, whose future stays exactly as it was.
    pub parent: BranchId,
    /// The frame the fork sits on.
    pub frame: u64,
    /// The recorded decisions the rebuild must reproduce to reach the fork.
    pub script: ReplayScript,
}

/// What a finished rebuild brought back.
#[derive(Clone, Debug, PartialEq)]
pub struct RebuildOutcome {
    /// The frames the child branch now has, starting from the fork.
    pub frames: Vec<FrameTick>,
    /// How many recorded decisions the prefix forced.
    pub replayed: usize,
}

/// What the rebuild of the current branch is doing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RebuildStatus {
    /// Nothing running.
    Idle,
    /// A child branch is being rebuilt; cancel is available.
    Running {
        /// Branch being produced.
        branch: BranchId,
        /// The frame it is rebuilding toward.
        frame: u64,
    },
    /// Finished, with the count of forced decisions.
    Done {
        /// Branch produced.
        branch: BranchId,
        /// Forced prefix decisions.
        replayed: usize,
    },
    /// Refused, with the reason kept so the panel can show it.
    Failed(String),
}

/// One node of the branch tree as the selector draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct BranchNode {
    pub id: BranchId,
    pub title: String,
    pub parent: Option<BranchId>,
    pub frames: usize,
    pub verified: bool,
    pub playable: bool,
    pub children: Vec<BranchId>,
    /// The frame this branch starts from, for an imported branch `None`.
    pub fork_frame: Option<u64>,
}

/// The application state: a project, a viewed frame, an optional live branch, and the rebuild that
/// may be running in the background.
#[derive(Debug)]
pub struct ReplayApp<H> {
    project: ReplayerProject,
    verification: Verification,
    current: BranchId,
    handle: Option<H>,
    /// Frames the app has seen for each branch: imported history, live frames, rebuild output.
    frames: std::collections::BTreeMap<BranchId, Vec<FrameTick>>,
    viewed: std::collections::BTreeMap<BranchId, usize>,
    rebuild: RebuildStatus,
    /// The occurrence the reader last answered, so a second click cannot pretend otherwise.
    ///
    /// Keyed on [`OfferId`], never on the fingerprint. Identical-looking offers in succession are
    /// separate questions — a seat over capacity is asked "remove a unit" several times in one step,
    /// each with the single option `remove|0` — and keying on shape hid every one after the first,
    /// leaving the engine parked with no panel on screen. An occurrence id is never reused, so a
    /// match means this very offer and nothing else.
    answered: Option<OfferId>,
    /// The last thing the app did, for the status line.
    notice: Option<String>,
}

impl<H: BranchHandle> ReplayApp<H> {
    /// Take over a project that has been verified against the files on disk.
    #[must_use]
    pub fn new(project: ReplayerProject, verification: Verification, base: &Path) -> Self {
        let source = BranchId::SOURCE;
        let frames = usize::try_from(project.branch(source).map_or(0, |branch| branch.frames))
            .unwrap_or(usize::MAX);
        let mut viewed = std::collections::BTreeMap::new();
        viewed.insert(source, frames.saturating_sub(1));
        Self {
            project,
            verification,
            current: source,
            handle: None,
            frames: std::collections::BTreeMap::new(),
            viewed,
            rebuild: RebuildStatus::Idle,
            answered: None,
            notice: Some(format!(
                "Opened {}",
                base.file_name().map_or_else(
                    || "project".to_owned(),
                    |name| name.to_string_lossy().into_owned()
                )
            )),
        }
    }

    /// The project being replayed.
    #[must_use]
    pub const fn project(&self) -> &ReplayerProject {
        &self.project
    }

    /// Mutable access, for the caller that saves and extends it.
    #[must_use]
    pub const fn project_mut(&mut self) -> &mut ReplayerProject {
        &mut self.project
    }

    /// What the inputs on disk matched to.
    #[must_use]
    pub const fn verification(&self) -> &Verification {
        &self.verification
    }

    /// The branch being viewed.
    #[must_use]
    pub const fn current(&self) -> BranchId {
        self.current
    }

    /// What the last command did, for the status line.
    #[must_use]
    pub fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    /// Attach a running branch to the current branch id.
    pub fn attach(&mut self, handle: H, frames: Vec<FrameTick>) {
        self.frames.insert(self.current, frames);
        self.handle = Some(handle);
        self.answered = None;
    }

    /// Point the app at a different branch's gate without touching the frames it holds.
    ///
    /// A fork's gate exists from the moment `Play` is pressed, but its frames do not exist until the
    /// rebuild has proved them. [`attach`](Self::attach) does both at once, which is right when a
    /// branch is ready and wrong while one is being rebuilt: until this existed, a seat flipped
    /// during a rebuild was sent to the gate of the branch that had just been left behind, and the
    /// fork started with the modes captured when the button was pressed. The old gate is stopped,
    /// because the window plays one history at a time.
    pub fn attach_handle(&mut self, handle: H) {
        if let Some(previous) = self.handle.replace(handle) {
            previous.stop();
        }
        self.answered = None;
    }

    /// Detach the running branch, keeping whatever frames were seen.
    pub fn detach(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.stop();
        }
        self.answered = None;
    }

    /// The handle, when a branch is running.
    #[must_use]
    pub fn handle(&self) -> Option<&H> {
        self.handle.as_ref()
    }

    /// Frames the app knows for a branch.
    #[must_use]
    pub fn frames(&self, branch: BranchId) -> &[FrameTick] {
        self.frames.get(&branch).map_or(&[][..], Vec::as_slice)
    }

    /// The frame being viewed on a branch.
    #[must_use]
    pub fn viewed(&self, branch: BranchId) -> Option<usize> {
        self.viewed.get(&branch).copied()
    }

    /// Whether the viewed frame is the live tip, which the UI marks distinctly because a reviewer
    /// looking at frame 40 of a live branch is *not* watching what happens next.
    #[must_use]
    pub fn at_tip(&self) -> bool {
        let frames = self.frames(self.current).len();
        frames > 0 && self.viewed_current() == frames.checked_sub(1)
    }

    fn viewed_current(&self) -> Option<usize> {
        self.viewed(self.current)
    }

    /// The live state of the attached branch, or `Idle`-equivalent when nothing is attached.
    #[must_use]
    pub fn live_state(&self) -> Option<LiveState> {
        self.handle.as_ref().map(|handle| handle.snapshot().state)
    }

    /// What the control surface says: state, seat modes, pending choice.
    #[must_use]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.handle.as_ref().map(BranchHandle::snapshot)
    }

    /// The choice on screen, unless the reader already answered it - in which case the panel shows
    /// the answer and waits for the branch to move.
    #[must_use]
    pub fn pending(&self) -> Option<PendingManualChoice> {
        let pending = self.handle.as_ref()?.snapshot().pending?;
        if self.answered == Some(pending.offer) {
            None
        } else {
            Some(pending)
        }
    }

    /// Seat modes as the attached branch has them, falling back to the branch's saved settings.
    #[must_use]
    pub fn seats(&self) -> SeatControl {
        if let Some(snapshot) = self.snapshot() {
            return snapshot.seats;
        }
        let mut seats = SeatControl::all_auto();
        let branch = self.current;
        if let Some(branch) = self.project.branch(branch) {
            for setting in &branch.seats {
                seats.set_mode(&PlayerId::new(&setting.player), setting.mode);
            }
        }
        seats
    }

    /// Toggle one seat. Independent of every other seat, and of whether the branch is running.
    pub fn toggle_seat(&mut self, seat: &PlayerId) -> SeatMode {
        let mode = match self.seats().mode(seat) {
            SeatMode::Auto => SeatMode::Manual,
            SeatMode::Manual => SeatMode::Auto,
        };
        self.set_seat_mode(seat, mode)
    }

    /// Ask for one mode on one seat.
    pub fn set_seat_mode(&mut self, seat: &PlayerId, mode: SeatMode) -> SeatMode {
        let previous = self.seats().mode(seat);
        if let Some(handle) = &self.handle {
            let effect = handle.set_mode(seat, mode);
            let state = handle.snapshot().state;
            if matches!(effect, ModeEffect::ReleasedBot { .. }) {
                self.answered = None;
                self.notice = Some(format!(
                    "{seat} is on {mode:?}: the choice it was waiting on goes to the policy"
                ));
            } else if state.accepts_run() {
                self.notice = Some(format!("{seat} is on {mode:?}"));
            } else {
                // The chip changed and the branch cannot act on it, which is not the same thing as
                // the chip not working. Saying so is the difference between "this button is broken"
                // and "fork from a frame and the seat is yours".
                self.notice = Some(format!(
                    "{seat} is on {mode:?}, but this branch is {} and will not ask anything else.                      Fork from a frame to play it on.",
                    state.as_str()
                ));
            }
        } else {
            self.notice = Some(format!("{seat} is on {mode:?} (saved for the next run)"));
        }
        self.remember_seat_mode(seat, mode);
        let _ = previous;
        mode
    }

    /// Save the seat mode on the branch, so the next run of this branch starts where the reader left
    /// it. Modes are presentation of intent; the engine is unaffected until a run begins.
    fn remember_seat_mode(&mut self, seat: &PlayerId, mode: SeatMode) {
        if let Some(branch) = self
            .project
            .branches
            .iter_mut()
            .find(|branch| branch.id == self.current)
        {
            let player = seat.to_string();
            if let Some(setting) = branch.seats.iter_mut().find(|s| s.player == player) {
                setting.mode = mode;
            } else {
                branch
                    .seats
                    .push(crate::project::SeatSetting { player, mode });
            }
        }
    }

    /// Click an option. The gate re-validates against what is actually pending; the app additionally
    /// refuses a second click on a choice it has already answered.
    pub fn submit(&mut self, submission: &ManualSubmission) -> SubmitOutcome {
        let Some(handle) = &self.handle else {
            self.notice = Some("Nothing is running to answer".to_owned());
            return SubmitOutcome::NoPendingChoice;
        };
        // The click carries the occurrence it was drawn on, so no snapshot of the gate is needed to
        // tell a double click from a fresh question: a match on the id is this very offer, and an
        // identical-looking later offer has a different id. The gate checks it again under its lock.
        if self.answered == Some(submission.offer) {
            self.notice = Some("That choice was already answered".to_owned());
            return SubmitOutcome::Duplicate;
        }
        let outcome = handle.submit(submission);
        match &outcome {
            SubmitOutcome::Accepted { option_id } => {
                self.answered = Some(submission.offer);
                self.notice = Some(format!("Answered {option_id}"));
            }
            SubmitOutcome::Stale { .. } => {
                self.notice = Some("That choice had already changed".to_owned());
            }
            SubmitOutcome::NotOffered { .. } => {
                self.notice = Some("That option was not on offer".to_owned());
            }
            SubmitOutcome::Duplicate => self.notice = Some("Already answered".to_owned()),
            SubmitOutcome::NoPendingChoice => {
                self.notice = Some("Nothing was waiting for a human".to_owned());
            }
        }
        outcome
    }

    /// Let the policy answer this one decision for the parked seat.
    ///
    /// # Errors
    ///
    /// [`LiveError::ThreadLost`] with nothing attached, and the branch's own refusal when nothing is
    /// parked or the parked seat is not manual.
    pub fn delegate_pending(&mut self, offer: OfferId) -> Result<PlayerId, LiveError> {
        let Some(handle) = &self.handle else {
            return Err(LiveError::ThreadLost);
        };
        let actor = handle.delegate_pending(offer)?;
        // Delegating answers this occurrence; a later click on the same panel is a duplicate.
        self.answered = Some(offer);
        self.notice = Some(format!("{actor} answers this one from the policy"));
        Ok(actor)
    }

    /// Ask the branch to advance.
    ///
    /// # Errors
    ///
    /// [`LiveError::ThreadLost`] when no branch is attached, and the gate's own refusal when it
    /// cannot take the goal where it stands.
    pub fn advance(&mut self, goal: AdvanceGoal) -> Result<(), LiveError> {
        let handle = self.handle.as_ref().ok_or(LiveError::ThreadLost)?;
        self.answered = None;
        handle.run(goal)
    }

    /// Pause at the next step boundary.
    pub fn pause(&mut self) {
        if let Some(handle) = &self.handle {
            handle.pause();
            self.notice = Some("Pausing at the next step boundary".to_owned());
        }
    }

    /// The result of asking to close while a rebuild is running: the rebuild is cancelled, the
    /// project is left as it was, and the caller is told whether anything is still in flight.
    #[must_use]
    pub fn rebuild_status(&self) -> RebuildStatus {
        self.rebuild.clone()
    }

    /// Navigation never forks. Selecting a frame is a read of history.
    pub fn select_frame(&mut self, index: usize) {
        let len = self.frames(self.current).len();
        if len == 0 {
            return;
        }
        self.viewed.insert(self.current, index.min(len - 1));
    }

    /// Step one frame back in time.
    pub fn previous_frame(&mut self) {
        let viewed = self.viewed_current().unwrap_or(0);
        self.select_frame(viewed.saturating_sub(1));
    }

    /// Step one frame forward in time.
    pub fn next_frame(&mut self) {
        let viewed = self.viewed_current().unwrap_or(0);
        self.select_frame(viewed + 1);
    }

    /// Jump to the live tip.
    pub fn go_to_tip(&mut self) {
        let len = self.frames(self.current).len();
        self.select_frame(len.saturating_sub(1));
    }

    /// Switch the whole view to another branch of the project. Frames already seen for it are kept.
    pub fn select_branch(&mut self, branch: BranchId) -> bool {
        if self.project.branch(branch).is_none() {
            return false;
        }
        if self.handle.is_some() {
            self.detach();
        }
        self.current = branch;
        self.answered = None;
        if !self.viewed.contains_key(&branch) {
            let frames = self.frames(branch).len();
            self.viewed.insert(branch, frames.saturating_sub(1));
        }
        self.notice = Some(format!("Viewing branch {branch}"));
        true
    }

    /// The tree as the selector draws it.
    #[must_use]
    pub fn tree(&self) -> Vec<BranchNode> {
        self.project
            .branches
            .iter()
            .map(|branch| branch.id)
            .map(|id| {
                let branch = &self.project.branch(id);
                BranchNode {
                    id,
                    title: branch.map_or_else(
                        || format!("branch {}", id.index()),
                        |b| {
                            b.title
                                .clone()
                                .unwrap_or_else(|| format!("branch {}", id.index()))
                        },
                    ),
                    parent: branch.and_then(|b| b.parent),
                    frames: self
                        .frames(id)
                        .len()
                        .max(usize::try_from(branch.map_or(0, |b| b.frames)).unwrap_or(0)),
                    verified: branch.is_some_and(|b| b.verified),
                    playable: branch.is_some_and(|b| b.playable(&self.verification)),
                    children: self.project.children_of(id),
                    fork_frame: branch.and_then(crate::project::Branch::fork_frame),
                }
            })
            .collect()
    }

    /// Whether "Play from this frame" is available, with the tooltip if it is not.
    ///
    /// # Errors
    ///
    /// Returns the reason the button would do nothing, which doubles as the tooltip.
    pub fn play_check(&self) -> Result<(), PlayBlock> {
        if !self.verification.matches {
            return Err(PlayBlock::InputsUnverified(
                "the checkpoint, map pool or content corpus on disk does not hash to what the                  project recorded at import"
                    .to_owned(),
            ));
        }
        // A finished or failed rebuild is history the reader may want to re-read; only one in
        // flight is a reason to refuse.
        if matches!(self.rebuild, RebuildStatus::Running { .. }) {
            return Err(PlayBlock::Rebuilding);
        }
        // Only a branch that could still move gets in the way. `Ready` - attached and never
        // advanced - must not block Play, or the app would refuse the first thing a reader does
        // after importing a session and starting a run.
        if let Some(state) = self.live_state()
            && matches!(state, LiveState::Running | LiveState::WaitingForHuman)
        {
            return Err(PlayBlock::BranchRunning(state));
        }
        let branch = self
            .project
            .branch(self.current)
            .ok_or(PlayBlock::BranchUnverified)?;
        if !branch.playable(&self.verification) {
            return Err(PlayBlock::BranchUnverified);
        }
        let viewed = self.viewed_current().ok_or(PlayBlock::NoFrame)?;
        if self.frames(self.current).is_empty() {
            return Err(PlayBlock::NoFrame);
        }
        if self
            .frames(self.current)
            .get(viewed)
            .is_some_and(|frame| frame.finished)
        {
            return Err(PlayBlock::TerminalFrame);
        }
        if u64::try_from(self.project.branches.len())
            .unwrap_or(u64::MAX)
            .saturating_add(1)
            > u64::from(crate::MAX_BRANCHES_PER_PROJECT)
        {
            return Err(PlayBlock::TooManyBranches {
                limit: crate::MAX_BRANCHES_PER_PROJECT,
            });
        }
        let projected = self
            .project
            .total_frames()
            .saturating_sub(u64::try_from(viewed).unwrap_or(u64::MAX));
        if projected > crate::project::MAX_TOTAL_FRAMES {
            return Err(PlayBlock::FrameBudget {
                total: self.project.total_frames(),
                limit: crate::project::MAX_TOTAL_FRAMES,
            });
        }
        Ok(())
    }

    /// Fork at the viewed frame and describe the rebuild the caller must run.
    ///
    /// The parent keeps every frame and every answer it had: branching copies nothing and destroys
    /// nothing, which is what makes the original future survive.
    ///
    /// # Errors
    ///
    /// Every refusal is a [`PlayBlock`] carrying the sentence to show the reader.
    pub fn plan_play(
        &mut self,
        seats: Vec<crate::project::SeatSetting>,
    ) -> Result<PlayPlan, PlayBlock> {
        self.play_check()?;
        let parent = self.current;
        let frame = u64::try_from(self.viewed_current().ok_or(PlayBlock::NoFrame)?)
            .map_err(|_| PlayBlock::NoFrame)?;
        let title = format!("branch from frame {frame}");
        let child = self
            .project
            .fork(parent, frame, Some(title), seats)
            .map_err(|error| PlayBlock::Refused(error.to_string()))?;
        let script = self
            .project
            .replay_script(child)
            .map_err(|error| PlayBlock::Refused(error.to_string()))?;
        self.rebuild = RebuildStatus::Running {
            branch: child,
            frame,
        };
        self.notice = Some(format!(
            "Forked branch {child} from {parent} at frame {frame}; rebuilding"
        ));
        Ok(PlayPlan {
            child,
            parent,
            frame,
            script,
        })
    }

    /// A rebuild finished: the child becomes verified and its frames become viewable.
    pub fn finish_rebuild(&mut self, outcome: RebuildOutcome, branch: BranchId) {
        let mut frames = self.frames(branch).to_vec();
        frames.extend(outcome.frames);
        if let Some(entry) = self
            .project
            .branches
            .iter_mut()
            .find(|entry| entry.id == branch)
        {
            entry.frames = u64::try_from(frames.len()).unwrap_or(u64::MAX);
        }
        self.project.mark_verified(branch).ok();
        let len = frames.len();
        self.frames.insert(branch, frames);
        self.viewed.insert(branch, len.saturating_sub(1));
        self.current = branch;
        self.rebuild = RebuildStatus::Done {
            branch,
            replayed: outcome.replayed,
        };
        self.notice = Some(format!(
            "Branch {branch} is live: {} frames, {} prefix decisions forced",
            len, outcome.replayed
        ));
    }

    /// A rebuild refused: drop the child, because a branch that cannot be reproduced must not sit in
    /// the tree looking playable.
    pub fn fail_rebuild(&mut self, branch: BranchId, why: &str) {
        self.discard_fork(branch);
        self.frames.remove(&branch);
        self.viewed.remove(&branch);
        self.rebuild = RebuildStatus::Failed(why.to_owned());
        self.notice = Some(format!("Rebuild of branch {branch} refused: {why}"));
    }

    /// Cancel a running rebuild. The child branch is dropped for the same reason as a refusal.
    pub fn cancel_rebuild(&mut self) {
        if let RebuildStatus::Running { branch, .. } = self.rebuild {
            self.discard_fork(branch);
            self.frames.remove(&branch);
            self.viewed.remove(&branch);
            self.rebuild = RebuildStatus::Idle;
            self.notice = Some(format!("Cancelled the rebuild of branch {branch}"));
        }
    }

    /// Remove a branch the app itself created and never started using.
    ///
    /// This is not pruning in the sense the plan defers - a user-facing operation on branches with
    /// history. It removes a fork created moments ago whose rebuild was refused or cancelled, which
    /// must not stay in the tree looking playable. Ids are never reused, and a branch with children
    /// is left alone, so the tree stays valid.
    fn discard_fork(&mut self, branch: BranchId) {
        if self.project.children_of(branch).is_empty() && branch != BranchId::SOURCE {
            self.project.branches.retain(|entry| entry.id != branch);
        }
    }

    /// Close the app. Cancelling an in-flight rebuild is part of closing, so that the caller never
    /// has to know whether one is running.
    pub fn close(&mut self) -> RebuildStatus {
        self.detach();
        let was = self.rebuild.clone();
        self.cancel_rebuild();
        was
    }

    /// Record frames arriving from the live branch.
    pub fn record_frames(&mut self, frames: &[FrameTick]) {
        if frames.is_empty() {
            return;
        }
        // Whether the reader was watching the newest frame has to be answered *before* the new ones
        // land, or the answer is always "no": `at_tip` compares the viewed index against the last
        // index, and appending moves the last index every time. That is what made the board stop
        // following a running game - the view stayed on whatever frame the reader was on when the
        // first batch arrived, which for a table started here is frame zero.
        let was_at_tip = self.frames(self.current).is_empty()
            || self.at_tip()
            || self.viewed_current().is_none();
        let known = self.frames(self.current).to_vec();
        let mut merged = known;
        for frame in frames {
            if merged.last().is_some_and(|last| last.index >= frame.index) {
                continue;
            }
            merged.push(frame.clone());
        }
        let len = merged.len();
        self.frames.insert(self.current, merged);
        if was_at_tip {
            self.viewed.insert(self.current, len - 1);
        }
        // The project counts frames too, and `fork` refuses a frame past the end of the parent. A
        // live table starts at zero frames recorded, so without this every Play from it is refused
        // with "branch-0 ends at frame 0".
        self.note_frames(len);
    }

    /// Tell the project how long the current branch is now, and say so if it refuses.
    fn note_frames(&mut self, len: usize) {
        let frames = u64::try_from(len).unwrap_or(u64::MAX);
        let current = self.current;
        if let Err(error) = self.project.note_frames(current, frames) {
            self.notice = Some(format!("Branch {current} could not grow: {error}"));
        }
    }
}
