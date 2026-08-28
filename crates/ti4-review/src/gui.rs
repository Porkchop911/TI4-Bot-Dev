//! Native egui front end for live and saved reviews.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Sense, Shape, Stroke, Vec2};
use ti4_model::id::SystemId;

use crate::{
    AdvanceUnit, LiveReview, MAX_COMMAND_STEPS, ProfileTable, ReviewFrame, ReviewSession,
    SessionOutcome, SimulationConfig, export_html, load_session, save_session,
};

const STEPS_PER_UI_FRAME: usize = 128;

#[derive(Clone, Debug)]
enum RunTarget {
    Count { unit: AdvanceUnit, remaining: usize },
    Round(u32),
    End,
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
    status: String,
    selected_tile: Option<String>,
}

impl ReviewApp {
    fn new(context: &eframe::CreationContext<'_>) -> Self {
        context.egui_ctx.set_visuals(egui::Visuals::dark());
        Self {
            checkpoint: String::new(),
            map_pool: String::new(),
            seed: "42".to_owned(),
            rotation: 0,
            table: ProfileTable::Learner,
            run_count: "10".to_owned(),
            run_unit: AdvanceUnit::Step,
            live: None,
            replay: None,
            viewed: 0,
            run_target: None,
            command_steps: 0,
            autosave: None,
            status: "Choose a checkpoint and map pool, then load the starting table.".to_owned(),
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
            Ok(()) => self.status = format!("Saved {}", path.display()),
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
            Ok(session) => {
                self.viewed = 0;
                self.live = None;
                self.replay = Some(session);
                self.run_target = None;
                self.autosave = None;
                self.status = format!("Opened {} in view-only mode", path.display());
            }
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
                egui::ComboBox::from_id_salt("profile_table")
                    .selected_text(self.table.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.table, ProfileTable::Learner, "Learner");
                        ui.selectable_value(
                            &mut self.table,
                            ProfileTable::Accepted,
                            "Accepted champion",
                        );
                    });
                if ui.button("Load starting table").clicked() {
                    self.load_start();
                }
                ui.separator();
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

    fn player_panel(root: &mut egui::Ui, frame: &ReviewFrame) {
        egui::Panel::left("players")
            .resizable(true)
            .default_size(310.0)
            .show(root, |ui| {
                ui.heading("Omniscient players");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for player in &frame.state.players {
                        egui::CollapsingHeader::new(format!(
                            "{} · {} · {} VP",
                            player.id, player.faction, player.victory_points
                        ))
                        .default_open(true)
                        .show(ui, |ui| {
                            ui.label(format!(
                                "TG {} · commodities {} · tokens T/F/S {}/{}/{} · {}",
                                player.trade_goods,
                                player.commodities,
                                player.tactic_tokens,
                                player.fleet_tokens,
                                player.strategic_tokens,
                                if player.passed { "passed" } else { "active" }
                            ));
                            ui.label(format!("Strategy: {:?}", player.strategy_cards));
                            ui.label(format!("Tech: {:?}", player.technologies));
                            ui.label(format!("Secrets: {:?}", player.secret_objectives));
                            ui.label(format!("Action cards: {:?}", player.action_cards));
                            ui.label(format!("Relics: {:?}", player.relics));
                            ui.label(format!("Leaders: {:?}", player.leaders));
                            ui.label(format!("Fragments: {:?}", player.relic_fragments));
                            ui.label(format!("Plots: {:?}", player.plots));
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
                                            ui.label(format!("{:.5}", feature.weight));
                                            ui.label(format!("{:.5}", feature.contribution));
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
                    Stroke::new(
                        if selected { 3.0 } else { 1.5 },
                        Color32::from_rgb(112, 152, 189),
                    ),
                ));
                painter.text(
                    point + Vec2::new(0.0, -22.0 * scale),
                    Align2::CENTER_CENTER,
                    &tile.label,
                    FontId::proportional(11.0 * scale.max(0.8)),
                    Color32::WHITE,
                );
                let system_state = frame.state.board.get(&SystemId::new(&tile.system));
                let unit_count = system_state.map_or(0, |state| state.units.len());
                let owners = system_state.map_or_else(String::new, |state| {
                    let mut owners: Vec<String> = state
                        .units
                        .iter()
                        .map(|unit| unit.owner.to_string())
                        .collect();
                    owners.sort();
                    owners.dedup();
                    owners.join(",")
                });
                painter.text(
                    point,
                    Align2::CENTER_CENTER,
                    format!("{unit_count} space units {owners}"),
                    FontId::proportional(9.0 * scale.max(0.8)),
                    Color32::LIGHT_BLUE,
                );
                painter.text(
                    point + Vec2::new(0.0, 20.0 * scale),
                    Align2::CENTER_CENTER,
                    tile.planets
                        .iter()
                        .map(|planet| planet.label.as_str())
                        .collect::<Vec<_>>()
                        .join(" · "),
                    FontId::proportional(8.5 * scale.max(0.8)),
                    Color32::LIGHT_GRAY,
                );
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
        Self::player_panel(root, &frame);
        self.decision_panel(root, &frame);
        self.board(root, &session, &frame);
        if self.run_target.is_some() {
            root.ctx().request_repaint();
        }
    }
}

#[allow(dead_code)]
fn _is_review(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".ti4review.json"))
}
