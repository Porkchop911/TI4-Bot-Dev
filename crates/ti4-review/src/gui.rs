//! Native egui front end for live and saved reviews.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Vec2};
use serde::{Deserialize, Serialize};
use ti4_content::ContentStore;
use ti4_model::content_types::FULL;
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::units::Unit;

use crate::{
    AdvanceUnit, LiveReview, MAX_COMMAND_STEPS, ProfileTable, ReviewFrame, ReviewSession,
    SessionOutcome, SimulationConfig, export_html, load_session, save_session,
};

const STEPS_PER_UI_FRAME: usize = 128;
const SETTINGS_PATH: &str = "out/reviews/reviewer-settings.json";
const MAX_SETTINGS_BYTES: u64 = 64 * 1024;

const SEAT_COLORS: [Color32; 6] = [
    Color32::from_rgb(224, 66, 66),
    Color32::from_rgb(66, 142, 235),
    Color32::from_rgb(242, 198, 56),
    Color32::from_rgb(54, 184, 116),
    Color32::from_rgb(173, 103, 224),
    Color32::from_rgb(238, 126, 49),
];

fn seat_index(player: &PlayerId) -> usize {
    player
        .to_string()
        .strip_prefix("seat")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or_default()
        % SEAT_COLORS.len()
}

fn player_color(player: &PlayerId) -> Color32 {
    SEAT_COLORS[seat_index(player)]
}

fn short_trait(value: &str) -> &'static str {
    if value.eq_ignore_ascii_case("cultural") {
        "C"
    } else if value.eq_ignore_ascii_case("hazardous") {
        "H"
    } else if value.eq_ignore_ascii_case("industrial") {
        "I"
    } else {
        "·"
    }
}

fn short_specialty(value: &str) -> &'static str {
    let lower = value.to_ascii_lowercase();
    if lower.contains("biotic") || lower.contains("green") {
        "G"
    } else if lower.contains("cybernetic") || lower.contains("yellow") {
        "Y"
    } else if lower.contains("propulsion") || lower.contains("blue") {
        "B"
    } else if lower.contains("warfare") || lower.contains("red") {
        "R"
    } else {
        "T"
    }
}

fn item_section(ui: &mut egui::Ui, icon: &str, title: &str, items: Vec<String>, color: Color32) {
    ui.horizontal(|ui| {
        ui.colored_label(color, icon);
        ui.strong(format!("{title} · {}", items.len()));
    });
    if items.is_empty() {
        ui.weak("None");
    } else {
        ui.horizontal_wrapped(|ui| {
            for item in items {
                ui.label(
                    egui::RichText::new(item)
                        .background_color(color.gamma_multiply(0.28))
                        .color(Color32::WHITE),
                );
            }
        });
    }
}

fn stat_badge(ui: &mut egui::Ui, icon: &str, label: &str, value: impl std::fmt::Display) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong(icon);
            ui.label(format!("{label} {value}"));
        });
    });
}

fn unit_base(content: &ContentStore, unit: &Unit) -> String {
    ti4_content::units::unit_type(content, unit.type_id.as_str(), FULL).map_or_else(
        || unit.type_id.to_string(),
        |kind| kind.base_type().to_owned(),
    )
}

fn polygon(center: Pos2, radius: f32, sides: usize, offset: f32) -> Vec<Pos2> {
    (0..sides)
        .map(|index| {
            let angle = offset + std::f32::consts::TAU * index as f32 / sides as f32;
            center + Vec2::angled(angle) * radius
        })
        .collect()
}

fn draw_unit_symbol(
    painter: &egui::Painter,
    center: Pos2,
    base: &str,
    color: Color32,
    count: usize,
    damaged: bool,
    galvanized: bool,
    scale: f32,
) {
    let size = 5.5 * scale.max(0.75);
    let dark = Color32::from_rgb(12, 18, 27);
    let stroke = Stroke::new(1.1 * scale.max(0.8), dark);
    match base {
        "fighter" => {
            painter.add(Shape::convex_polygon(
                polygon(center, size, 3, -std::f32::consts::FRAC_PI_2),
                color,
                stroke,
            ));
        }
        "destroyer" => {
            painter.add(Shape::convex_polygon(
                polygon(center, size, 4, std::f32::consts::FRAC_PI_4),
                color,
                stroke,
            ));
        }
        "cruiser" => {
            painter.add(Shape::convex_polygon(
                polygon(center, size, 4, 0.0),
                color,
                stroke,
            ));
        }
        "carrier" => {
            painter.add(Shape::convex_polygon(
                vec![
                    center + Vec2::new(-size * 1.3, -size * 0.6),
                    center + Vec2::new(size * 1.3, -size * 0.6),
                    center + Vec2::new(size, size * 0.6),
                    center + Vec2::new(-size, size * 0.6),
                ],
                color,
                stroke,
            ));
        }
        "dreadnought" => {
            painter.add(Shape::convex_polygon(
                polygon(center, size, 6, 0.0),
                color,
                stroke,
            ));
        }
        "flagship" => {
            painter.circle_filled(center, size, color);
            painter.circle_stroke(center, size, stroke);
            painter.line_segment(
                [
                    center - Vec2::splat(size * 0.7),
                    center + Vec2::splat(size * 0.7),
                ],
                stroke,
            );
            painter.line_segment(
                [
                    center + Vec2::new(-size * 0.7, size * 0.7),
                    center + Vec2::new(size * 0.7, -size * 0.7),
                ],
                stroke,
            );
        }
        "war_sun" => {
            painter.circle_filled(center, size * 1.15, color);
            painter.circle_stroke(center, size * 1.15, Stroke::new(1.5, Color32::WHITE));
        }
        "infantry" => {
            painter.circle_filled(center, size * 0.75, color);
            painter.circle_stroke(center, size * 0.75, stroke);
        }
        "mech" => {
            painter.add(Shape::convex_polygon(
                polygon(center, size, 5, -std::f32::consts::FRAC_PI_2),
                color,
                stroke,
            ));
        }
        "pds" => {
            painter.rect_filled(
                egui::Rect::from_center_size(center, Vec2::splat(size * 1.4)),
                1.0,
                color,
            );
            painter.line_segment(
                [
                    center + Vec2::new(0.0, -size),
                    center + Vec2::new(0.0, size),
                ],
                stroke,
            );
        }
        "space_dock" => {
            painter.add(Shape::convex_polygon(
                vec![
                    center + Vec2::new(-size, size),
                    center + Vec2::new(size, size),
                    center + Vec2::new(size * 0.65, -size),
                    center + Vec2::new(-size * 0.65, -size),
                ],
                color,
                stroke,
            ));
        }
        _ => {
            painter.circle_filled(center, size, color);
            painter.circle_stroke(center, size, stroke);
        }
    }
    if damaged {
        painter.line_segment(
            [
                center + Vec2::new(-size, size),
                center + Vec2::new(size, -size),
            ],
            Stroke::new(1.6, Color32::RED),
        );
    }
    if galvanized {
        painter.circle_stroke(center, size * 1.45, Stroke::new(1.2, Color32::YELLOW));
    }
    let abbreviation: String = base.chars().take(2).collect();
    painter.text(
        center + Vec2::new(0.0, size + 4.0 * scale),
        Align2::CENTER_TOP,
        format!("{abbreviation}×{count}"),
        FontId::monospace(5.8 * scale.max(0.9)),
        Color32::WHITE,
    );
}

#[derive(Clone, Debug)]
enum RunTarget {
    Count { unit: AdvanceUnit, remaining: usize },
    Round(u32),
    End,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct ReviewerSettings {
    checkpoint: String,
    map_pool: String,
    profile_table: ProfileTable,
    last_review: Option<String>,
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
    run_count: String,
    run_unit: AdvanceUnit,
    live: Option<LiveReview>,
    replay: Option<ReviewSession>,
    viewed: usize,
    run_target: Option<RunTarget>,
    command_steps: usize,
    autosave: Option<PathBuf>,
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
            seed: "42".to_owned(),
            rotation: 0,
            table: settings.profile_table,
            run_count: "10".to_owned(),
            run_unit: AdvanceUnit::Step,
            live: None,
            replay: None,
            viewed: 0,
            run_target: None,
            command_steps: 0,
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

    fn latest_index(&self) -> usize {
        self.session()
            .map_or(0, |session| session.frames.len().saturating_sub(1))
    }

    fn settings(&self) -> ReviewerSettings {
        ReviewerSettings {
            checkpoint: self.checkpoint.trim().to_owned(),
            map_pool: self.map_pool.trim().to_owned(),
            profile_table: self.table,
            last_review: self
                .last_review
                .as_ref()
                .map(|path| path.display().to_string()),
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
        };
        match LiveReview::start(&config) {
            Ok(live) => {
                self.autosave = Some(PathBuf::from("out/reviews").join(format!(
                    "autosave-{seed}-rotation{}-{}.ti4review.json",
                    self.rotation,
                    match self.table {
                        ProfileTable::Learner => "learner",
                        ProfileTable::Accepted => "accepted",
                    }
                )));
                self.live = Some(live);
                self.replay = None;
                self.viewed = 0;
                self.run_target = None;
                "Starting table loaded; no engine step has run.".clone_into(&mut self.status);
                self.autosave_now();
                if let Some(path) = &self.autosave {
                    self.last_review = Some(path.clone());
                }
                self.persist_settings();
            }
            Err(error) => self.status = format!("Load failed: {error}"),
        }
    }

    fn autosave_now(&mut self) {
        let Some(path) = self.autosave.clone() else {
            return;
        };
        let Some(session) = self.session() else {
            return;
        };
        if let Err(error) = save_session(&path, session) {
            self.status = format!("Autosave failed: {error}");
        }
    }

    fn save_as(&mut self) {
        let Some(session) = self.session() else {
            "Nothing to save.".clone_into(&mut self.status);
            return;
        };
        let Some(path) = rfd::FileDialog::new()
            .add_filter("TI4 review", &["json"])
            .set_file_name("game.ti4review.json")
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
            .add_filter("TI4 review", &["json"])
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
            if self.command_steps % 1024 < STEPS_PER_UI_FRAME {
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
                ui.label("Rotation");
                egui::ComboBox::from_id_salt("rotation")
                    .selected_text(self.rotation.to_string())
                    .show_ui(ui, |ui| {
                        for rotation in 0..6 {
                            ui.selectable_value(&mut self.rotation, rotation, rotation.to_string());
                        }
                    });
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
                    .add_enabled(can_run, egui::Button::new("Next action"))
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
                        ui.selectable_value(&mut self.run_unit, AdvanceUnit::Action, "Actions");
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
                    self.status = format!("Running to round {round}…");
                }
                if ui
                    .add_enabled(can_run, egui::Button::new("End game"))
                    .clicked()
                {
                    self.run_target = Some(RunTarget::End);
                    self.command_steps = 0;
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

    fn player_panel(root: &mut egui::Ui, session: &ReviewSession, frame: &ReviewFrame) {
        egui::Panel::left("players")
            .resizable(true)
            .default_size(340.0)
            .show(root, |ui| {
                ui.heading("Omniscient players");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let content = ContentStore::embedded();
                    for player in &frame.state.players {
                        let color = player_color(&player.id);
                        egui::CollapsingHeader::new(format!(
                            "● {} · {} · {} VP",
                            player.id, player.faction, player.victory_points
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.colored_label(
                                color,
                                format!(
                                    "{} · {}",
                                    player.faction,
                                    if player.passed { "PASSED" } else { "ACTIVE" }
                                ),
                            );
                            ui.horizontal_wrapped(|ui| {
                                stat_badge(ui, "★", "VP", player.victory_points);
                                stat_badge(ui, "◆", "TG", player.trade_goods);
                                stat_badge(ui, "◇", "Com", player.commodities);
                            });
                            ui.horizontal_wrapped(|ui| {
                                stat_badge(ui, "▲", "Tactic", player.tactic_tokens);
                                stat_badge(ui, "⬟", "Fleet", player.fleet_tokens);
                                stat_badge(ui, "●", "Strategy", player.strategic_tokens);
                            });

                            let strategy = player
                                .strategy_cards
                                .iter()
                                .map(|card| {
                                    if player.exhausted_strategy_cards.contains(card) {
                                        format!("{card} · used")
                                    } else {
                                        card.to_string()
                                    }
                                })
                                .collect();
                            item_section(ui, "◆", "Strategy cards", strategy, color);

                            let controlled_planets: Vec<String> = session
                                .board
                                .iter()
                                .flat_map(|tile| {
                                    let state = frame.state.board.get(&SystemId::new(&tile.system));
                                    tile.planets
                                        .iter()
                                        .filter(move |planet| {
                                            state.and_then(|state| {
                                                state.planet_control.get(planet.id.as_str())
                                            }) == Some(&player.id)
                                        })
                                        .map(|planet| {
                                            let exhausted = frame
                                                .state
                                                .exhausted_planets
                                                .contains(planet.id.as_str());
                                            format!(
                                                "{} {}/{}{}",
                                                planet.label,
                                                planet.resources,
                                                planet.influence,
                                                if exhausted { " · exhausted" } else { "" }
                                            )
                                        })
                                })
                                .collect();
                            item_section(ui, "●", "Planets", controlled_planets, color);

                            let mut unit_counts: BTreeMap<String, usize> = BTreeMap::new();
                            for state in frame.state.board.values() {
                                for unit in state.units.iter().chain(
                                    state.planet_units.values().flat_map(|units| units.iter()),
                                ) {
                                    if unit.owner == player.id {
                                        *unit_counts
                                            .entry(unit_base(content, unit))
                                            .or_default() += 1;
                                    }
                                }
                            }
                            item_section(
                                ui,
                                "⬡",
                                "Units on board",
                                unit_counts
                                    .into_iter()
                                    .map(|(kind, count)| format!("{kind} ×{count}"))
                                    .collect(),
                                color,
                            );

                            item_section(
                                ui,
                                "⚙",
                                "Technologies",
                                player
                                    .technologies
                                    .iter()
                                    .map(|technology| {
                                        if player.exhausted_technologies.contains(technology) {
                                            format!("{technology} · exhausted")
                                        } else {
                                            technology.to_string()
                                        }
                                    })
                                    .collect(),
                                color,
                            );
                            item_section(
                                ui,
                                "✓",
                                "Scored objectives",
                                frame
                                    .state
                                    .scored_objectives
                                    .get(&player.id)
                                    .into_iter()
                                    .flatten()
                                    .map(ToString::to_string)
                                    .collect(),
                                color,
                            );
                            item_section(
                                ui,
                                "?",
                                "Secret objectives",
                                player
                                    .secret_objectives
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect(),
                                color,
                            );
                            item_section(
                                ui,
                                "▣",
                                "Action cards",
                                player
                                    .action_cards
                                    .iter()
                                    .map(ToString::to_string)
                                    .collect(),
                                color,
                            );
                            item_section(
                                ui,
                                "✦",
                                "Relics and fragments",
                                player
                                    .relics
                                    .iter()
                                    .map(ToString::to_string)
                                    .chain(player.relic_fragments.iter().map(
                                        |(trait_name, count)| {
                                            format!("{trait_name} fragment ×{count}")
                                        },
                                    ))
                                    .collect(),
                                color,
                            );
                            item_section(
                                ui,
                                "♟",
                                "Leaders",
                                player
                                    .leaders
                                    .iter()
                                    .map(|(leader, status)| format!("{leader} · {status:?}"))
                                    .collect(),
                                color,
                            );
                            item_section(ui, "⌁", "Plots", player.plots.clone(), color);
                            if let Some(breakthrough) = &player.breakthrough {
                                item_section(
                                    ui,
                                    "⚡",
                                    "Breakthrough",
                                    vec![breakthrough.to_string()],
                                    color,
                                );
                            }
                            ui.collapsing("Complete player JSON", |ui| {
                                let text = serde_json::to_string_pretty(player)
                                    .unwrap_or_else(|error| error.to_string());
                                ui.monospace(text);
                            });
                        });
                    }
                });
            });
    }

    fn decision_panel(&mut self, root: &mut egui::Ui, frame: &ReviewFrame) {
        egui::Panel::right("decision")
            .resizable(true)
            .default_size(410.0)
            .show(root, |ui| {
                ui.heading("Step and policy detail");
                ui.label(format!(
                    "Step {} · decision {} · action {}",
                    frame.engine_step, frame.decision_count, frame.action_count
                ));
                ui.label(format!(
                    "Round {} · {:?} · active {}",
                    frame.round,
                    frame.phase,
                    frame.active.as_deref().unwrap_or("—")
                ));
                if let Some(error) = &frame.error {
                    ui.colored_label(Color32::LIGHT_RED, error);
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if frame.decisions.is_empty() {
                        ui.label("This engine step resolved no policy choice.");
                    } else {
                        for (decision_index, decision) in frame.decisions.iter().enumerate() {
                            ui.separator();
                            ui.strong(format!(
                                "Decision {} · {} · {}",
                                decision.sequence, decision.player, decision.faction
                            ));
                            ui.label(&decision.prompt);
                            ui.label(format!(
                                "{} → {} · temperature {:?} · chosen {}",
                                decision.requested_head,
                                decision.resolved_head,
                                decision.temperature,
                                decision.chosen.as_deref().unwrap_or("ERROR")
                            ));
                            for option in &decision.options {
                                let selected =
                                    decision.chosen.as_deref() == Some(option.id.as_str());
                                egui::CollapsingHeader::new(format!(
                                    "{}{} · score {} · p {}",
                                    if selected { "✓ " } else { "" },
                                    option.label,
                                    option
                                        .score
                                        .map_or_else(|| "—".to_owned(), |v| format!("{v:.5}")),
                                    option
                                        .probability
                                        .map_or_else(|| "—".to_owned(), |v| format!("{v:.5}"))
                                ))
                                .default_open(selected)
                                .show(ui, |ui| {
                                    ui.label(format!("id={} kind={}", option.id, option.kind));
                                    egui::Grid::new(format!(
                                        "features-{}-{decision_index}-{}",
                                        frame.index, option.id
                                    ))
                                    .striped(true)
                                    .show(ui, |ui| {
                                        ui.strong("feature");
                                        ui.strong("value");
                                        ui.strong("weight");
                                        ui.strong("contribution");
                                        ui.end_row();
                                        for feature in &option.features {
                                            ui.label(&feature.name);
                                            ui.label(format!("{:.3}", feature.value));
                                            ui.label(feature.weight.map_or_else(
                                                || "nonlinear".to_owned(),
                                                |value| format!("{value:.5}"),
                                            ));
                                            ui.label(feature.contribution.map_or_else(
                                                || "—".to_owned(),
                                                |value| format!("{value:.5}"),
                                            ));
                                            ui.end_row();
                                        }
                                    });
                                });
                            }
                        }
                    }
                    ui.separator();
                    ui.strong("New engine events");
                    if frame.new_events.is_empty() {
                        ui.label("—");
                    } else {
                        for event in &frame.new_events {
                            ui.monospace(event);
                        }
                    }
                    if let Some(tile) = &self.selected_tile {
                        ui.separator();
                        ui.strong(format!("Selected system {tile}"));
                        if let Some(state) = frame.state.board.get(&SystemId::new(tile)) {
                            ui.monospace(
                                serde_json::to_string_pretty(state)
                                    .unwrap_or_else(|error| error.to_string()),
                            );
                        }
                    }
                });
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
                        format!("● {} {}", player.id, player.faction),
                    );
                }
            });
            ui.small(
                "Planet circle: resources/influence · traits C cultural, H hazardous, I industrial · tech B propulsion, G biotic, R warfare, Y cybernetic · ★ legendary. Unit labels show class×count; red slash = damaged, yellow ring = galvanized.",
            );
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, Sense::click());
            let center = response.rect.center();
            let scale = (available.x / 1150.0)
                .min(available.y / 900.0)
                .clamp(0.45, 1.2);
            let radius = 58.0 * scale;
            for tile in &session.board {
                let x = center.x + 126.0 * scale * (tile.q as f32 + tile.r as f32 / 2.0);
                let y = center.y + 108.0 * scale * tile.r as f32;
                let point = Pos2::new(x, y);
                let points: Vec<Pos2> = (0..6)
                    .map(|corner| {
                        let angle = std::f32::consts::TAU * corner as f32 / 6.0;
                        point + Vec2::new(radius * angle.cos(), radius * angle.sin())
                    })
                    .collect();
                let selected = self.selected_tile.as_deref() == Some(tile.system.as_str());
                let fill = if tile.hyperlane {
                    Color32::from_rgb(55, 39, 91)
                } else if selected {
                    Color32::from_rgb(48, 91, 116)
                } else {
                    Color32::from_rgb(23, 44, 69)
                };
                painter.add(Shape::convex_polygon(
                    points.clone(),
                    fill,
                    Stroke::new(1.4, Color32::from_rgb(112, 152, 189)),
                ));
                let system_state = frame.state.board.get(&SystemId::new(&tile.system));
                let mut space_owners: Vec<&PlayerId> = system_state
                    .into_iter()
                    .flat_map(|state| state.units.iter())
                    .filter(|unit| {
                        ti4_content::units::unit_type(content, unit.type_id.as_str(), FULL)
                            .is_some_and(|kind| kind.is_ship())
                    })
                    .map(|unit| &unit.owner)
                    .collect();
                space_owners.sort();
                space_owners.dedup();
                if let [owner] = space_owners.as_slice() {
                    painter.add(Shape::closed_line(
                        points.clone(),
                        Stroke::new(5.0 * scale.max(0.75), player_color(owner)),
                    ));
                }
                if selected {
                    let inner: Vec<Pos2> = points
                        .iter()
                        .map(|corner| point + (*corner - point) * 0.91)
                        .collect();
                    painter.add(Shape::closed_line(inner, Stroke::new(2.5, Color32::WHITE)));
                }
                painter.text(
                    point + Vec2::new(0.0, -45.0 * scale),
                    Align2::CENTER_CENTER,
                    &tile.label,
                    FontId::proportional(9.5 * scale.max(0.85)),
                    Color32::WHITE,
                );

                if let Some(state) = system_state {
                    let mut groups: BTreeMap<(PlayerId, String, bool, bool), usize> =
                        BTreeMap::new();
                    for unit in &state.units {
                        *groups
                            .entry((
                                unit.owner.clone(),
                                unit_base(content, unit),
                                unit.sustained_damage,
                                unit.galvanized,
                            ))
                            .or_default() += 1;
                    }
                    for (index, ((owner, base, damaged, galvanized), count)) in
                        groups.iter().enumerate()
                    {
                        let column = index % 5;
                        let row = index / 5;
                        let unit_center = point
                            + Vec2::new(
                                (column as f32 - 2.0) * 18.0 * scale,
                                (-23.0 + row as f32 * 20.0) * scale,
                            );
                        draw_unit_symbol(
                            &painter,
                            unit_center,
                            base,
                            player_color(owner),
                            *count,
                            *damaged,
                            *galvanized,
                            scale,
                        );
                    }
                    for (index, owner) in state.command_tokens.iter().enumerate() {
                        painter.circle_filled(
                            point + Vec2::new((-42.0 + index as f32 * 10.0) * scale, 42.0 * scale),
                            3.5 * scale,
                            player_color(owner),
                        );
                        painter.circle_stroke(
                            point + Vec2::new((-42.0 + index as f32 * 10.0) * scale, 42.0 * scale),
                            3.5 * scale,
                            Stroke::new(1.0, Color32::WHITE),
                        );
                    }
                }

                let planet_count = tile.planets.len();
                for (planet_index, planet) in tile.planets.iter().enumerate() {
                    let offset_x = match planet_count {
                        1 => 0.0,
                        2 => (planet_index as f32 * 2.0 - 1.0) * 22.0,
                        _ => (planet_index as f32 - (planet_count - 1) as f32 / 2.0) * 19.0,
                    };
                    let planet_center = point
                        + Vec2::new(
                            offset_x * scale,
                            if planet_count > 2 { 24.0 } else { 27.0 } * scale,
                        );
                    let owner = system_state.and_then(|state| {
                        state
                            .planet_control
                            .get(&ti4_model::id::PlanetId::new(&planet.id))
                    });
                    let planet_color =
                        owner.map_or(Color32::from_rgb(91, 96, 105), player_color);
                    let planet_radius = 14.5 * scale.max(0.72);
                    painter.circle_filled(
                        planet_center,
                        planet_radius,
                        planet_color.gamma_multiply(0.72),
                    );
                    painter.circle_stroke(
                        planet_center,
                        planet_radius,
                        Stroke::new(2.0 * scale.max(0.8), planet_color),
                    );
                    let trait_label = planet
                        .traits
                        .iter()
                        .map(|value| short_trait(value))
                        .collect::<Vec<_>>()
                        .join("");
                    let tech_label = planet
                        .tech_specialties
                        .iter()
                        .map(|value| short_specialty(value))
                        .collect::<Vec<_>>()
                        .join("");
                    painter.text(
                        planet_center + Vec2::new(0.0, -5.0 * scale),
                        Align2::CENTER_CENTER,
                        format!("{}/{}", planet.resources, planet.influence),
                        FontId::monospace(7.2 * scale.max(0.85)),
                        Color32::WHITE,
                    );
                    painter.text(
                        planet_center + Vec2::new(0.0, 5.0 * scale),
                        Align2::CENTER_CENTER,
                        format!(
                            "{}{}{}",
                            trait_label,
                            if trait_label.is_empty() || tech_label.is_empty() {
                                ""
                            } else {
                                "·"
                            },
                            tech_label
                        ),
                        FontId::monospace(6.2 * scale.max(0.9)),
                        Color32::WHITE,
                    );
                    painter.text(
                        planet_center + Vec2::new(0.0, planet_radius + 1.0),
                        Align2::CENTER_TOP,
                        format!(
                            "{}{}",
                            if planet.legendary { "★" } else { "" },
                            planet.label
                        ),
                        FontId::proportional(6.4 * scale.max(0.9)),
                        Color32::LIGHT_GRAY,
                    );

                    if let Some(state) = system_state
                        && let Some(units) = state
                            .planet_units
                            .get(&ti4_model::id::PlanetId::new(&planet.id))
                    {
                        let mut ground_groups: BTreeMap<(PlayerId, String, bool, bool), usize> =
                            BTreeMap::new();
                        for unit in units {
                            *ground_groups
                                .entry((
                                    unit.owner.clone(),
                                    unit_base(content, unit),
                                    unit.sustained_damage,
                                    unit.galvanized,
                                ))
                                .or_default() += 1;
                        }
                        for (unit_index, ((unit_owner, base, damaged, galvanized), count)) in
                            ground_groups.iter().enumerate()
                        {
                            draw_unit_symbol(
                                &painter,
                                planet_center
                                    + Vec2::new(
                                        (-8.0 + unit_index as f32 * 12.0) * scale,
                                        -planet_radius * 0.85,
                                    ),
                                base,
                                player_color(unit_owner),
                                *count,
                                *damaged,
                                *galvanized,
                                scale * 0.72,
                            );
                        }
                    }
                }
                if response.clicked()
                    && response
                        .interact_pointer_pos()
                        .is_some_and(|cursor| cursor.distance(point) <= radius)
                {
                    self.selected_tile = Some(tile.system.clone());
                }
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
        let Some(session) = self.session().cloned() else {
            egui::CentralPanel::default().show(root, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.heading("Load a real learned-policy starting table to begin.");
                });
            });
            return;
        };
        self.viewed = self.viewed.min(session.frames.len().saturating_sub(1));
        let frame = session.frames[self.viewed].clone();
        Self::player_panel(root, &session, &frame);
        self.decision_panel(root, &frame);
        self.board(root, &session, &frame);
        if self.run_target.is_some() {
            root.ctx().request_repaint();
        }
    }
}

fn is_review(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".ti4review.json"))
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
            last_review: Some("out/reviews/autosave.ti4review.json".to_owned()),
        };
        let bytes = serde_json::to_vec(&settings).unwrap();
        let restored: ReviewerSettings = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(restored.checkpoint, settings.checkpoint);
        assert_eq!(restored.map_pool, settings.map_pool);
        assert_eq!(restored.profile_table, settings.profile_table);
        assert_eq!(restored.last_review, settings.last_review);
    }

    #[test]
    fn previous_game_discovery_accepts_only_review_sessions() {
        assert!(is_review(Path::new("game.ti4review.json")));
        assert!(!is_review(Path::new("reviewer-settings.json")));
        assert!(!is_review(Path::new("game.html")));
    }
}
