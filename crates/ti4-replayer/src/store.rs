//! What the window can draw, branch by branch.
//!
//! The window never holds a game - a `LiveReview` is not `Send` and stays on its branch's thread - so
//! it holds what the branch sent: one header (manifest, board, planet catalog) and the frames, appended
//! in engine order. Everything about that collection that can be wrong is wrong somewhere: a frame
//! arrives twice because a drain raced a rebuild; frames arrive before the header that makes them
//! drawable; a fork means two histories in one process and both are hundreds of megabytes. This module
//! is where those cases are decided, so the widget code can ask a single question - "what do I draw for
//! branch X, frame N" - and get an honest answer or `None`.
//!
//! It is deliberately free of egui. The alternative was to decide this inside the paint loop, where it
//! cannot be tested and where the failure mode is a window that quietly draws yesterday's frame.

use std::collections::BTreeMap;

use crate::live::{Feed, FrameTick};
use crate::{BranchId, ReplayRecord};
use ti4_review::{ReviewFrame, ReviewSession};

/// One branch's drawable history: the header may still be on its way.
#[derive(Debug)]
struct Held {
    /// The session shell the frames belong to. Until it arrives, frames are held but not drawable,
    /// because the board the tiles are drawn on comes from it.
    session: Option<ReviewSession>,
    frames: Vec<ReviewFrame>,
}

/// The frames of every branch the window has looked at, with a ceiling on how many are kept.
#[derive(Debug)]
pub struct Store {
    held: BTreeMap<BranchId, Held>,
    /// Least recently used first. Reading a branch puts it at the back.
    order: Vec<BranchId>,
    /// How many branches keep their frames at once.
    capacity: usize,
    /// Frames the branch dropped because the window was not looking, total.
    missing: usize,
    /// Branches whose frames have been released to hold the ceiling.
    released: usize,
}

/// A store that keeps one branch, which is enough for a caller that borrows it out and puts it back
/// before looking at anything else - see [`crate::gui`], which takes it out of the opened project for
/// the duration of a paint so frames can be borrowed while the app beside them is mutated.
impl Default for Store {
    fn default() -> Self {
        Self::new(1)
    }
}

impl Store {
    /// A store that keeps `capacity` branches' frames.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            held: BTreeMap::new(),
            order: Vec::new(),
            capacity: capacity.max(1),
            missing: 0,
            released: 0,
        }
    }

    /// Adopt a session the window already has in full - an imported R01 recording, or a session a
    /// branch finished and handed over.
    pub fn import(&mut self, branch: BranchId, session: ReviewSession) {
        let frames = session.frames.clone();
        let mut shell = session;
        shell.frames = Vec::new();
        self.held.insert(
            branch,
            Held {
                session: Some(shell),
                frames,
            },
        );
        self.touch(branch);
        self.trim(branch);
    }

    /// Fold one feed batch into a branch's history, appending what is new.
    ///
    /// Frames are keyed by their index rather than by arrival: a rebuild is reported both as it walks
    /// and again when it completes, and a window that appended by arrival would show the same frame
    /// twice and then wonder why its timeline lies.
    pub fn apply(&mut self, branch: BranchId, feed: Feed) -> usize {
        self.missing += feed.missing;
        let held = self.held.entry(branch).or_insert_with(|| Held {
            session: None,
            frames: Vec::new(),
        });
        // The header does not change once a branch has one: the board a game was played on is that
        // game's, and swapping it would draw a new map underneath the frames already on screen.
        if let Some(header) = feed.header
            && held.session.is_none()
        {
            let mut shell = header;
            shell.frames = Vec::new();
            held.session = Some(shell);
        }
        let mut appended = 0;
        for frame in feed.frames {
            let last = held.frames.last().map(|last| last.index);
            if last.is_some_and(|last| frame.index <= last) {
                continue;
            }
            held.frames.push(frame);
            appended += 1;
        }
        self.touch(branch);
        self.trim(branch);
        appended
    }

    /// Whether the window can draw this branch: it needs the header as well as frames.
    #[must_use]
    pub fn drawable(&self, branch: BranchId) -> bool {
        self.held
            .get(&branch)
            .is_some_and(|held| held.session.is_some() && !held.frames.is_empty())
    }

    /// The reducer's view of this branch: one [`FrameTick`] per drawable frame.
    ///
    /// Opening a recording has to tell both halves of the window - the store, which can draw a frame,
    /// and the reducer, which knows only how many there are and which one is being looked at. Telling
    /// only the store is the bug where the slider shows a position in a game the reducer has never seen
    /// and Play answers `NoFrame` forever.
    #[must_use]
    pub fn ticks(&self, branch: BranchId) -> Vec<crate::live::FrameTick> {
        self.frames(branch).iter().map(tick).collect()
    }

    /// The frames of a branch, oldest first, if it has them.
    #[must_use]
    pub fn frames(&self, branch: BranchId) -> &[ReviewFrame] {
        self.held
            .get(&branch)
            .map_or(&[][..], |held| held.frames.as_slice())
    }

    /// One frame by its index in the branch, not by engine index.
    #[must_use]
    pub fn frame(&self, branch: BranchId, position: usize) -> Option<&ReviewFrame> {
        self.frames(branch).get(position)
    }

    /// The shell to hand the shared board and panel views, if this branch has one.
    #[must_use]
    pub fn session(&self, branch: BranchId) -> Option<&ReviewSession> {
        self.held.get(&branch)?.session.as_ref()
    }

    /// How many frames this branch holds.
    #[must_use]
    pub fn len(&self, branch: BranchId) -> usize {
        self.frames(branch).len()
    }

    /// Frames the branches dropped because the window was not draining.
    #[must_use]
    pub fn missing(&self) -> usize {
        self.missing
    }

    /// Branches whose frames were released to stay under the ceiling.
    #[must_use]
    pub fn released(&self) -> usize {
        self.released
    }

    /// Forget a branch entirely, e.g. a fork whose rebuild was cancelled.
    pub fn forget(&mut self, branch: BranchId) {
        self.held.remove(&branch);
        self.order.retain(|id| *id != branch);
    }

    fn touch(&mut self, branch: BranchId) {
        self.order.retain(|id| *id != branch);
        self.order.push(branch);
    }

    /// Keep the ceiling, never at the cost of the branch being looked at.
    ///
    /// A forked game is two full histories and an R01 session is hundreds of megabytes, so "keep
    /// everything the window has ever shown" is not a policy that survives an afternoon. What gets
    /// released is the oldest-looked branch that is not the current one, and the count is kept so the
    /// window can say "this branch's frames were released" instead of showing an empty board as if the
    /// game had no frames.
    fn trim(&mut self, keep: BranchId) {
        while self.held.len() > self.capacity {
            let Some(victim) = self.order.iter().copied().find(|id| *id != keep) else {
                return;
            };
            self.held.remove(&victim);
            self.order.retain(|id| *id != victim);
            self.released += 1;
        }
    }
}

/// The one-line summary of a frame that the timeline and the branch list need.
///
/// [`FrameTick`] is what [`crate::app::ReplayApp`] reasons about; a window has full frames, and this is
/// the projection between them.
#[must_use]
pub fn tick(frame: &ReviewFrame) -> FrameTick {
    FrameTick {
        index: frame.index,
        engine_step: frame.engine_step,
        round: frame.round,
        phase: format!("{:?}", frame.phase),
        active: frame.active.clone(),
        decisions: frame.decisions.len(),
        finished: frame.finished,
        error: frame.error.clone(),
    }
}

/// Write a live branch's own answers back into its project, so the file can reproduce it later.
///
/// This is the one moment a running branch becomes durable. It charges the branch's frame count against
/// the project's budget through [`extend`](crate::ReplayerProject::extend), and stores only what the
/// branch answered itself - see [`own_answers`].
///
/// # Errors
/// Forwards [`crate::ProjectError`] from `extend`: an impossible frame count, or an answer that was
/// never offered, is refused rather than written down.
pub fn fold_answers(
    project: &mut crate::ReplayerProject,
    branch: BranchId,
    records: &[ReplayRecord],
    frames: u64,
) -> Result<(), crate::ProjectError> {
    let fork_frame = project
        .branch(branch)
        .and_then(crate::project::Branch::fork_frame);
    let answers = own_answers(records, fork_frame, branch);
    project.extend(branch, answers, frames)
}

/// The answers that belong to a branch itself, stamped with that branch's id.
///
/// A branch stores what it answered *past* its fork frame; everything up to and including that frame
/// belongs to its parent and is inherited by [`prefix`](crate::ReplayerProject::prefix) — the fork
/// frame itself is the position the child starts from, so the answers that produced it are the
/// parent's. Storing the parent's answers again would double them in the prefix, which is the one way
/// a saved project could replay a game that never happened.
#[must_use]
pub fn own_answers(
    records: &[ReplayRecord],
    fork_frame: Option<u64>,
    branch: BranchId,
) -> Vec<ReplayRecord> {
    records
        .iter()
        .filter(|record| fork_frame.is_none_or(|frame| record.frame > frame))
        .map(|record| {
            let mut owned = record.clone();
            owned.branch = branch;
            owned
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(seed: u64) -> ReviewSession {
        ReviewSession {
            schema: "x".to_owned(),
            version: 1,
            manifest: ti4_review::SessionManifest {
                checkpoint_path: "c.json".to_owned(),
                checkpoint_sha256: "c".repeat(64),
                map_pool_path: "m.json".to_owned(),
                map_pool_sha256: "m".repeat(64),
                seed,
                tile_seed: seed,
                rotation: 0,
                profile_table: ti4_review::ProfileTable::Learner,
                temperature: 0.5,
                policy: ti4_review::PolicySummary::default(),
                factions: vec!["a".to_owned()],
                initial_speaker: None,
                map_arrangement_index: None,
                map_arrangement_sha256: None,
                engine_commit: None,
                engine_dirty: false,
                content_sha256: None,
                source_scope: None,
                diplomacy: false,
                lineup: None,
            },
            board: vec![ti4_review::BoardTile {
                system: "sol".to_owned(),
                label: "Sol".to_owned(),
                q: 0,
                r: 0,
                hyperlane: false,
                special_area: None,
                anomalies: vec![],
                wormholes: vec![],
                egress: false,
                planets: vec![],
            }],
            planet_catalog: vec![],
            frames: Vec::new(),
            outcome: ti4_review::SessionOutcome::InProgress,
        }
    }

    /// One real frame of a real starting table, shared by every test in this module.
    ///
    /// A `GameState` has eighty-odd fields and no `Default`, so a hand-written fixture would either
    /// lie about the shape or spend its life chasing serde errors. The engine's setup frame is free
    /// to ask for, is definitely a frame the viewer has to cope with, and is built once per test
    /// binary.
    fn setup_frame() -> ReviewFrame {
        static FRAME: std::sync::OnceLock<ReviewFrame> = std::sync::OnceLock::new();
        FRAME
            .get_or_init(|| {
                let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .parent()
                    .and_then(std::path::Path::parent)
                    .expect("the crate lives under the workspace")
                    .to_path_buf();
                let config = ti4_review::SimulationConfig {
                    checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
                    map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
                    seed: 4_242,
                    rotation: 1,
                    table: ti4_review::ProfileTable::Learner,
                    temperature: 0.5,
                    diplomacy: false,
                    lineup: None,
                };
                let review = ti4_review::LiveReview::start(&config)
                    .expect("a real starting table to take frames from");
                review
                    .session
                    .frames
                    .first()
                    .cloned()
                    .expect("the setup frame")
            })
            .clone()
    }

    fn frame(index: usize) -> ReviewFrame {
        ReviewFrame {
            index,
            engine_step: index,
            ..setup_frame()
        }
    }

    fn batch(frames: &[usize], seed: u64) -> Feed {
        Feed {
            header: Some(header(seed)),
            frames: frames.iter().map(|index| frame(*index)).collect(),
            missing: 0,
        }
    }

    #[test]
    fn a_frame_arrives_once_however_often_it_is_reported() {
        let mut store = Store::new(2);
        let branch = BranchId::SOURCE;
        assert_eq!(store.apply(branch, batch(&[0, 1, 2], 1)), 3);
        assert_eq!(
            store.apply(branch, batch(&[1, 2, 3], 1)),
            1,
            "the rebuild reports the prefix again; only frame 3 is new"
        );
        assert_eq!(
            store
                .frames(branch)
                .iter()
                .map(|f| f.index)
                .collect::<Vec<_>>(),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn frames_without_a_header_are_held_rather_than_drawn() {
        let mut store = Store::new(2);
        let branch = BranchId::SOURCE;
        let mut feed = batch(&[0, 1], 7);
        feed.header = None;
        store.apply(branch, feed);
        assert!(
            !store.drawable(branch),
            "there is no board to draw the tiles on yet"
        );
        store.apply(branch, batch(&[2], 7));
        assert!(
            store.drawable(branch),
            "the header turns the held frames into a picture"
        );
        assert_eq!(store.len(branch), 3);
    }

    #[test]
    fn the_header_of_a_branch_is_never_replaced() {
        let mut store = Store::new(2);
        let branch = BranchId::SOURCE;
        store.apply(branch, batch(&[0], 1));
        store.apply(branch, batch(&[1], 999));
        assert_eq!(
            store.session(branch).expect("a session").manifest.seed,
            1,
            "a second header cannot change the board a game is being drawn on"
        );
    }

    #[test]
    fn the_ceiling_frees_the_oldest_branch_never_the_one_being_viewed() {
        let mut store = Store::new(2);
        store.apply(BranchId::new(0), batch(&[0], 1));
        store.apply(BranchId::new(1), batch(&[0], 2));
        store.apply(BranchId::new(2), batch(&[0], 3));
        assert!(
            !store.drawable(BranchId::new(0)),
            "the least recently looked at went to make room"
        );
        assert!(store.drawable(BranchId::new(1)) && store.drawable(BranchId::new(2)));
        assert_eq!(store.released(), 1);
        // Touching branch-1 makes branch-2 the next candidate instead, so the branch on screen is
        // never the one that goes blank.
        store.apply(BranchId::new(1), batch(&[1], 2));
        store.apply(BranchId::new(3), batch(&[0], 4));
        assert!(store.drawable(BranchId::new(1)));
        assert!(!store.drawable(BranchId::new(2)));
    }

    #[test]
    fn an_import_keeps_its_frames_and_its_header() {
        let mut store = Store::new(2);
        let mut session = header(5);
        session.frames = vec![frame(0), frame(1)];
        store.import(BranchId::SOURCE, session);
        assert!(store.drawable(BranchId::SOURCE));
        assert_eq!(store.len(BranchId::SOURCE), 2);
        assert!(store.frame(BranchId::SOURCE, 5).is_none());
    }

    #[test]
    fn forgetting_a_branch_removes_it_rather_than_emptying_it() {
        let mut store = Store::new(2);
        store.apply(BranchId::new(1), batch(&[0, 1], 3));
        store.forget(BranchId::new(1));
        assert_eq!(store.len(BranchId::new(1)), 0);
        assert!(store.session(BranchId::new(1)).is_none());
    }

    #[test]
    fn own_answers_keeps_what_follows_the_fork_and_stamps_the_branch() {
        let record = ReplayRecord {
            branch: BranchId::SOURCE,
            frame: 4,
            ask: 0,
            actor: ti4_model::id::PlayerId::new("p1"),
            faction: None,
            prompt: "What now?".to_owned(),
            offered: vec!["a".to_owned(), "b".to_owned()],
            chosen: "a".to_owned(),
            context: None,
            fingerprint: crate::ChoiceFingerprint::from_choice(&ti4_engine::choice::Choice::new(
                ti4_model::id::PlayerId::new("p1"),
                "What now?",
                vec![],
            )),
            provenance: crate::Provenance::Human,
        };
        let child = BranchId::new(3);
        let before = own_answers(std::slice::from_ref(&record), Some(5), child);
        assert!(before.is_empty(), "frame 4 belongs to the parent");
        // The fork frame itself is the position the child inherits, and the answers stamped with it
        // are the ones that produced that position - so they are the parent's, not the child's.
        let at = own_answers(std::slice::from_ref(&record), Some(4), child);
        assert!(
            at.is_empty(),
            "the fork frame's own answers produced the position forked from"
        );
        let after = own_answers(std::slice::from_ref(&record), Some(3), child);
        assert_eq!(after.len(), 1, "frame 4 is past a fork at frame 3");
        assert_eq!(after[0].branch, child, "and it is this branch's answer");
        let all = own_answers(std::slice::from_ref(&record), None, child);
        assert_eq!(all.len(), 1, "branch-0 keeps everything it settled");
    }
}
