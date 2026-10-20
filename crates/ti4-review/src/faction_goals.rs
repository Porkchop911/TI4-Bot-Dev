//! The Faction goals window: per seat, each milestone term its faction is rewarded for, whether it
//! was earned in time, and the potential currently standing, read from public state only.

use eframe::egui;
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::Observed;
use ti4_model::content_types::FULL;
use ti4_model::id::PlayerId;
use ti4_policy::progress::{self, Baseline, Progress};
use ti4_training::faction_terms::{self, TermStatus};

use crate::{ReviewFrame, view};

/// One seat's terms after the viewed frame.
#[derive(Debug, Clone)]
pub struct SeatGoals {
    pub player: PlayerId,
    pub terms: Vec<TermStatus>,
    /// Potential currently standing, in VP.
    pub paying_vp: f64,
}

/// Seats in `frames[upto]` order; the history is `frames[..=upto]`. The faction measurement is
/// filled directly, so the global `MEASURE_FACTION_TERMS` switch is left alone.
#[must_use]
pub fn goals(setup: &Galaxy, frames: &[ReviewFrame], upto: usize) -> Vec<SeatGoals> {
    let content = ContentStore::embedded();
    let Some(first) = frames.first() else {
        return Vec::new();
    };
    let upto = upto.min(frames.len() - 1);
    let first_galaxy = super::power::galaxy_at(setup, &first.state);
    let first_seen = Observed::new(&first.state, content, FULL, Some(&first_galaxy));
    let players: Vec<PlayerId> = first_seen.players().into_iter().cloned().collect();
    let mut history: Vec<Vec<Progress>> = vec![Vec::new(); players.len()];
    for frame in &frames[..=upto] {
        let galaxy = super::power::galaxy_at(setup, &frame.state);
        let seen = Observed::new(&frame.state, content, FULL, Some(&galaxy));
        for (player, snaps) in players.iter().zip(&mut history) {
            let mut p = progress::measure(&seen, player, Baseline::taken(&first_seen, player));
            p.faction_terms = ti4_policy::faction_measure::measure(&seen, player);
            snaps.push(p);
        }
    }
    players
        .into_iter()
        .zip(history)
        .map(|(player, snaps)| {
            let terms = faction_terms::status(&snaps);
            let paying_vp = terms.iter().map(|t| t.paying_vp).sum();
            SeatGoals { player, terms, paying_vp }
        })
        .collect()
}

/// A term's state in words.
#[must_use]
pub fn state_label(t: &TermStatus, round: u32) -> &'static str {
    match (t.earned, t.met_now) {
        (true, true) => "earned, held",
        (true, false) => "earned",
        (false, true) => "met now",
        (false, false) if t.deadline.is_some_and(|d| round > d) => "missed",
        (false, false) => "not yet",
    }
}

/// The window's contents.
pub fn goals_sheet(ui: &mut egui::Ui, frame: &ReviewFrame, goals: &[SeatGoals]) {
    let content = ContentStore::embedded();
    ui.small(
        "Milestones each faction is rewarded for in training. Weight is in VP; a term earned after \
         its deadline round does not count. Paying is the potential standing now.",
    );
    for seat in goals {
        ui.separator();
        ui.horizontal(|ui| {
            ui.colored_label(
                view::player_color(&seat.player),
                view::seat_name(frame, &seat.player, content),
            );
            ui.strong(format!("paying {:.2} VP", seat.paying_vp));
        });
        if seat.terms.is_empty() {
            ui.weak("No faction terms measured for this faction.");
            continue;
        }
        egui::Grid::new(format!("goals-{}", seat.player))
            .striped(true)
            .num_columns(5)
            .show(ui, |ui| {
                for h in ["Term", "Weight", "By round", "State", "Paying"] {
                    ui.strong(h);
                }
                ui.end_row();
                for t in &seat.terms {
                    ui.label(t.id);
                    ui.monospace(format!("{:.2}", t.weight_vp));
                    ui.monospace(t.deadline.map_or("–".to_owned(), |d| d.to_string()));
                    ui.label(state_label(t, frame.round));
                    ui.monospace(format!("{:.2}", t.paying_vp));
                    ui.end_row();
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(deadline: Option<u32>, earned: bool, met_now: bool) -> TermStatus {
        TermStatus { id: "x", weight_vp: 0.5, deadline, once: false, earned, met_now, paying_vp: 0.0 }
    }

    #[test]
    fn states_read_in_order() {
        assert_eq!(state_label(&term(Some(2), true, true), 3), "earned, held");
        assert_eq!(state_label(&term(Some(2), true, false), 3), "earned");
        assert_eq!(state_label(&term(Some(2), false, true), 3), "met now");
        assert_eq!(state_label(&term(Some(2), false, false), 3), "missed");
        assert_eq!(state_label(&term(Some(2), false, false), 2), "not yet");
        assert_eq!(state_label(&term(None, false, false), 9), "not yet");
    }

    /// On a small constructed history the terms follow the table's order and the seat total is the
    /// potential the trainer would see.
    #[test]
    fn status_follows_the_table_and_sums_to_the_potential() {
        let Some((n, row)) = (1_u8..=40).find_map(|n| faction_terms::row_for(n).map(|r| (n, r))) else {
            panic!("no measured faction");
        };
        let mut snap = Progress::default();
        snap.faction_terms.faction = n;
        snap.round_number = 1;
        let history = [snap, Progress { round_number: 2, ..snap }];
        let terms = faction_terms::status(&history);
        let ids: Vec<&str> = terms.iter().map(|t| t.id).collect();
        let table: Vec<&str> = row.terms.iter().map(|t| t.id).collect();
        assert_eq!(ids, table);
        let paying: f64 = terms.iter().map(|t| t.paying_vp).sum();
        let last = *faction_terms::potentials(&history).last().expect("potential");
        assert!((paying - last).abs() < 1e-9);
    }
}
