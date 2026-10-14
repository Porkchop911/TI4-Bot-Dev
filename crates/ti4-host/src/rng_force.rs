//! Forced random positions and card identity for replays of a redone timeline (H7 turn redo).
//!
//! A game's dice and decks come from seeded streams that only advance when something draws.
//! After a turn is redone, the decisions that follow it are other seats' recorded answers, made
//! against the *original* dice. If the new turn drew a different number of dice, a plain replay
//! from the seed would hand those later decisions different faces. A history therefore carries
//! `rng_marks`: for some decision indexes, where every stream stood when the decision was
//! answered. Every replay path restores those positions before the decision's consequences draw.
//! The game seed is never touched; only stream positions move.
//!
//! Cards are different: decks are shuffled once and drawn from the front, so the same marks also
//! carry a [`DeckPlan`] that makes a draw of the recorded tail take the card it took originally
//! (see `ti4_model::deck_reserve`). The replay driver tells the force which decision it is
//! answering ([`RngForce::before_answer`]) and when the recorded decisions are over
//! ([`RngForce::go_live`]).

use std::collections::BTreeMap;
use std::ops::Deref;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use ti4_engine::game::Game;
use ti4_engine::rng::{RngPositions, RngSync};
use ti4_model::deck_reserve::{DeckPlan, DeckReserve, IDLE};

pub use ti4_model::deck_reserve::{LIVE, OPEN};

/// What a turn-redo timeline forces while it is replayed: stream positions by decision index and
/// the card reservations ([`DeckPlan`]) that keep drawn cards' identity.
///
/// Stored as one JSON object: decision indexes map to positions as before; the plan, when there
/// is one, sits under the extra key `"decks"`. Saves from before the plan existed load unchanged.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RngMarks {
    marks: BTreeMap<usize, RngPositions>,
    /// Card reservations, oldest redo first.
    pub deck_plan: DeckPlan,
}

impl Deref for RngMarks {
    type Target = BTreeMap<usize, RngPositions>;
    fn deref(&self) -> &Self::Target {
        &self.marks
    }
}

impl FromIterator<(usize, RngPositions)> for RngMarks {
    fn from_iter<I: IntoIterator<Item = (usize, RngPositions)>>(iter: I) -> Self {
        Self {
            marks: iter.into_iter().collect(),
            deck_plan: Vec::new(),
        }
    }
}

impl RngMarks {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Nothing to force: no positions and no reservations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.marks.is_empty() && self.deck_plan.is_empty()
    }

    pub fn insert(&mut self, index: usize, positions: RngPositions) {
        self.marks.insert(index, positions);
    }

    /// The history was cut back to `len` decisions: forget what only applied after that point.
    /// A reservation segment that began at or after the cut goes; one that reached past it ends
    /// there (an unfinished redo, [`OPEN`], stays open).
    pub fn truncate_to(&mut self, len: usize) {
        self.marks.retain(|index, _| *index < len);
        self.deck_plan.retain(|segment| segment.from < len);
        for segment in &mut self.deck_plan {
            if segment.until != OPEN {
                segment.until = segment.until.min(len);
            }
        }
    }
}

impl Serialize for RngMarks {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let extra = usize::from(!self.deck_plan.is_empty());
        let mut map = serializer.serialize_map(Some(self.marks.len() + extra))?;
        for (index, positions) in &self.marks {
            map.serialize_entry(&index.to_string(), positions)?;
        }
        if !self.deck_plan.is_empty() {
            map.serialize_entry("decks", &self.deck_plan)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for RngMarks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw: BTreeMap<String, serde_json::Value> = BTreeMap::deserialize(deserializer)?;
        let mut out = Self::default();
        for (key, value) in raw {
            if key == "decks" {
                out.deck_plan = serde_json::from_value(value).map_err(D::Error::custom)?;
            } else {
                let index: usize = key.parse().map_err(D::Error::custom)?;
                let positions = serde_json::from_value(value).map_err(D::Error::custom)?;
                out.marks.insert(index, positions);
            }
        }
        Ok(out)
    }
}

/// Applies [`RngMarks`] during a replay. Cheap to clone; clones share the same side channel.
#[derive(Clone)]
pub struct RngForce {
    sync: Arc<RngSync>,
    marks: Arc<RngMarks>,
    cursor: Arc<AtomicUsize>,
}

impl RngForce {
    /// `None` for an empty mark set, so histories without marks replay exactly as before.
    #[must_use]
    pub fn new(marks: &RngMarks) -> Option<Self> {
        (!marks.is_empty()).then(|| Self::always(marks))
    }

    /// A force that always exists, so a run can capture positions even with nothing to force.
    #[must_use]
    pub fn always(marks: &RngMarks) -> Self {
        Self {
            sync: RngSync::new(),
            marks: Arc::new(marks.clone()),
            cursor: Arc::new(AtomicUsize::new(IDLE)),
        }
    }

    /// Connect the game's random source and its decks to this force.
    pub fn attach(&self, game: &mut Game<'_>) {
        game.set_rng_sync(Some(Arc::clone(&self.sync)));
        // A game forked from a step-boundary copy already carries the reserve as it stood there
        // (pending reservations consumed so far, the draw log, an independent cursor): keep it
        // and follow this force's cursor from the copy's position. A new game starts one from
        // the recorded plan.
        match game.state.deck_reserve.as_mut() {
            Some(reserve) => reserve.rebind(&self.cursor),
            None => {
                game.state.deck_reserve = Some(DeckReserve::new(
                    Arc::clone(&self.cursor),
                    self.marks.deck_plan.clone(),
                ));
            }
        }
    }

    /// Call when decision `index` is about to be answered from the record.
    pub fn before_answer(&self, index: usize) {
        self.cursor.store(index, Ordering::Relaxed);
        if let Some(positions) = self.marks.get(&index) {
            self.sync.restore_at_next_draw(positions.clone());
        }
    }

    /// Call when the recorded decisions are used up and a live answer is next: draws from here
    /// on belong to no recorded decision (only a redone turn still being played stays governed).
    pub fn go_live(&self) {
        self.cursor.store(LIVE, Ordering::Relaxed);
    }

    /// Record where the streams stand when decision `index` is answered (read at the next draw,
    /// after any restore for the same decision).
    pub fn capture(&self, index: usize) {
        self.sync.mark_at_next_draw(index);
    }

    /// Resolve captures that no draw has read yet against the game's final positions.
    pub fn finish(&self, game: &Game<'_>) {
        self.sync.resolve_pending(&game.rng_positions());
    }

    /// Everything captured so far, by decision index.
    #[must_use]
    pub fn captured(&self) -> BTreeMap<usize, RngPositions> {
        self.sync.marks()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_model::deck_reserve::{DeckSegment, ReservedDraw};

    #[test]
    fn a_save_without_a_card_plan_loads_unchanged() {
        let old = r#"{"7":{"deck:agendas":2,"dice":4}}"#;
        let marks: RngMarks = serde_json::from_str(old).unwrap();
        assert_eq!(marks.len(), 1);
        assert!(marks.deck_plan.is_empty());
        assert_eq!(serde_json::to_string(&marks).unwrap(), old);
    }

    #[test]
    fn the_card_plan_round_trips_beside_the_positions_and_truncates_with_the_history() {
        let mut marks = RngMarks::new();
        marks.insert(3, RngPositions::from([("dice".to_owned(), 8)]));
        marks.deck_plan = vec![
            DeckSegment {
                from: 2,
                until: 6,
                reserved: vec![ReservedDraw {
                    tag: 4,
                    deck: "action_card".to_owned(),
                    card: "direct_hit".to_owned(),
                    recipient: Some("p2".to_owned()),
                }],
            },
            DeckSegment {
                from: 9,
                until: OPEN,
                reserved: Vec::new(),
            },
        ];
        let back: RngMarks = serde_json::from_str(&serde_json::to_string(&marks).unwrap()).unwrap();
        assert_eq!(back, marks);
        let mut cut = marks.clone();
        cut.truncate_to(5);
        assert_eq!(
            cut.deck_plan.len(),
            1,
            "a segment that began after the cut goes"
        );
        assert_eq!(
            cut.deck_plan[0].until, 5,
            "one that reached past it ends there"
        );
        let mut open = marks;
        open.truncate_to(20);
        assert_eq!(
            open.deck_plan[1].until, OPEN,
            "an unfinished redo stays open"
        );
    }
}
