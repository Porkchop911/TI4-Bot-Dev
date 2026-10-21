//! Readable faction-specific state per seat and per tile, from the engine's public accessors:
//! Mahact's fleet pool, plots and who they puppet, Cabal captures, Titans sleepers, Empyrean Void
//! Tethers, Last Bastion galvanize tokens and Crimson breach activity. The review panels show the
//! full game state to the reviewer, so facedown plots are shown too, marked as facedown.

use std::collections::BTreeMap;

use ti4_engine::factions::{bastion, crimson, mahact};
use ti4_model::id::{PlayerId, SystemId};
use ti4_model::state::GameState;

/// `a, b ×2` for a list of names.
fn counted(names: impl IntoIterator<Item = String>) -> String {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for name in names {
        *counts.entry(name).or_default() += 1;
    }
    counts
        .into_iter()
        .map(|(name, n)| if n > 1 { format!("{name} ×{n}") } else { name })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Void Tether borders as `(owner, system a, system b)`, owners and borders in stored order.
#[must_use]
pub fn tethers(state: &GameState) -> Vec<(PlayerId, SystemId, SystemId)> {
    let mut out = Vec::new();
    for (key, value) in &state.faction_marks {
        let Some(owner) = key.strip_prefix("empyrean|tether|") else {
            continue;
        };
        for pair in value.split(';') {
            if let Some((a, b)) = pair.split_once('|') {
                out.push((PlayerId::new(owner), SystemId::new(a), SystemId::new(b)));
            }
        }
    }
    out
}

/// The word after a tile's "Breach" token: whether the breach is active.
#[must_use]
pub fn breach_state(state: &GameState, system: &SystemId) -> &'static str {
    if crimson::is_active(state, system) {
        "active"
    } else {
        "inactive"
    }
}

/// Extra token labels for one tile: its Void Tether borders (the label names the other side).
#[must_use]
pub fn tile_labels(state: &GameState, system: &SystemId) -> Vec<String> {
    tethers(state)
        .into_iter()
        .filter_map(|(owner, a, b)| {
            let other = if &a == system {
                b
            } else if &b == system {
                a
            } else {
                return None;
            };
            Some(format!("Void Tether {owner}↔{other}"))
        })
        .collect()
}

fn side(faceup: bool) -> &'static str {
    if faceup { "faceup" } else { "facedown" }
}

/// What `player`'s faction holds that the plain panel does not spell out, one line each.
#[must_use]
pub fn seat_lines(state: &GameState, player: &PlayerId) -> Vec<String> {
    let mut lines = Vec::new();
    let faction = state
        .player(player)
        .map(|seat| seat.faction.to_string())
        .unwrap_or_default();
    if mahact::is_mahact(state, player) {
        let pool = mahact::fleet_pool_owners(state, player);
        if !pool.is_empty() {
            lines.push(format!(
                "Fleet pool holds {} foreign command token(s): {}",
                pool.len(),
                counted(pool.iter().map(ToString::to_string))
            ));
        }
    }
    if let Some(seat) = state.player(player) {
        for plot in ti4_model::plots::read(&seat.plots) {
            lines.push(format!(
                "Plot {} ({}) targets {}",
                plot.card.as_deref().unwrap_or("?"),
                side(plot.faceup),
                counted(plot.tokens.iter().map(ToString::to_string))
            ));
        }
        let mut by_owner: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (owner, unit) in &seat.captured_units {
            by_owner
                .entry(owner.to_string())
                .or_default()
                .push(unit.to_string());
        }
        for (owner, units) in by_owner {
            lines.push(format!("Captured from {owner}: {}", counted(units)));
        }
    }
    // Plots carrying this seat's token, and its own units held by others.
    let mut taken = Vec::new();
    for other in state.players.iter().filter(|other| &other.id != player) {
        for plot in ti4_model::plots::read(&other.plots) {
            if plot.tokens.contains(player) {
                lines.push(format!(
                    "Token on {}'s plot {} ({})",
                    other.id,
                    plot.card.as_deref().unwrap_or("?"),
                    side(plot.faceup)
                ));
            }
        }
        for (owner, unit) in &other.captured_units {
            if owner == player {
                taken.push(format!("{unit} (held by {})", other.id));
            }
        }
    }
    if !taken.is_empty() {
        lines.push(format!("Own units captured: {}", counted(taken)));
    }
    let sleepers: Vec<&str> = state
        .faction_marks
        .iter()
        .filter_map(|(key, value)| {
            let planet = key.strip_prefix("titans:sleeper:")?;
            (value.split('|').next() == Some(player.as_str())).then_some(planet)
        })
        .collect();
    if !sleepers.is_empty() {
        lines.push(format!("Sleeper tokens on: {}", sleepers.join(", ")));
    }
    let borders: Vec<String> = tethers(state)
        .into_iter()
        .filter(|(owner, _, _)| owner == player)
        .map(|(_, a, b)| format!("{a}↔{b}"))
        .collect();
    if !borders.is_empty() {
        lines.push(format!("Void Tether borders: {}", borders.join(", ")));
    }
    if faction == bastion::FACTION {
        lines.push(format!(
            "Galvanize tokens: {} on board, {} in supply",
            bastion::galvanized_on_board(state),
            bastion::tokens_left(state)
        ));
    }
    if faction == "crimson" {
        let join = |v: Vec<SystemId>| {
            v.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        };
        lines.push(format!(
            "Breaches: active [{}], inactive [{}], {} in supply",
            join(crimson::active_breaches(state)),
            join(crimson::inactive_breaches(state)),
            crimson::tokens_in_supply(state)
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::id::{FactionId, UnitTypeId};

    fn game() -> GameState {
        let ids = [PlayerId::new("p1"), PlayerId::new("p2")];
        GameState::new(&ids, &[], BTreeMap::new(), None, 1)
    }

    #[test]
    fn mahact_pool_reads_out() {
        let mut s = game();
        s.player_mut(&PlayerId::new("p1")).unwrap().faction = FactionId::new("mahact");
        s.faction_marks
            .insert("mahact:fleet:p1".to_owned(), "p2,p2".to_owned());
        assert_eq!(
            seat_lines(&s, &PlayerId::new("p1")),
            vec!["Fleet pool holds 2 foreign command token(s): p2 ×2"]
        );
    }

    #[test]
    fn plots_captures_sleepers_and_tethers_read_out() {
        let mut s = game();
        let (p1, p2) = (PlayerId::new("p1"), PlayerId::new("p2"));
        s.player_mut(&p1).unwrap().plots = vec!["d:siphon:p2".to_owned()];
        s.player_mut(&p1).unwrap().captured_units = vec![(p2.clone(), UnitTypeId::new("infantry"))];
        s.faction_marks
            .insert("empyrean|tether|p1".to_owned(), "18|19".to_owned());
        s.faction_marks
            .insert("titans:sleeper:mr".to_owned(), "p1|18".to_owned());
        let a = seat_lines(&s, &p1);
        assert!(a.contains(&"Plot siphon (facedown) targets p2".to_owned()));
        assert!(a.contains(&"Captured from p2: infantry".to_owned()));
        assert!(a.contains(&"Void Tether borders: 18↔19".to_owned()));
        assert!(a.contains(&"Sleeper tokens on: mr".to_owned()));
        let b = seat_lines(&s, &p2);
        assert!(b.contains(&"Token on p1's plot siphon (facedown)".to_owned()));
        assert!(b.contains(&"Own units captured: infantry (held by p1)".to_owned()));
        assert_eq!(
            tile_labels(&s, &SystemId::new("19")),
            vec!["Void Tether p1↔18"]
        );
        assert_eq!(breach_state(&s, &SystemId::new("19")), "inactive");
    }
}
