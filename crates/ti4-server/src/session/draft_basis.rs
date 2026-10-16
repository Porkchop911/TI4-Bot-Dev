//! The position that a draft is played on: the game at its last completed live step.

use ti4_engine::game::Game;

/// A copy of the game at its last completed step, kept for the drafts of the seats
/// ([`super::run_draft_on`], [`super::GameSession::apply_draft`], the ready secondary), and the
/// strategic action that is resolved ([`SecondaryWatch`](ti4_host::view::secondary::SecondaryWatch)).
#[derive(Default)]
pub(crate) struct DraftBasis {
    // Game isn't Send. This engine-provided factory carries its copied driver
    // state across threads and creates fresh input bindings when called.
    checkpoint: Option<Box<dyn FnOnce() -> Game<'static> + Send>>,
    /// How many decisions the log of the game had at the checkpoint.
    checkpoint_decisions: usize,
    closed: bool,
    /// The strategic action that is resolved, for the facts of an update and for a draft of a
    /// secondary that is run from a script (`ti4_host::view::secondary`).
    pub(crate) watch: ti4_host::view::secondary::SecondaryWatch,
}

impl DraftBasis {
    /// Called with the session lock held, never from a nested offer callback.
    pub fn completed_step(&mut self, game: &Game<'static>) {
        if self.closed {
            return;
        }
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        self.checkpoint_decisions = game.table.log.records.len();
        self.watch.completed_step(game);
    }

    /// A copy of the game at its last completed step, and how many decisions its log had
    /// there. The copy is made on the thread that asks. `None` once the session is closed.
    pub fn fork(&mut self) -> Option<(Game<'static>, usize)> {
        if self.closed {
            return None;
        }
        let game = self.checkpoint.take()?();
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        Some((game, self.checkpoint_decisions))
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.checkpoint = None;
    }
}
