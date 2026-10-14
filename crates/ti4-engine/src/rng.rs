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
use std::sync::{Arc, Mutex};

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
    /// Map tile selection. Its own stream, so drawing a different board does not shift the
    /// dice or the decks — a seed has to name one thing at a time or nothing is reproducible.
    pub const GALAXY: &str = "galaxy";
    /// Shuffling the exploration decks.
    pub const EXPLORATION: &str = "deck:exploration";
    /// Selecting map tiles.
    pub const MAP: &str = "map";
    /// Choosing the opening speaker (and so the first strategy-phase pick order).
    pub const SPEAKER: &str = "setup:speaker";
}

/// Where every stream of a game stands: domain name to `ChaCha8` word position.
///
/// A domain that has never been drawn from is absent, which means position zero.
pub type RngPositions = BTreeMap<String, u128>;

#[derive(Debug, Default)]
struct SyncInner {
    /// Positions to impose before the next draw.
    restore: Option<RngPositions>,
    /// Tags whose positions are read before the next draw.
    pending_marks: Vec<usize>,
    marks: BTreeMap<usize, RngPositions>,
}

/// A side channel into a game's random source for code that cannot reach the [`GameRng`]
/// itself, such as a decider that runs in the middle of an engine step.
///
/// It exists for replaying a game whose random draws must be *forced* to a recorded position
/// (a turn that is redone while the other seats' recorded decisions are replayed against the
/// original dice). Both operations are lazy, applied at the next draw: nothing in a game moves
/// a stream between a decision being answered and the next draw, so "positions at the next
/// draw" are exactly "positions when the decision was answered".
#[derive(Debug, Default)]
pub struct RngSync {
    inner: Mutex<SyncInner>,
}

impl RngSync {
    #[must_use]
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Put every stream at `positions` just before the next draw (a later call replaces an
    /// earlier one that has not been applied yet).
    pub fn restore_at_next_draw(&self, positions: RngPositions) {
        self.inner.lock().expect("rng sync lock").restore = Some(positions);
    }

    /// Record where the streams stand, under `tag`, just before the next draw (after any
    /// pending restore has been applied).
    pub fn mark_at_next_draw(&self, tag: usize) {
        self.inner
            .lock()
            .expect("rng sync lock")
            .pending_marks
            .push(tag);
    }

    /// Whether a restore or a capture still waits for the next draw.
    #[must_use]
    pub fn is_pending(&self) -> bool {
        let inner = self.inner.lock().expect("rng sync lock");
        inner.restore.is_some() || !inner.pending_marks.is_empty()
    }

    /// Tags still waiting for a draw to read their positions.
    #[must_use]
    pub fn pending_marks(&self) -> Vec<usize> {
        self.inner
            .lock()
            .expect("rng sync lock")
            .pending_marks
            .clone()
    }

    /// Resolve every pending mark against `positions` now (at the end of a run, when no
    /// further draw will come).
    pub fn resolve_pending(&self, positions: &RngPositions) {
        let mut inner = self.inner.lock().expect("rng sync lock");
        for tag in std::mem::take(&mut inner.pending_marks) {
            inner.marks.insert(tag, positions.clone());
        }
    }

    /// The positions recorded so far, by tag.
    #[must_use]
    pub fn marks(&self) -> BTreeMap<usize, RngPositions> {
        self.inner.lock().expect("rng sync lock").marks.clone()
    }

    fn take_due(&self) -> (Option<RngPositions>, Vec<usize>) {
        let mut inner = self.inner.lock().expect("rng sync lock");
        (
            inner.restore.take(),
            std::mem::take(&mut inner.pending_marks),
        )
    }

    fn record(&self, tags: Vec<usize>, positions: &RngPositions) {
        let mut inner = self.inner.lock().expect("rng sync lock");
        for tag in tags {
            inner.marks.insert(tag, positions.clone());
        }
    }
}

/// A seeded random source, split into independent streams by purpose.
#[derive(Debug, Clone)]
pub struct GameRng {
    seed: u64,
    streams: BTreeMap<String, ChaCha8Rng>,
    observation: Option<crate::observation::ExecutionObservation>,
    sync: Option<Arc<RngSync>>,
}

impl GameRng {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self {
            seed,
            streams: BTreeMap::new(),
            observation: None,
            sync: None,
        }
    }

    /// Attach a [`RngSync`]; clones of this source share it.
    pub fn set_sync(&mut self, sync: Option<Arc<RngSync>>) {
        self.sync = sync;
    }

    /// Whether the attached side channel holds a forced position or capture that no draw has
    /// applied yet (`false` without a channel).
    #[must_use]
    pub fn sync_pending(&self) -> bool {
        self.sync.as_ref().is_some_and(|sync| sync.is_pending())
    }

    /// Where every stream that has been drawn from stands.
    #[must_use]
    pub fn positions(&self) -> RngPositions {
        self.streams
            .iter()
            .map(|(domain, stream)| (domain.clone(), stream.get_word_pos()))
            .collect()
    }

    /// Put the streams at `positions`. A domain missing from `positions` goes back to its start
    /// (it is created again, at position zero, on its next draw). The seed never changes.
    pub fn set_positions(&mut self, positions: &RngPositions) {
        self.streams
            .retain(|domain, _| positions.contains_key(domain));
        for (domain, position) in positions {
            let seed = self.seed;
            let stream = self
                .streams
                .entry(domain.clone())
                .or_insert_with(|| ChaCha8Rng::from_seed(Self::derive_seed(seed, domain)));
            stream.set_word_pos(*position);
        }
    }

    /// Apply a pending restore and resolve pending captures now instead of at the next draw.
    ///
    /// Equivalent to waiting: nothing moves a stream between the decision being answered and
    /// the next draw, so "positions at the next draw" are "positions now". It lets a step
    /// boundary hold no deferred work, which a snapshot needs.
    pub fn flush_sync(&mut self) {
        self.apply_sync();
    }

    fn apply_sync(&mut self) {
        let Some(sync) = self.sync.clone() else {
            return;
        };
        let (restore, marks) = sync.take_due();
        if let Some(positions) = restore {
            self.set_positions(&positions);
        }
        if !marks.is_empty() {
            sync.record(marks, &self.positions());
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
        self.observe_randomness();
        self.apply_sync();
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

    pub fn bind_observation(
        &mut self,
        observation: Option<crate::observation::ExecutionObservation>,
    ) {
        self.observation = observation;
    }

    /// Dice call this even when test-preloaded faces bypass the random stream.
    pub fn observe_randomness(&self) {
        if let Some(observation) = &self.observation {
            observation.randomness();
        }
    }

    /// Ordered deck reads can reveal an unknown result without using randomness.
    pub fn observe_hidden_information(&self) {
        if let Some(observation) = &self.observation {
            observation.hidden_information();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip_and_restore_a_stream() {
        let mut rng = GameRng::new(5);
        for _ in 0..7 {
            rng.die(domain::DICE, 10);
        }
        let _ = rng.shuffled(domain::AGENDAS, &deck());
        let at = rng.positions();
        let next: Vec<u32> = (0..20).map(|_| rng.die(domain::DICE, 10)).collect();
        // Draw more from another game, then put it back to `at`.
        let mut other = GameRng::new(5);
        for _ in 0..50 {
            other.die(domain::DICE, 10);
        }
        let _ = other.shuffled(domain::RELICS, &deck());
        other.set_positions(&at);
        assert_eq!(other.positions(), at, "a domain not in the map is reset");
        let again: Vec<u32> = (0..20).map(|_| other.die(domain::DICE, 10)).collect();
        assert_eq!(next, again);
        assert_eq!(other.seed(), 5, "the seed is never changed");
    }

    #[test]
    fn sync_restores_and_marks_lazily_at_the_next_draw() {
        let sync = RngSync::new();
        let mut a = GameRng::new(9);
        a.set_sync(Some(sync.clone()));
        let mut reference = GameRng::new(9);
        for _ in 0..3 {
            reference.die(domain::DICE, 6);
        }
        let at3 = reference.positions();
        let expected: Vec<u32> = (0..5).map(|_| reference.die(domain::DICE, 6)).collect();
        for _ in 0..40 {
            a.die(domain::DICE, 6);
        }
        sync.restore_at_next_draw(at3.clone());
        sync.mark_at_next_draw(11);
        let got: Vec<u32> = (0..5).map(|_| a.die(domain::DICE, 6)).collect();
        assert_eq!(got, expected);
        assert_eq!(
            sync.marks().get(&11),
            Some(&at3),
            "mark sees the forced position"
        );
        assert!(sync.pending_marks().is_empty());
        sync.mark_at_next_draw(12);
        sync.resolve_pending(&a.positions());
        assert_eq!(sync.marks().get(&12), Some(&a.positions()));
    }

    fn deck() -> Vec<u32> {
        (0..40).collect()
    }

    #[test]
    fn temporary_clones_and_direct_stream_access_share_observation() {
        let observation = crate::observation::ExecutionObservation::default();
        let mut rng = GameRng::new(7);
        rng.bind_observation(Some(observation.clone()));
        let mut temporary = rng.clone();
        let _ = temporary.stream(domain::DICE);
        assert!(observation.activity().randomness);
        assert!(rng.active_domains().is_empty());

        let fresh = crate::observation::ExecutionObservation::default();
        rng.bind_observation(Some(fresh.clone()));
        let _ = rng.stream("direct");
        assert!(fresh.activity().randomness);
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
