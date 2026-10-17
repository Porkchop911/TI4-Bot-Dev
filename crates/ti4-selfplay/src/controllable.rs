//! Controllable victory points: what a seat earns by playing (public and secret objectives, the
//! custodians, Imperial's Mecatol point), without the sources that are close to noise for the
//! bot's decisions (agenda results, Support for the Throne swaps, Imperial riders, relic points
//! such as the Shard of the Throne or the Crown of Emphidia, exploration or frontier rewards).
//!
//! Computed from the stored per-round timeline of a game (`vp_events` of [`crate::timeline`]),
//! so no game is replayed. A game without a timeline has no split and is skipped by the
//! controllable metrics (`compare` counts them).

use std::sync::OnceLock;

use crate::play::GameResult;

/// Categories the events fall into, in report order. The first four are controllable by default.
pub const CATEGORIES: [&str; 8] = [
    "objective",
    "secret_objective",
    "custodians",
    "imperial_primary",
    "support_for_the_throne",
    "agenda",
    "imperial_rider",
    "relic_other",
];

static EXCLUDE: OnceLock<Option<Vec<String>>> = OnceLock::new();

/// `--exclude-sources a,b`: count every category except these as controllable (names of
/// [`CATEGORIES`]); `None`: the default set. The first call wins.
pub fn configure(exclude: Option<Vec<String>>) {
    let _ = EXCLUDE.set(exclude);
}

/// The category of a ledger source name (`agenda:mutiny` -> `agenda`; anything unknown, relics
/// and exploration included, is `relic_other`).
#[must_use]
pub fn category(src: &str) -> usize {
    let head = src.split([':', '(', '#']).next().unwrap_or(src).trim();
    if head == "agenda" {
        return 5;
    }
    CATEGORIES[..7].iter().position(|c| *c == head).unwrap_or(7)
}

/// Whether category `i` counts as controllable.
#[must_use]
pub fn is_controllable(i: usize) -> bool {
    match EXCLUDE.get().and_then(Option::as_ref) {
        Some(excluded) => !excluded.iter().any(|e| e == CATEGORIES[i]),
        None => i < 4,
    }
}

/// Cumulative victory points per seat, round and category: `[seat][round][category]`, round 0
/// being the start and the last index (`ROUNDS`) the end of the game.
pub struct Table {
    cum: Vec<Vec<[f64; 8]>>,
}

/// Rounds tracked (events later than this count in the last slot).
pub const ROUNDS: usize = 12;

impl Table {
    /// The table of a game with a timeline.
    #[must_use]
    pub fn of(g: &GameResult) -> Option<Self> {
        let events = g.timeline.as_ref()?.get("vp_events")?.as_array()?;
        let seats = g.vp.len();
        let mut per = vec![vec![[0.0f64; 8]; ROUNDS + 1]; seats];
        for e in events {
            let seat = "abcdefgh".find(e.get("seat")?.as_str()?)?;
            let round = usize::try_from(e.get("round")?.as_u64()?).ok()?.min(ROUNDS);
            let d = e.get("d")?.as_f64()?;
            if seat < seats {
                per[seat][round][category(e.get("src")?.as_str()?)] += d;
            }
        }
        for seat in &mut per {
            for r in 1..=ROUNDS {
                let before = seat[r - 1];
                for (c, b) in before.iter().enumerate() {
                    seat[r][c] += b;
                }
            }
        }
        Some(Self { cum: per })
    }

    /// Points of the controllable categories after `round` (`None`: at the end), or of one
    /// category when `only` is given (a [`CATEGORIES`] index), or of all other categories when
    /// `noisy`.
    #[must_use]
    pub fn at(&self, seat: usize, round: Option<usize>, only: Option<usize>, noisy: bool) -> f64 {
        let slot = round.map_or(ROUNDS, |r| r.min(ROUNDS));
        let row = &self.cum[seat][slot];
        (0..8)
            .filter(|&c| match only {
                Some(o) => c == o,
                None => is_controllable(c) != noisy,
            })
            .map(|c| row[c])
            .sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn categories_and_cumulative_points() {
        assert_eq!(category("agenda:mutiny"), 5);
        assert_eq!(category("objective"), 0);
        assert_eq!(category("shard"), 7);
        let g = GameResult {
            vp: vec![3, 2],
            timeline: Some(json!({"vp_events": [
                {"round": 1, "seat": "a", "d": 1, "src": "objective", "why": "objective"},
                {"round": 2, "seat": "a", "d": 1, "src": "support_for_the_throne", "why": "x"},
                {"round": 3, "seat": "a", "d": 1, "src": "custodians", "why": "custodians"},
                {"round": 3, "seat": "b", "d": 2, "src": "agenda", "why": "agenda:mutiny"},
            ]})),
            ..crate::play::empty_for_tests()
        };
        let t = Table::of(&g).expect("timeline");
        assert!((t.at(0, Some(2), None, false) - 1.0).abs() < 1e-9);
        assert!((t.at(0, None, None, false) - 2.0).abs() < 1e-9);
        assert!((t.at(0, None, None, true) - 1.0).abs() < 1e-9);
        assert!((t.at(1, None, None, true) - 2.0).abs() < 1e-9);
        assert!((t.at(0, Some(3), Some(2), false) - 1.0).abs() < 1e-9);
    }
}
