//! The presentation layer both reviewers draw from.
//!
//! R01's viewer and R02's replayer are meant to be indistinguishable wherever they show the same
//! frame, so the palette, the seat colours, the abbreviations, the section chrome and the board
//! glyphs live here once. Everything in this module is a pure function of the data handed to it: no
//! app state, no `LiveReview`, no clock, no filesystem. A re-tone is a change to this file and
//! nowhere else, and the snapshot tests pin the values the two apps promise each other.

use std::collections::BTreeMap;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Shape, Stroke, Vec2};
use ti4_content::ContentStore;
use ti4_model::content_types::{ContentType, FULL};
use ti4_model::id::PlayerId;
use ti4_model::units::Unit;

use ti4_model::id::{PlanetId, SystemId};

use crate::{BoardTile, OFF_MAP_AREA, PlanetMeta, ReviewFrame, ReviewSession};

pub const SEAT_COLORS: [Color32; 6] = [
    Color32::from_rgb(224, 66, 66),
    Color32::from_rgb(66, 142, 235),
    Color32::from_rgb(242, 198, 56),
    Color32::from_rgb(54, 184, 116),
    Color32::from_rgb(173, 103, 224),
    Color32::from_rgb(238, 126, 49),
];

pub const NEUTRAL_COLOR: Color32 = Color32::from_rgb(166, 174, 184);

/// Text on the light player panel. Colour marks a seat only as a swatch, never as the text itself.
pub const PANEL_TEXT: Color32 = Color32::BLACK;

pub const PANEL_FILL: Color32 = Color32::from_rgb(246, 247, 250);

pub fn seat_index(player: &PlayerId) -> Option<usize> {
    player
        .to_string()
        .strip_prefix("seat")
        .and_then(|value| value.parse::<usize>().ok())
        .map(|index| index % SEAT_COLORS.len())
}

pub fn player_color(player: &PlayerId) -> Color32 {
    seat_index(player).map_or(NEUTRAL_COLOR, |index| SEAT_COLORS[index])
}

pub fn short_trait(value: &str) -> &'static str {
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

pub fn short_specialty(value: &str) -> &'static str {
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

/// A panel section under a heading-sized header the reader can fold away; open by default.
pub fn section<R>(ui: &mut egui::Ui, title: &str, add_contents: impl FnOnce(&mut egui::Ui) -> R) {
    section_with_id(ui, title, title, add_contents);
}

/// [`section`] whose fold state survives a changing title.
pub fn section_with_id<R>(
    ui: &mut egui::Ui,
    title: &str,
    id: &str,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) {
    egui::CollapsingHeader::new(egui::RichText::new(title).heading())
        .id_salt(("panel-section", id))
        .default_open(true)
        .show(ui, add_contents);
}

pub fn item_section(
    ui: &mut egui::Ui,
    icon: &str,
    title: &str,
    items: Vec<String>,
    color: Color32,
) {
    ui.horizontal(|ui| {
        ui.label(icon);
        ui.strong(format!("{title} · {}", items.len()));
    });
    if items.is_empty() {
        ui.weak("None");
    } else {
        ui.horizontal_wrapped(|ui| {
            for item in items {
                ui.label(
                    egui::RichText::new(item)
                        .background_color(color.gamma_multiply(0.35))
                        .color(PANEL_TEXT),
                );
            }
        });
    }
}

/// The attachments on a planet, named rather than counted.
///
/// `· 2 attachment(s)` is a number where the reader needed a card name: an attachment is the difference
/// between a planet that invites an invasion and one that does not. The names are the ceiling of what
/// this engine can honestly say - its 22 attachment records carry `id`, `name`, `source` and
/// `techSpeciality`, and no rules text - so the effect of an attachment has to come from the rules, not
/// from a view layer guessing at it. An id the content does not know is still listed, by id: a thing the
/// game has attached and the content cannot name is exactly what a reader should see.
#[must_use]
pub fn attachment_names(ids: &[String], content: &ContentStore) -> Vec<String> {
    ids.iter()
        .map(|id| {
            content
                .get(ContentType::Attachments, id)
                .and_then(|record| record.text("name"))
                .map_or_else(|| id.clone(), std::borrow::ToOwned::to_owned)
        })
        .collect()
}

/// What an attachment does, read from its corpus record: its resource and influence modifiers, a
/// technology specialty it grants, traits it adds (UI-03). `None` when the record says nothing the
/// viewer can state (a token such as the Demilitarized Zone); nothing is invented here.
#[must_use]
pub fn attachment_effect(id: &str, content: &ContentStore) -> Option<String> {
    let record = content.get(ContentType::Attachments, id)?;
    let mut parts = Vec::new();
    let signed = |value: i64, what: &str| format!("{value:+} {what}");
    if let Some(value) = record.int("resourcesModifier").filter(|value| *value != 0) {
        parts.push(signed(value, "resources"));
    }
    if let Some(value) = record.int("influenceModifier").filter(|value| *value != 0) {
        parts.push(signed(value, "influence"));
    }
    let specialties = record.strings("techSpeciality");
    if !specialties.is_empty() {
        parts.push(format!("gives the {} specialty", specialties.join("/")));
    }
    let traits = record.strings("planetTypes");
    if !traits.is_empty() {
        parts.push(format!("adds traits {}", traits.join(", ")));
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// Attachments named with what they do: `Dyson Sphere (+2 resources, +1 influence)`.
#[must_use]
pub fn attachment_labels(ids: &[String], content: &ContentStore) -> Vec<String> {
    ids.iter()
        .zip(attachment_names(ids, content))
        .map(|(id, name)| match attachment_effect(id, content) {
            Some(effect) => format!("{name} ({effect})"),
            None => name,
        })
        .collect()
}

/// This round's exploration draws, most recent first (UI-01):
/// `seat2 "…" · Mehar Xull · hazardous · Mining World → attached Mining World (+2 resources)`.
#[must_use]
pub fn exploration_lines(
    session: &ReviewSession,
    frame: &ReviewFrame,
    content: &ContentStore,
) -> Vec<String> {
    frame
        .state
        .exploration_log
        .iter()
        .rev()
        .filter(|record| record.round == frame.state.round)
        .map(|record| {
            let who = seat_name(frame, &record.player, content);
            let card = content
                .get(ContentType::Explores, &record.card)
                .and_then(|entry| entry.text("name"))
                .map_or_else(|| record.card.clone(), ToOwned::to_owned);
            let place = record.planet.as_ref().map_or_else(
                || "frontier".to_owned(),
                |planet| {
                    session
                        .planet_catalog
                        .iter()
                        .find(|meta| meta.id == planet.as_str())
                        .map_or_else(|| planet.to_string(), |meta| meta.label.clone())
                },
            );
            let outcome = match record.outcome.split_once(':') {
                Some(("attached", attachment)) => format!(
                    "attached {}",
                    attachment_labels(&[attachment.to_owned()], content).join("")
                ),
                _ => match record.outcome.as_str() {
                    "fragment" => "relic fragment".to_owned(),
                    "resolved" => "resolved".to_owned(),
                    "unresolved" => "NOT RESOLVED (no engine handler)".to_owned(),
                    "discarded" => "discarded (nothing to attach to)".to_owned(),
                    other => other.to_owned(),
                },
            };
            format!(
                "{who} · {place} · {} · {card} → {outcome}",
                record.deck.to_ascii_lowercase()
            )
        })
        .collect()
}

/// A seat's negotiation budget: `negotiations left: 1 of 2 this action, 4 of 6 this round`.
#[must_use]
pub fn negotiation_budget(frame: &ReviewFrame, player: &PlayerId) -> String {
    use ti4_model::state::{MAX_NEGOTIATIONS_PER_ACTION, MAX_NEGOTIATIONS_PER_ROUND};
    let used =
        |tally: &std::collections::BTreeMap<PlayerId, u8>| tally.get(player).copied().unwrap_or(0);
    let action =
        MAX_NEGOTIATIONS_PER_ACTION.saturating_sub(used(&frame.state.negotiations_this_action));
    let round =
        MAX_NEGOTIATIONS_PER_ROUND.saturating_sub(used(&frame.state.negotiations_this_round));
    format!(
        "negotiations left: {} of {MAX_NEGOTIATIONS_PER_ACTION} this action, {} of {MAX_NEGOTIATIONS_PER_ROUND} this round",
        action.min(round),
        round
    )
}

/// What a seat's planets add up to: ready and total resources and influence, and how many of them
/// carry each trait and tech specialty.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanetTotals {
    pub planets: usize,
    pub ready: usize,
    pub resources_ready: i64,
    pub resources_total: i64,
    pub influence_ready: i64,
    pub influence_total: i64,
    /// Trait → planets controlled, exhausted or not: trait objectives count control, not readiness.
    pub traits: BTreeMap<String, usize>,
    /// Specialty → (ready, total): only a ready specialty can stand in for a prerequisite.
    pub specialties: BTreeMap<String, (usize, usize)>,
}

/// A seat's planet totals, valued the way the engine charges them.
///
/// Values come from `production::planet_value_now`, so a law or relic that attaches to a planet moves
/// the total exactly as it moves a payment; a printed-value sum would disagree with the price the
/// engine asks. Traits and specialties come from the session's planet catalog, which is the same
/// content record the engine reads.
#[must_use]
pub fn planet_totals(
    session: &ReviewSession,
    frame: &ReviewFrame,
    player: &PlayerId,
    content: &ContentStore,
) -> PlanetTotals {
    use ti4_engine::production::{Spend, planet_value_now};

    let mut totals = PlanetTotals::default();
    for (_, planet) in frame.state.controlled_planets(player) {
        let ready = !frame.state.exhausted_planets.contains(planet);
        let resources = planet_value_now(&frame.state, content, FULL, planet, Spend::Resources);
        let influence = planet_value_now(&frame.state, content, FULL, planet, Spend::Influence);
        totals.planets += 1;
        totals.resources_total += resources;
        totals.influence_total += influence;
        if ready {
            totals.ready += 1;
            totals.resources_ready += resources;
            totals.influence_ready += influence;
        }
        let Some(meta) = session
            .planet_catalog
            .iter()
            .find(|meta| meta.id == planet.as_str())
        else {
            continue;
        };
        for name in &meta.traits {
            *totals.traits.entry(name.to_ascii_lowercase()).or_default() += 1;
        }
        for name in &meta.tech_specialties {
            let entry = totals
                .specialties
                .entry(name.to_ascii_lowercase())
                .or_default();
            entry.1 += 1;
            if ready {
                entry.0 += 1;
            }
        }
    }
    totals
}

/// The totals as rider lines: `Resources 7 ready / 12`, `Influence 4 ready / 9`,
/// `Traits cultural 2 · industrial 3`, `Specialties biotic 1/1 · warfare 0/1` (ready/total).
#[must_use]
pub fn planet_totals_lines(totals: &PlanetTotals) -> Vec<String> {
    let mut lines = vec![
        format!(
            "Resources {} ready / {}",
            totals.resources_ready, totals.resources_total
        ),
        format!(
            "Influence {} ready / {}",
            totals.influence_ready, totals.influence_total
        ),
    ];
    if !totals.traits.is_empty() {
        lines.push(format!(
            "Traits {}",
            totals
                .traits
                .iter()
                .map(|(name, count)| format!("{name} {count}"))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    if !totals.specialties.is_empty() {
        lines.push(format!(
            "Specialties {} (ready/total)",
            totals
                .specialties
                .iter()
                .map(|(name, (ready, total))| format!("{name} {ready}/{total}"))
                .collect::<Vec<_>>()
                .join(" · ")
        ));
    }
    lines
}

/// A seat as the table knows it: `seat2 "Clan of Hacan"`, or the bare id before seating is known.
///
/// Six chips that read `seat0` … `seat5` make the reader hold a mapping nobody agreed to remember, and
/// the frame already carries each seat's faction. R01's existing strings are deliberately untouched -
/// this is for the surfaces where the reader asked, and a name printed here is a name the content store
/// printed. A faction this build does not know keeps its raw id in parentheses, because an unfamiliar id
/// on screen beats a name invented in a view layer.
#[must_use]
pub fn seat_name(frame: &ReviewFrame, player: &PlayerId, content: &ContentStore) -> String {
    let id = player.as_str();
    let Some(faction) = frame
        .state
        .players
        .iter()
        .find(|seat| seat.id == *player)
        .map(|seat| seat.faction.as_str().to_owned())
        .filter(|faction| !faction.is_empty())
    else {
        return id.to_owned();
    };
    match ti4_content::factions::get(content, &faction).and_then(|f| f.name()) {
        Some(name) => format!("{id} \"{name}\""),
        None => format!("{id} ({faction})"),
    }
}

/// A seat name in black, preceded by its colour swatch.
pub fn seat_label(ui: &mut egui::Ui, player: &PlayerId, text: impl Into<String>) {
    ui.horizontal(|ui| {
        ui.colored_label(player_color(player), "●");
        ui.label(egui::RichText::new(text.into()).color(PANEL_TEXT));
    });
}

pub fn stat_badge(ui: &mut egui::Ui, icon: &str, label: &str, value: impl std::fmt::Display) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            ui.strong(icon);
            ui.label(format!("{label} {value}"));
        });
    });
}

pub fn content_label(
    content: &ContentStore,
    category: ContentType,
    id: impl std::fmt::Display,
) -> String {
    let id = id.to_string();
    let Some(record) = content.get(category, &id) else {
        return id;
    };
    let Some(name) = record
        .text("name")
        .or_else(|| record.text("shortName"))
        .or_else(|| record.text("title"))
        .filter(|name| !name.eq_ignore_ascii_case(&id))
    else {
        return id;
    };
    format!("{name} [{id}]")
}

pub fn unit_base(content: &ContentStore, unit: &Unit) -> String {
    ti4_content::units::unit_type(content, unit.type_id.as_str(), FULL).map_or_else(
        || unit.type_id.to_string(),
        |kind| kind.base_type().to_owned(),
    )
}

pub fn planets_for_tile(
    session: &ReviewSession,
    frame: &ReviewFrame,
    tile: &crate::BoardTile,
) -> Vec<crate::PlanetMeta> {
    let mut planets = tile.planets.clone();
    for (planet, system) in &frame.state.placed_planets {
        if system.as_str() != tile.system || planets.iter().any(|known| known.id == planet.as_str())
        {
            continue;
        }
        if let Some(meta) = session
            .planet_catalog
            .iter()
            .find(|candidate| candidate.id == planet.as_str())
        {
            planets.push(meta.clone());
        }
    }
    planets
}

/// A system named the way a person at the table finds it: the number printed on the tile, and the
/// planets inside it.
///
/// The engine's own name for a system is the tile number — `43`, `102` — which is exact and tells a
/// reader nothing. `43 · Supernova` and `14 · Archon Ren/Archon Tau` are the same identity with the
/// contents attached. A system this session's board does not contain is returned unchanged, because
/// inventing a name for it would be worse than printing the number.
#[must_use]
pub fn system_label(session: &ReviewSession, system: &str) -> String {
    let Some(tile) = session
        .board
        .iter()
        .find(|candidate| candidate.system == system)
    else {
        return system.to_owned();
    };
    if tile.planets.is_empty() {
        // An empty system still has a printed name worth reading: Supernova, Alpha Wormhole.
        return format!("{system} · {}", tile.label);
    }
    let planets = tile
        .planets
        .iter()
        .map(|planet| planet.label.as_str())
        .collect::<Vec<_>>()
        .join("/");
    format!("{system} · {planets}")
}

/// Rewrite every bare tile number in `text` as [`system_label`] spells it.
///
/// The engine writes prompts and option labels with the tile number alone — `activate 43`, `commit
/// ground forces in 75` — and those are the sentences a reader has to judge a play from. Only tokens
/// that are exactly a system id *of this game's board* are rewritten, so a `1` meaning one trade good
/// is left alone: board ids are zero-padded (`01`), and a number that names no tile names nothing.
#[must_use]
pub fn annotate_systems(session: &ReviewSession, text: &str) -> String {
    if session.board.is_empty() || text.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        let boundary = rest
            .find(|character: char| character.is_ascii_digit())
            .unwrap_or(rest.len());
        out.push_str(&rest[..boundary]);
        rest = &rest[boundary..];
        if rest.is_empty() {
            break;
        }
        let end = rest
            .find(|character: char| !character.is_ascii_digit())
            .unwrap_or(rest.len());
        let (token, tail) = rest.split_at(end);
        // A digit run only names a system when nothing alphanumeric touches it on the left: `75` in
        // "in 75" is a tile, the `75` inside "sol_75x" is not.
        let attached = out
            .chars()
            .next_back()
            .is_some_and(|previous| previous.is_alphanumeric() || previous == '_');
        let detached = tail
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '_');
        if attached || !detached {
            out.push_str(token);
        } else {
            out.push_str(&system_label(session, token));
        }
        rest = tail;
    }
    out
}

/// Rewrite every bare seat id in `text` as [`seat_name`] spells it: `seat2` becomes
/// `seat2 "The Emirates of Hacan"`.
///
/// The operator's rule: a seat and its faction are always named together. Engine prompts, option
/// labels and events say `seat2` alone, so this is applied to the same strings [`annotate_systems`]
/// is. A seat already followed by a name (` "` or ` (`) is left alone, so text that went through
/// [`seat_name`] or this function once is not named twice. The faction comes from the session's
/// newest frame, or from the manifest's seat-ordered faction list when the session is a frame-less
/// shell (the replayer's case); seating does not change during a game.
#[must_use]
pub fn annotate_seats(session: &ReviewSession, text: &str) -> String {
    if !text.contains("seat") {
        return text.to_owned();
    }
    let content = ContentStore::embedded();
    let name = |seat: &str| -> String {
        let player = PlayerId::new(seat);
        if let Some(frame) = session.frames.last() {
            return seat_name(frame, &player, content);
        }
        let faction = seat
            .strip_prefix("seat")
            .and_then(|index| index.parse::<usize>().ok())
            .and_then(|index| session.manifest.factions.get(index))
            .filter(|faction| !faction.is_empty());
        match faction {
            None => seat.to_owned(),
            Some(faction) => {
                match ti4_content::factions::get(content, faction).and_then(|f| f.name()) {
                    Some(full) => format!("{seat} \"{full}\""),
                    None => format!("{seat} ({faction})"),
                }
            }
        }
    };
    let mut out = String::with_capacity(text.len() + 32);
    let mut rest = text;
    while let Some(start) = rest.find("seat") {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let digits = rest[4..]
            .find(|character: char| !character.is_ascii_digit())
            .map_or(rest.len(), |end| end + 4);
        let (token, tail) = rest.split_at(digits);
        let attached = out
            .chars()
            .next_back()
            .is_some_and(|previous| previous.is_alphanumeric() || previous == '_');
        let detached = tail
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric() && next != '_');
        let already_named = tail.starts_with(" \"") || tail.starts_with(" (");
        if token.len() == 4 || attached || !detached || already_named {
            out.push_str(token);
        } else {
            out.push_str(&name(token));
        }
        rest = tail;
    }
    out.push_str(rest);
    out
}

/// The policy's numbers on an option, named for what they are: ` · logit -11.59 · bot picks 0.6%`.
///
/// The score is the model's raw logit, and the percentage is the softmax share of *the options on
/// offer* at the sampling temperature: how often the bot would take this one here. Neither is a
/// chance of winning, and the wording must never let it read as one (UI-05, `DIPLOMACY_HONESTY.md`).
#[must_use]
pub fn policy_odds(score: Option<f64>, probability: Option<f64>) -> String {
    use std::fmt::Write as _;

    let mut out = String::new();
    if let Some(score) = score {
        let _ = write!(out, " · logit {score:.2}");
    }
    if let Some(probability) = probability {
        let _ = write!(out, " · bot picks {:.1}%", probability * 100.0);
    }
    out
}

/// The legend printed once under a list of scored options, so the rule is on screen, not in a plan.
#[must_use]
pub fn policy_odds_legend(temperature: f64) -> String {
    format!(
        "logit = the policy's raw preference. bot picks % = how often the bot would choose that option          among these, sampling at temperature {temperature}. Not a chance of winning."
    )
}

/// Engine text as a reader wants it: systems with their planets, seats with their factions.
#[must_use]
pub fn annotate(session: &ReviewSession, text: &str) -> String {
    annotate_seats(session, &annotate_systems(session, text))
}

pub fn polygon(center: Pos2, radius: f32, sides: usize, offset: f32) -> Vec<Pos2> {
    (0..sides)
        .map(|index| {
            let angle = offset + std::f32::consts::TAU * index as f32 / sides as f32;
            center + Vec2::angled(angle) * radius
        })
        .collect()
}

pub fn anomaly_style(kinds: &[String]) -> Option<(Color32, &'static str)> {
    if kinds.iter().any(|kind| kind == "entropic scar") {
        Some((Color32::from_rgb(48, 24, 64), "╳ SCAR"))
    } else if kinds.iter().any(|kind| kind == "supernova") {
        Some((Color32::from_rgb(105, 38, 20), "✹ NOVA"))
    } else if kinds.iter().any(|kind| kind == "gravity rift") {
        Some((Color32::from_rgb(50, 28, 82), "◉ RIFT"))
    } else if kinds.iter().any(|kind| kind == "nebula") {
        Some((Color32::from_rgb(55, 45, 91), "☁ NEBULA"))
    } else if kinds.iter().any(|kind| kind == "asteroid field") {
        Some((Color32::from_rgb(67, 61, 52), "✦ ASTEROIDS"))
    } else {
        None
    }
}

pub fn wormhole_style(kind: &str) -> (Color32, &str) {
    match kind.to_ascii_uppercase().as_str() {
        "ALPHA" => (Color32::from_rgb(225, 82, 82), "α"),
        "BETA" => (Color32::from_rgb(72, 190, 111), "β"),
        "GAMMA" => (Color32::from_rgb(230, 184, 68), "γ"),
        "DELTA" => (Color32::from_rgb(100, 154, 238), "δ"),
        _ => (Color32::LIGHT_GRAY, "?"),
    }
}

pub fn draw_wormhole(
    painter: &egui::Painter,
    center: Pos2,
    kind: &str,
    token: bool,
    suppressed: bool,
    scale: f32,
) {
    let (mut color, symbol) = wormhole_style(kind);
    if suppressed {
        color = color.gamma_multiply(0.35);
    }
    let radius = 7.2 * scale.max(0.75);
    if token {
        painter.circle_filled(center, radius + 2.2 * scale, Color32::from_rgb(13, 20, 30));
        painter.circle_stroke(
            center,
            radius + 2.2 * scale,
            Stroke::new(1.3 * scale, Color32::WHITE),
        );
    }
    painter.circle_stroke(center, radius, Stroke::new(2.1 * scale, color));
    painter.text(
        center,
        Align2::CENTER_CENTER,
        symbol,
        FontId::proportional(10.0 * scale.max(0.8)),
        color,
    );
    if suppressed {
        painter.line_segment(
            [
                center + Vec2::new(-radius, radius),
                center + Vec2::new(radius, -radius),
            ],
            Stroke::new(1.8 * scale, Color32::LIGHT_RED),
        );
    }
}

pub fn draw_fracture_portal(painter: &egui::Painter, center: Pos2, ingress: bool, scale: f32) {
    let color = if ingress {
        Color32::from_rgb(71, 220, 225)
    } else {
        Color32::from_rgb(190, 105, 235)
    };
    let radius = 8.5 * scale.max(0.75);
    painter.circle_stroke(center, radius, Stroke::new(2.4 * scale, color));
    painter.circle_stroke(center, radius * 0.55, Stroke::new(1.3 * scale, color));
    painter.text(
        center,
        Align2::CENTER_CENTER,
        if ingress { "IN" } else { "OUT" },
        FontId::monospace(5.2 * scale.max(0.9)),
        Color32::WHITE,
    );
}

pub fn draw_unit_symbol(
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

/// Why a tile is the colour it is, in the order the rules are consulted.
///
/// The order matters more than the colours: a purged system keeps its fill even when it is selected,
/// and an anomaly beats the selection tint, because what the reader has to see first is what the
/// rules of the board forbid there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileFill {
    /// Nothing lives here any more.
    Purged,
    /// The Fracture's own special area.
    Fracture,
    /// The Nexus, drawn outside the ordinary geometry.
    Nexus,
    /// A system in play beside the board with no hex of its own (an off-map home).
    OffMap,
    /// An ordinary system the hyperlane network crosses.
    Hyperlane,
    /// An anomaly tints the tile; its label comes from [`anomaly_style`].
    Anomaly,
    /// The tile the reader selected, on an otherwise ordinary system.
    Selected,
    /// Deep space.
    Plain,
}

/// The board's fill rule, consulted in the order the variants are named.
#[must_use]
pub fn tile_fill(
    purged: bool,
    special_area: Option<&str>,
    hyperlane: bool,
    anomalies: &[String],
    selected: bool,
) -> (TileFill, Color32) {
    if purged {
        (TileFill::Purged, Color32::from_rgb(24, 24, 28))
    } else if special_area == Some("fracture") {
        (TileFill::Fracture, Color32::from_rgb(39, 25, 57))
    } else if special_area == Some("nexus") {
        (TileFill::Nexus, Color32::from_rgb(25, 48, 61))
    } else if special_area == Some(OFF_MAP_AREA) {
        (TileFill::OffMap, Color32::from_rgb(52, 46, 30))
    } else if hyperlane {
        (TileFill::Hyperlane, Color32::from_rgb(55, 39, 91))
    } else if let Some((color, _)) = anomaly_style(anomalies) {
        (TileFill::Anomaly, color)
    } else if selected {
        (TileFill::Selected, Color32::from_rgb(48, 91, 116))
    } else {
        (TileFill::Plain, Color32::from_rgb(23, 44, 69))
    }
}

/// The six corners of a tile, in the orientation the board and its control rings share.
#[must_use]
pub fn hex_corners(point: Pos2, radius: f32) -> Vec<Pos2> {
    (0..6)
        .map(|corner| {
            let angle = std::f32::consts::FRAC_PI_6 + std::f32::consts::TAU * corner as f32 / 6.0;
            point + Vec2::new(radius * angle.cos(), radius * angle.sin())
        })
        .collect()
}

/// Where the centre of a planet sits inside its tile, relative to the tile centre.
///
/// One planet is centred, two move apart, and three or more spread on a tighter pitch and lift
/// slightly so their labels clear the bottom edge.
#[allow(clippy::cast_precision_loss)] // planet counts per tile are tiny; this is presentation maths
#[must_use]
pub fn planet_offset(index: usize, count: usize) -> (f32, f32) {
    let offset_x = match count {
        // A tile with no planets is never drawn, and answers as though it had one centred planet
        // rather than underflowing the count.
        0 | 1 => 0.0,
        2 => (index as f32 * 2.0 - 1.0) * 22.0,
        _ => (index as f32 - (count - 1) as f32 / 2.0) * 19.0,
    };
    (offset_x, if count > 2 { 24.0 } else { 27.0 })
}

/// Units the board draws as one glyph: everything the reader can tell apart at a glance.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnitStack {
    pub owner: PlayerId,
    pub base: String,
    pub damaged: bool,
    pub galvanized: bool,
    pub count: usize,
}

/// Group units the way the board wants them: by owner, base type, and the two flags it draws.
///
/// The grouping is keyed by the same tuple it is built from, so a stack does not jump between frames
/// because a unit arrived in a different order.
#[must_use]
pub fn unit_stacks(content: &ContentStore, units: &[Unit]) -> Vec<UnitStack> {
    let mut groups: std::collections::BTreeMap<(PlayerId, String, bool, bool), usize> =
        std::collections::BTreeMap::new();
    for unit in units {
        *groups
            .entry((
                unit.owner.clone(),
                unit_base(content, unit),
                unit.sustained_damage,
                unit.galvanized,
            ))
            .or_default() += 1;
    }
    groups
        .into_iter()
        .map(|((owner, base, damaged, galvanized), count)| UnitStack {
            owner,
            base,
            damaged,
            galvanized,
            count,
        })
        .collect()
}

/// A wormhole on a tile, in the order the board draws them, with suppression already resolved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WormholeView {
    pub kind: String,
    /// Whether this is a placed token (white outer rim) or a printed wormhole.
    pub token: bool,
    /// Suppressed wormholes keep their glyph and gain a red slash.
    pub suppressed: bool,
}

/// A planet as the board draws it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanetView {
    pub meta: PlanetMeta,
    pub owner: Option<PlayerId>,
    pub purged: bool,
    pub color: Color32,
    /// `I·B` style summary of the traits and the tech specialty, for the line under the disc.
    pub trait_label: String,
    /// The mark before the name: destroyed, a station, legendary, or nothing.
    pub badge: &'static str,
    /// Other players sharing the planet under a coexistence marker.
    pub coexisting: Vec<PlayerId>,
    pub attachments: usize,
    pub ground: Vec<UnitStack>,
}

/// One visible tile at one frame, with every decision the renderer makes already made.
///
/// Geometry is deliberately absent: positions depend on the space the application was given, while
/// everything here depends only on the frame. That is what lets the reviewer and the replayer agree
/// about a board while drawing it at different sizes.
// Four independent facts about a tile, each drawing its own stroke; collapsing them into one enum
// would lose combinations that occur, such as a selected system that is also an ingress portal.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, PartialEq)]
pub struct TileView {
    pub system: String,
    pub label: String,
    pub q: i32,
    pub r: i32,
    pub special_area: Option<String>,
    pub fill: TileFill,
    pub color: Color32,
    pub anomaly_label: Option<&'static str>,
    /// The single player controlling space, or `None` for empty or contested.
    pub space_owner: Option<PlayerId>,
    /// Sorted, deduplicated controllers of the tile's planets. One draws a ring, several a split.
    pub planet_owners: Vec<PlayerId>,
    pub selected: bool,
    /// Whether the selected system's portal links to this one.
    pub portal_linked: bool,
    pub ingress: bool,
    pub egress: bool,
    pub wormholes: Vec<WormholeView>,
    pub units: Vec<UnitStack>,
    pub command_tokens: Vec<PlayerId>,
    pub token_labels: Vec<String>,
    pub planets: Vec<PlanetView>,
}

/// Axial neighbour steps, in a fixed order.
const HEX_STEPS: [(i32, i32); 6] = [(1, 0), (1, -1), (0, -1), (-1, 0), (-1, 1), (0, 1)];

/// The session's board as it stands in `state`: the recorded tile edits applied (a Creuss hero swap,
/// a Muaat supernova), and every off-map system given a free hex beside the board.
///
/// An off-map system goes next to the on-grid system its printed wormholes reach (the Creuss home
/// beside the Creuss Gate, wherever the Gate is in this frame), on the side facing away from the
/// centre, and further out if that hex is taken. One that reaches nothing on the grid goes outside
/// the ring. Nothing here depends on which factions are seated, so any lineup is laid out the same
/// way; and the result is a function of the frame alone, so every frame of a game agrees.
#[must_use]
pub fn frame_board(
    content: &ContentStore,
    board: &[BoardTile],
    state: &ti4_model::state::GameState,
) -> Vec<BoardTile> {
    let mut tiles = board.to_vec();
    let on_grid = |tile: &BoardTile| tile.special_area.is_none();
    for edit in ti4_engine::movement::recorded_map_edits(state) {
        match edit {
            ti4_engine::movement::MapEdit::Swap { a, b } => {
                let at = |id: &str| tiles.iter().position(|t| on_grid(t) && t.system == id);
                if let (Some(i), Some(j)) = (at(&a), at(&b)) {
                    let (qi, ri) = (tiles[i].q, tiles[i].r);
                    tiles[i].q = tiles[j].q;
                    tiles[i].r = tiles[j].r;
                    tiles[j].q = qi;
                    tiles[j].r = ri;
                }
            }
            ti4_engine::movement::MapEdit::Replace { old, new } => {
                if let Some(i) = tiles.iter().position(|t| on_grid(t) && t.system == old)
                    && let Some(tile) =
                        crate::system_metadata(content, &new, tiles[i].q, tiles[i].r, None)
                {
                    tiles[i] = tile;
                }
            }
        }
    }

    let mut occupied: std::collections::BTreeSet<(i32, i32)> = tiles
        .iter()
        .filter(|tile| on_grid(tile))
        .map(|tile| (tile.q, tile.r))
        .collect();
    let grid: Vec<(String, (i32, i32), Vec<String>)> = tiles
        .iter()
        .filter(|tile| on_grid(tile))
        .map(|tile| {
            (
                tile.system.clone(),
                (tile.q, tile.r),
                tile.wormholes.clone(),
            )
        })
        .collect();
    for tile in tiles
        .iter_mut()
        .filter(|tile| tile.special_area.as_deref() == Some(OFF_MAP_AREA))
    {
        let anchor = grid
            .iter()
            .filter(|(_, _, kinds)| kinds.iter().any(|kind| tile.wormholes.contains(kind)))
            .min_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, hex, _)| *hex);
        let hex = beside(anchor, &occupied);
        tile.q = hex.0;
        tile.r = hex.1;
        occupied.insert(hex);
    }
    tiles
}

/// The first free hex beside `anchor` (the centre when there is none), trying the steps that point
/// most directly away from the centre first and walking further out along each until one is free.
fn beside(
    anchor: Option<(i32, i32)>,
    occupied: &std::collections::BTreeSet<(i32, i32)>,
) -> (i32, i32) {
    let (q, r) = anchor.unwrap_or((0, 0));
    let flat = |q: i32, r: i32| (q as f32 + r as f32 / 2.0, r as f32 * 0.866);
    let (ox, oy) = flat(q, r);
    let mut steps: Vec<(usize, f32)> = HEX_STEPS
        .iter()
        .enumerate()
        .map(|(index, (dq, dr))| {
            let (x, y) = flat(*dq, *dr);
            (index, x * ox + y * oy)
        })
        .collect();
    // Most outward first; the fixed step order breaks ties, so the choice is deterministic.
    steps.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    for distance in 1..=12 {
        for (index, _) in &steps {
            let (dq, dr) = HEX_STEPS[*index];
            let hex = (q + dq * distance, r + dr * distance);
            if !occupied.contains(&hex) {
                return hex;
            }
        }
    }
    (q + 12, r)
}

/// The board as one frame shows it: visible tiles only, in the order the session lists them.
#[must_use]
pub fn board_view(
    content: &ContentStore,
    session: &ReviewSession,
    frame: &ReviewFrame,
    selected: Option<&str>,
) -> Vec<TileView> {
    let state = &frame.state;
    let fracture_visible = state.fracture_in_play;
    let selected_is_ingress =
        selected.is_some_and(|system| state.ingress_tokens.contains(&SystemId::new(system)));
    let selected_is_egress = selected.is_some_and(|system| {
        session
            .board
            .iter()
            .any(|tile| tile.system == system && tile.egress)
    });
    let alpha_beta_suppressed = ti4_engine::laws::wormholes_suppressed(state);

    let board = frame_board(content, &session.board, state);
    let mut tiles = Vec::new();
    for tile in &board {
        if tile.special_area.as_deref() == Some("fracture") && !fracture_visible {
            continue;
        }
        // The Wormhole Nexus: one tile beside the board, two faces, and the face is a fact about
        // this frame. It used to be shown only when `state.board` held that system id — and
        // `GameState::board` is written the first time a unit, a capture or a token touches a
        // system, so the tile was hidden until somebody had already flown into the one place they
        // could only find by looking at it. Whether it is in play at all is the session's business
        // (`board_metadata` lists it only when the map knows it); which face is up is the frame's.
        if tile.special_area.as_deref() == Some("nexus") {
            let face: &str = if state.nexus_unlocked {
                ti4_engine::seating::OPEN_NEXUS
            } else {
                ti4_engine::seating::LOCKED_NEXUS
            };
            if tile.system != face {
                continue;
            }
        }
        let system_id = SystemId::new(&tile.system);
        let system_purged = state.purged_systems.contains(&system_id);
        let is_selected = selected == Some(tile.system.as_str());
        let (fill, color) = tile_fill(
            system_purged,
            tile.special_area.as_deref(),
            tile.hyperlane,
            &tile.anomalies,
            is_selected,
        );
        let nexus_suppressed = tile.special_area.as_deref() == Some("nexus")
            && ti4_engine::laws::nexus_wormholes_suppressed(state);
        let system_state = state.board.get(&system_id);
        let tile_planets = planets_for_tile(session, frame, tile);

        let mut space_owners: Vec<&PlayerId> = system_state
            .into_iter()
            .flat_map(|system| system.units.iter())
            .filter(|unit| {
                ti4_content::units::unit_type(content, unit.type_id.as_str(), FULL)
                    .is_some_and(|kind| kind.is_ship())
            })
            .map(|unit| &unit.owner)
            .collect();
        space_owners.sort();
        space_owners.dedup();

        let mut planet_owners: Vec<&PlayerId> = system_state
            .into_iter()
            .flat_map(|system| {
                tile_planets.iter().filter_map(|planet| {
                    let planet_id = PlanetId::new(&planet.id);
                    (!system.purged_planets.contains(&planet_id))
                        .then(|| system.planet_control.get(&planet_id))
                        .flatten()
                })
            })
            .collect();
        planet_owners.sort();
        planet_owners.dedup();

        let mut wormholes: Vec<WormholeView> = Vec::new();
        for kind in &tile.wormholes {
            let suppressed = matches!(kind.as_str(), "ALPHA" | "BETA")
                && (alpha_beta_suppressed || nexus_suppressed);
            wormholes.push(WormholeView {
                kind: kind.clone(),
                token: false,
                suppressed,
            });
        }
        for (kind, system) in &state.wormhole_tokens {
            if system != &system_id {
                continue;
            }
            let suppressed = matches!(kind.as_str(), "ALPHA" | "BETA") && alpha_beta_suppressed;
            wormholes.push(WormholeView {
                kind: kind.clone(),
                token: true,
                suppressed,
            });
        }
        if let Some((system, face)) = &state.ion_storm
            && system == &system_id
        {
            let suppressed = matches!(face.as_str(), "ALPHA" | "BETA") && alpha_beta_suppressed;
            wormholes.push(WormholeView {
                kind: face.clone(),
                token: true,
                suppressed,
            });
        }

        let units =
            system_state.map_or_else(Vec::new, |system| unit_stacks(content, &system.units));
        let command_tokens = system_state.map_or_else(Vec::new, |system| {
            system.command_tokens.iter().cloned().collect()
        });

        let mut token_labels = Vec::new();
        if state.frontier_tokens.contains(&system_id) {
            token_labels.push("Frontier".to_owned());
        }
        if state.breach_tokens.contains(&system_id) {
            token_labels.push("Breach".to_owned());
        }
        if state.thunders_edge_system.as_ref() == Some(&system_id) {
            token_labels.push("Thunder's Edge".to_owned());
        }

        let planets = tile_planets
            .iter()
            .map(|planet| {
                let planet_id = PlanetId::new(&planet.id);
                let owner =
                    system_state.and_then(|system| system.planet_control.get(&planet_id).cloned());
                let purged =
                    system_state.is_some_and(|system| system.purged_planets.contains(&planet_id));
                let color = if purged {
                    Color32::from_rgb(38, 38, 42)
                } else {
                    owner
                        .as_ref()
                        .map_or(Color32::from_rgb(91, 96, 105), player_color)
                };
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
                let trait_label = if trait_label.is_empty() || tech_label.is_empty() {
                    trait_label
                } else {
                    format!("{trait_label}·{tech_label}")
                };
                let ground = system_state
                    .and_then(|system| system.planet_units.get(&planet_id))
                    .map_or_else(Vec::new, |units| unit_stacks(content, units));
                PlanetView {
                    meta: planet.clone(),
                    owner,
                    purged,
                    color,
                    trait_label,
                    badge: if purged {
                        "× "
                    } else if planet.space_station {
                        "S "
                    } else if planet.legendary {
                        "★"
                    } else {
                        ""
                    },
                    coexisting: system_state
                        .and_then(|system| system.coexisting.get(&planet_id))
                        .map_or_else(Vec::new, |set| set.iter().cloned().collect()),
                    attachments: state
                        .planet_attachments
                        .get(&planet_id)
                        .filter(|attachments| !attachments.is_empty())
                        .map_or(0, Vec::len),
                    ground,
                }
            })
            .collect();

        tiles.push(TileView {
            system: tile.system.clone(),
            label: if system_purged {
                format!("{} · PURGED", tile.label)
            } else if tile.special_area.as_deref() == Some(OFF_MAP_AREA) {
                format!("{} · off-map", tile.label)
            } else {
                tile.label.clone()
            },
            q: tile.q,
            r: tile.r,
            special_area: tile.special_area.clone(),
            fill,
            color,
            anomaly_label: anomaly_style(&tile.anomalies).map(|(_, label)| label),
            // Only a single controller paints the thick ring; a contested system paints none.
            space_owner: match space_owners.as_slice() {
                [owner] => Some((*owner).clone()),
                _ => None,
            },
            planet_owners: planet_owners.into_iter().cloned().collect(),
            selected: is_selected,
            portal_linked: (selected_is_ingress && tile.egress)
                || (selected_is_egress && state.ingress_tokens.contains(&system_id)),
            ingress: state.ingress_tokens.contains(&system_id),
            egress: tile.egress,
            wormholes,
            units,
            command_tokens,
            token_labels,
            planets,
        });
    }
    tiles
}

/// A learned number as the reviewer writes it: five decimals, or an em dash for "not recorded".
#[must_use]
pub fn precision(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.5}"))
}

/// A feature value as the reviewer writes it: three decimals, or an em dash.
#[must_use]
pub fn precision3(value: Option<f64>) -> String {
    value.map_or_else(|| "—".to_owned(), |value| format!("{value:.3}"))
}

/// Pretty JSON, or the error that explains why there isn't any. The panel has never swallowed a
/// serialisation failure, and it still doesn't.
#[must_use]
pub fn json_pretty(value: &serde_json::Value) -> String {
    serde_json::to_string_pretty(value).unwrap_or_else(|error| error.to_string())
}

/// The line of numbers a reader looks at before the map: where the engine is, and who it is waiting
/// on.
#[derive(Clone, Debug, PartialEq)]
pub struct StepView {
    pub engine_step: usize,
    pub decision_count: usize,
    pub action_count: usize,
    pub round: u32,
    /// The phase as the reviewer spells it, which is the debug spelling of the model's enum.
    pub phase: String,
    /// The active seat, or the em dash the panel shows when the engine is between players.
    pub active: String,
    pub active_system: String,
    pub pending: String,
    pub combat_round: u32,
    /// Votes, predictions and the reroll count, in the order the panel lists them.
    pub agenda_lines: Vec<String>,
    pub error: Option<String>,
}

/// What frame a reader is looking at, in the words the reviewer uses.
#[must_use]
pub fn step_view(frame: &ReviewFrame) -> StepView {
    let state = &frame.state;
    let mut agenda_lines = Vec::new();
    if !state.reroll_staging.is_empty() {
        agenda_lines.push(format!(
            "Reroll staging: {} player(s)",
            state.reroll_staging.len()
        ));
    }
    for (player, vote) in &state.agenda_votes {
        agenda_lines.push(format!("Vote: {player} → {vote}"));
    }
    for (player, prediction) in &state.agenda_predictions {
        agenda_lines.push(format!("Prediction: {player} → {prediction}"));
    }
    StepView {
        engine_step: frame.engine_step,
        decision_count: frame.decision_count,
        action_count: frame.action_count,
        round: frame.round,
        phase: format!("{:?}", frame.phase),
        active: frame.active.clone().unwrap_or_else(|| "—".to_owned()),
        active_system: state
            .active_system
            .as_ref()
            .map_or_else(|| "—".to_owned(), |system| system.as_str().to_owned()),
        pending: state.pending.clone().unwrap_or_else(|| "—".to_owned()),
        combat_round: state.combat_round_seq,
        agenda_lines,
        error: frame.error.clone(),
    }
}

/// Who actually chose, which is not always the model: a fleet plan carries out a decision made
/// earlier, and showing it as a policy choice would misrepresent the run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionPath {
    /// A decision the policy made when asked: `seeing-mlp`, `seeing` or `blind`.
    Policy,
    /// A fleet plan executing an earlier decision.
    FleetPlan,
    /// Anything else, kept verbatim because an unknown path is information.
    Other(String),
}

impl DecisionPath {
    /// What the panel prints beside the decision, or `None` for an ordinary policy choice.
    #[must_use]
    pub fn annotation(&self) -> Option<&str> {
        match self {
            Self::Policy => None,
            Self::FleetPlan => Some("fleet decision"),
            Self::Other(other) => Some(other.as_str()),
        }
    }
}

/// Classify a decision path. The three policy spellings are exhaustive of what the engine records
/// today; anything new arrives as [`DecisionPath::Other`] and is shown rather than hidden.
#[must_use]
pub fn path_kind(path: &str) -> DecisionPath {
    match path {
        "seeing-mlp" | "seeing" | "blind" => DecisionPath::Policy,
        "fleet decision" => DecisionPath::FleetPlan,
        other => DecisionPath::Other(other.to_owned()),
    }
}

/// One row of the feature/weight/contribution grid.
#[derive(Clone, Debug, PartialEq)]
pub struct FeatureRow {
    pub name: String,
    pub value: String,
    pub weight: String,
    pub contribution: String,
}

/// One option as the reviewer presents it, chosen options first-class because the header opens on
/// the choice the policy actually made.
#[derive(Clone, Debug, PartialEq)]
pub struct OptionRow {
    pub id: String,
    pub kind: String,
    pub label: String,
    pub score: Option<f64>,
    pub probability: Option<f64>,
    /// `✓ ` when the policy chose it.
    pub selected: bool,
    /// The header text: label, score and probability in the reviewer's format.
    pub title: String,
    /// Diplomacy reading of the option, one strong line each.
    pub detail_lines: Vec<String>,
    pub payload: Option<String>,
    pub preview: Option<String>,
    pub features: Vec<FeatureRow>,
}

/// Where the chosen option sat among the ranked ones.
#[derive(Clone, Debug, PartialEq)]
pub struct RankInfo {
    pub position: usize,
    pub ranked: usize,
    pub probability: String,
    pub best: String,
    /// True when the policy sampled something other than the highest-probability option.
    pub below_greedy: bool,
}

/// One policy decision of the engine step, with everything the reviewer shows about it.
#[derive(Clone, Debug, PartialEq)]
pub struct DecisionRow {
    pub sequence: usize,
    pub player: String,
    pub faction: String,
    pub path: DecisionPath,
    pub prompt: String,
    pub context: Option<String>,
    /// `head → resolved · temperature T · chosen C`, the reviewer's one-line summary.
    pub summary: String,
    pub rank: Option<RankInfo>,
    pub options: Vec<OptionRow>,
}

/// The decisions of one engine step, in the order the engine settled them.
#[must_use]
pub fn decision_rows(frame: &ReviewFrame) -> Vec<DecisionRow> {
    frame
        .decisions
        .iter()
        .map(|decision| {
            let chosen = decision.chosen.as_deref();
            let selected_probability = chosen.and_then(|chosen| {
                decision
                    .options
                    .iter()
                    .find(|option| option.id == chosen)
                    .and_then(|option| option.probability)
            });
            let mut ranked: Vec<&crate::OptionDetail> = decision
                .options
                .iter()
                .filter(|option| option.probability.is_some())
                .collect();
            ranked.sort_by(|left, right| {
                right
                    .probability
                    .unwrap_or_default()
                    .total_cmp(&left.probability.unwrap_or_default())
            });
            let rank = selected_probability.map(|probability| RankInfo {
                position: ranked
                    .iter()
                    .position(|option| Some(option.id.as_str()) == chosen)
                    .map_or(0, |index| index + 1),
                ranked: ranked.len(),
                probability: format!("{probability:.5}"),
                best: format!(
                    "{:.5}",
                    ranked
                        .first()
                        .and_then(|option| option.probability)
                        .unwrap_or(probability)
                ),
                // The reviewer's wording is about rank, not about probability: a tie at the top with
                // the choice in second place still reads as "below the greedy choice".
                below_greedy: ranked
                    .iter()
                    .position(|option| Some(option.id.as_str()) == chosen)
                    .is_some_and(|index| index > 0),
            });
            let options = decision
                .options
                .iter()
                .map(|option| {
                    let selected = chosen == Some(option.id.as_str());
                    OptionRow {
                        id: option.id.clone(),
                        kind: option.kind.clone(),
                        label: option.label.clone(),
                        score: option.score,
                        probability: option.probability,
                        selected,
                        title: format!(
                            "{}{} · score {} · p {}",
                            if selected { "✓ " } else { "" },
                            option.label,
                            precision(option.score),
                            precision(option.probability)
                        ),
                        detail_lines: crate::diplomacy::option_lines(&frame.state, option),
                        payload: (!option.payload.is_empty()).then(|| {
                            json_pretty(&serde_json::to_value(&option.payload).unwrap_or_default())
                        }),
                        preview: option.preview.as_ref().map(json_pretty),
                        features: option
                            .features
                            .iter()
                            .map(|feature| FeatureRow {
                                name: feature.name.clone(),
                                value: precision3(Some(feature.value)),
                                weight: feature.weight.map_or_else(
                                    || "nonlinear".to_owned(),
                                    |value| format!("{value:.5}"),
                                ),
                                contribution: precision(feature.contribution),
                            })
                            .collect(),
                    }
                })
                .collect();
            DecisionRow {
                sequence: decision.sequence,
                player: decision.player.clone(),
                faction: decision.faction.clone(),
                path: path_kind(&decision.path),
                prompt: decision.prompt.clone(),
                context: decision.context.as_ref().map(json_pretty),
                summary: format!(
                    "{} → {} · temperature {:?} · chosen {}",
                    decision.requested_head,
                    decision.resolved_head,
                    decision.temperature,
                    decision.chosen.as_deref().unwrap_or("ERROR")
                ),
                rank,
                options,
            }
        })
        .collect()
}

/// The turn-level summary above the decisions: what the active player did, or is doing.
#[derive(Clone, Debug, PartialEq)]
pub struct ActionSummaryView {
    /// `Action in progress` or `Latest completed action`, which is the difference between reading a
    /// turn as half-played and reading it as history.
    pub title: &'static str,
    pub headline: Option<String>,
    pub span: Option<String>,
    pub details: Vec<String>,
}

/// The action summary a frame shows, which is the in-progress one when there is one and otherwise
/// the most recent completed action, however many frames back that is.
#[must_use]
pub fn action_summary(session: &ReviewSession, frame: &ReviewFrame) -> ActionSummaryView {
    action_summary_in(&session.frames, frame)
}

/// `action_summary`, reading its history out of a frame list the caller supplies.
///
/// The replayer's store keeps one session *shell* per branch with `frames` emptied and the frames in a
/// list beside it - a recording is hundreds of megabytes and a branch tree must not multiply that - so
/// nothing a replayer sheet reads can come from `session.frames`. Reaching for it there is how opening
/// a table ended the window: `session.frames[..=frame.index]` on an empty shell panics with
/// "range end index 0 out of range for slice of length 0", and again with "index out of bounds: the len
/// is 0 but the index is 39" at the previous-frame row. A sheet may lose a history row when the history
/// it was handed does not contain the frame; it may not take the game with it.
#[must_use]
pub fn action_summary_in(frames: &[ReviewFrame], frame: &ReviewFrame) -> ActionSummaryView {
    let in_progress = frame.action_in_progress.is_some();
    let summary = frame.action_in_progress.as_ref().or_else(|| {
        frames
            .iter()
            .rev()
            .filter(|candidate| candidate.index <= frame.index)
            .find_map(|candidate| candidate.action_summary.as_ref())
    });
    let Some(summary) = summary else {
        return ActionSummaryView {
            title: if in_progress {
                "Action in progress"
            } else {
                "Latest completed action"
            },
            headline: None,
            span: None,
            details: Vec::new(),
        };
    };
    ActionSummaryView {
        title: if in_progress {
            "Action in progress"
        } else {
            "Latest completed action"
        },
        headline: Some(summary.headline.clone()),
        span: Some(format!(
            "frames {}–{} · active-player period{}",
            summary.start_frame,
            summary.end_frame,
            if summary.in_progress {
                " · IN PROGRESS"
            } else {
                ""
            }
        )),
        details: summary.details.clone(),
    }
}

/// The agenda on the table at this frame: revealed and not yet resolved or discarded.
///
/// The vote the engine asks for names only its outcomes ("vote for which outcome"), so a reader
/// needs the card from somewhere else; the engine's own `AGENDA_REVEALED:<alias>` event says which
/// it is. Scans back from this frame and stops at the first sign of an agenda closing, or at a frame
/// outside the agenda phase.
#[must_use]
pub fn current_agenda(frames: &[ReviewFrame], frame: &ReviewFrame) -> Option<String> {
    if frame.phase != ti4_model::state::Phase::Agenda {
        return None;
    }
    let earlier = frames
        .iter()
        .filter(|candidate| candidate.index < frame.index)
        .rev();
    for candidate in std::iter::once(frame).chain(earlier) {
        if candidate.phase != ti4_model::state::Phase::Agenda {
            return None;
        }
        for event in candidate.new_events.iter().rev() {
            if event.starts_with("AGENDA_RESOLVED:") || event.starts_with("AGENDA_DISCARDED:") {
                return None;
            }
            if let Some(alias) = event.strip_prefix("AGENDA_REVEALED:") {
                return Some(alias.to_owned());
            }
        }
    }
    None
}

/// An agenda card as lines: name with its type and what is elected, then its text.
#[must_use]
pub fn agenda_card(content: &ContentStore, alias: &str) -> Vec<String> {
    let Some(record) = content.get(ContentType::Agendas, alias) else {
        return vec![format!("Agenda: {alias}")];
    };
    let field = |key: &str| record.text(key).unwrap_or_default().to_owned();
    let mut lines = vec![format!(
        "Agenda: {} · {} · {}",
        record.text("name").unwrap_or(alias),
        field("type"),
        field("target")
    )];
    lines.extend(
        ["text1", "text2"]
            .into_iter()
            .map(field)
            .filter(|text| !text.is_empty()),
    );
    lines
}

/// The dice behind what is on screen: every roll from the start of the action this frame belongs
/// to up to this frame, so a combat reads whole rather than one step at a time. Outside any action
/// (setup, strategy, agenda) it is this frame's own rolls.
#[must_use]
pub fn rolls_for_action(
    frames: &[ReviewFrame],
    frame: &ReviewFrame,
) -> Vec<ti4_engine::dice::Roll> {
    let start = frame
        .action_in_progress
        .as_ref()
        .map(|summary| summary.start_frame)
        .or_else(|| {
            frame
                .action_summary
                .as_ref()
                .map(|summary| summary.start_frame)
        });
    let Some(start) = start else {
        return frame.rolls.clone();
    };
    let mut rolls: Vec<ti4_engine::dice::Roll> = frames
        .iter()
        .filter(|candidate| candidate.index >= start && candidate.index < frame.index)
        .flat_map(|candidate| candidate.rolls.iter().cloned())
        .collect();
    // This frame last, and from the frame itself: a replayer shell's history may not hold it yet.
    rolls.extend(frame.rolls.iter().cloned());
    rolls
}

/// One roll as a line: `seat2 "The Emirates of Hacan" · space combat · hits on 7+: 3 8 9 → 2 hits`.
///
/// Rerolled dice are marked `*`. A roll that is not a hit roll shows its faces alone, and a roll
/// whose roller the engine did not record says so rather than guessing.
#[must_use]
pub fn roll_line(
    frame: &ReviewFrame,
    roll: &ti4_engine::dice::Roll,
    content: &ContentStore,
) -> String {
    let who = roll.by.as_ref().map_or_else(
        || "(roller not recorded)".to_owned(),
        |seat| seat_name(frame, &PlayerId::new(seat), content),
    );
    let faces = roll
        .faces
        .iter()
        .enumerate()
        .map(|(index, face)| {
            if roll.rerolled.contains(&index) {
                format!("{face}*")
            } else {
                face.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ");
    match roll.hits_on {
        // A rift die is not a hit roll: 4+ is the ship surviving the exit (41.2).
        Some(on) if roll.reason == "gravity rift" => format!(
            "{who} · {} · survives on {on}+: {faces} → {}",
            roll.reason,
            if roll.hits() > 0 {
                "survives"
            } else {
                "destroyed"
            }
        ),
        Some(on) => {
            let hits = roll.hits();
            format!(
                "{who} · {} · hits on {on}+: {faces} → {hits} hit{}",
                roll.reason,
                if hits == 1 { "" } else { "s" }
            )
        }
        None => format!("{who} · {} · {faces}", roll.reason),
    }
}

/// One structured engine event, ready to fold open.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRow {
    pub id: u64,
    pub event_type: String,
    pub cancelled: bool,
    pub title: String,
    pub payload: String,
}

/// The events this step added, in engine order.
#[must_use]
pub fn event_rows(frame: &ReviewFrame) -> Vec<EventRow> {
    frame
        .structured_events
        .iter()
        .map(|event| EventRow {
            id: event.id,
            event_type: event.event_type.clone(),
            cancelled: event.cancelled,
            title: format!(
                "#{} {}{}",
                event.id,
                event.event_type,
                if event.cancelled { " · CANCELLED" } else { "" }
            ),
            payload: json_pretty(&serde_json::to_value(&event.payload).unwrap_or_default()),
        })
        .collect()
}

/// The tile the reader clicked, in the words the reviewer uses. Emphasis is the caller's choice; the
/// order and the content are the model's.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct SelectedSystemView {
    pub title: String,
    /// False for a session that recorded no map metadata for this system, which the panel says out
    /// loud instead of showing an empty card.
    pub has_metadata: bool,
    pub lines: Vec<String>,
    pub planets: Vec<String>,
    /// The dynamic system state as JSON, or `None` when nothing is in the system.
    pub dynamic: Option<String>,
}

/// Describe a selected system. A legacy session without map metadata says so rather than showing an
/// empty card, which is the difference between "nothing here" and "nothing recorded".
#[must_use]
pub fn selected_system(
    session: &ReviewSession,
    frame: &ReviewFrame,
    system: &str,
) -> SelectedSystemView {
    let mut view = SelectedSystemView {
        title: format!("Selected system {system}"),
        ..SelectedSystemView::default()
    };
    if let Some(metadata) = session
        .board
        .iter()
        .find(|candidate| candidate.system == system)
    {
        view.has_metadata = true;
        view.title = format!("Selected system {} [{}]", metadata.label, metadata.system);
        view.lines
            .push(format!("Map coordinate: {}, {}", metadata.q, metadata.r));
        if let Some(area) = &metadata.special_area {
            view.lines.push(format!("Special area: {area}"));
        }
        if metadata.hyperlane {
            view.lines.push("Hyperlane system".to_owned());
        }
        if !metadata.anomalies.is_empty() {
            view.lines
                .push(format!("Anomalies: {}", metadata.anomalies.join(", ")));
        }
        if !metadata.wormholes.is_empty() {
            view.lines
                .push(format!("Wormholes: {}", metadata.wormholes.join(", ")));
        }
        if metadata.egress {
            view.lines.push("Fracture egress".to_owned());
        }
        let planets = planets_for_tile(session, frame, metadata);
        view.planets = planets
            .iter()
            .map(|planet| {
                let traits = if planet.traits.is_empty() {
                    "—".to_owned()
                } else {
                    planet.traits.join(", ")
                };
                let specialties = if planet.tech_specialties.is_empty() {
                    "—".to_owned()
                } else {
                    planet.tech_specialties.join(", ")
                };
                format!(
                    "• {} [{}] · {}/{} · trait {traits} · specialty {specialties}{}{}",
                    planet.label,
                    planet.id,
                    planet.resources,
                    planet.influence,
                    if planet.legendary {
                        " · legendary"
                    } else {
                        ""
                    },
                    if planet.space_station {
                        " · space station"
                    } else {
                        ""
                    },
                )
            })
            .collect();
    } else {
        view.lines
            .push("Map metadata unavailable in this legacy review.".to_owned());
    }
    view.dynamic = frame
        .state
        .board
        .get(&ti4_model::id::SystemId::new(system))
        .map(|state| json_pretty(&serde_json::to_value(state).unwrap_or_default()));
    view
}

/// Where the board goes in the space it was given.
///
/// Meaning is a function of the frame ([`board_view`]); position is a function of the window, and
/// this is that part. Keeping the two apart is what lets two applications paint the same board into
/// different rectangles without either restating the other's arithmetic - and it makes the geometry
/// testable, which a call into a painter never was.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoardLayout {
    /// The painter's rectangle, whose edges anchor the Fracture row and the Nexus.
    pub rect: egui::Rect,
    /// The space the UI asked for before egui trimmed it; the scale used to be computed from this.
    pub size: Vec2,
    /// Pixels per design unit, clamped so a maximised window does not dwarf the hexes.
    pub scale: f32,
    /// Centre of the hex ring, nudged up when the Fracture needs its own band below.
    pub center: Pos2,
    /// One tile's circumradius.
    pub radius: f32,
}

impl BoardLayout {
    /// Lay the board out inside `rect`, given `size` as the space that was asked for.
    #[must_use]
    pub fn new(rect: egui::Rect, size: Vec2, fracture_visible: bool) -> Self {
        Self::with_design(rect, size, fracture_visible, Vec2::new(1150.0, 900.0), 0.45)
    }

    /// [`Self::new`], sized to the tiles this frame draws: a board whose hexes reach past the
    /// usual three rings (an off-map home placed beside the edge, a larger map) shrinks to keep
    /// every tile inside `rect`. A standard board gets exactly the scale [`Self::new`] gives.
    #[must_use]
    pub fn fitted(rect: egui::Rect, size: Vec2, tiles: &[TileView]) -> Self {
        let on_grid = tiles.iter().filter(|tile| {
            tile.special_area.is_none() || tile.special_area.as_deref() == Some(OFF_MAP_AREA)
        });
        let (mut reach_x, mut reach_r) = (3.0_f32, 3.0_f32);
        for tile in on_grid {
            reach_x = reach_x.max((tile.q as f32 + tile.r as f32 / 2.0).abs());
            reach_r = reach_r.max((tile.r as f32).abs());
        }
        let design = Vec2::new(
            1150.0 + 2.0 * 126.0 * (reach_x - 3.0),
            900.0 + 2.0 * 108.0 * (reach_r - 3.0),
        );
        // A wider board may go smaller than the standard floor, or its outer tiles leave the rect.
        let floor = if design.x > 1150.0 || design.y > 900.0 {
            0.3
        } else {
            0.45
        };
        Self::with_design(rect, size, fracture_shown(tiles), design, floor)
    }

    fn with_design(
        rect: egui::Rect,
        size: Vec2,
        fracture_visible: bool,
        design: Vec2,
        floor: f32,
    ) -> Self {
        let scale = (size.x / design.x).min(size.y / design.y).clamp(floor, 1.2);
        let center =
            rect.center() - Vec2::new(0.0, if fracture_visible { 72.0 * scale } else { 0.0 });
        Self {
            rect,
            size,
            scale,
            center,
            radius: 58.0 * scale,
        }
    }
}

/// The centre of one tile: the Fracture's own band along the bottom, the Nexus in the lower-left
/// corner, and everything else on the axial hex grid.
#[must_use]
pub fn tile_point(layout: &BoardLayout, tile: &TileView) -> Pos2 {
    match tile.special_area.as_deref() {
        Some("fracture") => Pos2::new(
            layout.rect.center().x + (tile.q as f32 - 3.0) * 100.0 * layout.scale,
            layout.rect.bottom() - 61.0 * layout.scale,
        ),
        Some("nexus") => Pos2::new(
            layout.rect.left() + 72.0 * layout.scale,
            layout.rect.bottom() - 61.0 * layout.scale,
        ),
        _ => Pos2::new(
            layout.center.x + 126.0 * layout.scale * (tile.q as f32 + tile.r as f32 / 2.0),
            layout.center.y + 108.0 * layout.scale * tile.r as f32,
        ),
    }
}

/// Whether the Fracture is on the board at all, which is what decides whether its band gets a title.
///
/// This is [`ReviewFrame`]'s `fracture_in_play` seen through the tiles: [`board_view`] only puts the
/// Fracture on the board when it is in play, so asking the tiles keeps the painter from needing the
/// frame a second time.
#[must_use]
pub fn fracture_shown(tiles: &[TileView]) -> bool {
    tiles
        .iter()
        .any(|tile| tile.special_area.as_deref() == Some("fracture"))
}

/// Paint one frame's board and report the tile the pointer chose, if any.
///
/// The reviewer and the replayer call this. It returns the chosen system rather than storing it, so
/// each application keeps its own selection state while sharing every pixel of the drawing.
pub fn draw_board(
    painter: &egui::Painter,
    response: &egui::Response,
    layout: &BoardLayout,
    tiles: &[TileView],
) -> Option<String> {
    let mut selected: Option<String> = None;
    if fracture_shown(tiles) {
        painter.text(
            Pos2::new(
                layout.rect.center().x,
                layout.rect.bottom() - 124.0 * layout.scale,
            ),
            Align2::CENTER_CENTER,
            "THE FRACTURE · SPECIAL AREA",
            FontId::proportional(10.0 * layout.scale.max(0.85)),
            Color32::from_rgb(205, 151, 239),
        );
    }
    for tile in tiles {
        let point = tile_point(layout, tile);
        let points = hex_corners(point, layout.radius);
        painter.add(Shape::convex_polygon(
            points.clone(),
            tile.color,
            Stroke::new(1.4, Color32::from_rgb(112, 152, 189)),
        ));
        if let Some(label) = tile.anomaly_label {
            painter.text(
                point + Vec2::new(0.0, -32.0 * layout.scale),
                Align2::CENTER_CENTER,
                label,
                FontId::monospace(6.2 * layout.scale.max(0.9)),
                Color32::from_rgb(239, 214, 178),
            );
        }
        if let Some(owner) = &tile.space_owner {
            painter.add(Shape::closed_line(
                points.clone(),
                Stroke::new(5.0 * layout.scale.max(0.75), player_color(owner)),
            ));
        }
        let ownership_ring: Vec<Pos2> = points
            .iter()
            .map(|corner| point + (*corner - point) * 0.92)
            .collect();
        if let [owner] = tile.planet_owners.as_slice() {
            painter.add(Shape::closed_line(
                ownership_ring.clone(),
                Stroke::new(2.2 * layout.scale.max(0.8), player_color(owner)),
            ));
        } else if tile.planet_owners.len() > 1 {
            let owners = &tile.planet_owners;
            for index in 0..ownership_ring.len() {
                painter.line_segment(
                    [
                        ownership_ring[index],
                        ownership_ring[(index + 1) % ownership_ring.len()],
                    ],
                    Stroke::new(
                        2.5 * layout.scale.max(0.8),
                        player_color(&owners[index % owners.len()]),
                    ),
                );
            }
        }
        if tile.selected || tile.portal_linked {
            let inner: Vec<Pos2> = points
                .iter()
                .map(|corner| point + (*corner - point) * 0.91)
                .collect();
            painter.add(Shape::closed_line(inner, Stroke::new(2.5, Color32::WHITE)));
        }
        painter.text(
            point + Vec2::new(0.0, -45.0 * layout.scale),
            Align2::CENTER_CENTER,
            tile.label.clone(),
            FontId::proportional(9.5 * layout.scale.max(0.85)),
            Color32::WHITE,
        );
        for (index, wormhole) in tile.wormholes.iter().enumerate() {
            draw_wormhole(
                painter,
                point
                    + Vec2::new(
                        (-42.0 + index as f32 * 18.0) * layout.scale,
                        -18.0 * layout.scale,
                    ),
                &wormhole.kind,
                wormhole.token,
                wormhole.suppressed,
                layout.scale,
            );
        }
        if tile.ingress {
            draw_fracture_portal(
                painter,
                point + Vec2::new(43.0 * layout.scale, 31.0 * layout.scale),
                true,
                layout.scale,
            );
        }
        if tile.egress {
            draw_fracture_portal(
                painter,
                point + Vec2::new(43.0 * layout.scale, 31.0 * layout.scale),
                false,
                layout.scale,
            );
        }

        for (index, stack) in tile.units.iter().enumerate() {
            let column = index % 5;
            let row = index / 5;
            let unit_center = point
                + Vec2::new(
                    (column as f32 - 2.0) * 18.0 * layout.scale,
                    (-23.0 + row as f32 * 20.0) * layout.scale,
                );
            draw_unit_symbol(
                painter,
                unit_center,
                &stack.base,
                player_color(&stack.owner),
                stack.count,
                stack.damaged,
                stack.galvanized,
                layout.scale,
            );
        }
        for (index, owner) in tile.command_tokens.iter().enumerate() {
            let at = point
                + Vec2::new(
                    (-42.0 + index as f32 * 10.0) * layout.scale,
                    42.0 * layout.scale,
                );
            painter.circle_filled(at, 3.5 * layout.scale, player_color(owner));
            painter.circle_stroke(at, 3.5 * layout.scale, Stroke::new(1.0, Color32::WHITE));
        }

        if !tile.token_labels.is_empty() {
            painter.text(
                point + Vec2::new(0.0, 45.0 * layout.scale),
                Align2::CENTER_CENTER,
                tile.token_labels.join(" · "),
                FontId::proportional(6.2 * layout.scale.max(0.85)),
                Color32::from_rgb(129, 221, 237),
            );
        }

        for (planet_index, planet) in tile.planets.iter().enumerate() {
            let (offset_x, offset_y) = planet_offset(planet_index, tile.planets.len());
            let planet_center = point + Vec2::new(offset_x * layout.scale, offset_y * layout.scale);
            let planet_radius = 14.5 * layout.scale.max(0.72);
            painter.circle_filled(
                planet_center,
                planet_radius,
                planet.color.gamma_multiply(0.72),
            );
            painter.circle_stroke(
                planet_center,
                planet_radius,
                Stroke::new(2.0 * layout.scale.max(0.8), planet.color),
            );
            painter.text(
                planet_center + Vec2::new(0.0, -5.0 * layout.scale),
                Align2::CENTER_CENTER,
                format!("{}/{}", planet.meta.resources, planet.meta.influence),
                FontId::monospace(7.2 * layout.scale.max(0.85)),
                Color32::WHITE,
            );
            painter.text(
                planet_center + Vec2::new(0.0, 5.0 * layout.scale),
                Align2::CENTER_CENTER,
                planet.trait_label.clone(),
                FontId::monospace(6.2 * layout.scale.max(0.9)),
                Color32::WHITE,
            );
            painter.text(
                planet_center + Vec2::new(0.0, planet_radius + 1.0),
                Align2::CENTER_TOP,
                format!("{}{}", planet.badge, planet.meta.label),
                FontId::proportional(6.4 * layout.scale.max(0.9)),
                Color32::LIGHT_GRAY,
            );

            for (index, coexistor) in planet.coexisting.iter().enumerate() {
                let angle =
                    std::f32::consts::TAU * index as f32 / planet.coexisting.len().max(1) as f32;
                let marker =
                    planet_center + Vec2::angled(angle) * (planet_radius + 4.0 * layout.scale);
                painter.circle_filled(marker, 2.8 * layout.scale, player_color(coexistor));
                painter.circle_stroke(marker, 2.8 * layout.scale, Stroke::new(0.8, Color32::WHITE));
            }
            if planet.attachments > 0 {
                painter.text(
                    planet_center + Vec2::new(planet_radius, -planet_radius),
                    Align2::CENTER_CENTER,
                    format!("+{}", planet.attachments),
                    FontId::monospace(6.0 * layout.scale.max(0.9)),
                    Color32::YELLOW,
                );
            }
            for (unit_index, stack) in planet.ground.iter().enumerate() {
                draw_unit_symbol(
                    painter,
                    planet_center
                        + Vec2::new(
                            (-8.0 + unit_index as f32 * 12.0) * layout.scale,
                            -planet_radius * 0.85,
                        ),
                    &stack.base,
                    player_color(&stack.owner),
                    stack.count,
                    stack.damaged,
                    stack.galvanized,
                    layout.scale * 0.72,
                );
            }
        }
        if response.clicked()
            && response
                .interact_pointer_pos()
                .is_some_and(|cursor| cursor.distance(point) <= layout.radius)
        {
            selected = Some(tile.system.clone());
        }
    }
    selected
}
