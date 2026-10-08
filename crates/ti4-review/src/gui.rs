//! Native egui front end for live and saved reviews.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui::{self, Sense};
use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;

use crate::{
    AdvanceUnit, LiveReview, MAX_COMMAND_STEPS, ProfileTable, ReviewFrame, ReviewSession,
    SessionOutcome, SimulationConfig, default_sampling_temperature, export_html, load_session,
    save_session,
};

use crate::view::{BoardLayout, PANEL_FILL, PANEL_TEXT, board_view, draw_board, player_color};

const STEPS_PER_UI_FRAME: usize = 128;

/// The floor and ceiling on the adaptive autosave interval, in engine steps.
///
/// The floor keeps a short review saving often enough to be worth having. The ceiling stops a very
/// expensive save from pushing the next one so far out that a crash loses the run.
const AUTOSAVE_MIN_STEPS: usize = 1024;
const AUTOSAVE_MAX_STEPS: usize = 65_536;

/// The share of running time autosaving is allowed to take.
///
/// Ten means a save that took one second buys ten seconds of play before the next, so the cost
/// stays proportional to the run rather than to the run squared.
const AUTOSAVE_DUTY: u32 = 10;
const SETTINGS_PATH: &str = "out/reviews/reviewer-settings.json";
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug)]
enum RunTarget {
    Count { unit: AdvanceUnit, remaining: usize },
    Round(u32),
    End,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ReviewerSettings {
    checkpoint: String,
    map_pool: String,
    profile_table: ProfileTable,
    temperature: f64,
    last_review: Option<String>,
    diplomacy: bool,
    /// The seed and faction rotation last played with, kept as text because that is how they are
    /// edited and because a seed is a number the operator types, not one they want re-derived.
    seed: String,
    rotation: usize,
}

impl Default for ReviewerSettings {
    fn default() -> Self {
        Self {
            checkpoint: String::new(),
            map_pool: String::new(),
            profile_table: ProfileTable::default(),
            temperature: default_sampling_temperature(),
            last_review: None,
            diplomacy: false,
            seed: DEFAULT_SEED.to_owned(),
            rotation: 0,
        }
    }
}

/// The seed the reviewer starts with when nothing has been remembered yet.
const DEFAULT_SEED: &str = "42";

/// A remembered seed, or the default if nothing usable was remembered.
///
/// The field is editable text, so "it is empty" is a state the settings file can reach - usually by
/// the operator clearing it to type something and quitting before starting a run. Falling back here
/// keeps the next launch showing a seed that will parse.
fn normalize_seed(seed: &str) -> String {
    let seed = seed.trim().to_owned();
    if seed.is_empty() {
        DEFAULT_SEED.to_owned()
    } else {
        seed
    }
}

fn load_settings() -> std::result::Result<ReviewerSettings, String> {
    let path = Path::new(SETTINGS_PATH);
    if !path.is_file() {
        return Ok(ReviewerSettings::default());
    }
    let metadata =
        fs::metadata(path).map_err(|error| format!("read settings metadata: {error}"))?;
    if metadata.len() > MAX_SETTINGS_BYTES {
        return Err(format!(
            "settings file is {} bytes, above the {MAX_SETTINGS_BYTES}-byte limit",
            metadata.len()
        ));
    }
    let bytes = fs::read(path).map_err(|error| format!("read settings: {error}"))?;
    serde_json::from_slice(&bytes).map_err(|error| format!("parse settings: {error}"))
}

pub fn run() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TI4 learned-game reviewer")
            .with_inner_size([1500.0, 950.0]),
        ..Default::default()
    };
    eframe::run_native(
        "TI4 learned-game reviewer",
        options,
        Box::new(|context| Ok(Box::new(ReviewApp::new(context)))),
    )
}

struct ReviewApp {
    checkpoint: String,
    map_pool: String,
    seed: String,
    rotation: usize,
    table: ProfileTable,
    temperature: f64,
    diplomacy: bool,
    run_count: String,
    run_unit: AdvanceUnit,
    live: Option<LiveReview>,
    replay: Option<ReviewSession>,
    viewed: usize,
    run_target: Option<RunTarget>,
    command_steps: usize,
    autosave: Option<PathBuf>,
    /// The step count at which the running command may autosave again.
    ///
    /// A fixed interval is what made a long review crawl and then appear to hang. `save_session`
    /// validates every frame, serialises every frame's whole `GameState` into one buffer and writes
    /// the lot, so one autosave costs O(frames); firing that on a fixed step interval makes a run
    /// O(frames^2). By two thousand frames the session is hundreds of megabytes and a single
    /// autosave blocks the UI thread for seconds -- low CPU the whole time, because it is
    /// serialisation and disk, which is exactly how it was reported.
    ///
    /// So the interval is set from what the last autosave actually cost: see
    /// [`ReviewApp::autosave_now`]. Autosaving stays a bounded share of the run however long it gets,
    /// and the file format is unchanged.
    next_autosave_step: usize,
    /// Whether the players (left) and decision (right) panels are open.
    show_players: bool,
    show_decision: bool,
    /// When the running command started, for the rate the budget above is spent against.
    command_started: Option<std::time::Instant>,
    last_review: Option<PathBuf>,
    status: String,
    selected_tile: Option<String>,
}

impl ReviewApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_visuals(egui::Visuals::dark());
        let (settings, settings_error) = match load_settings() {
            Ok(settings) => (settings, None),
            Err(error) => (ReviewerSettings::default(), Some(error)),
        };
        Self {
            checkpoint: settings.checkpoint,
            map_pool: settings.map_pool,
            // What this table last played with, not what the source code ships with. The seed and the
            // rotation used to be the two fields that forgot: a session opened at 03:00 to finish a
            // run came back to seed 42 and rotation 0, which is a different game.
            seed: normalize_seed(&settings.seed),
            rotation: settings.rotation,
            table: settings.profile_table,
            temperature: settings.temperature,
            diplomacy: settings.diplomacy,
            run_count: "10".to_owned(),
            run_unit: AdvanceUnit::Step,
            live: None,
            replay: None,
            viewed: 0,
            run_target: None,
            command_steps: 0,
            next_autosave_step: AUTOSAVE_MIN_STEPS,
            show_players: true,
            show_decision: true,
            command_started: None,
            autosave: None,
            last_review: settings.last_review.map(PathBuf::from),
            status: settings_error.map_or_else(
                || "Restored the last checkpoint/profile and map-pool selections.".to_owned(),
                |error| format!("Settings were not restored: {error}"),
            ),
            selected_tile: None,
        }
    }

    fn session(&self) -> Option<&ReviewSession> {
        self.live
            .as_ref()
            .map(|live| &live.session)
            .or(self.replay.as_ref())
    }

    /// Lend the session to a `&mut self` method without copying it.
    ///
    /// The panels need `&mut self` for their own UI state while reading the session, which the
    /// borrow checker will not allow directly. Cloning it to get around that is what `ui` used to
    /// do, and it is ruinous here: a clone is a deep copy of every frame's whole `GameState`, so a
    /// 1,800-frame review copied hundreds of megabytes *per repaint* -- which is why the window got
    /// slower the longer a run went and why scrolling crawled.
    ///
    /// Moving it out and back costs two pointer writes. The session is restored on every path,
    /// including when `act` panics, because it is put back by the guard's `Drop`.
    fn with_session<R>(&mut self, act: impl FnOnce(&mut Self, &ReviewSession) -> R) -> Option<R> {
        /// Whichever field the session was taken from, so it goes back to the same one.
        enum Held {
            Live(Box<LiveReview>),
            Replay(Box<ReviewSession>),
        }

        // `LiveReview` moves whole rather than by its `session` field: `ReviewSession` is not
        // `Default`, so there is nothing to leave behind in its place. Once it is out, it is an
        // ordinary local -- borrowing it shared while `self` is borrowed mutably is disjoint, which
        // is what makes this safe as well as cheap.
        let held = match self.live.take() {
            Some(live) => Held::Live(Box::new(live)),
            None => Held::Replay(Box::new(self.replay.take()?)),
        };
        let result = {
            let session = match &held {
                Held::Live(live) => &live.session,
                Held::Replay(session) => session,
            };
            act(self, session)
        };
        match held {
            Held::Live(live) => self.live = Some(*live),
            Held::Replay(session) => self.replay = Some(*session),
        }
        Some(result)
    }

    fn latest_index(&self) -> usize {
        self.session()
            .map_or(0, |session| session.frames.len().saturating_sub(1))
    }

    fn settings(&self) -> ReviewerSettings {
        ReviewerSettings {
            checkpoint: self.checkpoint.trim().to_owned(),
            map_pool: self.map_pool.trim().to_owned(),
            profile_table: self.table,
            temperature: self.temperature,
            last_review: self
                .last_review
                .as_ref()
                .map(|path| path.display().to_string()),
            diplomacy: self.diplomacy,
            seed: self.seed.trim().to_owned(),
            rotation: self.rotation,
        }
    }

    fn persist_settings(&mut self) {
        let bytes = match serde_json::to_vec_pretty(&self.settings()) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.status = format!("Save settings failed: {error}");
                return;
            }
        };
        if let Err(error) = super::replace_file(Path::new(SETTINGS_PATH), &bytes) {
            self.status = format!("Save settings failed: {error}");
        }
    }

    fn install_replay(&mut self, path: &Path, session: ReviewSession) {
        self.viewed = 0;
        self.live = None;
        self.replay = Some(session);
        self.run_target = None;
        self.autosave = None;
        self.last_review = Some(path.to_path_buf());
        self.status = format!("Opened {} in view-only mode", path.display());
        self.persist_settings();
    }

    fn previous_candidates(&self) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if let Some(path) = &self.last_review {
            candidates.push(path.clone());
        }
        let mut discovered: Vec<(SystemTime, PathBuf)> = fs::read_dir("out/reviews")
            .into_iter()
            .flatten()
            .filter_map(std::result::Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_review(path))
            .filter_map(|path| {
                let modified = fs::metadata(&path).ok()?.modified().ok()?;
                Some((modified, path))
            })
            .collect();
        discovered.sort_by(|left, right| right.0.cmp(&left.0));
        candidates.extend(discovered.into_iter().map(|(_, path)| path));
        candidates.dedup();
        candidates
    }

    fn open_previous(&mut self) {
        let mut last_error = None;
        for path in self.previous_candidates() {
            match load_session(&path) {
                Ok(session) => {
                    self.install_replay(&path, session);
                    return;
                }
                Err(error) => last_error = Some(format!("{}: {error}", path.display())),
            }
        }
        self.status = last_error.map_or_else(
            || "No previous saved or autosaved game was found.".to_owned(),
            |error| format!("No valid previous game was found; last error: {error}"),
        );
    }

    fn load_start(&mut self) {
        let seed = match self.seed.trim().parse::<u64>() {
            Ok(seed) => seed,
            Err(error) => {
                self.status = format!("Invalid seed: {error}");
                return;
            }
        };
        let config = SimulationConfig {
            checkpoint: PathBuf::from(self.checkpoint.trim()),
            map_pool: PathBuf::from(self.map_pool.trim()),
            seed,
            rotation: self.rotation,
            table: self.table,
            temperature: self.temperature,
            diplomacy: self.diplomacy,
            lineup: None,
        };
        match LiveReview::start(&config) {
            Ok(live) => {
                let lineup = live.session.manifest.factions.join(" → ");
                self.autosave = Some(PathBuf::from("out/reviews").join(format!(
                    "autosave-{seed}-rotation{}-{}{}.ti4review.json.zst",
                    self.rotation,
                    match self.table {
                        ProfileTable::Learner => "learner",
                        ProfileTable::Accepted => "accepted",
                    },
                    if self.diplomacy { "-diplomacy" } else { "" }
                )));
                self.live = Some(live);
                self.replay = None;
                self.viewed = 0;
                self.run_target = None;
                self.status =
                    format!("Starting table loaded; no engine step has run. Seats 0–5: {lineup}");
                self.autosave_now();
                if let Some(path) = &self.autosave {
                    self.last_review = Some(path.clone());
                }
                self.persist_settings();
            }
            Err(error) => self.status = format!("Load failed: {error}"),
        }
    }

    /// Start a running command's autosave budget over.
    ///
    /// Each command is timed on its own. Carrying the previous one's rate across would let a run
    /// that was interrupted early set the interval for one that is not.
    fn begin_autosave_budget(&mut self) {
        self.command_started = Some(std::time::Instant::now());
        self.next_autosave_step = AUTOSAVE_MIN_STEPS;
    }

    fn autosave_now(&mut self) {
        let Some(path) = self.autosave.clone() else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        let started = std::time::Instant::now();
        if let Err(error) = save_session(&path, session) {
            self.status = format!("Autosave failed: {error}");
        }
        let cost = started.elapsed();

        // Buy back what the save cost, in steps, from the rate the run is actually managing. A
        // cheap save on a short session keeps the floor interval; an expensive one on a long
        // session earns a proportionally longer wait, which is what stops the quadratic blow-up.
        let elapsed = self
            .command_started
            .map_or(cost, |at| at.elapsed())
            .max(cost);
        let steps_per_second = if elapsed.as_secs_f64() > 0.0 {
            #[expect(
                clippy::cast_precision_loss,
                reason = "step counts are far below f64's exact-integer range"
            )]
            let steps = self.command_steps as f64;
            steps / elapsed.as_secs_f64()
        } else {
            0.0
        };
        let budget = cost.as_secs_f64() * f64::from(AUTOSAVE_DUTY) * steps_per_second;
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamped into usize range on the next line"
        )]
        let interval = (budget as usize).clamp(AUTOSAVE_MIN_STEPS, AUTOSAVE_MAX_STEPS);
        self.next_autosave_step = self.command_steps.saturating_add(interval);
    }

    fn save_as(&mut self) {
        let Some(session) = self.session() else {
            "Nothing to save.".clone_into(&mut self.status);
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Compressed TI4 review", &["zst"])
            .add_filter("Uncompressed TI4 review", &["json"])
            .set_file_name("game.ti4review.json.zst")
            .save_file()
        else {
            return;
        };
        match save_session(&path, session) {
            Ok(()) => {
                self.last_review = Some(path.clone());
                self.status = format!("Saved {}", path.display());
                self.persist_settings();
            }
            Err(error) => self.status = format!("Save failed: {error}"),
        }
    }

    fn open_review(&mut self) {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("TI4 review", &["zst", "json"])
            .pick_file()
        else {
            return;
        };
        match load_session(&path) {
            Ok(session) => self.install_replay(&path, session),
            Err(error) => self.status = format!("Open failed: {error}"),
        }
    }

    fn export(&mut self) {
        let Some(session) = self.session() else {
            "Nothing to export.".clone_into(&mut self.status);
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("HTML", &["html"])
            .set_file_name("game-review.html")
            .save_file()
        else {
            return;
        };
        match export_html(&path, session) {
            Ok(()) => self.status = format!("Exported {}", path.display()),
            Err(error) => self.status = format!("Export failed: {error}"),
        }
    }

    fn begin_count(&mut self, unit: AdvanceUnit, count: usize) {
        if self.live.is_none() {
            "A saved review is view-only; load a starting table to simulate."
                .clone_into(&mut self.status);
            return;
        }
        if count == 0 {
            "Run count is zero; no engine step was attempted.".clone_into(&mut self.status);
            return;
        }
        self.run_target = Some(RunTarget::Count {
            unit,
            remaining: count,
        });
        self.command_steps = 0;
        self.begin_autosave_budget();
        self.status = format!("Running {count} {unit:?}(s)…");
    }

    fn tick_run(&mut self, context: &egui::Context) {
        if self.run_target.is_none() {
            return;
        }
        for _ in 0..STEPS_PER_UI_FRAME {
            let Some(target) = self.run_target.clone() else {
                break;
            };
            let Some(live) = self.live.as_mut() else {
                self.run_target = None;
                break;
            };
            if live.is_terminal() {
                self.status = match &live.session.outcome {
                    SessionOutcome::Completed => "Game completed naturally.".to_owned(),
                    SessionOutcome::EngineFailed { error } => format!("Engine failed: {error}"),
                    other => format!("Simulation stopped: {other:?}"),
                };
                self.run_target = None;
                break;
            }
            let frame = live.step_once().clone();
            self.command_steps += 1;
            let done = match target {
                RunTarget::Count {
                    unit,
                    mut remaining,
                } => {
                    let crossed = match unit {
                        AdvanceUnit::Step => 1,
                        AdvanceUnit::Decision => frame.decisions.len(),
                        AdvanceUnit::Action => usize::from(frame.action_completed),
                    };
                    if crossed > 0 {
                        remaining = remaining.saturating_sub(crossed);
                    }
                    if remaining == 0 {
                        true
                    } else {
                        self.run_target = Some(RunTarget::Count { unit, remaining });
                        false
                    }
                }
                RunTarget::Round(round) => frame.round >= round,
                RunTarget::End => frame.finished,
            };
            if done {
                self.status = format!(
                    "Command complete at step {}, round {}, {:?}.",
                    frame.engine_step, frame.round, frame.phase
                );
                self.run_target = None;
            }
            if self.command_steps >= MAX_COMMAND_STEPS {
                live.session.outcome = SessionOutcome::SafetyLimit {
                    steps: self.command_steps,
                };
                self.status = format!(
                    "Command hit the {MAX_COMMAND_STEPS}-step safety limit; session is incomplete."
                );
                self.run_target = None;
            }
            self.viewed = self.latest_index();
            if self.run_target.is_none() {
                self.autosave_now();
                break;
            }
        }
        if self.run_target.is_some() {
            if self.command_steps >= self.next_autosave_step {
                self.autosave_now();
            }
            context.request_repaint();
        }
    }

    fn top_bar(&mut self, root: &mut egui::Ui) {
        egui::Panel::top("inputs").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Choose checkpoint…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("JSON checkpoint", &["json"])
                        .pick_file()
                {
                    self.checkpoint = path.display().to_string();
                    self.persist_settings();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.checkpoint)
                        .desired_width(280.0)
                        .hint_text("checkpoint JSON"),
                );
                if ui.button("Choose map pool…").clicked()
                    && let Some(path) = rfd::FileDialog::new()
                        .add_filter("Map pool", &["json", "gz"])
                        .pick_file()
                {
                    self.map_pool = path.display().to_string();
                    self.persist_settings();
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.map_pool)
                        .desired_width(280.0)
                        .hint_text("map pool JSON.GZ"),
                );
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("Seed");
                ui.add(egui::TextEdit::singleline(&mut self.seed).desired_width(110.0));
                ui.label("Faction rotation");
                egui::ComboBox::from_id_salt("rotation")
                    .selected_text(self.rotation.to_string())
                    .show_ui(ui, |ui| {
                        for rotation in 0..6 {
                            ui.selectable_value(&mut self.rotation, rotation, rotation.to_string());
                        }
                    })
                    .response
                    .on_hover_text(
                        "The seed deterministically permutes faction order; rotation then cyclically shifts that same permutation across physical seats.",
                    );
                let profile_response = egui::ComboBox::from_id_salt("profile_table")
                    .selected_text(self.table.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.table, ProfileTable::Learner, "Learner");
                        ui.selectable_value(
                            &mut self.table,
                            ProfileTable::Accepted,
                            "Accepted champion",
                        );
                    });
                if profile_response.response.changed() {
                    self.persist_settings();
                }
                ui.label("Temperature");
                let temperature_response = ui
                    .add(
                        egui::DragValue::new(&mut self.temperature)
                            .range(0.01..=10.0)
                            .speed(0.05)
                            .max_decimals(2),
                    )
                    .on_hover_text(
                        "Lower values prefer the highest-scored move; higher values explore more. The setting applies when a new starting table is loaded.",
                    );
                if temperature_response.changed() {
                    self.persist_settings();
                }
                let diplomacy_response = ui
                    .checkbox(&mut self.diplomacy, "Structured diplomacy")
                    .on_hover_text(
                        "Seats may open contacts, offer and counter deals, send signals and keep or break promises; relationships between seats are tracked. Applies when a new starting table is loaded.",
                    );
                if diplomacy_response.changed() {
                    self.persist_settings();
                }
                if ui.button("Load starting table").clicked() {
                    self.load_start();
                }
                ui.separator();
                if ui.button("Previous game").clicked() {
                    self.open_previous();
                }
                if ui.button("Open review…").clicked() {
                    self.open_review();
                }
                if ui.button("Save As…").clicked() {
                    self.save_as();
                }
                if ui.button("Export HTML…").clicked() {
                    self.export();
                }
            });
            ui.label(&self.status);
        });
    }

    fn controls(&mut self, root: &mut egui::Ui) {
        egui::Panel::bottom("controls").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.toggle_value(&mut self.show_players, "◧ Players");
                ui.toggle_value(&mut self.show_decision, "◨ Decisions");
                ui.separator();
                let can_run = self.live.is_some() && self.run_target.is_none();
                if ui.add_enabled(can_run, egui::Button::new("Step")).clicked() {
                    self.begin_count(AdvanceUnit::Step, 1);
                }
                if ui
                    .add_enabled(can_run, egui::Button::new("Next decision"))
                    .clicked()
                {
                    self.begin_count(AdvanceUnit::Decision, 1);
                }
                if ui
                    .add_enabled(can_run, egui::Button::new("Next action (full turn)"))
                    .on_hover_text(
                        "Runs until the current active player hands off the turn; nested prompts, transactions, and Fleet Logistics stay inside it.",
                    )
                    .clicked()
                {
                    self.begin_count(AdvanceUnit::Action, 1);
                }
                ui.separator();
                ui.label("Run");
                ui.add(egui::TextEdit::singleline(&mut self.run_count).desired_width(70.0));
                egui::ComboBox::from_id_salt("run_unit")
                    .selected_text(format!("{:?}s", self.run_unit))
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.run_unit, AdvanceUnit::Step, "Steps");
                        ui.selectable_value(&mut self.run_unit, AdvanceUnit::Decision, "Decisions");
                        ui.selectable_value(
                            &mut self.run_unit,
                            AdvanceUnit::Action,
                            "Actions (full turns)",
                        );
                    });
                if ui
                    .add_enabled(can_run, egui::Button::new("Run N"))
                    .clicked()
                {
                    match self.run_count.trim().parse::<usize>() {
                        Ok(count) if count <= crate::MAX_RUN_COUNT => {
                            self.begin_count(self.run_unit, count);
                        }
                        Ok(_) => {
                            self.status =
                                format!("Run count exceeds the {} limit.", crate::MAX_RUN_COUNT);
                        }
                        Err(error) => self.status = format!("Invalid run count: {error}"),
                    }
                }
                if ui
                    .add_enabled(can_run, egui::Button::new("End round"))
                    .clicked()
                    && let Some(round) = self
                        .live
                        .as_ref()
                        .map(|live| live.session.latest().round + 1)
                {
                    self.run_target = Some(RunTarget::Round(round));
                    self.command_steps = 0;
        self.begin_autosave_budget();
                    self.status = format!("Running to round {round}…");
                }
                if ui
                    .add_enabled(can_run, egui::Button::new("End game"))
                    .clicked()
                {
                    self.run_target = Some(RunTarget::End);
                    self.command_steps = 0;
        self.begin_autosave_budget();
                    "Running to natural completion…".clone_into(&mut self.status);
                }
                if ui
                    .add_enabled(self.run_target.is_some(), egui::Button::new("Stop"))
                    .clicked()
                {
                    self.run_target = None;
                    "Stopped at a clean engine-step boundary; session is incomplete."
                        .clone_into(&mut self.status);
                    self.autosave_now();
                }
            });
            if let Some(session) = self.session() {
                let frame_count = session.frames.len();
                let latest = frame_count.saturating_sub(1);
                let outcome = session.outcome.clone();
                ui.horizontal(|ui| {
                    if ui.button("Previous frame").clicked() {
                        self.viewed = self.viewed.saturating_sub(1);
                    }
                    ui.add(
                        egui::Slider::new(&mut self.viewed, 0..=latest)
                            .text("history frame")
                            .show_value(true),
                    );
                    if ui.button("Next frame").clicked() {
                        self.viewed = (self.viewed + 1).min(latest);
                    }
                    if ui.button("Latest").clicked() {
                        self.viewed = latest;
                    }
                    ui.label(format!("{frame_count} frames · {outcome:?}"));
                });
            }
        });
    }

    fn player_panel(
        root: &mut egui::Ui,
        open: &mut bool,
        session: &ReviewSession,
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
            .show_collapsible(root, open, |ui| {
                // A light sheet with black text: the dark theme stays on the board and the
                // decision panel, where colour carries the map.
                *ui.visuals_mut() = egui::Visuals::light();
                ui.visuals_mut().override_text_color = Some(PANEL_TEXT);
                crate::panels::players_sheet(ui, &crate::panels::Sheets::whole(session), frame);
            });
    }

    fn decision_panel(
        &mut self,
        root: &mut egui::Ui,
        session: &ReviewSession,
        frame: &ReviewFrame,
    ) {
        egui::Panel::right("decision")
            .resizable(true)
            .default_size(410.0)
            .show_collapsible(root, &mut self.show_decision, |ui| {
                crate::panels::decision_sheet(
                    ui,
                    &crate::panels::Sheets::whole(session),
                    frame,
                    self.selected_tile.as_deref(),
                    crate::panels::SystemNaming::TileOnly,
                );
            });
    }

    fn board(&mut self, root: &mut egui::Ui, session: &ReviewSession, frame: &ReviewFrame) {
        egui::CentralPanel::default().show(root, |ui| {
            let content = ContentStore::embedded();
            ui.horizontal_wrapped(|ui| {
                ui.strong("Players:");
                for player in &frame.state.players {
                    ui.colored_label(
                        player_color(&player.id),
                        format!("● {}", crate::view::seat_name(frame, &player.id, content)),
                    );
                }
            });
            ui.small(
                "Thick outer edge = space control; thin inner edge = planet control (split when mixed). Wormholes: lettered rings; white outer rim = placed token; red slash = suppressed. IN/OUT portals connect the galaxy to the Fracture. Planet: resources/influence · C/H/I trait · B/G/R/Y specialty · ★ legendary · S station · × destroyed. Gray units are neutral; red slash = damaged; yellow ring = galvanized.",
            );
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, Sense::click());
            // What each tile *means* is `board_view`'s answer for the frame; where it goes is
            // `BoardLayout`; the strokes are `draw_board`. All three are shared, so the replayer
            // paints this board without restating a single number from it.
            let tiles = board_view(content, session, frame, self.selected_tile.as_deref());
            let layout = BoardLayout::fitted(response.rect, available, &tiles);
            if let Some(system) = draw_board(&painter, &response, &layout, &tiles) {
                self.selected_tile = Some(system);
            }
        });
    }
}

impl eframe::App for ReviewApp {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_run(context);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.top_bar(root);
        self.controls(root);
        if self.session().is_none() {
            egui::CentralPanel::default().show(root, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.heading("Load a real learned-policy starting table to begin.");
                });
            });
            return;
        }
        // Borrowed, not cloned -- see `with_session`. The viewed frame is borrowed too: it carries
        // a whole `GameState`, so copying it once per repaint was the second-largest cost here.
        self.with_session(|app, session| {
            app.viewed = app.viewed.min(session.frames.len().saturating_sub(1));
            let frame = &session.frames[app.viewed];
            Self::player_panel(root, &mut app.show_players, session, frame);
            app.decision_panel(root, session, frame);
            app.board(root, session, frame);
        });
        if self.run_target.is_some() {
            root.ctx().request_repaint();
        }
    }
}

fn is_review(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.ends_with(".ti4review.json") || name.ends_with(".ti4review.json.zst")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reviewer_settings_preserve_input_selections_and_previous_game() {
        let settings = ReviewerSettings {
            checkpoint: "out/checkpoints/run-003/checkpoint-532156/slots.json".to_owned(),
            map_pool: "out/pools/save52_noadj_train.json".to_owned(),
            profile_table: ProfileTable::Accepted,
            temperature: 0.25,
            last_review: Some("out/reviews/autosave.ti4review.json".to_owned()),
            diplomacy: true,
            seed: "90210".to_owned(),
            rotation: 3,
        };
        let bytes = serde_json::to_vec(&settings).unwrap();
        let restored: ReviewerSettings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.checkpoint, settings.checkpoint);
        assert_eq!(restored.map_pool, settings.map_pool);
        assert_eq!(restored.profile_table, settings.profile_table);
        assert!((restored.temperature - settings.temperature).abs() <= f64::EPSILON);
        assert_eq!(restored.last_review, settings.last_review);
        assert_eq!(restored.seed, settings.seed, "the seed came back blank");
        assert_eq!(
            restored.rotation, settings.rotation,
            "the rotation came back"
        );
    }

    /// A settings file written before seed and rotation were remembered still has to load, and an
    /// empty remembered seed must not come back as a field that refuses to parse.
    #[test]
    fn old_settings_files_and_blank_seeds_survive() {
        let legacy: ReviewerSettings =
            serde_json::from_slice(br#"{"checkpoint":"cp/slots.json","temperature":0.05}"#)
                .expect("a settings file from before seed and rotation still loads");
        assert_eq!(legacy.seed, DEFAULT_SEED);
        assert_eq!(legacy.rotation, 0);
        assert_eq!(normalize_seed(""), DEFAULT_SEED);
        assert_eq!(normalize_seed("  7 1 "), "7 1");
    }

    #[test]
    fn previous_game_discovery_accepts_only_review_sessions() {
        assert!(is_review(Path::new("game.ti4review.json")));
        assert!(is_review(Path::new("game.ti4review.json.zst")));
        assert!(!is_review(Path::new("reviewer-settings.json")));
        assert!(!is_review(Path::new("game.html")));
    }
}
