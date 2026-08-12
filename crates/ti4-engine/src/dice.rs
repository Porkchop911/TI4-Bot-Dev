//! Dice.
//!
//! Every roll goes through the seeded generator in [`crate::rng`] and is recorded, so a game
//! is reproducible from its seed and its decision log together.
//!
//! Ported from the oracle's `engine/dice.py`.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::rng::{GameRng, domain};

/// The standard TI4 die.
pub const SIDES: u32 = 10;

/// One recorded roll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Roll {
    pub reason: String,
    pub faces: Vec<u32>,
    /// The value this roll hits on, if it was a hit roll at all.
    pub hits_on: Option<u32>,
    /// Positions replaced by a reroll, if this roll came from one.
    ///
    /// Kept because some abilities care *which* dice were rerolled rather than only what
    /// they now show — the Crown of Thalnos destroys "each of their units that did not
    /// produce a hit with its reroll", which is unanswerable from the faces alone.
    pub rerolled: BTreeSet<usize>,
}

impl Roll {
    #[must_use]
    pub fn hits(&self) -> usize {
        let Some(hits_on) = self.hits_on else {
            return 0;
        };
        self.faces.iter().filter(|f| **f >= hits_on).count()
    }

    /// Positions that did not hit, optionally restricted to a subset.
    #[must_use]
    pub fn missed(&self, positions: Option<&BTreeSet<usize>>) -> Vec<usize> {
        let Some(hits_on) = self.hits_on else {
            return Vec::new();
        };
        let considered: Vec<usize> = positions.map_or_else(
            || (0..self.faces.len()).collect(),
            |subset| subset.iter().copied().collect(),
        );
        considered
            .into_iter()
            .filter(|i| self.faces.get(*i).is_some_and(|f| *f < hits_on))
            .collect()
    }
}

/// A seeded ten-sided roller with a full history.
#[derive(Debug, Clone)]
pub struct Dice {
    sides: u32,
    history: Vec<Roll>,
    /// Pre-loaded faces for tests. Drained left-to-right; exhausted slots fall through
    /// to the RNG. Only populated when constructed via [`Dice::from_faces`].
    #[cfg(test)]
    preload: Option<Vec<u32>>,
}

impl Default for Dice {
    fn default() -> Self {
        Self::new()
    }
}

impl Dice {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            sides: SIDES,
            history: Vec::new(),
            #[cfg(test)]
            preload: None,
        }
    }

    /// A roller with a non-standard die, for content that calls for one.
    /// A roller with a non-standard die, for content that calls for one.
    #[must_use]
    pub const fn with_sides(sides: u32) -> Self {
        Self {
            sides,
            history: Vec::new(),
            #[cfg(test)]
            preload: None,
        }
    }

    /// A roller that yields the given faces in order, then falls back to the RNG.
    ///
    /// For tests that must force a specific branch of an ability. When the sequence
    /// runs out, subsequent rolls draw from `rng` as normal — this lets a test preload
    /// only the faces it needs without draining the seeded stream.
    ///
    /// **For tests and content only.** Do not use in production code.
    #[cfg(test)]
    #[must_use]
    pub fn from_faces(faces: impl IntoIterator<Item = u32>) -> Self {
        Self {
            sides: SIDES,
            history: Vec::new(),
            preload: Some(faces.into_iter().collect()),
        }
    }

    /// Roll `count` dice, recording the result.
    ///
    /// The generator is passed in rather than owned so that dice share one game's seed and
    /// draw from the `dice` domain, leaving deck order untouched however many are rolled.
    pub fn roll(
        &mut self,
        rng: &mut GameRng,
        count: usize,
        reason: &str,
        hits_on: Option<u32>,
    ) -> Roll {
        let faces = if count == 0 {
            Vec::new()
        } else {
            self.roll_with_rng(rng, count)
        };
        let record = Roll {
            reason: reason.to_owned(),
            faces,
            hits_on,
            rerolled: BTreeSet::new(),
        };
        self.history.push(record.clone());
        record
    }

    /// Roll `count` dice using `rng`, respecting preload in test builds.
    #[cfg(not(test))]
    fn roll_with_rng(&self, rng: &mut GameRng, count: usize) -> Vec<u32> {
        (0..count)
            .map(|_| rng.die(domain::DICE, self.sides))
            .collect()
    }

    #[cfg(test)]
    fn roll_with_rng(&mut self, rng: &mut GameRng, count: usize) -> Vec<u32> {
        if let Some(ref mut preload) = self.preload {
            (0..count)
                .map(|_| {
                    preload
                        .drain(..1)
                        .next()
                        .unwrap_or_else(|| rng.die(domain::DICE, self.sides))
                })
                .collect()
        } else {
            (0..count)
                .map(|_| rng.die(domain::DICE, self.sides))
                .collect()
        }
    }

    /// Roll named dice again, keeping the rest (LRR: a reroll replaces the result).
    ///
    /// Returns a *new* [`Roll`] and records it, rather than mutating the original. Both stay
    /// in the history, which matters for replay — the sequence of draws from the generator
    /// is part of what a seed reproduces, and a reroll that quietly overwrote its
    /// predecessor would make the log disagree with the game.
    ///
    /// Positions outside the roll are ignored rather than refused: abilities name dice by
    /// unit, and a unit may already have been destroyed by the time they resolve.
    pub fn reroll(
        &mut self,
        rng: &mut GameRng,
        roll: &Roll,
        positions: impl IntoIterator<Item = usize>,
        reason: Option<&str>,
    ) -> Roll {
        let mut faces = roll.faces.clone();
        let mut replaced = BTreeSet::new();
        for index in positions {
            if index < faces.len() {
                faces[index] = rng.die(domain::DICE, self.sides);
                replaced.insert(index);
            }
        }

        let record = Roll {
            reason: reason.map_or_else(|| format!("{}:reroll", roll.reason), str::to_owned),
            faces,
            hits_on: roll.hits_on,
            rerolled: replaced,
        };
        self.history.push(record.clone());
        record
    }

    /// Every roll made for one reason.
    #[must_use]
    pub fn rolled(&self, reason: &str) -> Vec<&Roll> {
        self.history.iter().filter(|r| r.reason == reason).collect()
    }

    #[must_use]
    pub fn history(&self) -> &[Roll] {
        &self.history
    }

    #[must_use]
    pub const fn count(&self) -> usize {
        self.history.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roller() -> (Dice, GameRng) {
        (Dice::new(), GameRng::new(7))
    }

    #[test]
    fn a_roll_records_its_reason_and_faces() {
        let (mut dice, mut rng) = roller();
        let roll = dice.roll(&mut rng, 3, "space combat", Some(7));
        assert_eq!(roll.faces.len(), 3);
        assert_eq!(roll.reason, "space combat");
        assert!(roll.faces.iter().all(|f| (1..=10).contains(f)));
        assert_eq!(dice.count(), 1);
    }

    #[test]
    fn hits_count_faces_at_or_above_the_threshold() {
        let roll = Roll {
            reason: "x".into(),
            faces: vec![1, 6, 7, 10],
            hits_on: Some(7),
            rerolled: BTreeSet::new(),
        };
        assert_eq!(roll.hits(), 2, "7 and 10");
        assert_eq!(roll.missed(None), vec![0, 1]);
    }

    #[test]
    fn a_roll_with_no_threshold_hits_nothing() {
        // An exploration or an ability roll is not a hit roll.
        let roll = Roll {
            reason: "explore".into(),
            faces: vec![10, 10],
            hits_on: None,
            rerolled: BTreeSet::new(),
        };
        assert_eq!(roll.hits(), 0);
        assert!(roll.missed(None).is_empty());
    }

    #[test]
    fn misses_can_be_restricted_to_a_subset() {
        let roll = Roll {
            reason: "x".into(),
            faces: vec![1, 9, 2],
            hits_on: Some(7),
            rerolled: BTreeSet::new(),
        };
        assert_eq!(roll.missed(Some(&BTreeSet::from([0, 1]))), vec![0]);
    }

    #[test]
    fn the_same_seed_rolls_the_same_dice() {
        let mut a = Dice::new();
        let mut b = Dice::new();
        let (mut ra, mut rb) = (GameRng::new(3), GameRng::new(3));
        assert_eq!(
            a.roll(&mut ra, 8, "combat", Some(7)).faces,
            b.roll(&mut rb, 8, "combat", Some(7)).faces
        );
    }

    #[test]
    fn a_reroll_replaces_only_the_named_dice() {
        let (mut dice, mut rng) = roller();
        let first = dice.roll(&mut rng, 4, "combat", Some(7));
        let again = dice.reroll(&mut rng, &first, [1, 3], None);

        assert_eq!(again.faces[0], first.faces[0], "untouched");
        assert_eq!(again.faces[2], first.faces[2], "untouched");
        assert_eq!(again.rerolled, BTreeSet::from([1, 3]));
        assert_eq!(again.hits_on, first.hits_on, "the threshold carries over");
    }

    #[test]
    fn a_reroll_is_recorded_alongside_its_predecessor() {
        // The sequence of draws is part of what a seed reproduces; overwriting the first
        // roll would make the history disagree with the game.
        let (mut dice, mut rng) = roller();
        let first = dice.roll(&mut rng, 2, "combat", Some(7));
        dice.reroll(&mut rng, &first, [0], None);

        assert_eq!(dice.count(), 2);
        assert_eq!(dice.history()[0], first);
        assert_eq!(dice.history()[1].reason, "combat:reroll");
    }

    #[test]
    fn a_reroll_ignores_positions_outside_the_roll() {
        // Abilities name dice by unit, and a unit may already be destroyed.
        let (mut dice, mut rng) = roller();
        let first = dice.roll(&mut rng, 2, "combat", Some(7));
        let again = dice.reroll(&mut rng, &first, [0, 99], None);
        assert_eq!(again.rerolled, BTreeSet::from([0]));
        assert_eq!(again.faces.len(), 2);
    }

    #[test]
    fn a_reroll_can_be_given_its_own_reason() {
        let (mut dice, mut rng) = roller();
        let first = dice.roll(&mut rng, 1, "combat", Some(7));
        let again = dice.reroll(&mut rng, &first, [0], Some("crown of thalnos"));
        assert_eq!(again.reason, "crown of thalnos");
    }

    #[test]
    fn rolls_can_be_found_by_reason() {
        let (mut dice, mut rng) = roller();
        dice.roll(&mut rng, 1, "space combat", Some(7));
        dice.roll(&mut rng, 1, "bombardment", Some(5));
        dice.roll(&mut rng, 1, "space combat", Some(7));

        assert_eq!(dice.rolled("space combat").len(), 2);
        assert_eq!(dice.rolled("bombardment").len(), 1);
        assert!(dice.rolled("nothing").is_empty());
    }

    #[test]
    fn rolling_zero_dice_records_an_empty_roll() {
        let (mut dice, mut rng) = roller();
        let roll = dice.roll(&mut rng, 0, "no units", Some(7));
        assert!(roll.faces.is_empty());
        assert_eq!(roll.hits(), 0);
        assert_eq!(dice.count(), 1, "it still happened");
    }

    #[test]
    fn rolling_does_not_disturb_deck_order() {
        // The domain split, seen from the dice side.
        let mut quiet = GameRng::new(5);
        let expected = quiet.shuffled(crate::rng::domain::AGENDAS, &(0..30).collect::<Vec<u32>>());

        let mut busy = GameRng::new(5);
        let mut dice = Dice::new();
        for _ in 0..200 {
            dice.roll(&mut busy, 3, "combat", Some(7));
        }
        assert_eq!(
            busy.shuffled(crate::rng::domain::AGENDAS, &(0..30).collect::<Vec<u32>>()),
            expected
        );
    }

    #[test]
    fn a_roll_round_trips_through_json() {
        let (mut dice, mut rng) = roller();
        let roll = dice.roll(&mut rng, 3, "combat", Some(7));
        let json = serde_json::to_string(&roll).unwrap();
        assert_eq!(serde_json::from_str::<Roll>(&json).unwrap(), roll);
    }
}
