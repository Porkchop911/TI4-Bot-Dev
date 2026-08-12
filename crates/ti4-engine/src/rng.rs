//! The pinned random source.
//!
//! Everything random in a game comes from here, so that a game is reproducible from its
//! seed and its decision log together. Reaching for a thread RNG anywhere else silently
//! breaks replay, which is why nothing else in the engine depends on `rand` directly.
//!
//! # Domain separation
//!
//! One stream for the whole game would couple every random decision to every other: adding
//! a die roll early in a round would shift the agenda deck, the exploration deck, and every
//! later roll. A regression test pinned to a seed would then fail for reasons unrelated to
//! what changed, and — worse — a fix that changed the *number* of rolls would silently
//! renumber every later draw.
//!
//! So each purpose draws from its own stream, seeded by hashing the game seed together with
//! the domain name. Streams are independent: consuming from one never moves another.
//!
//! # Not the oracle's stream
//!
//! The oracle uses Python's Mersenne Twister through `random.Random(seed)`. Its shuffle is
//! not reproducible outside `CPython`, so this is a *native pinned* generator rather than a
//! port — which is what M03-006 specifies. The same seed therefore produces a different
//! (equally legal) game. Reproducing a specific oracle game needs its decision log, or the
//! legacy entropy translator planned in M03-007.

use std::collections::BTreeMap;

use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sha2::{Digest, Sha256};

/// Domain names used by the engine. A new purpose gets a new name, never an existing one.
pub mod domain {
    /// Combat, bombardment, space cannon — every die.
    pub const DICE: &str = "dice";
    /// Shuffling the objective deck.
    pub const OBJECTIVES: &str = "deck:objectives";
    /// Shuffling the agenda deck.
    pub const AGENDAS: &str = "deck:agendas";
    /// Shuffling the action card deck.
    pub const ACTION_CARDS: &str = "deck:action_cards";
    /// Shuffling the secret objective deck.
    pub const SECRETS: &str = "deck:secrets";
    /// Shuffling the relic deck.
    pub const RELICS: &str = "deck:relics";
    /// Shuffling the exploration decks.
    pub const EXPLORATION: &str = "deck:exploration";
    /// Selecting map tiles.
    pub const MAP: &str = "map";
}

/// A seeded random source, split into independent streams by purpose.
#[derive(Debug, Clone)]
pub struct GameRng {
    seed: u64,
    streams: BTreeMap<String, ChaCha8Rng>,
}

impl GameRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            streams: BTreeMap::new(),
        }
    }

    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.seed
    }

    /// The seed for one domain: `SHA-256(seed_le_bytes || domain)`.
    ///
    /// Hashing rather than adding or XOR-ing the domain in: two domains whose names differ
    /// by one bit must not produce related streams.
    #[must_use]
    pub fn derive_seed(seed: u64, domain: &str) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(seed.to_le_bytes());
        hasher.update(domain.as_bytes());
        hasher.finalize().into()
    }

    /// The stream for one domain, created on first use.
    pub fn stream(&mut self, domain: &str) -> &mut ChaCha8Rng {
        self.streams
            .entry(domain.to_owned())
            .or_insert_with(|| ChaCha8Rng::from_seed(Self::derive_seed(self.seed, domain)))
    }

    /// Shuffle in place, drawing from one domain's stream.
    pub fn shuffle<T>(&mut self, domain: &str, items: &mut [T]) {
        let rng = self.stream(domain);
        // Fisher-Yates, back to front.
        for i in (1..items.len()).rev() {
            let j = rng.random_range(0..=i);
            items.swap(i, j);
        }
    }

    /// A shuffled copy, for building a deck without disturbing its source order.
    #[must_use]
    pub fn shuffled<T: Clone>(&mut self, domain: &str, items: &[T]) -> Vec<T> {
        let mut copy = items.to_vec();
        self.shuffle(domain, &mut copy);
        copy
    }

    /// An integer in `1..=sides`, drawing from one domain's stream.
    pub fn die(&mut self, domain: &str, sides: u32) -> u32 {
        self.stream(domain).random_range(1..=sides)
    }

    /// Which domains have been drawn from. Diagnostic; order is deterministic.
    #[must_use]
    pub fn active_domains(&self) -> Vec<&str> {
        self.streams.keys().map(String::as_str).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deck() -> Vec<u32> {
        (0..40).collect()
    }

    #[test]
    fn the_same_seed_produces_the_same_shuffle() {
        let mut a = GameRng::new(7);
        let mut b = GameRng::new(7);
        assert_eq!(
            a.shuffled(domain::AGENDAS, &deck()),
            b.shuffled(domain::AGENDAS, &deck())
        );
    }

    #[test]
    fn different_seeds_produce_different_shuffles() {
        let mut a = GameRng::new(1);
        let mut b = GameRng::new(2);
        assert_ne!(
            a.shuffled(domain::AGENDAS, &deck()),
            b.shuffled(domain::AGENDAS, &deck())
        );
    }

    #[test]
    fn different_domains_of_one_seed_are_independent() {
        // Two decks shuffled from one seed must not arrive in the same order.
        let mut rng = GameRng::new(7);
        assert_ne!(
            rng.shuffled(domain::AGENDAS, &deck()),
            rng.shuffled(domain::RELICS, &deck())
        );
    }

    #[test]
    fn drawing_from_one_domain_does_not_move_another() {
        // The whole point of the split: adding a die roll must not reshuffle a deck.
        let mut quiet = GameRng::new(7);
        let expected = quiet.shuffled(domain::AGENDAS, &deck());

        let mut busy = GameRng::new(7);
        for _ in 0..1000 {
            busy.die(domain::DICE, 10);
        }
        let _ = busy.shuffled(domain::RELICS, &deck()); // drawn for the side effect
        assert_eq!(busy.shuffled(domain::AGENDAS, &deck()), expected);
    }

    #[test]
    fn a_domain_stream_is_created_once_and_then_advances() {
        let mut rng = GameRng::new(7);
        let first = rng.shuffled(domain::AGENDAS, &deck());
        let second = rng.shuffled(domain::AGENDAS, &deck());
        assert_ne!(first, second, "a second draw continues the stream");
    }

    #[test]
    fn domains_that_differ_by_one_character_are_unrelated() {
        // Hashing rather than adding the domain in is what buys this.
        let a = GameRng::derive_seed(7, "deck:a");
        let b = GameRng::derive_seed(7, "deck:b");
        let differing = a.iter().zip(&b).filter(|(x, y)| x != y).count();
        assert!(differing > 20, "only {differing} of 32 bytes differ");
    }

    #[test]
    fn a_shuffle_is_a_permutation() {
        let mut rng = GameRng::new(3);
        let shuffled = rng.shuffled(domain::OBJECTIVES, &deck());
        let mut sorted = shuffled.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, deck());
        assert_ne!(shuffled, deck(), "and it actually moved something");
    }

    #[test]
    fn shuffling_a_short_list_does_not_panic() {
        let mut rng = GameRng::new(1);
        assert!(rng.shuffled::<u32>(domain::MAP, &[]).is_empty());
        assert_eq!(rng.shuffled(domain::MAP, &[9]), vec![9]);
    }

    #[test]
    fn a_shuffle_reaches_every_position() {
        // A Fisher-Yates written with the wrong bound leaves the first element fixed, and
        // a deck whose top card never moves is a deck that always reveals the same thing.
        let mut seen_first = std::collections::BTreeSet::new();
        for seed in 0..50 {
            let mut rng = GameRng::new(seed);
            seen_first.insert(rng.shuffled(domain::OBJECTIVES, &deck())[0]);
        }
        assert!(
            seen_first.len() > 10,
            "only {} distinct tops",
            seen_first.len()
        );
    }

    #[test]
    fn a_die_stays_within_its_faces() {
        let mut rng = GameRng::new(11);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..500 {
            let face = rng.die(domain::DICE, 10);
            assert!((1..=10).contains(&face), "rolled {face}");
            seen.insert(face);
        }
        assert_eq!(seen.len(), 10, "every face should appear in 500 rolls");
    }

    #[test]
    fn active_domains_are_reported_deterministically() {
        let mut rng = GameRng::new(1);
        rng.die(domain::DICE, 10);
        let _ = rng.shuffled(domain::AGENDAS, &deck()); // drawn for the side effect
        assert_eq!(rng.active_domains(), vec![domain::AGENDAS, domain::DICE]);
    }
}
