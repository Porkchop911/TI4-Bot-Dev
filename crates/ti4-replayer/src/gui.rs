//! The replayer window: the reviewer's board, plus a hand on the table.
//!
//! The layout is the reviewer's, on purpose. The top bar is files and status; the bottom bars are
//! seats, running, and the frame strip; the left sheet is the branch list, then the players; the right
//! sheet is the choice that is waiting, with the step detail under it; the middle is the map. Only four
//! things here do not exist in R01, and they are the reason a second application exists: six seat
//! chips, the panel that appears when a manual seat is asked something, the branch strip, and Play.
//!
//! The window decides nothing by itself. Every question a click raises - may I fork here, what does this
//! button do to a parked decision, is this branch replayable, is a rebuild in flight - goes to
//! [`ReplayApp`], and the answer comes back as a decision or as the sentence to show. What the window
//! owns is what it draws: the frames each branch has sent, the tile under the pointer, and the panel
//! widths.
//!
//! It also cannot hold a game. A `LiveReview` is not `Send`, so every branch runs on its own thread and
//! sends its frames through the gate's feed; [`Replayer::poll`] drains that feed once per repaint and
//! appends what arrived. Two borrowing tricks make the panels compile without copying: the [`Opened`]
//! record is taken out of `self` for the duration of a paint, and the frame store inside it is taken
//! out in turn, so frames can be borrowed by name while the app beside them is mutated.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Color32, Sense};
use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_review::panels;
use ti4_review::view::{
    self, BoardLayout, PANEL_FILL, PANEL_TEXT, board_view, draw_board, section,
};
use ti4_review::{ProfileTable, ReviewFrame, ReviewSession, SimulationConfig, load_session};

use crate::app::{
    PlayBlock, RebuildOutcome, RebuildStatus, ReplayApp, ReplaySettings, SETTINGS_PATH,
    SetupDefaults,
};
use crate::fingerprint::FrameFingerprint;
use crate::live::{AdvanceGoal, Gate, LiveBranch, LiveEvent, LiveState, ReplayRequest};
use crate::net::{DEFAULT_PORT, NetHost};
use crate::project::{ReplayInputs, SeatSetting};
use crate::rebuild::{RebuildBounds, RebuildTarget};
use crate::store::{self, Store};
use crate::{
    BranchId, ManualSubmission, ReplayerProject, SeatControl, SeatMode, SubmitOutcome,
    load_project, save_project,
};

/// How many branches keep their frames in the window at once.
///
/// A branch's frames are its whole game - the example sessions run near 200 KB a frame - so "remember
/// every branch I ever clicked" is not a policy that survives an afternoon of exploring. Three is
/// enough to compare a fork with its parent and the sibling you came from, and what was released is
/// counted so the branch sheet can say so out loud instead of drawing an empty board.
const BRANCHES_KEPT: usize = 3;

/// A project the window has open, with the branch thread that belongs to it.
struct Opened {
    app: ReplayApp<Arc<Gate>>,
    /// The thread running the current branch, if any. Switching branches stops it: one window, one
    /// history in motion.
    branch: Option<LiveBranch>,
    /// Frames per branch, as those branches sent them.
    store: Store,
    /// The file this project came from, or should go to.
    path: Option<PathBuf>,
    /// A fork waiting on its rebuild. Its frames are buffered here rather than folded into the view,
    /// because until the rebuild proves the position the child is not a branch anybody may look at.
    rebuilding: Option<Rebuilding>,
}

struct Rebuilding {
    branch: BranchId,
    started: Instant,
    frames: usize,
}

/// The window's toggles are four genuinely independent switches - three panels and the setup form -
/// and each one mirrors a field of the same name in the settings file, so they stay four.
#[allow(clippy::struct_excessive_bools)]
pub struct Replayer {
    opened: Option<Opened>,
    status: String,
    selected_tile: Option<String>,
    show_players: bool,
    show_decision: bool,
    show_branches: bool,
    /// The Power window: every seat's immediate defence and one-activation projection.
    show_power: bool,
    /// The last power report, keyed by branch and frame so it is computed once per frame shown.
    power: Option<PowerCached>,
    /// What the map shows on top of the board: nothing, one seat's power view, or every seat's.
    overlay: ti4_review::power::Overlay,
    /// A save asked for during the paint, performed once the paint has put the project back.
    pending_save: Option<PendingSave>,
    /// The table the operator can start instead of opening something: the same seven knobs the
    /// reviewer offers, in the reviewer's own words, because "I want to play a game" should not
    /// require knowing what a session file is first.
    setup: SetupForm,
    setup_open: bool,
    /// The window's current size, kept so that closing at 2560x1400 opens tomorrow at 2560x1400.
    window: [f32; 2],
    /// The live table served to remote seats, while hosting.
    host: Option<NetHost>,
    /// The port typed into the bar, kept as text so a half-typed number is not a parse error.
    host_port: String,
    /// A join code fixed on the command line, instead of a fresh random one.
    host_code: Option<String>,
    /// Start hosting by itself as soon as a table is live (`--host` on the command line).
    auto_host: bool,
}

/// Which save button was pressed.
#[derive(Clone, Copy, Debug)]
enum PendingSave {
    Known,
    As,
}

/// A power report and the map it was computed on, for one branch's frame.
struct PowerCached {
    branch: String,
    frame: usize,
    galaxy: Option<ti4_content::galaxy::Galaxy>,
    report: Option<ti4_review::power::PowerReport>,
}

/// Hosting asked for on the command line: `ti4-replayer --host <port> [--code <code>]`.
#[derive(Clone, Debug)]
pub struct HostPlan {
    pub port: u16,
    pub code: Option<String>,
}

/// The profile table behind a remembered name, or the fallback when the name is not one this build
/// knows - a settings file from a newer build is a reason to be boring, not to fail.
fn profile_table_named(name: &str, fallback: ProfileTable) -> ProfileTable {
    match name.trim().to_ascii_lowercase().as_str() {
        "learner" => ProfileTable::Learner,
        "accepted" => ProfileTable::Accepted,
        _ => fallback,
    }
}

/// The name a profile table is stored under in the settings file.
fn profile_table_text(table: ProfileTable) -> &'static str {
    match table {
        ProfileTable::Learner => "Learner",
        ProfileTable::Accepted => "Accepted",
    }
}

/// A table to start: checkpoint, map pool, seed, faction rotation, profile table, temperature,
/// structured diplomacy - plus whether to take a seat from the first decision or watch the learned
/// policy until one is wanted.
struct SetupForm {
    checkpoint: String,
    map_pool: String,
    seed: String,
    rotation: usize,
    table: ProfileTable,
    temperature: f64,
    diplomacy: bool,
    take_a_seat: bool,
    /// The six factions the table seats, in the order the rotation turns them round the seats.
    lineup: Vec<String>,
}

/// The standard six, the lineup every table used before any other could be picked.
fn standard_lineup() -> Vec<String> {
    ti4_review::FACTIONS.map(str::to_owned).to_vec()
}

impl Default for SetupForm {
    fn default() -> Self {
        let root = Replayer::base();
        let existing = |relative: &str| {
            let path = root.join(relative);
            path.is_file().then(|| path.display().to_string())
        };
        Self {
            checkpoint: existing("examples/reviewer/checkpoint-473312/slots.json")
                .unwrap_or_default(),
            map_pool: existing("examples/reviewer/full_np8_12_holdout.json").unwrap_or_default(),
            seed: "4242".to_owned(),
            rotation: 0,
            table: ProfileTable::Learner,
            temperature: 0.5,
            diplomacy: false,
            take_a_seat: true,
            lineup: standard_lineup(),
        }
    }
}

impl SetupForm {
    /// The form as the last table left it, field by field, falling back to the shipped example inputs
    /// where nothing has been remembered.
    ///
    /// A remembered path that no longer exists is still shown: the point of remembering is to save the
    /// operator from retyping a long checkpoint path, and a moved file is easier to fix when the field
    /// shows what it used to be. Only a blank memory is skipped, because a blank is not a mistake, it
    /// is nothing remembered.
    fn remembered(settings: &ReplaySettings) -> Self {
        let kept = |remembered: &str, fallback: String| {
            if remembered.trim().is_empty() {
                fallback
            } else {
                remembered.to_owned()
            }
        };
        let mut form = Self::default();
        let remembered = &settings.setup;
        form.checkpoint = kept(&remembered.checkpoint, form.checkpoint);
        form.map_pool = kept(&remembered.map_pool, form.map_pool);
        form.seed = kept(&remembered.seed, form.seed);
        form.rotation = remembered.rotation;
        form.table = profile_table_named(&remembered.profile_table, form.table);
        if remembered.temperature.is_finite() && remembered.temperature > 0.0 {
            form.temperature = remembered.temperature;
        }
        form.diplomacy = remembered.diplomacy;
        // A remembered lineup is kept only if it can still be seated; otherwise the standard six.
        if ti4_review::check_lineup(ContentStore::embedded(), &remembered.lineup).is_ok() {
            form.lineup.clone_from(&remembered.lineup);
        }
        form
    }

    /// The reviewer's configuration for this form, or the plain reason it is not a table yet.
    fn simulation(&self) -> Result<SimulationConfig, String> {
        if self.checkpoint.trim().is_empty() || !Path::new(self.checkpoint.trim()).exists() {
            return Err("Choose the checkpoint the table should play from.".to_owned());
        }
        if self.map_pool.trim().is_empty() || !Path::new(self.map_pool.trim()).exists() {
            return Err("Choose the map pool the table should draw from.".to_owned());
        }
        let seed = self
            .seed
            .trim()
            .parse::<u64>()
            .map_err(|_| "The seed has to be a whole number.".to_owned())?;
        ti4_review::check_lineup(ContentStore::embedded(), &self.lineup)
            .map_err(|reason| format!("The lineup cannot be seated: {reason}."))?;
        let lineup = (self.lineup != standard_lineup()).then(|| self.lineup.clone());
        Ok(SimulationConfig {
            checkpoint: PathBuf::from(self.checkpoint.trim()),
            map_pool: PathBuf::from(self.map_pool.trim()),
            seed,
            rotation: self.rotation,
            table: self.table,
            temperature: self.temperature,
            diplomacy: self.diplomacy,
            lineup,
        })
    }
}

/// Run the replayer window.
///
/// # Errors
/// Whatever eframe reports when the native window or its GL context cannot be created.
pub fn run() -> eframe::Result<()> {
    run_with(None)
}

/// Open the window with something already in it.
///
/// `ti4-replayer <file>` is how the operator gets from a recording on disk to a table without three
/// dialog clicks, and it is how this window gets smoke-tested against a real session on a machine that
/// nobody is sitting at. A session is imported into a new project; a `.r02.json` is opened as one.
///
/// # Errors
/// Propagates eframe's own failure to open a window. The file itself is not read until the app is
/// constructed, and a file that will not open is reported in the window's status line, not here.
pub fn run_with(open: Option<PathBuf>) -> eframe::Result<()> {
    run_hosting(open, None)
}

/// [`run_with`], and host the first live table on the planned port without a click.
///
/// # Errors
/// Propagates eframe's own failure to open a window.
pub fn run_hosting(open: Option<PathBuf>, plan: Option<HostPlan>) -> eframe::Result<()> {
    let settings = ReplaySettings::load(Path::new(SETTINGS_PATH));
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TI4 game replayer")
            .with_inner_size([settings.window_width, settings.window_height]),
        ..Default::default()
    };
    eframe::run_native(
        "TI4 game replayer",
        options,
        Box::new(move |context| {
            let mut app = Replayer::new(context, &settings);
            if let Some(plan) = &plan {
                app.host_port = plan.port.to_string();
                app.host_code.clone_from(&plan.code);
                app.auto_host = true;
            }
            if let Some(path) = &open {
                app.open_path(path);
            }
            Ok(Box::new(app))
        }),
    )
}

impl Replayer {
    #[must_use]
    pub fn new(context: &eframe::CreationContext<'_>, settings: &ReplaySettings) -> Self {
        context.egui_ctx.set_visuals(egui::Visuals::dark());
        let status = settings.last_project.as_deref().map_or_else(
            || {
                "Nothing open. Start a table below and take a seat, or open a game that somebody \
                 already recorded."
                    .to_owned()
            },
            |path| {
                format!(
                    "Last time was {path}. Open it from the bar above, start a new table, or open a \
                     recorded game."
                )
            },
        );
        Self {
            opened: None,
            status,
            selected_tile: None,
            show_players: settings.players_open,
            show_decision: settings.decisions_open,
            show_branches: settings.branches_open,
            show_power: false,
            power: None,
            overlay: ti4_review::power::Overlay::Off,
            pending_save: None,
            setup: SetupForm::remembered(settings),
            window: [settings.window_width, settings.window_height],
            // With nothing to look at, say what starting a table means instead of leaving a blank
            // window and a bar of buttons named after file formats.
            setup_open: settings.last_project.is_none(),
            host: None,
            host_port: DEFAULT_PORT.to_string(),
            host_code: None,
            auto_host: false,
        }
    }

    /// The directory project files resolve their relative input paths against.
    fn base() -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    // -------------------------------------------------------------------- files

    /// Open either kind of file this window understands, chosen by what the name says it is.
    fn open_path(&mut self, path: &Path) {
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        if name.contains(".r02.json") {
            self.open_project(path);
        } else {
            self.open_session(path);
        }
    }

    fn open_session_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TI4 review session", &["json", "zst"])
            .pick_file()
        {
            self.open_session(&path);
        }
    }

    /// Load an R01 session and adopt it as a new project.
    ///
    /// The load is synchronous and a session is hundreds of megabytes, so the window stalls here for a
    /// few seconds. That is the same trade R01 makes when you open a review, and it buys the honest
    /// version of every number on screen: nothing is drawn from a partial read.
    fn open_session(&mut self, path: &Path) {
        self.status = format!("Loading {}…", path.display());
        let session = match load_session(path) {
            Ok(session) => session,
            Err(error) => {
                self.status = format!("Could not load the session: {error}");
                return;
            }
        };
        let base = Self::base();
        let inputs = ReplayInputs::from_manifest(&session.manifest);
        let project = match ReplayerProject::import_with(path, &session, &inputs, &base, None, None)
        {
            Ok(project) => project,
            Err(error) => {
                self.status = format!("This session cannot become a project: {error}");
                return;
            }
        };
        let verification = match project.verify_inputs(&base) {
            Ok(verification) => verification,
            Err(error) => {
                self.status = format!(
                    "The checkpoint or map pool this session was played with is not where it recorded them: {error}"
                );
                return;
            }
        };
        let frames = u64::try_from(session.frames.len()).unwrap_or(u64::MAX);
        let mut store = Store::new(BRANCHES_KEPT);
        store.import(BranchId::SOURCE, session);
        let mut app = ReplayApp::new(project, verification, path);
        // The app counts frames; the store holds them. Both have to be told, or the timeline shows a
        // position in a game the reducer has never heard of and Play answers `NoFrame`.
        app.record_frames(&store.ticks(BranchId::SOURCE));
        self.opened = Some(Opened {
            app,
            branch: None,
            store,
            path: None,
            rebuilding: None,
        });
        self.selected_tile = None;
        self.status = format!(
            "Imported {frames} frames from {}. Toggle a seat, or press Play to fork from the frame you are looking at - the prefix is rebuilt and proved before anything is playable.",
            path.display()
        );
    }

    fn open_project_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TI4 replayer project", &["json", "zst"])
            .pick_file()
        {
            self.open_project(&path);
        }
    }

    /// Open a project file, and the R01 recording it grew out of.
    ///
    /// A project is a recipe: it stores inputs, answers and frame counts, not frames. The recording is
    /// what makes branch-0 drawable, so the window reopens it and says so in the status line when it is
    /// gone rather than pretending an empty board is a game nobody played.
    fn open_project(&mut self, path: &Path) {
        let project = match load_project(path) {
            Ok(project) => project,
            Err(error) => {
                self.status = format!("Could not read the project: {error}");
                return;
            }
        };
        let base = Self::base();
        let verification = match project.verify_inputs(&base) {
            Ok(verification) => verification,
            Err(error) => {
                self.status = format!("This project's inputs have moved or changed: {error}");
                return;
            }
        };
        let source = PathBuf::from(&project.source.session);
        let mut store = Store::new(BRANCHES_KEPT);
        let mut note = String::new();
        if source.is_file() {
            match load_session(&source) {
                Ok(session) => store.import(BranchId::SOURCE, session),
                Err(error) => note = format!(" The source session would not reopen: {error}"),
            }
        } else {
            " The source session is not where this file says, so only branches this window plays will be drawable."
                .clone_into(&mut note);
        }
        let mut app = ReplayApp::new(project, verification, path);
        app.record_frames(&store.ticks(BranchId::SOURCE));
        self.opened = Some(Opened {
            app,
            branch: None,
            store,
            path: Some(path.to_path_buf()),
            rebuilding: None,
        });
        self.selected_tile = None;
        self.status = format!("Opened project {}.{note}", path.display());
    }

    fn save_project(&mut self, path: PathBuf) {
        let mut opened = self.opened.take();
        let result = match opened.as_mut() {
            None => Err("Nothing is open to save.".to_owned()),
            Some(opened) => fold_branch_into_project(opened)
                .and_then(|()| {
                    save_project(&path, opened.app.project()).map_err(|error| error.to_string())
                })
                .map(|()| path),
        };
        self.opened = opened;
        self.status = match result {
            Ok(path) => format!("Saved {}.", path.display()),
            Err(error) => format!("The project was not saved: {error}"),
        };
    }

    fn save_as_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("TI4 replayer project", &["json", "zst"])
            .set_file_name("replay.r02.json.zst")
            .save_file()
        {
            self.save_project(path);
        }
    }

    /// Save to the file this project came from, or to a name derived from its recording.
    fn save_known(&mut self) {
        let path = self
            .opened
            .as_ref()
            .and_then(|opened| opened.path.clone())
            .or_else(|| {
                self.opened.as_ref().and_then(|opened| {
                    opened
                        .app
                        .project()
                        .source
                        .session
                        .rsplit(['/', '\\'])
                        .next()
                        // A table started here has no recording to name it after: ask instead.
                        .filter(|name| !name.trim().is_empty())
                        .map(|name| {
                            let stem = name
                                .trim_end_matches(".zst")
                                .trim_end_matches(".ti4review.json")
                                .trim_end_matches(".json");
                            PathBuf::from("out/replays").join(format!("{stem}.r02.json.zst"))
                        })
                })
            });
        match path {
            Some(path) => {
                if let Some(parent) = path.parent()
                    && !parent.as_os_str().is_empty()
                {
                    let _ = std::fs::create_dir_all(parent);
                }
                self.save_project(path);
            }
            None => self.save_as_dialog(),
        }
    }

    // -------------------------------------------------------------------- logic

    /// Drain whatever the branch thread has said since the last repaint.
    fn poll(&mut self, context: &egui::Context) {
        let mut opened = self.opened.take();
        let mut animate = false;
        if let Some(opened) = opened.as_mut() {
            let events = opened
                .branch
                .as_ref()
                .map(|branch| (branch.drain_events(), Arc::clone(branch.gate())));
            if let Some((events, gate)) = events {
                for event in events {
                    self.consider_event(opened, &gate, event);
                }
                let feed = gate.take_feed();
                let frames = !feed.frames.is_empty() || feed.header.is_some();
                let missing = feed.missing;
                if frames || missing > 0 {
                    let target = opened
                        .rebuilding
                        .as_ref()
                        .map_or_else(|| opened.app.current(), |rebuilding| rebuilding.branch);
                    let appended = opened.store.apply(target, feed);
                    if appended > 0 {
                        if let Some(rebuilding) = opened.rebuilding.as_mut() {
                            rebuilding.frames += appended;
                        } else {
                            let ticks: Vec<_> = opened
                                .store
                                .frames(target)
                                .iter()
                                .rev()
                                .take(appended)
                                .rev()
                                .map(store::tick)
                                .collect();
                            opened.app.record_frames(&ticks);
                        }
                    }
                }
                if let Some(host) = &self.host
                    && opened.rebuilding.is_none()
                {
                    let branch = opened.app.current();
                    host.sync(
                        &gate,
                        opened.store.session(branch),
                        opened.store.frames(branch),
                    );
                }
                animate = matches!(
                    opened.app.live_state(),
                    Some(LiveState::Running | LiveState::WaitingForHuman)
                ) || opened.rebuilding.is_some();
            }
        }
        self.opened = opened;
        if animate {
            context.request_repaint();
        }
    }

    /// One event from the branch thread. Only two of them change what the window holds.
    fn consider_event(&mut self, opened: &mut Opened, gate: &Arc<Gate>, event: LiveEvent) {
        match event {
            LiveEvent::Rebuilt {
                replayed,
                steps,
                frames,
            } => {
                let Some(rebuilding) = opened.rebuilding.take() else {
                    return;
                };
                let branch = rebuilding.branch;
                let ticks: Vec<_> = opened
                    .store
                    .frames(branch)
                    .iter()
                    .map(store::tick)
                    .collect();
                let seconds = rebuilding.started.elapsed().as_secs_f32();
                opened.app.finish_rebuild(
                    RebuildOutcome {
                        frames: ticks,
                        replayed,
                    },
                    branch,
                );
                let seen = opened.app.frames(branch).to_vec();
                opened.app.attach(Arc::clone(gate), seen);
                self.status = format!(
                    "Branch {branch} is live at frame {frames}: {replayed} recorded decisions forced through the policy in {steps} steps, in {seconds:.1} s. Flip a seat to Manual and run on."
                );
            }
            LiveEvent::Failed(why) => {
                if let Some(rebuilding) = opened.rebuilding.take() {
                    opened.app.fail_rebuild(rebuilding.branch, &why);
                }
                opened.branch = None;
                self.status = format!("The branch stopped: {why}");
            }
            _ => {}
        }
    }

    // --------------------------------------------------------------------- play

    /// Fork at the viewed frame and start the thread that proves the prefix and then plays.
    ///
    /// The window cannot run the rebuild itself: the review it produces lives on the branch thread and
    /// never leaves it. So this allocates the fork through the app, builds the gate the fork will be
    /// driven by, and hands both to [`LiveBranch::replay`].
    fn play(&mut self, opened: &mut Opened) {
        // Everything the running branch has answered has to be in the project *before* the fork is
        // planned, because the plan's script is built from the project. Until this line a table
        // started here carried no answers at all until it was saved, so a fork from it replayed a
        // prefix of nothing: with the seats on Auto that happened to reproduce, and with a seat on
        // Manual the rebuild parked and asked the operator to answer the game a second time.
        if let Err(error) = fold_branch_into_project(opened) {
            self.status = format!(
                "This branch's answers could not be recorded, so it cannot be forked: {error}"
            );
            return;
        }
        let plan = match opened.app.plan_play(seat_settings(&opened.app.seats())) {
            Ok(plan) => plan,
            Err(block) => {
                self.status = block.tooltip();
                return;
            }
        };
        let parent = plan.parent;
        let needed = usize::try_from(plan.frame.saturating_add(1)).unwrap_or(usize::MAX);
        let held = opened.store.len(parent);
        if held < needed {
            let why = format!(
                "branch {parent} holds {held} of the {needed} frames the prefix needs in this window"
            );
            opened.app.fail_rebuild(plan.child, &why);
            "That branch's frames were released to keep memory bounded. Reopen the project to fork from it again."
                .clone_into(&mut self.status);
            return;
        }
        let fingerprints: Vec<FrameFingerprint> = opened
            .store
            .frames(parent)
            .iter()
            .take(needed)
            .map(FrameFingerprint::of)
            .collect();
        let Some(config) = opened.app.project().inputs.simulation_config(&Self::base()) else {
            opened.app.fail_rebuild(
                plan.child,
                "the project names a profile table this build does not have",
            );
            "This project names a profile table this build does not have."
                .clone_into(&mut self.status);
            return;
        };
        let gate = Arc::new(Gate::replaying_interactive(
            seat_control(&opened.app.seats()),
            plan.script,
        ));
        gate.attach_feed();
        let request = ReplayRequest {
            config,
            fingerprints,
            target: RebuildTarget::Frame(plan.frame),
            bounds: RebuildBounds::default(),
            record: true,
        };
        // The branch that was running, if any, goes with the fork: the window plays one history at a
        // time, and a stopped branch cannot be confused with the one now being proved.
        opened.branch = None;
        match LiveBranch::replay(Arc::clone(&gate), request) {
            Ok(branch) => {
                opened.rebuilding = Some(Rebuilding {
                    branch: plan.child,
                    started: Instant::now(),
                    frames: 0,
                });
                // The fork's gate becomes the one the chips talk to straight away, though its frames
                // are not viewable until the rebuild has proved them. Otherwise a seat taken while
                // the prefix is replaying is set on the branch that was just left behind, and the
                // fork silently starts with the modes the button press captured.
                opened.app.attach_handle(Arc::clone(&gate));
                opened.branch = Some(branch);
                self.status = format!(
                    "Forked branch {} from {} at frame {}. Rebuilding and checking the prefix…",
                    plan.child, parent, plan.frame
                );
            }
            Err(error) => {
                opened.app.fail_rebuild(plan.child, &error.to_string());
                self.status = format!("The fork could not start: {error}");
            }
        }
    }

    fn cancel_rebuild(&mut self, opened: &mut Opened) {
        // Stop the thread before the app forgets the branch, or a rebuild still walking would keep
        // sending frames for a child that no longer exists.
        opened.branch = None;
        opened.rebuilding = None;
        opened.app.cancel_rebuild();
        "Rebuild cancelled. The fork was dropped and nothing else changed; the parent still has every frame it had."
            .clone_into(&mut self.status);
    }

    fn advance(&mut self, opened: &mut Opened, goal: AdvanceGoal) {
        self.status = match opened.app.advance(goal) {
            Ok(()) => match goal {
                AdvanceGoal::Steps(1) => "One step.".to_owned(),
                other => format!("Advancing {other:?}."),
            },
            Err(error) => format!("Could not advance: {error}"),
        };
    }

    fn submit(&mut self, opened: &mut Opened, submission: &ManualSubmission) {
        let outcome = opened.app.submit(submission);
        self.status = describe_submission(&outcome);
    }

    fn delegate(&mut self, opened: &mut Opened, offer: crate::OfferId) {
        self.status = match opened.app.delegate_pending(offer) {
            Ok(actor) => format!("{actor} answers this one from the policy."),
            Err(error) => format!("Nothing to delegate: {error}"),
        };
    }

    // ------------------------------------------------------------------- panels

    /// Start a table now, with this build, and put the window in front of it.
    ///
    /// This is the reviewer's "Load starting table" with one addition: the seats are controllable from
    /// the first decision, which is the whole reason the replayer exists. Nothing is rebuilt, because
    /// nothing was recorded before - the table is the first thing, and the frame feed carries it out.
    fn start_table(&mut self) {
        // The setup is checked before anything open is touched, so a bad knob never costs the table
        // on screen.
        let config = match self.setup.simulation() {
            Ok(config) => config,
            Err(why) => {
                self.status = why;
                return;
            }
        };
        // One table per window: the one open is closed to make room. It used to refuse instead,
        // telling the reader to press Stop first - but Stop ends the thread without detaching it, so
        // after the first table every later one was refused (UI-08). A table with a file is saved
        // first; one without has nowhere to go, and the status line says it was closed unsaved.
        let Some(previous) = self.close_for_new_table() else {
            return;
        };
        let inputs = ReplayInputs::of(&config);
        let base = Self::base();
        let mut seats = SeatControl::all_auto();
        if self.setup.take_a_seat {
            // The first seat, because it is the one the operator will look for; every other seat stays
            // with the learned policy until its chip is toggled.
            seats.set_mode(&PlayerId::new("seat0"), SeatMode::Manual);
        }
        let branch = match LiveBranch::start(config, seats) {
            Ok(branch) => branch,
            Err(error) => {
                self.status = format!("That table could not start: {error}");
                return;
            }
        };
        branch.gate().attach_feed();
        branch.gate().enable_recording();
        let Some((mut shell, frames)) = table_header(&branch) else {
            self.status = format!(
                "The table would not report its seating ({}). Nothing was started.",
                branch.gate().state().as_str()
            );
            return;
        };
        shell.frames = frames;
        let project = match ReplayerProject::live_table(&inputs, &base, &shell) {
            Ok(project) => project,
            Err(error) => {
                self.status = format!("That table could not be described to a project: {error}");
                return;
            }
        };
        let verification = match project.verify_inputs(&base) {
            Ok(verification) => verification,
            Err(error) => {
                self.status = format!("The checkpoint or map pool moved: {error}");
                return;
            }
        };
        let mut store = Store::new(BRANCHES_KEPT);
        store.import(BranchId::SOURCE, shell);
        let mut app = ReplayApp::new(project, verification, Path::new("this table"));
        app.record_frames(&store.ticks(BranchId::SOURCE));
        let ticks = app.frames(BranchId::SOURCE).to_vec();
        app.attach(Arc::clone(branch.gate()), ticks);
        let (seed, rotation, profile, temperature, diplomacy) = (
            app.project().inputs.seed,
            app.project().inputs.rotation,
            app.project().inputs.profile_table.clone(),
            app.project().inputs.temperature,
            app.project().inputs.diplomacy,
        );
        self.setup_open = false;
        // Remember what was played with before the first frame has even been drawn, so a window that
        // dies mid-table still opens tomorrow with the same checkpoint, pool, seed and profiles.
        self.remember_setup();
        self.opened = Some(Opened {
            app,
            branch: Some(branch),
            store,
            path: None,
            rebuilding: None,
        });
        self.status = format!(
            "Table live: seed {}, rotation {}, {} profiles, temperature {:.2}{}. {}Toggle a seat chip \
             to take that seat over; Run plays the learned policy for the others. Scrub to any frame \
             and Play forks from it. Saving writes the replayer file - inputs, branches and every \
             answer - which is what a fork needs to be reproducible.",
            seed,
            rotation,
            profile,
            temperature,
            if diplomacy { ", diplomacy" } else { "" },
            if self.setup.take_a_seat {
                "Seat 0 is yours from the first decision. "
            } else {
                ""
            }
        );
        self.status.insert_str(0, &previous);
    }

    /// Close whatever is open so a new table can take the window, saving it first when it has a
    /// file. Returns what happened to it, as a status-line prefix (empty when nothing was open), or
    /// `None` when its save failed: then it stays open and nothing new starts.
    fn close_for_new_table(&mut self) -> Option<String> {
        let Some(path) = self.opened.as_ref().map(|opened| opened.path.clone()) else {
            return Some(String::new());
        };
        let saved = match path {
            Some(path) => {
                self.save_project(path);
                if !self.status.starts_with("Saved ") {
                    self.status
                        .push_str(" The open table was kept; no new table was started.");
                    return None;
                }
                format!("Previous table: {} ", self.status)
            }
            None => String::from(
                "Previous table closed unsaved (it had no file; Save As before starting another to                  keep it). ",
            ),
        };
        if let Some(mut old) = self.opened.take() {
            let _ = old.app.close();
            old.branch = None;
            old.rebuilding = None;
        }
        Some(saved)
    }

    /// The seven setup knobs, in the reviewer's words. Returns whether Load was pressed.
    fn setup_controls(&mut self, ui: &mut egui::Ui) -> bool {
        let mut start = false;
        ui.horizontal_wrapped(|ui| {
            if ui.button("Choose checkpoint…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("JSON checkpoint", &["json"])
                    .pick_file()
            {
                self.setup.checkpoint = path.display().to_string();
            }
            ui.add(egui::TextEdit::singleline(&mut self.setup.checkpoint).desired_width(280.0));
            if ui.button("Choose map pool…").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Map pool", &["json", "gz"])
                    .pick_file()
            {
                self.setup.map_pool = path.display().to_string();
            }
            ui.add(egui::TextEdit::singleline(&mut self.setup.map_pool).desired_width(280.0));
        });
        ui.horizontal_wrapped(|ui| {
            ui.label("Seed");
            ui.add(egui::TextEdit::singleline(&mut self.setup.seed).desired_width(110.0));
            ui.label("Faction rotation");
            egui::ComboBox::from_id_salt("setup-rotation")
                .selected_text(self.setup.rotation.to_string())
                .show_ui(ui, |ui| {
                    for rotation in 0..6 {
                        ui.selectable_value(
                            &mut self.setup.rotation,
                            rotation,
                            rotation.to_string(),
                        );
                    }
                })
                .response
                .on_hover_text(
                    "The seed permutes faction order; rotation shifts that permutation across the \
                     physical seats. Same checkpoint, pool, seed and rotation as the reviewer, so a \
                     table here is a table there.",
                );
            egui::ComboBox::from_id_salt("setup-table")
                .selected_text(match self.setup.table {
                    ProfileTable::Learner => "Learner",
                    ProfileTable::Accepted => "Accepted champion",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.setup.table, ProfileTable::Learner, "Learner");
                    ui.selectable_value(
                        &mut self.setup.table,
                        ProfileTable::Accepted,
                        "Accepted champion",
                    );
                })
                .response
                .on_hover_text("The learned profiles the seats play with.");
            ui.label("Temperature");
            ui.add(
                egui::DragValue::new(&mut self.setup.temperature)
                    .range(0.01..=10.0)
                    .speed(0.05)
                    .max_decimals(2),
            )
            .on_hover_text(
                "Low prefers the highest-scored move; high explores. Takes effect when a table starts.",
            );
            ui.checkbox(&mut self.setup.diplomacy, "Structured diplomacy");
            // Worth saying out loud, because leaving it off is not a quieter game, it is a different
            // game: no proposals, no deals, no transactions with anybody at the table.
            if !self.setup.diplomacy {
                ui.weak("Off: nobody at this table can offer a deal or a transaction.");
            }
            ui.label("Lineup");
            let seatable = ti4_review::seatable_factions(ContentStore::embedded());
            for (index, faction) in self.setup.lineup.iter_mut().enumerate() {
                egui::ComboBox::from_id_salt(("setup-lineup", index))
                    .selected_text(faction.clone())
                    .width(92.0)
                    .show_ui(ui, |ui| {
                        for option in &seatable {
                            ui.selectable_value(faction, (*option).to_owned(), *option);
                        }
                    });
            }
            if ui
                .button("Standard six")
                .on_hover_text("The six factions the policy trained on.")
                .clicked()
            {
                self.setup.lineup = standard_lineup();
            }
            if ui
                .button("Draw from seed")
                .on_hover_text(
                    "Six different factions drawn from every seatable faction with this seed, the                      same draw a wide-roster trainer makes for it.",
                )
                .clicked()
            {
                match self.setup.seed.trim().parse::<u64>().map_err(|_| {
                    "The seed has to be a whole number.".to_owned()
                }) {
                    Ok(seed) => match ti4_review::drawn_lineup(ContentStore::embedded(), seed) {
                        Ok(lineup) => self.setup.lineup = lineup,
                        Err(reason) => self.status = reason,
                    },
                    Err(reason) => self.status = reason,
                }
            }
            if let Err(reason) = ti4_review::check_lineup(ContentStore::embedded(), &self.setup.lineup)
            {
                ui.colored_label(egui::Color32::from_rgb(230, 140, 110), reason);
            } else if self.setup.lineup != standard_lineup() {
                ui.weak(
                    "Factions outside the standard six play an untrained policy row unless the                      checkpoint was trained with them.",
                );
            }
            ui.checkbox(&mut self.setup.take_a_seat, "Take seat 0 now")
                .on_hover_text(
                    "Seat 0 waits for you at its first decision. Any other seat is taken by toggling \
                     its chip in the bar at the bottom.",
                );
            start = ui.button("Load starting table").clicked();
        });
        start
    }

    /// What this window is, said once, in the plain case: nothing open yet. Returns whether Load was
    /// pressed - the caller starts the table, because the table becomes the record this paint took out
    /// of `self`.
    fn welcome(&mut self, root: &mut egui::Ui) -> bool {
        let mut start = false;
        egui::CentralPanel::default().show(root, |ui| {
            ui.heading("Start a table, or open a game that was already played");
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    "Starting a table plays a game here, now, with the reviewer's checkpoint, map pool, \
                     seed, rotation, profiles and temperature. Take a seat and the engine stops to ask \
                     you when that seat is asked; leave one to the learned policy and it answers itself.\n\n\
                     A recorded game (the reviewer's file, .ti4review.json) is one game somebody \
                     already played, frame by frame. Opening one lets you look through it and fork from \
                     any frame - the prefix is rebuilt and checked first, which is why a fork takes a \
                     moment and why a recording from a different engine build cannot be forked.\n\n\
                     A replayer file (.r02.json) is the other thing: the inputs, the branches and every \
                     answer given, saved so the same table can be reproduced later. It is a recipe, not \
                     the recording, and it is what Saving writes here.",
                );
            });
            ui.separator();
            start = self.setup_controls(ui);
        });
        start
    }

    /// The same knobs, floating over a table that is already open, so starting a second one does not
    /// need the first to be closed and reopened from the bar.
    fn setup_window(&mut self, root: &mut egui::Ui) -> bool {
        let mut open = self.setup_open;
        let mut start = false;
        egui::Window::new("Start a table")
            .open(&mut open)
            .show(root, |ui| {
                start = self.setup_controls(ui);
            });
        self.setup_open = open;
        start
    }

    fn top_bar(&mut self, root: &mut egui::Ui, opened: Option<&Opened>) {
        egui::Panel::top("files").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Start a table…").clicked() {
                    self.setup_open = !self.setup_open;
                }
                if ui.button("Open recorded game…").clicked() {
                    self.open_session_dialog();
                }
                ui.label("·")
                    .on_hover_text("A recorded game is one game, frame by frame, as the reviewer wrote it.");
                if ui.button("Open replayer file…").clicked() {
                    self.open_project_dialog();
                }
                ui.label("·").on_hover_text(
                    "A replayer file holds the inputs, the branches and the answers you gave - the \
                     recipe a fork needs, not the frames themselves.",
                );
                // Recorded, not performed: the open project is taken out of `self` for the length
                // of the paint, so saving from here found "nothing open". It runs after the paint,
                // once the project is back, the same way starting a table does.
                if ui.button("Save project").clicked() {
                    self.pending_save = Some(PendingSave::Known);
                }
                if ui.button("Save As…").clicked() {
                    self.pending_save = Some(PendingSave::As);
                }
                ui.separator();
                let mut changed = false;
                changed |= ui.toggle_value(&mut self.show_branches, "⎇ Branches").changed();
                changed |= ui.toggle_value(&mut self.show_players, "◧ Players").changed();
                changed |= ui.toggle_value(&mut self.show_decision, "◨ Choices").changed();
                ui.toggle_value(&mut self.show_power, "⚔ Power")
                    .on_hover_text("Each seat's defence and one-activation projection, per system.");
                if changed {
                    self.persist();
                }
                ui.separator();
                self.host_controls(ui, opened.is_some_and(|opened| opened.branch.is_some()));
                ui.separator();
                if let Some(opened) = opened {
                    let inputs = &opened.app.project().inputs;
                    let verification = opened.app.verification();
                    ui.label(format!(
                        "seed {} · rotation {} · {} · temp {:.2}{} · checkpoint {}",
                        inputs.seed,
                        inputs.rotation,
                        inputs.profile_table,
                        inputs.temperature,
                        if inputs.diplomacy { " · diplomacy" } else { "" },
                        if verification.matches {
                            "hashed and matching"
                        } else {
                            "NOT MATCHING"
                        },
                    ));
                    if !verification.engine_matches {
                        ui.colored_label(
                            Color32::from_rgb(220, 170, 60),
                            "recorded on a different engine build",
                        )
                        .on_hover_text(format!(
                            "This session was recorded at engine commit {} and this build is {}. The hashes of the inputs are what a rebuild is checked against, so the frames are still reproducible or they are not; the commit is reported because a difference there has explained surprises before.",
                            verification.engine_commit,
                            "this one"
                        ));
                    }
                }
            });
            ui.label(&self.status);
        });
    }

    /// Say so when the waiting choice belongs to a seat somebody is playing online. The window must
    /// not offer its buttons for it: the choice is theirs, and two answers racing is a click lost.
    fn remote_waiting(
        &self,
        ui: &mut egui::Ui,
        session: &ReviewSession,
        pending: Option<&crate::PendingManualChoice>,
    ) -> bool {
        let (Some(pending), Some(host)) = (pending, self.host.as_ref()) else {
            return false;
        };
        let Some((seat, name, connected)) = host
            .remote_seats()
            .into_iter()
            .find(|(seat, _, _)| *seat == pending.actor)
        else {
            return false;
        };
        ui.heading(format!(
            "{} is asked",
            view::annotate(session, seat.as_str())
        ));
        ui.label(format!(
            "{name} is playing this seat online{}. Their answer comes over the network; flip the seat to Auto below to let the policy take over.",
            if connected { "" } else { " but is disconnected" }
        ));
        ui.separator();
        true
    }

    /// Start or stop serving the live table, and say who has joined.
    fn host_controls(&mut self, ui: &mut egui::Ui, live: bool) {
        if let Some(host) = &self.host {
            ui.label(format!("Hosting on port {} · code", host.address().port()));
            let code = host.code().to_owned();
            if ui
                .add(egui::Label::new(egui::RichText::new(&code).monospace()).sense(Sense::click()))
                .on_hover_text("Click to copy. Players join with: ti4-replayer join <your-address>:<port> --code <code>")
                .clicked()
            {
                ui.ctx().copy_text(code);
                "Join code copied.".clone_into(&mut self.status);
            }
            for (seat, name, connected) in host.remote_seats() {
                ui.colored_label(
                    view::player_color(&seat),
                    format!("● {seat} {name}{}", if connected { "" } else { " (away)" }),
                );
            }
            if ui.button("Stop hosting").clicked() {
                self.host = None;
                "Stopped hosting; remote seats were disconnected and stay on Manual."
                    .clone_into(&mut self.status);
            }
            return;
        }
        ui.label("port");
        ui.add(egui::TextEdit::singleline(&mut self.host_port).desired_width(52.0));
        let button = ui.add_enabled(live, egui::Button::new("Host online"));
        let _ = button
            .clone()
            .on_disabled_hover_text("Start a table first; hosting serves the live branch.");
        // `--host` hosts the first live table by itself, once; after a failure it is the button's job.
        if button.clicked() || (live && std::mem::take(&mut self.auto_host)) {
            match self.host_port.trim().parse::<u16>() {
                Ok(port) => {
                    match NetHost::start_with_code(
                        std::net::SocketAddr::from(([0, 0, 0, 0], port)),
                        self.host_code.clone(),
                    ) {
                        Ok(host) => {
                            self.status = format!(
                                "Hosting on port {}. Remote players each take a free Auto seat, which turns Manual for them; seats you have on Manual stay yours. Your own window still shows every hand.",
                                host.address().port()
                            );
                            self.host = Some(host);
                        }
                        Err(error) => {
                            self.status = format!("Could not host on port {port}: {error}");
                        }
                    }
                }
                Err(_) => self.status = format!("{} is not a port number.", self.host_port),
            }
        }
    }

    /// The six seats, the run buttons, and Play.
    fn control_bar(&mut self, opened: &mut Opened, root: &mut egui::Ui) {
        egui::Panel::bottom("seats").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                self.seat_chips(opened, ui);
                ui.separator();
                self.run_controls(opened, ui);
                ui.separator();
                self.play_control(opened, ui);
            });
        });
    }

    /// One chip per seat, each saying whether that seat is the one the engine is holding a question
    /// open for, and whether it has already passed this round. Both matter: a chip that only shows the
    /// mode is how "I took that seat and nothing happened" arrives as a bug report.
    fn seat_chips(&mut self, opened: &mut Opened, ui: &mut egui::Ui) {
        ui.strong("Seats");
        let asked = opened.app.pending().map(|pending| pending.actor);
        // A seat that has already passed this round will not be asked again until the next
        // one, which is the honest answer to "I took that seat and nothing happened".
        // The newest frame answers both questions the chips used to guess at: who has passed, and who
        // this seat actually is.
        let newest = opened.store.frames(opened.app.current()).last();
        let passed = newest.map_or_else(Vec::new, |frame| {
            frame
                .state
                .players
                .iter()
                .filter(|player| player.passed)
                .map(|player| player.id.clone())
                .collect::<Vec<_>>()
        });
        let content = ContentStore::embedded();
        for seat in visible_seats(opened) {
            let mode = opened.app.seats().mode(&seat);
            let waiting = asked.as_ref() == Some(&seat);
            let has_passed = passed.contains(&seat);
            let named = newest.map_or_else(
                || seat.as_str().to_owned(),
                |frame| view::seat_name(frame, &seat, content),
            );
            let label = format!(
                "{}{named} · {}{}",
                if waiting { "▶ " } else { "" },
                format!("{mode:?}").to_lowercase(),
                if has_passed { " · passed" } else { "" }
            );
            let button = egui::Button::new(egui::RichText::new(&label).strong()).fill(
                if mode == SeatMode::Manual {
                    view::player_color(&seat)
                } else {
                    Color32::from_gray(58)
                },
            );
            let advice = if opened.branch.is_none() {
                "A recording has no live branch, so nothing is waiting on this chip yet. Set it                          here and the fork you make inherits it: press Play, the prefix is rebuilt, and                          the seat is yours from the first decision after the frame you forked at."
                            .to_owned()
            } else if waiting {
                "This seat is the one the engine is holding a question open for. Answer it in                          the panel on the right, or hand this one decision to the policy."
                            .to_owned()
            } else if has_passed {
                format!(
                    "Click to hand {seat} to yourself or back to the learned policy. It has                              passed this round, so the next thing it is asked comes in the round after                              this one - not in the next few steps."
                )
            } else {
                "Click to hand this seat to yourself or back to the learned policy. A seat on                          Manual stops the engine mid-step and asks you, so the answer lands exactly                          where the game paused."
                            .to_owned()
            };
            if ui.add(button).on_hover_text(advice).clicked() {
                let mode = opened.app.toggle_seat(&seat);
                // The app's own notice, because it is the one that knows whether the branch
                // can still act on the change; a chip that always claims success is how "the
                // seat toggle does nothing" reads from the outside.
                self.status = opened.app.notice().map_or_else(
                            || format!("{seat} is on {mode:?}."),
                            |notice| {
                                format!(
                                    "{notice}. {}{}",
                                    match mode {
                                        SeatMode::Manual =>
                                            "It is asked at its next decision, in the middle of the engine's step.",
                                        SeatMode::Auto =>
                                            "The policy answers it again, from where the game left off.",
                                    },
                                    if has_passed && mode == SeatMode::Manual {
                                        " It has passed this round, so that is the round after this one."
                                    } else {
                                        ""
                                    }
                                )
                            },
                        );
            }
        }
    }

    /// Step the game, run it to a round or to the end, pause it, stop it. Every one of these is a
    /// request to the branch, never a mutation of the game from this thread.
    fn run_controls(&mut self, opened: &mut Opened, ui: &mut egui::Ui) {
        let attached = opened.branch.is_some();

        let rebuilding = opened.rebuilding.is_some();
        let running = matches!(
            opened.app.live_state(),
            Some(LiveState::Running | LiveState::WaitingForHuman)
        );
        for (label, goal) in [
            ("Step", AdvanceGoal::Steps(1)),
            ("10 steps", AdvanceGoal::Steps(10)),
            ("To next round", AdvanceGoal::NextRound),
            ("End of game", AdvanceGoal::EndOfGame),
        ] {
            let response = ui.add_enabled(attached && !rebuilding, egui::Button::new(label));
            if response.clicked() {
                self.advance(opened, goal);
            }
            let _ = response.on_hover_text(if !attached {
                        "Nothing is running. Press Play to fork from the frame you are looking at: the prefix is rebuilt and proved, and then you are at the table."
                    } else if rebuilding {
                        "A rebuild is in flight. It will take this command as soon as it has proved the position."
                    } else {
                        "Ask the branch to advance. A seat on Manual stops it mid-step and asks you."
                    });
        }
        let pause = ui.add_enabled(running, egui::Button::new("Pause"));
        if pause.clicked() {
            opened.app.pause();
            "Pausing at the next step boundary.".clone_into(&mut self.status);
        }
        let _ = pause.on_hover_text(
                    "Stop at the next step boundary. A decision already being asked is answered first - that is what makes a pause safe to press anywhere.",
                );
        let stop = ui.add_enabled(attached, egui::Button::new("Stop"));
        if stop.clicked() {
            if let Some(branch) = opened.branch.as_ref() {
                branch.gate().stop();
            }
            "Stopped. Its frames stay; fork from a frame to continue.".clone_into(&mut self.status);
        }
        let _ = stop.on_hover_text(
                    "End this branch. It cannot run again, so continuing means forking from a frame - which is also how you go back.",
                );
    }

    /// Fork from the frame on screen, and the progress of a fork that is already replaying its prefix.
    fn play_control(&mut self, opened: &mut Opened, ui: &mut egui::Ui) {
        let block = opened.app.play_check();
        let rebuilding = opened.rebuilding.is_some();
        let play = ui.add_enabled(
            block.is_ok(),
            egui::Button::new(egui::RichText::new("▶ Play from this frame").strong()),
        );
        let clicked = play.clicked();
        let _ = play.on_hover_text(block.as_ref().err().map_or_else(
                    || "Fork at the frame you are looking at, replay everything up to it from the checkpoint, and check every frame against the recording before letting anybody take a seat. The branch you forked keeps every frame it had.".to_owned(),
                    PlayBlock::tooltip,
                ));
        if rebuilding {
            let cancel = ui.button("Cancel rebuild");
            let spinner = ui.spinner();
            let _ = spinner.on_hover_text("The prefix is being replayed and checked frame by frame. That is why the position is worth playing from.");
            if cancel.clicked() {
                self.cancel_rebuild(opened);
            }
        }
        if clicked {
            self.play(opened);
        }
    }

    fn branch_panel(&mut self, opened: &mut Opened, root: &mut egui::Ui) {
        egui::Panel::left("branches")
            .resizable(true)
            .default_size(230.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL_FILL)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(root, |ui| {
                *ui.visuals_mut() = egui::Visuals::light();
                ui.visuals_mut().override_text_color = Some(PANEL_TEXT);
                ui.heading("Branches");
                let current = opened.app.current();
                let tree = opened.app.tree();
                egui::ScrollArea::vertical()
                    .id_salt("branch-tree")
                    .show(ui, |ui| {
                        section(ui, &format!("{} in this project", tree.len()), |ui| {
                            for node in &tree {
                                let title = format!(
                                    "{}{} · {} fr{}",
                                    if node.id == current { "▸ " } else { "" },
                                    node.title,
                                    node.frames,
                                    if node.playable {
                                        " · playable"
                                    } else if node.verified {
                                        ""
                                    } else {
                                        " · unproved"
                                    }
                                );
                                let response = ui.selectable_label(node.id == current, title);
                                let picked = response.clicked() && node.id != current;
                                let _ = response.on_hover_text(format!(
                                    "{}\nparent: {}\nforked at: {}\nreproduced from its inputs: {}\nplayable now: {}\nframes held in this window: {}\n{}",
                                    node.title,
                                    node.parent.map_or("none - this is the recording".to_owned(), |p| p.to_string()),
                                    node.fork_frame.map_or("the start".to_owned(), |f| f.to_string()),
                                    node.verified,
                                    node.playable,
                                    opened.store.len(node.id),
                                    if node.playable {
                                        "Play forks from a frame of it."
                                    } else {
                                        "A branch that has not been reproduced cannot be forked from; that is the rule that keeps a bad prefix from becoming a new history."
                                    },
                                ));
                                if picked {
                                    opened.branch = None;
                                    opened.rebuilding = None;
                                    opened.app.select_branch(node.id);
                                    self.selected_tile = None;
                                    self.status = format!("Viewing branch {}.", node.id);
                                }
                            }
                        });
                    });
                if opened.store.released() > 0 {
                    ui.small(format!(
                        "{} branch(es)' frames were released to keep memory bounded: their history is listed above, their pictures are gone.",
                        opened.store.released()
                    ));
                }
                if opened.store.missing() > 0 {
                    ui.small(format!(
                        "{} frame(s) were dropped while the window was not looking.",
                        opened.store.missing()
                    ));
                }
                if !matches!(opened.app.rebuild_status(), RebuildStatus::Idle) {
                    ui.small(format!("Last rebuild: {:?}", opened.app.rebuild_status()));
                }
            });
    }

    /// The panel a parked decision puts on screen, with the reviewer's whole step sheet under it.
    ///
    /// Two things are on this sheet and they are not the same thing. The top is the question the
    /// engine is holding open right now, which only this application has; below it is R01's own
    /// decision panel, drawn from the shared module, which is what "enough to judge a play" means.
    /// The option list gets a bounded scroll of its own rather than the panel's leftover height,
    /// because a production or movement choice can offer dozens of options and the ones past the
    /// bottom edge used to be simply unreachable.
    fn choice_panel(
        &mut self,
        opened: &mut Opened,
        root: &mut egui::Ui,
        session: &ReviewSession,
        frames: &[ReviewFrame],
        frame: &ReviewFrame,
    ) {
        let pending = opened.app.pending();
        egui::Panel::right("choices")
            .resizable(true)
            .default_size(470.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL_FILL)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show(root, |ui| {
                *ui.visuals_mut() = egui::Visuals::light();
                ui.visuals_mut().override_text_color = Some(PANEL_TEXT);
                if self.remote_waiting(ui, session, pending.as_ref()) {
                    // A remote player answers this one; the window only says who.
                } else if let Some(pending) = pending {
                    ui.heading(format!(
                        "{} is asked",
                        view::annotate(session, pending.actor.as_str())
                    ));
                    ui.strong(view::annotate(session, &pending.prompt));
                    // A vote names only its outcomes: show the agenda card being voted on.
                    if let Some(alias) = view::current_agenda(frames, frame) {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            for (index, line) in
                                view::agenda_card(ContentStore::embedded(), &alias)
                                    .iter()
                                    .enumerate()
                            {
                                if index == 0 {
                                    ui.strong(line);
                                } else {
                                    ui.label(line);
                                }
                            }
                        });
                    }
                    ui.small(format!(
                        "frame {} · ask {} · {} option(s), in the order the engine offered them",
                        pending.frame,
                        pending.ask,
                        pending.options.len()
                    ));
                    if pending
                        .options
                        .iter()
                        .any(|option| option.score.is_some() || option.probability.is_some())
                    {
                        ui.small(view::policy_odds_legend(session.manifest.temperature));
                    }
                    // The occurrence this panel is drawing, captured now. Both buttons below answer
                    // *this* offer and no other: by the time a click is handled a newer offer that
                    // looks exactly like it may already be up, and binding the click to whatever is
                    // pending then would answer a question the person never saw.
                    let drawn = pending.clone();
                    let mut chosen: Option<String> = None;
                    // Half the panel at most, and never less than a few rows: the options are the
                    // thing being clicked, and the step sheet below them is the thing being read.
                    let room = (ui.available_height() * 0.5).clamp(120.0, 520.0);
                    egui::ScrollArea::vertical()
                        .id_salt("manual-options")
                        .max_height(room)
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            for option in &pending.options {
                                // The policy's own numbers, when the recording has them. A manual seat
                                // choosing against the top-ranked option is the point of the whole
                                // application, so the odds are on the button and not behind a tooltip.
                                let text = format!(
                                    "{}{}",
                                    view::annotate(session, &option.label),
                                    view::policy_odds(option.score, option.probability)
                                );
                                let button = ui.add(
                                    egui::Button::new(egui::RichText::new(text)).wrap_mode(
                                        egui::TextWrapMode::Wrap,
                                    ),
                                );
                                if button.clicked() {
                                    chosen = Some(option.id.clone());
                                }
                                let _ = button.on_hover_text(format!(
                                    "id {}\nkind {}\n{}",
                                    option.id,
                                    option.kind,
                                    if option.payload.is_empty() {
                                        "no structured payload".to_owned()
                                    } else {
                                        view::json_pretty(
                                            &serde_json::to_value(&option.payload)
                                                .unwrap_or_default(),
                                        )
                                    }
                                ));
                                // What the option actually commits, when the engine put a bundle on
                                // it. A diplomacy offer used to read as a label and a raw id, which
                                // is not a thing anybody can accept or refuse knowingly: the terms
                                // are the decision, and until now they were behind a hover over the
                                // payload JSON. The id and kind stay visible for the options that
                                // carry no terms, because for those the id is the information.
                                let terms =
                                    ti4_review::diplomacy::payload_lines(&frame.state, &option.payload);
                                if terms.is_empty() {
                                    ui.small(format!("{} · {}", option.id, option.kind));
                                } else {
                                    ui.indent(("terms", option.id.clone()), |ui| {
                                        for line in terms {
                                            ui.small(view::annotate(session, &line));
                                        }
                                        ui.small(format!("{} · {}", option.id, option.kind));
                                    });
                                }
                            }
                        });
                    if let Some(option_id) = chosen {
                        self.submit(opened, &ManualSubmission::to(&drawn, option_id));
                    }
                    if ui.button("Let the policy answer this one").clicked() {
                        self.delegate(opened, drawn.offer);
                    }
                    ui.separator();
                } else {
                    ui.heading("Choices");
                    ui.weak(
                        "No seat is waiting. Put a seat on Manual in the bar below and run on; the game stops and asks.",
                    );
                    ui.separator();
                }
                // R01's right-hand sheet, unabridged: the action in progress, every decision this
                // step settled with its options, features, payloads and consequence previews, the
                // new engine events, and the selected system.
                panels::decision_sheet(
                    ui,
                    // The branch's own frames, never `session.frames`: the store keeps the header with
                    // its frames emptied, so a branch tree does not duplicate a whole recording, and a
                    // sheet that reached for the latter crashed the window on its first live table.
                    &panels::Sheets {
                        header: session,
                        frames,
                    },
                    frame,
                    self.selected_tile.as_deref(),
                    panels::SystemNaming::TileAndPlanets,
                );
            });
    }

    /// R01's left sheet, whole: the policy behind the table, the table, and every player's holdings.
    fn players_panel(
        &mut self,
        root: &mut egui::Ui,
        session: &ReviewSession,
        frames: &[ReviewFrame],
        frame: &ReviewFrame,
    ) {
        egui::Panel::left("players")
            .resizable(true)
            .default_size(340.0)
            .frame(
                egui::Frame::new()
                    .fill(PANEL_FILL)
                    .inner_margin(egui::Margin::same(8)),
            )
            .show_collapsible(root, &mut self.show_players, |ui| {
                *ui.visuals_mut() = egui::Visuals::light();
                ui.visuals_mut().override_text_color = Some(PANEL_TEXT);
                panels::players_sheet(
                    ui,
                    &panels::Sheets {
                        header: session,
                        frames,
                    },
                    frame,
                );
            });
    }

    /// Compute the power report for the frame on screen, once per frame: it runs a movement search
    /// for every seat and system.
    fn ensure_power(
        &mut self,
        session: &ReviewSession,
        frame: &ReviewFrame,
        branch: &str,
        index: usize,
    ) {
        let stale = self.power.as_ref().is_none_or(|cached| {
            cached.branch != branch || cached.frame != index || cached.report.is_none()
        });
        if stale {
            let galaxy = match self.power.take() {
                Some(cached) if cached.branch == branch && cached.galaxy.is_some() => cached.galaxy,
                _ => ti4_review::power::galaxy_of(session, ContentStore::embedded()),
            };
            let report = galaxy
                .as_ref()
                .map(|galaxy| ti4_review::power::report(galaxy, frame));
            self.power = Some(PowerCached {
                branch: branch.to_owned(),
                frame: index,
                galaxy,
                report,
            });
        }
    }

    /// The Power window: every seat's totals, and the selected system in full.
    fn power_window(&mut self, root: &mut egui::Ui, session: &ReviewSession, frame: &ReviewFrame) {
        let mut open = self.show_power;
        egui::Window::new("⚔ Power")
            .open(&mut open)
            .default_size([900.0, 640.0])
            .resizable(true)
            .show(root.ctx(), |ui| {
                egui::ScrollArea::both().show(ui, |ui| {
                    match self.power.as_ref().and_then(|cached| cached.report.as_ref()) {
                        Some(report) => ti4_review::power::power_sheet(
                            ui,
                            session,
                            frame,
                            report,
                            self.selected_tile.as_deref(),
                        ),
                        None => {
                            ui.weak("The map of this session could not be rebuilt, so there is no power report.");
                        }
                    }
                });
            });
        self.show_power = open;
    }

    /// The map, with the reviewer's own seat row and legend over it.
    fn centre(&mut self, root: &mut egui::Ui, session: &ReviewSession, frame: &ReviewFrame) {
        egui::CentralPanel::default().show(root, |ui| {
            let content = ContentStore::embedded();
            // R01's board header, word for word. Which colour is whose, and what every stroke on the
            // hexes means: without it the map is a picture rather than a position.
            ui.horizontal_wrapped(|ui| {
                ui.strong("Players:");
                for player in &frame.state.players {
                    ui.colored_label(
                        view::player_color(&player.id),
                        format!("● {}", view::seat_name(frame, &player.id, content)),
                    );
                }
            });
            ui.small(
                "Thick outer edge = space control; thin inner edge = planet control (split when mixed). Wormholes: lettered rings; white outer rim = placed token; red slash = suppressed. IN/OUT portals connect the galaxy to the Fracture. Planet: resources/influence · C/H/I trait · B/G/R/Y specialty · ★ legendary · S station · × destroyed. Gray units are neutral; red slash = damaged; yellow ring = galvanized.",
            );
            ui.horizontal_wrapped(|ui| {
                use ti4_review::power::Overlay;
                ui.strong("Power overlay:");
                ui.selectable_value(&mut self.overlay, Overlay::Off, "off");
                for player in &frame.state.players {
                    ui.selectable_value(
                        &mut self.overlay,
                        Overlay::Seat(player.id.clone()),
                        egui::RichText::new(player.id.as_str()).color(view::player_color(&player.id)),
                    );
                }
                ui.selectable_value(&mut self.overlay, Overlay::All, "all seats");
                if let Overlay::Seat(_) = self.overlay {
                    ui.small(
                        "green fill = can take (≥60%), amber = contested (30–60%) · red/amber ring = threatened · dark = out of reach · ⚔ projection index, +ships arriving",
                    );
                } else if self.overlay == Overlay::All {
                    ui.small("one bar per seat: its one-activation projection into the hex (log scale)");
                }
            });
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, Sense::click());
            let tiles = board_view(content, session, frame, self.selected_tile.as_deref());
            let layout = BoardLayout::fitted(response.rect, available, &tiles);
            if let Some(system) = draw_board(&painter, &response, &layout, &tiles) {
                self.selected_tile = Some(system);
            }
            let report = self.power.as_ref().and_then(|cached| cached.report.as_ref());
            if let (Some(report), true) = (report, self.overlay != ti4_review::power::Overlay::Off) {
                ti4_review::power::draw_overlay(&painter, &layout, &tiles, report, &self.overlay);
                if let Some(pointer) = response.hover_pos() {
                    let hovered = tiles.iter().find(|tile| {
                        view::tile_point(&layout, tile).distance(pointer) <= layout.radius * 0.9
                    });
                    if let Some(tile) = hovered {
                        let lines = ti4_review::power::hover_lines(frame, report, &tile.system);
                        let title = view::system_label(session, &tile.system);
                        response.clone().on_hover_ui_at_pointer(|ui| {
                            ui.strong(title);
                            if lines.is_empty() {
                                ui.weak("No seat holds or reaches anything here.");
                            }
                            for (player, line) in &lines {
                                ui.colored_label(view::player_color(player), line);
                            }
                        });
                    }
                }
            }
        });
    }

    /// The frame strip, and the marker that says whether this is the live tip or a page of history.
    fn timeline(&mut self, opened: &mut Opened, root: &mut egui::Ui) {
        egui::Panel::bottom("timeline").show(root, |ui| {
            let branch = opened.app.current();
            let count = opened.store.len(branch);
            if count == 0 {
                ui.label("This branch has no frames to show yet.");
                return;
            }
            let mut position = opened
                .app
                .viewed(branch)
                .unwrap_or(count - 1)
                .min(count - 1);
            ui.horizontal_wrapped(|ui| {
                if ui.button("◁").clicked() {
                    opened.app.previous_frame();
                }
                let slider = ui.add(egui::Slider::new(&mut position, 0..=count - 1).text("frame"));
                if slider.changed() {
                    // Navigation goes through the app, which is the only place that knows looking at a
                    // frame must never branch anything.
                    opened.app.select_frame(position);
                }
                if ui.button("▷").clicked() {
                    opened.app.next_frame();
                }
                let tip = ui.button("Live tip");
                if tip.clicked() {
                    opened.app.go_to_tip();
                }
                let _ = tip.on_hover_text("Jump to the newest frame this branch has produced.");
                ui.separator();
                if opened.app.at_tip() {
                    ui.colored_label(
                        Color32::from_rgb(90, 180, 110),
                        format!(
                            "live tip · frame {}/{} of branch {}",
                            position + 1,
                            count,
                            branch
                        ),
                    );
                } else {
                    ui.colored_label(
                        Color32::from_rgb(220, 170, 60),
                        format!(
                            "history · frame {}/{} of branch {} · Play forks from here",
                            position + 1,
                            count,
                            branch
                        ),
                    );
                }
                if let Some(frame) = opened.store.frame(branch, position) {
                    ui.separator();
                    ui.label(format!(
                        "step {} · round {} · {:?} · {} decision(s){}",
                        frame.engine_step,
                        frame.round,
                        frame.phase,
                        frame.decisions.len(),
                        if frame.finished { " · finished" } else { "" }
                    ));
                }
                if let Some(selected) = &self.selected_tile {
                    ui.separator();
                    ui.label(format!(
                        "selected {}",
                        opened.store.session(branch).map_or_else(
                            || selected.clone(),
                            |session| view::system_label(session, selected)
                        )
                    ));
                    if ui.button("clear").clicked() {
                        self.selected_tile = None;
                    }
                }
            });
        });
    }

    /// Write the window's own settings. Never R01's file: that one belongs to the reviewer.
    fn persist(&self) {
        let settings = ReplaySettings {
            // The window the operator left, not the one the source ships. Rebuilding these from
            // `Default` is how a remembered window size never survived a session.
            window_width: self.window[0],
            window_height: self.window[1],
            players_open: self.show_players,
            decisions_open: self.show_decision,
            branches_open: self.show_branches,
            last_project: self
                .opened
                .as_ref()
                .and_then(|opened| opened.path.as_ref())
                .map(|path| path.display().to_string()),
            // Which branch of that project was selected is remembered by the project file itself, so
            // the window does not keep a second, easily stale copy.
            last_branch: None,
            setup: SetupDefaults {
                checkpoint: self.setup.checkpoint.trim().to_owned(),
                map_pool: self.setup.map_pool.trim().to_owned(),
                seed: self.setup.seed.trim().to_owned(),
                rotation: self.setup.rotation,
                profile_table: profile_table_text(self.setup.table).to_owned(),
                temperature: self.setup.temperature,
                diplomacy: self.setup.diplomacy,
                lineup: self.setup.lineup.clone(),
            },
        };
        if let Err(error) = settings.save(Path::new(SETTINGS_PATH)) {
            eprintln!("replayer settings were not saved: {error}");
        }
    }

    /// Write down what this table is being played with. Called when a table starts, not only on exit,
    /// because the point is to survive the crash rather than the clean quit.
    fn remember_setup(&self) {
        self.persist();
    }
}

impl eframe::App for Replayer {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(context);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // The window's size is only knowable while it is being painted, and it is only worth knowing so
        // that it can be written back out on the way.
        if let Some(rect) = root.ctx().input(|input| input.raw.screen_rect) {
            self.window = [rect.width(), rect.height()];
        }
        // Take the whole opened record out for the duration of the paint. Every panel below then holds
        // `&mut Opened` and `&mut Self` at once, which is what it needs to answer a click and write the
        // status line in the same breath.
        let mut opened = self.opened.take();
        self.top_bar(root, opened.as_ref());
        // Nothing open explains itself and offers a table; something open keeps the offer one click
        // away in a window. Starting is deferred until the record is back in place, because the table
        // being started *is* the record.
        let mut start = if opened.is_none() {
            self.welcome(root)
        } else if self.setup_open {
            self.setup_window(root)
        } else {
            false
        };
        if let Some(opened) = opened.as_mut() {
            // And take the frame store out in turn, so a frame can be borrowed by name while the app
            // beside it is still mutable. A `ReviewFrame` carries a whole `GameState`, so this is the
            // difference between two pointer writes and copying the game per repaint.
            // The bottom panels come *before* anything that claims the remaining space. In egui a panel
            // added after the central one gets what is left, and the central panel leaves nothing - so
            // with the previous order the seat chips, the run buttons, Play and the timeline were all
            // laid out with zero height. That is what "there are no buttons and nothing to click" looks
            // like from the outside, and no test caught it because none of them renders a pixel.
            self.control_bar(opened, root);
            self.timeline(opened, root);
            // Then take the frame store out, so a frame can be borrowed by name while the app beside it
            // is still mutable. A `ReviewFrame` carries a whole `GameState`, so this is the difference
            // between two pointer writes and copying the game per repaint.
            let store = std::mem::take(&mut opened.store);
            let branch = opened.app.current();
            let viewed = opened.app.viewed(branch);
            let session = store.session(branch);
            let frame = viewed.and_then(|index| store.frame(branch, index));
            match session.zip(frame) {
                None => {
                    egui::CentralPanel::default().show(root, |ui| {
                        ui.centered_and_justified(|ui| {
                            ui.heading("This branch has no frames to draw yet.");
                        });
                    });
                }
                Some((session, frame)) => {
                    if self.show_branches {
                        self.branch_panel(opened, root);
                    }
                    if self.show_players {
                        self.players_panel(root, session, store.frames(branch), frame);
                    }
                    if self.show_decision {
                        self.choice_panel(opened, root, session, store.frames(branch), frame);
                    }
                    if self.show_power || self.overlay != ti4_review::power::Overlay::Off {
                        self.ensure_power(
                            session,
                            frame,
                            &format!("{branch:?}"),
                            viewed.unwrap_or(0),
                        );
                    }
                    if self.show_power {
                        self.power_window(root, session, frame);
                    }
                    self.centre(root, session, frame);
                }
            }
            opened.store = store;
        }
        self.opened = opened;
        match self.pending_save.take() {
            Some(PendingSave::Known) => self.save_known(),
            Some(PendingSave::As) => self.save_as_dialog(),
            None => {}
        }
        if std::mem::take(&mut start) {
            self.start_table();
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(opened) = self.opened.as_mut() {
            // Ask the branch to unwind and report whatever rebuild was in flight. A closing window has
            // nowhere to put it; the project on disk is whatever was last saved, which is the promise.
            let _ = opened.app.close();
            opened.branch = None;
        }
        self.persist();
    }
}

/// The seats to show chips for: the engine's own seating order when a frame has one, and otherwise the
fn visible_seats(opened: &Opened) -> Vec<PlayerId> {
    let branch = opened.app.current();
    seats_from(opened.store.frames(branch), &opened.app.seats())
}

/// The seats to offer chips for.
///
/// The newest frame that knows the seating order wins, because that is the table as played. Failing
/// that - a table that has not produced a frame yet, or a branch whose frames were released from the
/// store - the answer is still every seat the engine plays, not the seats somebody has already touched.
/// Offering only the touched ones is how "only seat 0 can be taken" happened: before the first frame
/// arrived there was nothing to ask, so the only chip on the bar was the one the setup form had set.
#[must_use]
pub fn seats_from(frames: &[ReviewFrame], seats: &SeatControl) -> Vec<PlayerId> {
    if let Some(frame) = frames.last()
        && !frame.state.seating_order.is_empty()
    {
        return frame.state.seating_order.clone();
    }
    let mut chips = seats
        .changes()
        .into_iter()
        .map(|(seat, _)| seat)
        .chain((0..6).map(|seat| PlayerId::new(format!("seat{seat}"))))
        .collect::<Vec<_>>();
    chips.dedup();
    chips
}

/// The seat settings a fork records with its child.
#[must_use]
pub fn seat_settings(seats: &SeatControl) -> Vec<SeatSetting> {
    seats
        .changes()
        .into_iter()
        .map(|(player, mode)| SeatSetting {
            player: player.to_string(),
            mode,
        })
        .collect()
}

/// The seat modes a branch thread starts with.
#[must_use]
pub fn seat_control(seats: &SeatControl) -> SeatControl {
    let mut control = SeatControl::all_auto();
    for (player, mode) in seats.changes() {
        control.set_mode(&player, mode);
    }
    control
}

/// Put the live branch's own settled answers into the project, so the file can reproduce it later.
///
/// Only the answers from the fork frame onwards belong to the child; its ancestors are cut at the same
/// line by [`prefix`](crate::ReplayerProject::prefix), and storing the overlap twice would replay
/// a game that never happened.
fn fold_branch_into_project(opened: &mut Opened) -> Result<(), String> {
    let Some(branch) = opened.branch.as_ref() else {
        return Ok(());
    };
    let current = opened.app.current();
    let records = branch.gate().records();
    let frames = u64::try_from(opened.store.len(current)).unwrap_or(u64::MAX);
    store::fold_answers(opened.app.project_mut(), current, &records, frames)
        .map_err(|error| error.to_string())
}

fn describe_submission(outcome: &SubmitOutcome) -> String {
    match outcome {
        SubmitOutcome::Accepted { option_id } => format!("Answered {option_id}."),
        SubmitOutcome::Stale { .. } => {
            "That offer had already changed; the panel now shows what the engine is waiting on."
                .to_owned()
        }
        SubmitOutcome::NotOffered { .. } => "That option was not on offer.".to_owned(),
        SubmitOutcome::Duplicate => "That choice is already answered.".to_owned(),
        SubmitOutcome::NoPendingChoice => "Nothing was waiting for a human.".to_owned(),
    }
}

/// Wait for a starting table to describe itself, keeping the frames it publishes meanwhile.
///
/// The branch thread has to build the table before it can say who is sitting where, and the seating
/// order is part of what the project records - so the window waits for the header rather than guessing
/// at six seats. Frames published during the wait are kept rather than dropped: the first of them is
/// the opening position, and losing it would leave the timeline a frame short of the game and every
/// later frame's index out by one against the answers recorded for it.
fn table_header(branch: &LiveBranch) -> Option<(ReviewSession, Vec<ReviewFrame>)> {
    let mut frames = Vec::new();
    let until = Instant::now() + Duration::from_secs(120);
    loop {
        let feed = branch.gate().take_feed();
        frames.extend(feed.frames);
        if let Some(header) = feed.header {
            return Some((header, frames));
        }
        if branch.gate().state() == LiveState::Failed || Instant::now() > until {
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}
