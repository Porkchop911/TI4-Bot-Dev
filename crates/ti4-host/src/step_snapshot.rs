//! A copy of the live game at a step boundary, so a batch check need not replay the whole game.
//!
//! The session worker is the only caller of `Game::step`. At the top of each loop iteration,
//! before `step()`, the game is provably between steps, so a full copy can be taken there. A
//! batch check later forks that copy, replays only the decisions recorded since (the tail of
//! the step in progress) and then runs the staged choices (`batch::simulate`).
//!
//! The copy carries no decision log (the largest part of it, and the session already holds the
//! log): it records how long the log was and the last record, and a check rebuilds the log from
//! the prefix it validates against. A snapshot is used only when
//!
//! - it belongs to the history generation being checked,
//! - the log it was taken at is a prefix of the log being checked (same length bound and same
//!   last record), and
//! - it was taken with no deferred RNG work pending (`Game::flush_rng_sync` first).
//!
//! Every history change except appending decisions (undo, redo, restore, turn redo, batch
//! commit, recovery, history replacement) replaces the whole session, whose shared state starts
//! with no snapshot; the appends keep the prefix property, which is what `is_valid_for` checks.
//! Anything else falls back to the full replay.

use ti4_engine::choice::DecisionRecord;
use ti4_engine::game::{Game, GameSnapshot};

/// The live game as it was between two steps.
pub struct StepSnapshot {
    /// History generation of the session the copy was taken in.
    pub generation: u64,
    /// Decisions recorded when the copy was taken.
    pub log_len: usize,
    /// A digest of those decisions (who, what was asked, what was chosen): the guard that the
    /// log being checked starts with exactly the decisions the copy was taken after.
    digest: u64,
    game: GameSnapshot<'static>,
}

/// Order-sensitive digest of a decision log (deterministic within a process, never persisted).
fn log_digest(records: &[DecisionRecord]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    records.len().hash(&mut hasher);
    for record in records {
        record.player.hash(&mut hasher);
        record.prompt.hash(&mut hasher);
        record.chosen.hash(&mut hasher);
        record.offered.len().hash(&mut hasher);
    }
    hasher.finish()
}

impl std::fmt::Debug for StepSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StepSnapshot")
            .field("generation", &self.generation)
            .field("log_len", &self.log_len)
            .finish_non_exhaustive()
    }
}

impl StepSnapshot {
    /// Copy `game`, which must be between steps, or `None` when a copy would not be faithful.
    ///
    /// Applies any RNG position a turn redo forced but no draw has used yet (equivalent to
    /// waiting for the next draw), so the copy holds no deferred work.
    pub fn capture(game: &mut Game<'static>, generation: u64) -> Option<Self> {
        game.flush_rng_sync();
        if game.rng_sync_pending() {
            return None;
        }
        let records = &game.table.log.records;
        Some(Self {
            generation,
            log_len: records.len(),
            digest: log_digest(records),
            game: game.snapshot_unlogged(),
        })
    }

    /// Whether this copy may stand in for replaying `prefix` from the start of the game.
    #[must_use]
    pub fn is_valid_for(&self, generation: u64, prefix: &[DecisionRecord]) -> bool {
        self.generation == generation && self.is_prefix_of(prefix)
    }

    /// Whether the log this copy was taken at is a prefix of `records` (by length and newest
    /// record). A session replacing another one on the same history (a committed batch appends
    /// to it) is a new generation but still passes this.
    #[must_use]
    pub fn is_prefix_of(&self, records: &[DecisionRecord]) -> bool {
        self.log_len <= records.len() && log_digest(&records[..self.log_len]) == self.digest
    }

    /// A game positioned where the copy was taken, with `prefix[..log_len]` as its log and
    /// first-option deciders (the caller installs its own).
    ///
    /// # Panics
    /// When the copy is not valid for `prefix` (check [`StepSnapshot::is_valid_for`] first).
    #[must_use]
    pub fn fork(&self, prefix: &[DecisionRecord]) -> Game<'static> {
        assert!(
            self.log_len <= prefix.len(),
            "snapshot is newer than the prefix"
        );
        let mut game = self.game.instantiate();
        game.table.log.records = prefix[..self.log_len].to_vec();
        game
    }
}
