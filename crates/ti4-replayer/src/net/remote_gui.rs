//! The window a remote player sees: the reviewer's board and sheets, one seat's view of them, and
//! that seat's choices.
//!
//! It holds no game. Everything it draws came from the host already redacted for this seat, so the
//! players sheet shows other hands as counts and the step sheet lists only this seat's decisions.

use std::sync::MutexGuard;
use std::time::Duration;

use eframe::egui::{self, Color32, Sense};
use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_review::panels;
use ti4_review::view::{self, BoardLayout, PANEL_FILL, PANEL_TEXT, board_view, draw_board};

use super::DEFAULT_PORT;
use super::client::{JoinRequest, NetClient, Remote};
use super::protocol::Holder;

/// How often an idle window looks for news from the host.
const REFRESH: Duration = Duration::from_millis(100);

/// Open the join window. With a complete request it connects straight away.
///
/// # Errors
/// Propagates eframe's own failure to open a window.
pub fn run(request: JoinRequest) -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("TI4 online table")
            .with_inner_size([1600.0, 1000.0]),
        ..Default::default()
    };
    eframe::run_native(
        "TI4 online table",
        options,
        Box::new(move |_| {
            let mut app = RemoteTable::new(request);
            if !app.form.address.is_empty() && !app.form.code.is_empty() {
                app.connect();
            }
            Ok(Box::new(app))
        }),
    )
}

struct Form {
    address: String,
    code: String,
    seat: String,
    name: String,
}

pub struct RemoteTable {
    form: Form,
    client: Option<NetClient>,
    /// Kept across a disconnect, so Reconnect takes the same seat back.
    resume: Option<(PlayerId, String)>,
    status: String,
    /// The frame on screen; `None` follows the newest.
    viewed: Option<usize>,
    selected_tile: Option<String>,
}

impl RemoteTable {
    fn new(request: JoinRequest) -> Self {
        Self {
            form: Form {
                address: if request.address.is_empty() {
                    format!("127.0.0.1:{DEFAULT_PORT}")
                } else {
                    request.address
                },
                code: request.code,
                seat: request
                    .seat
                    .map(|seat| seat.to_string())
                    .unwrap_or_default(),
                name: request.name,
            },
            client: None,
            resume: None,
            status: String::new(),
            viewed: None,
            selected_tile: None,
        }
    }

    fn connect(&mut self) {
        let seat = self.form.seat.trim();
        let wanted = (!seat.is_empty()).then(|| PlayerId::new(seat));
        let resume = self
            .resume
            .as_ref()
            .filter(|(held, _)| wanted.as_ref().is_none_or(|wanted| wanted == held))
            .map(|(held, token)| (held.clone(), token.clone()));
        let request = JoinRequest {
            address: self.form.address.trim().to_owned(),
            code: self.form.code.trim().to_owned(),
            seat: resume.as_ref().map(|(seat, _)| seat.clone()).or(wanted),
            name: self.form.name.clone(),
            resume: resume.map(|(_, token)| token),
        };
        match NetClient::join(&request) {
            Ok(client) => {
                let seat = client.remote().seat.clone();
                self.status = format!(
                    "Seated at {} as {}.",
                    seat.as_ref().map_or("?", PlayerId::as_str),
                    self.form.name
                );
                self.client = Some(client);
                self.viewed = None;
            }
            Err(error) => self.status = format!("Could not join: {error}"),
        }
    }

    fn join_form(&mut self, root: &mut egui::Ui) {
        egui::CentralPanel::default().show(root, |ui| {
            ui.heading("Join an online table");
            ui.label(
                "The host starts a table in the replayer and presses Host online. It shows an address and a code; enter them here.",
            );
            egui::Grid::new("join").num_columns(2).show(ui, |ui| {
                ui.label("Host address");
                ui.text_edit_singleline(&mut self.form.address);
                ui.end_row();
                ui.label("Join code");
                ui.text_edit_singleline(&mut self.form.code);
                ui.end_row();
                ui.label("Seat (blank = any free)");
                ui.text_edit_singleline(&mut self.form.seat);
                ui.end_row();
                ui.label("Your name");
                ui.text_edit_singleline(&mut self.form.name);
                ui.end_row();
            });
            let label = if self.resume.is_some() { "Reconnect" } else { "Join" };
            if ui.button(label).clicked() {
                self.connect();
            }
            ui.label(&self.status);
        });
    }
}

impl eframe::App for RemoteTable {
    fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        let closed = self.client.as_ref().and_then(|client| {
            let remote = client.remote();
            remote
                .closed
                .clone()
                .map(|reason| (reason, remote.seat.clone().zip(remote.resume.clone())))
        });
        if let Some((reason, resume)) = closed {
            self.client = None;
            if resume.is_some() {
                self.resume = resume;
            }
            self.status = format!("Disconnected: {reason}");
        }
        context.request_repaint_after(REFRESH);
    }

    fn ui(&mut self, root: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let Some(client) = self.client.take() else {
            self.join_form(root);
            return;
        };
        let action = {
            let remote = client.remote();
            top_bar(root, &remote, &self.status);
            self.table(root, &remote)
        };
        match action {
            Some(Action::Submit(drawn, option)) => {
                if let Err(error) = client.submit(&drawn, option) {
                    self.status = format!("The answer did not reach the host: {error}");
                }
            }
            Some(Action::Delegate(offer)) => {
                if let Err(error) = client.delegate(offer) {
                    self.status = format!("The request did not reach the host: {error}");
                }
            }
            None => {}
        }
        self.client = Some(client);
    }
}

/// A click on the panel, bound to the occurrence that was drawn when it was made.
enum Action {
    Submit(Box<crate::control::PendingManualChoice>, String),
    Delegate(crate::control::OfferId),
}

fn top_bar(root: &mut egui::Ui, remote: &MutexGuard<'_, Remote>, status: &str) {
    egui::Panel::top("online").show(root, |ui| {
        ui.horizontal_wrapped(|ui| {
            let me = remote.seat.as_ref();
            ui.strong(format!("You are {}", me.map_or("?", PlayerId::as_str)));
            ui.separator();
            ui.label(format!("host: {}", remote.state));
            if let Some(waiting) = &remote.waiting_on {
                ui.separator();
                if Some(waiting) == me {
                    ui.colored_label(Color32::from_rgb(90, 180, 110), "your move");
                } else {
                    ui.label(format!("waiting on {waiting}"));
                }
            }
            ui.separator();
            for seat in &remote.seats {
                let who = match &seat.holder {
                    Holder::Bot => "bot".to_owned(),
                    Holder::Host => "host".to_owned(),
                    Holder::Remote { name, connected } => {
                        if *connected {
                            name.clone()
                        } else {
                            format!("{name} (away)")
                        }
                    }
                };
                ui.colored_label(
                    view::player_color(&seat.seat),
                    format!("● {} {who}", seat.seat),
                );
            }
        });
        if let Some((accepted, message)) = &remote.answer {
            let colour = if *accepted {
                Color32::from_rgb(90, 180, 110)
            } else {
                Color32::from_rgb(220, 120, 60)
            };
            ui.colored_label(colour, message);
        }
        ui.label(status);
    });
}

impl RemoteTable {
    fn table(&mut self, root: &mut egui::Ui, remote: &MutexGuard<'_, Remote>) -> Option<Action> {
        let (Some(session), false) = (remote.header.as_ref(), remote.frames.is_empty()) else {
            egui::CentralPanel::default().show(root, |ui| {
                ui.centered_and_justified(|ui| ui.heading("Waiting for the host's table…"));
            });
            return None;
        };
        let frames = remote.frames.as_slice();
        let last = frames.len() - 1;
        let mut position = self.viewed.unwrap_or(last).min(last);
        egui::Panel::bottom("timeline").show(root, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(egui::Slider::new(&mut position, 0..=last).text("frame"))
                    .changed()
                {
                    self.viewed = (position != last).then_some(position);
                }
                if ui.button("Live tip").clicked() {
                    self.viewed = None;
                    position = last;
                }
                let frame = &frames[position];
                ui.label(format!(
                    "step {} · round {} · {:?}{}",
                    frame.engine_step,
                    frame.round,
                    frame.phase,
                    if frame.finished { " · finished" } else { "" }
                ));
            });
        });
        let frame = &frames[position];
        let sheets = panels::Sheets {
            header: session,
            frames,
        };
        egui::Panel::left("players")
            .resizable(true)
            .default_size(340.0)
            .frame(panel_frame())
            .show(root, |ui| {
                light(ui);
                panels::players_sheet(ui, &sheets, frame);
            });
        let mut action = None;
        egui::Panel::right("choices")
            .resizable(true)
            .default_size(470.0)
            .frame(panel_frame())
            .show(root, |ui| {
                light(ui);
                action = choice(ui, remote, session, frames, frame);
                panels::decision_sheet(
                    ui,
                    &sheets,
                    frame,
                    self.selected_tile.as_deref(),
                    panels::SystemNaming::TileAndPlanets,
                );
            });
        egui::CentralPanel::default().show(root, |ui| {
            let content = ContentStore::embedded();
            let available = ui.available_size();
            let (response, painter) = ui.allocate_painter(available, Sense::click());
            let layout = BoardLayout::new(response.rect, available, frame.state.fracture_in_play);
            let tiles = board_view(content, session, frame, self.selected_tile.as_deref());
            if let Some(system) = draw_board(&painter, &response, &layout, &tiles) {
                self.selected_tile = Some(system);
            }
        });
        action
    }
}

fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(PANEL_FILL)
        .inner_margin(egui::Margin::same(8))
}

fn light(ui: &mut egui::Ui) {
    *ui.visuals_mut() = egui::Visuals::light();
    ui.visuals_mut().override_text_color = Some(PANEL_TEXT);
}

/// This seat's pending choice, as buttons. Drawn against the newest frame, which is the one the
/// engine is paused on, whatever frame the board is showing.
fn choice(
    ui: &mut egui::Ui,
    remote: &Remote,
    session: &ti4_review::ReviewSession,
    frames: &[ti4_review::ReviewFrame],
    frame: &ti4_review::ReviewFrame,
) -> Option<Action> {
    let Some(pending) = &remote.pending else {
        ui.heading("Choices");
        ui.weak("Nothing is being asked of you right now.");
        ui.separator();
        return None;
    };
    let tip = frames.last().unwrap_or(frame);
    let mut action = None;
    ui.heading("You are asked");
    ui.strong(view::annotate(session, &pending.prompt));
    for line in pending.question_lines(session) {
        ui.small(line);
    }
    if let Some(alias) = view::current_agenda(frames, tip) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            for line in view::agenda_card(ContentStore::embedded(), &alias) {
                ui.label(line);
            }
        });
    }
    let room = (ui.available_height() * 0.5).clamp(120.0, 520.0);
    egui::ScrollArea::vertical()
        .id_salt("remote-options")
        .max_height(room)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for option in &pending.options {
                let text = format!(
                    "{}{}",
                    view::annotate(session, &option.label),
                    view::policy_odds(option.score, option.probability)
                );
                let button = ui.add(
                    egui::Button::new(egui::RichText::new(text))
                        .wrap_mode(egui::TextWrapMode::Wrap),
                );
                if button.clicked() {
                    action = Some(Action::Submit(Box::new(pending.clone()), option.id.clone()));
                }
                for line in view::option_lines(
                    session,
                    &option.id,
                    &option.label,
                    &pending.details,
                    pending.previews(),
                ) {
                    ui.small(line);
                }
                let terms = ti4_review::diplomacy::payload_lines(&tip.state, &option.payload);
                ui.indent(("terms", option.id.clone()), |ui| {
                    for line in terms {
                        ui.small(view::annotate(session, &line));
                    }
                    ui.small(format!("{} · {}", option.id, option.kind));
                });
            }
        });
    if ui.button("Let the policy answer this one").clicked() {
        action = Some(Action::Delegate(pending.offer));
    }
    ui.separator();
    action
}
