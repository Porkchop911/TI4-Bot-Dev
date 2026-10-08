//! Power projection for one frame: what every seat holds in each system now (immediate defence),
//! what one activation could put there (projection), and what follows from comparing the two.
//!
//! The numbers are [`ti4_policy::power_map`]'s, the same ones the trainer's projection reward and
//! critic facts read: expected hits per combat round, hits a force can absorb, and a Lanchester
//! index after opening fire calibrated against the battle arena into a win probability.
//!
//! A session stores the map as tile coordinates, not as a `Galaxy`, so the galaxy is rebuilt from
//! [`crate::ReviewSession::board`]: on-map tiles by coordinate, the Nexus and the Fracture off-map.

use std::collections::BTreeMap;

use eframe::egui::{self, Color32, RichText};
use ti4_content::ContentStore;
use ti4_content::galaxy::Galaxy;
use ti4_engine::choice::Observed;
use ti4_model::content_types::FULL;
use ti4_model::hex::Hex;
use ti4_model::id::{PlayerId, SystemId};
use ti4_policy::fleet_strength::Descriptors;
use ti4_policy::power_map::{self, GROUND_CALIBRATION, SPACE_CALIBRATION, SystemPower};

use crate::view;
use crate::{ReviewFrame, ReviewSession};

/// One seat's projection in summary.
#[derive(Debug, Clone)]
pub struct SeatPower {
    pub player: PlayerId,
    /// Expected planet value one activation could take, summed over reachable systems.
    pub opportunity: f64,
    /// The best single system and what it is worth.
    pub best_target: Option<(SystemId, f64)>,
    /// Reachable systems with planets.
    pub reach_systems: usize,
    /// Expected planet value of this seat's planets that some opponent could take with one
    /// activation (per planet-holding system, the likeliest attacker).
    pub exposure: f64,
    /// The systems behind `exposure`: system, own value there, likeliest attacker, its chance.
    pub threatened: Vec<(SystemId, f64, PlayerId, f64)>,
    /// Per system: what this seat could do there and what could be done to it.
    pub systems: BTreeMap<SystemId, Standing>,
}

/// One seat's standing in one system, for the map overlay.
#[derive(Debug, Clone, Default)]
pub struct Standing {
    /// Expected planet value one activation could take here.
    pub opportunity: f64,
    /// Planet value here held by someone else or nobody: what `opportunity` is a share of.
    pub takeable: f64,
    /// Ships one activation could have here (projection), as a Lanchester index.
    pub projection_index: f64,
    /// Ships arriving from elsewhere in that projection.
    pub arriving: usize,
    /// Planet value this seat holds here.
    pub holds: f64,
    /// The seat has ships or ground forces here now.
    pub present: bool,
    /// The likeliest opponent to beat this seat's presence here with one activation, and its chance
    /// (space against the ships here; times ground against the forces here when planets are held).
    pub threat: Option<(PlayerId, f64)>,
}

impl Standing {
    /// The share of what is takeable here that one activation would expect to take.
    #[must_use]
    pub fn take_chance(&self) -> f64 {
        if self.takeable > 0.0 {
            (self.opportunity / self.takeable).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// The whole frame.
#[derive(Debug, Clone, Default)]
pub struct PowerReport {
    pub seats: Vec<SeatPower>,
    /// Every seat's full map, projection included.
    pub maps: BTreeMap<PlayerId, BTreeMap<SystemId, SystemPower>>,
    /// Planet value (resources + influence) each seat controls in each system.
    pub held: BTreeMap<(PlayerId, SystemId), f64>,
}

/// Rebuild the map the session was played on. `None` if a tile is unknown to the content.
#[must_use]
pub fn galaxy_of(session: &ReviewSession, content: &ContentStore) -> Option<Galaxy> {
    galaxy_from_board(&session.board, content)
}

/// [`galaxy_of`] from the board metadata alone.
#[must_use]
pub fn galaxy_from_board(board: &[crate::BoardTile], content: &ContentStore) -> Option<Galaxy> {
    let placed: Vec<(&str, Hex)> = board
        .iter()
        .filter(|tile| tile.special_area.is_none())
        .map(|tile| {
            (
                tile.system.as_str(),
                Hex {
                    q: tile.q,
                    r: tile.r,
                },
            )
        })
        .collect();
    let mut galaxy = Galaxy::placed(content, &placed, FULL).ok()?;
    for tile in board.iter().filter(|tile| tile.special_area.is_some()) {
        let _ = galaxy.place_off_map(content, &tile.system, FULL);
    }
    Some(galaxy)
}

fn win(attacker: &Descriptors, defender: &Descriptors, calibration: (f64, f64)) -> f64 {
    power_map::win_probability(attacker, defender, calibration)
}

/// Chance `attacker`'s projection takes `defender`'s planets in this system: space first (when the
/// defender has ships there), then ground (bombardment unless shielded).
fn take_chance(attacker: &SystemPower, defender: &SystemPower) -> f64 {
    if attacker.space_projection_units.count() == 0 || attacker.ground_projection_units.count() == 0
    {
        return 0.0;
    }
    let space = if defender.space_defence.durability > 0.0 {
        win(
            &attacker.space_projection,
            &defender.space_defence,
            SPACE_CALIBRATION,
        )
    } else {
        1.0
    };
    let ground = if defender.ground_defence.durability > 0.0 {
        let mut landing = attacker.ground_projection;
        landing.cannon = if defender.shielded {
            0.0
        } else {
            attacker.bombardment
        };
        win(&landing, &defender.ground_defence, GROUND_CALIBRATION)
    } else {
        1.0
    };
    space * ground
}

/// The power report for one frame. Costs a movement search per seat and system, so callers cache it.
#[must_use]
pub fn report(galaxy: &Galaxy, frame: &ReviewFrame) -> PowerReport {
    let content = ContentStore::embedded();
    let seen = Observed::new(&frame.state, content, FULL, Some(galaxy));
    let players: Vec<PlayerId> = seen.players().into_iter().cloned().collect();
    let maps: BTreeMap<PlayerId, BTreeMap<SystemId, SystemPower>> = players
        .iter()
        .map(|p| (p.clone(), power_map::power_map(&seen, p)))
        .collect();
    let mut held: BTreeMap<(PlayerId, SystemId), f64> = BTreeMap::new();
    for id in galaxy.system_ids() {
        let system = SystemId::new(id);
        for (planet, owner) in &seen.system(&system).planet_control {
            #[expect(
                clippy::cast_precision_loss,
                reason = "planet values are single digits"
            )]
            let value = ti4_content::galaxy::planet(content, planet.as_str(), FULL)
                .map_or(0, |p| p.resources() + p.influence()) as f64;
            *held.entry((owner.clone(), system.clone())).or_default() += value;
        }
    }
    let mut seats = Vec::new();
    for player in &players {
        let own = &maps[player];
        let others: Vec<(PlayerId, BTreeMap<SystemId, SystemPower>)> = maps
            .iter()
            .filter(|(p, _)| *p != player)
            .map(|(p, m)| (p.clone(), m.clone()))
            .collect();
        let by_system = power_map::opportunity_by_system(&seen, player, own, &others);
        let best_target = by_system
            .iter()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .filter(|(_, v)| **v > 0.0)
            .map(|(s, v)| (s.clone(), *v));
        let mut threatened = Vec::new();
        for ((owner, system), value) in &held {
            if owner != player {
                continue;
            }
            let Some(mine) = own.get(system) else {
                continue;
            };
            let likeliest = others
                .iter()
                .filter_map(|(q, map)| map.get(system).map(|theirs| (q, take_chance(theirs, mine))))
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((q, chance)) = likeliest.filter(|(_, c)| *c > 0.0) {
                threatened.push((system.clone(), *value, q.clone(), chance));
            }
        }
        threatened.sort_by(|a, b| (b.1 * b.3).total_cmp(&(a.1 * a.3)));
        let mut systems = BTreeMap::new();
        for (system, mine) in own {
            let holds = held
                .get(&(player.clone(), system.clone()))
                .copied()
                .unwrap_or(0.0);
            let total = planets_value(content, system);
            let present =
                mine.space_defence.durability > 0.0 || mine.ground_defence.durability > 0.0;
            let threat = if present || holds > 0.0 {
                others
                    .iter()
                    .filter_map(|(q, map)| {
                        let theirs = map.get(system)?;
                        if theirs.space_projection_units.count() == 0 {
                            return None;
                        }
                        let chance = if holds > 0.0 {
                            take_chance(theirs, mine)
                        } else if mine.space_defence.durability > 0.0 {
                            win(
                                &theirs.space_projection,
                                &mine.space_defence,
                                SPACE_CALIBRATION,
                            )
                        } else {
                            0.0
                        };
                        Some((q.clone(), chance))
                    })
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .filter(|(_, c)| *c > 0.0)
            } else {
                None
            };
            systems.insert(
                system.clone(),
                Standing {
                    opportunity: by_system.get(system).copied().unwrap_or(0.0),
                    takeable: (total - holds).max(0.0),
                    projection_index: mine.space_projection.firepower
                        * mine.space_projection.durability,
                    arriving: mine.arriving_ships,
                    holds,
                    present,
                    threat,
                },
            );
        }
        seats.push(SeatPower {
            systems,
            player: player.clone(),
            opportunity: by_system.values().sum(),
            best_target,
            reach_systems: by_system.len(),
            exposure: threatened.iter().map(|t| t.1 * t.3).sum(),
            threatened,
        });
    }
    PowerReport { seats, maps, held }
}

fn planets_value(content: &ContentStore, system: &SystemId) -> f64 {
    let planets = ti4_content::galaxy::system(content, system.as_str(), FULL)
        .map(|s| s.planets())
        .unwrap_or_default();
    #[expect(
        clippy::cast_precision_loss,
        reason = "planet values are single digits"
    )]
    planets
        .iter()
        .map(|p| {
            ti4_content::galaxy::planet(content, p, FULL)
                .map_or(0, |p| p.resources() + p.influence()) as f64
        })
        .sum()
}

/// What the board overlay shows.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Overlay {
    #[default]
    Off,
    /// One seat's view: where it can take, where it is threatened, where it cannot reach.
    Seat(PlayerId),
    /// Every seat's projection on every hex, as bars.
    All,
}

const GREEN: Color32 = Color32::from_rgb(60, 190, 90);
const AMBER: Color32 = Color32::from_rgb(235, 175, 40);
const RED: Color32 = Color32::from_rgb(230, 60, 60);

/// Paint the overlay over a board already drawn with [`crate::view::draw_board`].
pub fn draw_overlay(
    painter: &egui::Painter,
    layout: &crate::view::BoardLayout,
    tiles: &[crate::view::TileView],
    report: &PowerReport,
    overlay: &Overlay,
) {
    use egui::{Align2, FontId, Pos2, Shape, Stroke, Vec2};
    let scale = layout.scale.max(0.8);
    match overlay {
        Overlay::Off => {}
        Overlay::Seat(player) => {
            let Some(seat) = report.seats.iter().find(|s| &s.player == player) else {
                return;
            };
            for tile in tiles {
                let point = crate::view::tile_point(layout, tile);
                let corners = crate::view::hex_corners(point, layout.radius * 0.97);
                let Some(st) = seat.systems.get(&SystemId::new(&tile.system)) else {
                    continue;
                };
                let reachable = st.projection_index > 0.0 || st.present;
                if !reachable {
                    painter.add(Shape::convex_polygon(
                        corners,
                        Color32::from_black_alpha(130),
                        Stroke::NONE,
                    ));
                    continue;
                }
                let take = st.take_chance();
                let fill = if st.opportunity > 0.0 && take >= 0.6 {
                    Some(GREEN)
                } else if st.opportunity > 0.0 && take >= 0.3 {
                    Some(AMBER)
                } else {
                    None
                };
                if let Some(colour) = fill {
                    painter.add(Shape::convex_polygon(
                        corners.clone(),
                        colour.gamma_multiply(0.35),
                        Stroke::NONE,
                    ));
                }
                if let Some((_, chance)) = st.threat.as_ref().filter(|(_, c)| *c >= 0.25) {
                    let colour = if *chance >= 0.5 { RED } else { AMBER };
                    let inner: Vec<Pos2> = corners
                        .iter()
                        .map(|c| point + (*c - point) * 0.84)
                        .collect();
                    painter.add(Shape::closed_line(inner, Stroke::new(3.5 * scale, colour)));
                }
                let mut line = format!("⚔{:.1}", st.projection_index);
                if st.arriving > 0 {
                    line.push_str(&format!(" +{}", st.arriving));
                }
                if st.opportunity > 0.0 {
                    line.push_str(&format!(" · {:.0}%", take * 100.0));
                }
                if let Some((_, chance)) = st.threat.as_ref().filter(|(_, c)| *c >= 0.25) {
                    line.push_str(&format!(" · ⚠{:.0}%", chance * 100.0));
                }
                let at = point + Vec2::new(0.0, 33.0 * layout.scale);
                let galley =
                    painter.layout_no_wrap(line, FontId::proportional(9.0 * scale), Color32::WHITE);
                let rect = Align2::CENTER_CENTER
                    .anchor_size(at, galley.size())
                    .expand(2.0);
                painter.rect_filled(rect, 3.0, Color32::from_black_alpha(170));
                painter.galley(rect.min + Vec2::splat(2.0), galley, Color32::WHITE);
            }
        }
        Overlay::All => {
            let max = report
                .seats
                .iter()
                .flat_map(|s| s.systems.values().map(|st| st.projection_index))
                .fold(1.0_f64, f64::max);
            for tile in tiles {
                let point = crate::view::tile_point(layout, tile);
                let id = SystemId::new(&tile.system);
                let bars: Vec<(&PlayerId, f64)> = report
                    .seats
                    .iter()
                    .filter_map(|s| {
                        s.systems
                            .get(&id)
                            .map(|st| (&s.player, st.projection_index))
                    })
                    .filter(|(_, v)| *v > 0.0)
                    .collect();
                if bars.is_empty() {
                    continue;
                }
                let width = layout.radius * 1.1;
                let bar_h = 4.0 * scale;
                let left = point.x - width / 2.0;
                let top = point.y + 14.0 * layout.scale;
                for (i, (player, value)) in bars.iter().enumerate() {
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "a fraction of a tile width"
                    )]
                    let w = (f64::from(width) * ((1.0 + value).ln() / (1.0 + max).ln())) as f32;
                    #[expect(clippy::cast_precision_loss, reason = "at most six bars")]
                    let y = top + i as f32 * (bar_h + 1.5);
                    painter.rect_filled(
                        egui::Rect::from_min_size(Pos2::new(left, y), Vec2::new(width, bar_h)),
                        1.0,
                        Color32::from_black_alpha(140),
                    );
                    painter.rect_filled(
                        egui::Rect::from_min_size(Pos2::new(left, y), Vec2::new(w.max(2.0), bar_h)),
                        1.0,
                        view::player_color(player),
                    );
                }
            }
        }
    }
}

/// The hover text for one system: every seat's defence and projection there.
#[must_use]
pub fn hover_lines(
    frame: &ReviewFrame,
    report: &PowerReport,
    system: &str,
) -> Vec<(PlayerId, String)> {
    let content = ContentStore::embedded();
    let id = SystemId::new(system);
    let mut out = Vec::new();
    for seat in &report.seats {
        let Some(p) = report.maps.get(&seat.player).and_then(|m| m.get(&id)) else {
            continue;
        };
        let st = seat.systems.get(&id);
        let any = p.space_defence.durability > 0.0
            || p.ground_defence.durability > 0.0
            || p.space_projection.durability > 0.0
            || st.is_some_and(|s| s.holds > 0.0);
        if !any {
            continue;
        }
        let mut text = format!(
            "{}: defence {} / ground {} | projection {} (+{} ships), ground {}",
            view::seat_name(frame, &seat.player, content),
            fmt_force(&p.space_defence),
            fmt_force(&p.ground_defence),
            fmt_force(&p.space_projection),
            p.arriving_ships,
            fmt_force(&p.ground_projection)
        );
        if let Some(st) = st {
            if st.opportunity > 0.0 {
                text.push_str(&format!(
                    " | can take {:.1} of {:.0} ({:.0}%)",
                    st.opportunity,
                    st.takeable,
                    st.take_chance() * 100.0
                ));
            }
            if let Some((by, chance)) = &st.threat {
                text.push_str(&format!(
                    " | threatened by {} {:.0}%",
                    by.as_str(),
                    chance * 100.0
                ));
            }
        }
        out.push((seat.player.clone(), text));
    }
    out
}

fn fmt_force(d: &Descriptors) -> String {
    if d.durability <= 0.0 {
        return "–".to_owned();
    }
    format!(
        "{:.1} hits · {:.0} hp · idx {:.1}",
        d.firepower,
        d.durability,
        d.firepower * d.durability
    )
}

fn percent(p: f64) -> String {
    format!("{:.0}%", p * 100.0)
}

/// The Power window's contents: seat table, then the selected system's breakdown.
pub fn power_sheet(
    ui: &mut egui::Ui,
    session: &ReviewSession,
    frame: &ReviewFrame,
    report: &PowerReport,
    selected: Option<&str>,
) {
    let content = ContentStore::embedded();
    ui.small(
        "Opportunity = planet value (resources + influence) one activation could expect to take. \
         Exposure = own planet value an opponent could expect to take. Win chances are the Lanchester \
         index calibrated against the battle arena (space and ground separately).",
    );
    ui.add_space(4.0);
    egui::Grid::new("power-seats")
        .striped(true)
        .num_columns(6)
        .show(ui, |ui| {
            for heading in [
                "Seat",
                "Opportunity",
                "Best target",
                "Reach",
                "Exposure",
                "Most threatened",
            ] {
                ui.strong(heading);
            }
            ui.end_row();
            for seat in &report.seats {
                ui.colored_label(
                    view::player_color(&seat.player),
                    view::seat_name(frame, &seat.player, content),
                );
                ui.monospace(format!("{:.1}", seat.opportunity));
                ui.label(seat.best_target.as_ref().map_or_else(
                    || "–".to_owned(),
                    |(s, v)| format!("{} ({v:.1})", view::system_label(session, s.as_str())),
                ));
                ui.monospace(seat.reach_systems.to_string());
                ui.monospace(format!("{:.1}", seat.exposure.max(0.0) + 0.0));
                ui.label(seat.threatened.first().map_or_else(
                    || "–".to_owned(),
                    |(s, v, by, c)| {
                        format!(
                            "{} ({v:.0}) by {} {}",
                            view::system_label(session, s.as_str()),
                            view::seat_name(frame, by, content),
                            percent(*c)
                        )
                    },
                ));
                ui.end_row();
            }
        });
    ui.separator();
    let Some(system) = selected else {
        ui.weak("Click a system on the map to see each seat's defence and projection there.");
        return;
    };
    let id = SystemId::new(system);
    ui.strong(format!("System {}", view::system_label(session, system)));
    egui::Grid::new("power-system")
        .striped(true)
        .num_columns(6)
        .show(ui, |ui| {
            for heading in [
                "Seat",
                "Defence (space)",
                "Defence (ground)",
                "Projection (space)",
                "Projection (ground)",
                "Notes",
            ] {
                ui.strong(heading);
            }
            ui.end_row();
            for seat in &report.seats {
                let Some(p) = report.maps.get(&seat.player).and_then(|m| m.get(&id)) else {
                    continue;
                };
                let any = p.space_defence.durability > 0.0
                    || p.ground_defence.durability > 0.0
                    || p.space_projection.durability > 0.0
                    || p.ground_projection.durability > 0.0;
                if !any {
                    continue;
                }
                ui.colored_label(
                    view::player_color(&seat.player),
                    view::seat_name(frame, &seat.player, content),
                );
                ui.monospace(fmt_force(&p.space_defence));
                ui.monospace(fmt_force(&p.ground_defence));
                ui.monospace(format!(
                    "{} · +{} ships",
                    fmt_force(&p.space_projection),
                    p.arriving_ships
                ));
                ui.monospace(fmt_force(&p.ground_projection));
                let mut notes = Vec::new();
                if let Some(v) = report.held.get(&(seat.player.clone(), id.clone())) {
                    notes.push(format!("holds {v:.0}"));
                }
                if p.shielded {
                    notes.push("shield".to_owned());
                }
                if p.space_defence.cannon > 0.0 {
                    notes.push(format!("cannon {:.1}", p.space_defence.cannon));
                }
                if p.bombardment > 0.0 {
                    notes.push(format!("bombard {:.1}", p.bombardment));
                }
                if !p.can_activate {
                    notes.push("can't activate now".to_owned());
                }
                ui.label(notes.join(" · "));
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    ui.strong("Attacker projection vs defender here (space × ground win chance)");
    let present: Vec<&SeatPower> = report
        .seats
        .iter()
        .filter(|s| {
            report
                .maps
                .get(&s.player)
                .and_then(|m| m.get(&id))
                .is_some_and(|p| {
                    p.space_defence.durability > 0.0
                        || p.ground_defence.durability > 0.0
                        || report.held.contains_key(&(s.player.clone(), id.clone()))
                })
        })
        .collect();
    if present.is_empty() {
        ui.weak("No seat holds anything here.");
        return;
    }
    egui::Grid::new("power-matchups")
        .striped(true)
        .show(ui, |ui| {
            ui.strong("attacker \\ defender");
            for d in &present {
                ui.colored_label(view::player_color(&d.player), d.player.as_str());
            }
            ui.end_row();
            for a in &report.seats {
                let Some(att) = report.maps.get(&a.player).and_then(|m| m.get(&id)) else {
                    continue;
                };
                if att.space_projection_units.count() == 0 {
                    continue;
                }
                ui.colored_label(view::player_color(&a.player), a.player.as_str());
                for d in &present {
                    if d.player == a.player {
                        ui.weak("·");
                        continue;
                    }
                    let def = &report.maps[&d.player][&id];
                    let chance = take_chance(att, def);
                    let space = if def.space_defence.durability > 0.0 {
                        win(&att.space_projection, &def.space_defence, SPACE_CALIBRATION)
                    } else {
                        1.0
                    };
                    let colour = if chance >= 0.6 {
                        Color32::from_rgb(40, 140, 60)
                    } else if chance >= 0.3 {
                        Color32::from_rgb(190, 140, 20)
                    } else {
                        Color32::from_rgb(170, 60, 60)
                    };
                    ui.label(
                        RichText::new(format!("{} (space {})", percent(chance), percent(space)))
                            .color(colour),
                    )
                    .on_hover_text(format!(
                        "attacker {} vs defender {}",
                        fmt_force(&att.space_projection),
                        fmt_force(&def.space_defence)
                    ));
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_engine::choice::Decider;
    use ti4_model::id::FactionId;

    /// The window rebuilds the map from the session's tile list; every number it shows depends on
    /// that map being the one the game was played on.
    #[test]
    fn the_rebuilt_galaxy_is_the_played_one() {
        let content = ContentStore::embedded();
        let names = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
        let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
        let factions: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .zip(names)
            .map(|(p, f)| (p.clone(), FactionId::new(f)))
            .collect();
        for seed in [7_u64, 19, 31] {
            let game = ti4_training::rollout::setup_game_with_capabilities_and_decider_factory(
                content,
                &players,
                &factions,
                FULL,
                seed,
                &ti4_training::rollout::OpeningMap::RustVaried,
                ti4_training::rollout::SimulationCapabilities::default(),
                |_| {
                    Ok(players
                        .iter()
                        .map(|p| {
                            (
                                p.clone(),
                                Box::new(ti4_engine::choice::FirstOption) as Box<dyn Decider>,
                            )
                        })
                        .collect())
                },
            )
            .expect("seated");
            let galaxy = game.galaxy().expect("map");
            let board = crate::board_metadata(content, galaxy);
            let rebuilt = galaxy_from_board(&board, content).expect("rebuilt");
            for id in galaxy.system_ids() {
                assert_eq!(
                    galaxy.adjacent(id),
                    rebuilt.adjacent(id),
                    "seed {seed} system {id}"
                );
            }
            let played = Observed::new(&game.state, content, FULL, Some(galaxy));
            let shown = Observed::new(&game.state, content, FULL, Some(&rebuilt));
            for player in &players {
                assert_eq!(
                    power_map::summary(&played, player),
                    power_map::summary(&shown, player),
                    "seed {seed} {player}"
                );
            }
        }
    }

    /// The actor's power facts on a real activation choice: one list per option, every activation
    /// option carries what one activation could bring there, and somewhere on an opening board a
    /// seat can bring ships (each home fleet reaches its neighbours).
    #[test]
    fn activation_options_carry_power_facts() {
        let content = ContentStore::embedded();
        let names = ["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
        let players: Vec<PlayerId> = (0..6).map(|i| PlayerId::new(format!("seat{i}"))).collect();
        let factions: BTreeMap<PlayerId, FactionId> = players
            .iter()
            .zip(names)
            .map(|(p, f)| (p.clone(), FactionId::new(f)))
            .collect();
        let game = ti4_training::rollout::setup_game_with_capabilities_and_decider_factory(
            content,
            &players,
            &factions,
            FULL,
            11,
            &ti4_training::rollout::OpeningMap::RustVaried,
            ti4_training::rollout::SimulationCapabilities::default(),
            |_| {
                Ok(players
                    .iter()
                    .map(|p| {
                        (
                            p.clone(),
                            Box::new(ti4_engine::choice::FirstOption) as Box<dyn Decider>,
                        )
                    })
                    .collect())
            },
        )
        .expect("seated");
        let galaxy = game.galaxy().expect("map");
        let seen = Observed::new(&game.state, content, FULL, Some(galaxy));
        let table = ti4_policy::power_facts::TablePower::of(&seen);
        let mut arriving_somewhere = false;
        for player in &players {
            let choice = ti4_engine::tactical::activation_options_with(
                &game.state,
                content,
                FULL,
                galaxy,
                player,
            )
            .expect("an opening seat can activate");
            let summary = power_map::summary(&seen, player);
            let facts = ti4_policy::power_facts::decision_facts(
                &seen,
                &choice,
                player,
                &summary,
                Some(&table),
            );
            assert_eq!(facts.len(), choice.options.len());
            for (option, list) in choice.options.iter().zip(&facts) {
                let names: Vec<&str> = list.iter().map(|(n, _)| *n).collect();
                assert!(
                    names.contains(&"seat-state:power-opportunity"),
                    "{player} {}: {names:?}",
                    option.id
                );
                if option.kind == ti4_engine::tactical::ACTIVATE_KIND {
                    assert!(
                        names.contains(&"action-plan:power-target-arriving"),
                        "{player} {}: {names:?}",
                        option.id
                    );
                    arriving_somewhere |= list
                        .iter()
                        .any(|(n, v)| *n == "action-plan:power-target-arriving" && *v > 0.0);
                }
                for (name, value) in list {
                    assert!(value.is_finite() && *value >= 0.0, "{name} = {value}");
                }
            }
        }
        assert!(
            arriving_somewhere,
            "no seat could bring a ship anywhere on an opening board"
        );
    }
}
