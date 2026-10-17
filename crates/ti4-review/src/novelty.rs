//! Notes for rules effects that are out of the ordinary: the state that only exists because a
//! faction module wrote it (Mahact holding other colours' command tokens, Nekro's copied
//! technologies, captured units, attachments, every `faction_marks` entry).
//!
//! A reviewer hunting bugs wants to see the frame where such a thing *appears or changes*, and
//! above all the first time this game that a kind of effect happens at all, because a first
//! occurrence is the code path no earlier game has exercised.

use std::collections::{BTreeMap, BTreeSet};

use ti4_model::state::GameState;

use crate::ReviewFrame;

/// Most notes shown for one frame; the rest are counted.
const MAX_NOTES: usize = 12;
const MAX_VALUE: usize = 70;

/// One change to out-of-the-ordinary state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Note {
    pub text: String,
    /// No earlier frame of this session had this kind of effect.
    pub first: bool,
}

/// `key -> value` for every surface that only unusual rules effects write to.
fn surfaces(state: &GameState) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for (key, value) in &state.faction_marks {
        out.insert(format!("mark:{key}"), value.clone());
    }
    for player in &state.players {
        for (card, tech) in &player.assimilated_technologies {
            out.insert(
                format!("assimilated:{}:{card}", player.id),
                tech.to_string(),
            );
        }
        if !player.captured_units.is_empty() {
            let mut held: Vec<String> = player
                .captured_units
                .iter()
                .map(|(owner, unit)| format!("{unit} of {owner}"))
                .collect();
            held.sort();
            out.insert(format!("captured:{}", player.id), held.join(", "));
        }
    }
    for (planet, attachments) in &state.planet_attachments {
        if !attachments.is_empty() {
            out.insert(format!("attached:{planet}"), attachments.join(", "));
        }
    }
    out
}

/// The kind of effect a key stands for: the key without the ids of seats and planets in it.
fn kind(key: &str, players: &BTreeSet<String>) -> String {
    let mut parts = key.split(':');
    let surface = parts.next().unwrap_or_default();
    if surface != "mark" {
        return surface.to_owned();
    }
    let kept: Vec<&str> = parts
        .filter(|part| !players.contains(*part) && !part.chars().all(|c| c.is_ascii_digit()))
        .take(3)
        .collect();
    format!("mark:{}", kept.join(":"))
}

fn label(key: &str) -> String {
    key.replace(':', " · ")
}

fn short(value: &str) -> String {
    if value.chars().count() > MAX_VALUE {
        let cut: String = value.chars().take(MAX_VALUE).collect();
        format!("{cut}…")
    } else {
        value.to_owned()
    }
}

/// The notes for every frame, in frame order: what changed against the frame before it.
#[must_use]
pub fn notes_by_frame(frames: &[ReviewFrame]) -> Vec<Vec<Note>> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut previous: BTreeMap<String, String> = BTreeMap::new();
    let mut all = Vec::with_capacity(frames.len());
    for frame in frames {
        let current = surfaces(&frame.state);
        let players: BTreeSet<String> = frame
            .state
            .players
            .iter()
            .map(|player| player.id.to_string())
            .collect();
        let mut notes = Vec::new();
        let keys: BTreeSet<&String> = previous.keys().chain(current.keys()).collect();
        for key in keys {
            let text = match (previous.get(key), current.get(key)) {
                (None, Some(new)) => format!("+ {} = {}", label(key), short(new)),
                (Some(_), None) => format!("− {} cleared", label(key)),
                (Some(old), Some(new)) if old != new => {
                    format!("{}: {} → {}", label(key), short(old), short(new))
                }
                _ => continue,
            };
            let first = !seen.contains(&kind(key, &players));
            notes.push(Note { text, first });
        }
        for key in current.keys() {
            seen.insert(kind(key, &players));
        }
        // First-time effects lead, so the truncation never hides the one that matters.
        notes.sort_by_key(|note| !note.first);
        if notes.len() > MAX_NOTES {
            let more = notes.len() - MAX_NOTES;
            notes.truncate(MAX_NOTES);
            notes.push(Note {
                text: format!("… and {more} more"),
                first: false,
            });
        }
        all.push(notes);
        previous = current;
    }
    all
}

/// What is standing now that only an unusual rules effect could put there, for one seat: its
/// faction's marks, any mark naming it (Mahact holding its command tokens), the technologies its
/// Valefar Assimilators copy and the units it has captured. Empty for an ordinary seat.
#[must_use]
pub fn standing(frame: &ReviewFrame, player: &ti4_model::id::PlayerId) -> Vec<String> {
    let me = player.as_str();
    let faction = frame
        .state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    let mut lines = Vec::new();
    for (key, value) in &frame.state.faction_marks {
        let lead = key.split(':').next().unwrap_or_default();
        let named = key.split(':').any(|part| part == me)
            || value.split([',', '|']).any(|part| part == me);
        if (lead == faction && !faction.is_empty()) || (named && is_faction_lead(frame, lead)) {
            lines.push(format!("{}: {}", label(key), short(value)));
        }
    }
    if let Some(seat) = frame.state.player(player) {
        for (card, tech) in &seat.assimilated_technologies {
            lines.push(format!("assimilated · {card} copies {tech}"));
        }
        if !seat.captured_units.is_empty() {
            let mut held: Vec<String> = seat
                .captured_units
                .iter()
                .map(|(owner, unit)| format!("{unit} of {owner}"))
                .collect();
            held.sort();
            lines.push(format!("captured · {}", held.join(", ")));
        }
    }
    let held = frame.state.tokens_held_by_mahact(player);
    if held > 0 {
        lines.push(format!("{held} of this seat's command token(s) held in a Mahact fleet pool"));
    }
    lines
}

fn is_faction_lead(frame: &ReviewFrame, lead: &str) -> bool {
    frame.state.players.iter().any(|p| p.faction.as_str() == lead)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_ignore_seat_ids_and_numbers() {
        let players = BTreeSet::from(["p1".to_owned()]);
        assert_eq!(kind("mark:mahact:fleet:p1", &players), "mark:mahact:fleet");
        assert_eq!(kind("mark:nekro:ts:18", &players), "mark:nekro:ts");
        assert_eq!(kind("attached:mr", &players), "attached");
    }

    #[test]
    fn long_values_are_cut() {
        assert!(short(&"x".repeat(200)).chars().count() <= MAX_VALUE + 1);
    }
}
