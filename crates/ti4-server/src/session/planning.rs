//! Session-owned drafts, refreshed only at completed live engine steps.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};

use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::game::Game;
use ti4_model::id::{PlayerId, StrategyCardId};
use ti4_model::state::Phase;

use crate::planning::RecordedDecision;
use crate::planning::runner::{
    AttemptIdentity, PlanScope, PlanningRunner, PlayerPlan, SubmissionError,
};
use crate::protocol::server::{DraftApplication, DraftApplicationState, SecondaryDraftStatus};

// Checkpoint identities stay distinct even when undo replaces the whole session.
static NEXT_CHECKPOINT: AtomicU64 = AtomicU64::new(1);
const STEP_BOUND: usize = 32;
// A refreshed secondary draft replays a handful of answers. The live window
// waits this long for that replay to settle before asking the seat itself.
const SETTLE: std::time::Duration = std::time::Duration::from_millis(250);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanningError {
    Unavailable,
    UnknownSeat,
    ActivePlayer,
    NotStarted,
    NoActionOpportunity,
    ReplayMismatch,
    Submission(SubmissionError),
}

struct Application {
    decisions: Vec<RecordedDecision>,
    progress: DraftApplication,
}

/// One strategic action's secondaries, drafted by every follower at once.
///
/// The round starts when the card is certain to resolve and ends with its live
/// follower window. Each follower's draft is a private script against their own
/// disposable fork; the live window still asks the seats one at a time.
struct SecondaryRound {
    primary: PlayerId,
    card: StrategyCardId,
    /// The live window's unresolved followers, once the primary has resolved.
    /// None while it is still resolving: every other seat may draft.
    unresolved: Option<Vec<PlayerId>>,
    runners: BTreeMap<PlayerId, PlanningRunner>,
    /// Seats whose draft is submitted for them when the window reaches them.
    ready: BTreeSet<PlayerId>,
}

impl SecondaryRound {
    fn may_draft(&self, player: &PlayerId) -> bool {
        *player != self.primary
            && self
                .unresolved
                .as_ref()
                .is_none_or(|unresolved| unresolved.contains(player))
    }

    fn scope(&self) -> PlanScope {
        PlanScope::Secondary {
            primary: self.primary.clone(),
            card: self.card.clone(),
        }
    }
}

pub(crate) struct SessionPlanning {
    // Game isn't Send. This engine-provided factory carries its copied driver
    // state across threads and creates fresh input bindings when called.
    checkpoint: Option<Box<dyn FnOnce() -> Game<'static> + Send>>,
    /// How many decisions the log of the game had at the checkpoint.
    checkpoint_decisions: usize,
    pub(crate) checkpoint_id: u64,
    pub(crate) available: bool,
    closed: bool,
    saved: BTreeMap<PlayerId, PlayerPlan>,
    pub runners: BTreeMap<PlayerId, PlanningRunner>,
    applications: BTreeMap<PlayerId, Application>,
    secondary: Option<SecondaryRound>,
    // Kept apart from tactical applications: a seat may hold both drafts.
    secondary_applications: BTreeMap<PlayerId, Application>,
    /// The strategic action that is resolved, for the facts of an update and for a draft of a
    /// secondary that is run from a script (`ti4_view::secondary`).
    pub(crate) watch: ti4_view::secondary::SecondaryWatch,
}

impl SessionPlanning {
    pub fn new(saved: BTreeMap<PlayerId, PlayerPlan>) -> Self {
        Self {
            checkpoint: None,
            checkpoint_decisions: 0,
            checkpoint_id: 0,
            available: false,
            closed: false,
            saved,
            runners: BTreeMap::new(),
            applications: BTreeMap::new(),
            secondary: None,
            secondary_applications: BTreeMap::new(),
            watch: ti4_view::secondary::SecondaryWatch::default(),
        }
    }

    /// Called with the session lock held, never from a nested offer callback.
    pub fn completed_step(&mut self, game: &Game<'static>) {
        if self.closed {
            return;
        }
        self.checkpoint_id = NEXT_CHECKPOINT.fetch_add(1, Ordering::Relaxed);
        self.available = game.state.phase == Phase::Action && !game.state.finished;
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        self.checkpoint_decisions = game.table.log.records.len();
        self.watch.completed_step(game);
        self.refresh_secondary(game);
        if !self.available {
            self.retire_runners();
            return;
        }
        for runner in self.runners.values_mut() {
            runner.refresh(game, self.checkpoint_id);
        }
        // Restored scripts also resume after a non-action phase or undo/redo.
        for (player, mut plan) in std::mem::take(&mut self.saved) {
            plan.revision += 1;
            plan.base_checkpoint_id = self.checkpoint_id;
            self.runners.insert(
                player.clone(),
                PlanningRunner::start(game, player, plan, STEP_BOUND),
            );
        }
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

    pub fn start(&mut self, player: &PlayerId) -> Result<(), PlanningError> {
        if self.closed || !self.available {
            return Err(PlanningError::Unavailable);
        }
        if self.runners.contains_key(player) {
            return Ok(());
        }
        let game = self.checkpoint.take().ok_or(PlanningError::Unavailable)?();
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        self.runners.insert(
            player.clone(),
            PlanningRunner::start(
                &game,
                player.clone(),
                PlayerPlan::new(self.checkpoint_id),
                STEP_BOUND,
            ),
        );
        Ok(())
    }

    pub fn plans(&self) -> BTreeMap<PlayerId, PlayerPlan> {
        let mut plans = self.saved.clone();
        plans.extend(
            self.runners
                .iter()
                .map(|(player, runner)| (player.clone(), runner.plan())),
        );
        plans
    }

    pub fn has_draft(&self, player: &PlayerId) -> bool {
        self.runners.contains_key(player) || self.saved.contains_key(player)
    }

    pub fn reset(
        &mut self,
        player: &PlayerId,
        identity: super::super::planning::runner::AttemptIdentity,
    ) -> Result<(), PlanningError> {
        if self.closed || !self.available {
            return Err(PlanningError::Unavailable);
        }
        if self.is_applying(player) {
            return Err(PlanningError::Submission(SubmissionError::NotWaiting));
        }
        let runner = self.runners.get(player).ok_or(PlanningError::NotStarted)?;
        if runner.identity() != identity {
            return Err(PlanningError::Submission(SubmissionError::Retired));
        }
        let game = self.checkpoint.take().ok_or(PlanningError::Unavailable)?();
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        self.runners
            .get_mut(player)
            .expect("runner")
            .edit(&game, self.checkpoint_id, Vec::new());
        self.applications.remove(player);
        Ok(())
    }

    /// The engine's announcement that `card` will resolve, made inside the step
    /// that resolves its primary. Followers may draft from the last completed
    /// step. Trade waits for its window: the primary changes what a follower's
    /// secondary costs and gains.
    pub fn announce_primary(&mut self, primary: &PlayerId, card: &StrategyCardId) {
        self.watch.announce(primary, card);
        self.secondary = None;
        if self.closed
            || !self.available
            || ti4_engine::strategy_cards::card_name(
                ti4_content::ContentStore::embedded(),
                card.as_str(),
            )
            .as_deref()
                == Some("Trade")
        {
            return;
        }
        self.secondary = Some(SecondaryRound {
            primary: primary.clone(),
            card: card.clone(),
            unresolved: None,
            runners: BTreeMap::new(),
            ready: BTreeSet::new(),
        });
    }

    /// Follow the live window at a completed step: keep the drafts of followers
    /// still to be asked, and drop the round with its window.
    fn refresh_secondary(&mut self, game: &Game<'static>) {
        let window = if self.closed || game.state.finished {
            None
        } else {
            game.open_secondary()
        };
        let Some((primary, card, unresolved)) = window else {
            self.secondary = None;
            self.secondary_applications.clear();
            return;
        };
        if !self
            .secondary
            .as_ref()
            .is_some_and(|round| round.primary == *primary && round.card == *card)
        {
            self.secondary = Some(SecondaryRound {
                primary: primary.clone(),
                card: card.clone(),
                unresolved: None,
                runners: BTreeMap::new(),
                ready: BTreeSet::new(),
            });
            self.secondary_applications.clear();
        }
        let round = self.secondary.as_mut().expect("round");
        round.unresolved = Some(unresolved.to_vec());
        round
            .runners
            .retain(|player, _| unresolved.contains(player));
        round.ready.retain(|player| unresolved.contains(player));
        self.secondary_applications
            .retain(|player, _| unresolved.contains(player));
        for runner in round.runners.values_mut() {
            runner.refresh(game, self.checkpoint_id);
        }
    }

    fn secondary_round(&self, player: &PlayerId) -> Result<&SecondaryRound, PlanningError> {
        if self.closed {
            return Err(PlanningError::Unavailable);
        }
        self.secondary
            .as_ref()
            .filter(|round| round.may_draft(player))
            .ok_or(PlanningError::Unavailable)
    }

    pub fn start_secondary(&mut self, player: &PlayerId) -> Result<(), PlanningError> {
        let round = self.secondary_round(player)?;
        if round.runners.contains_key(player) {
            return Ok(());
        }
        let scope = round.scope();
        let game = self.checkpoint.take().ok_or(PlanningError::Unavailable)?();
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        let runner = PlanningRunner::start_scoped(
            &game,
            player.clone(),
            scope,
            PlayerPlan::new(self.checkpoint_id),
            STEP_BOUND,
        );
        self.secondary
            .as_mut()
            .expect("round")
            .runners
            .insert(player.clone(), runner);
        Ok(())
    }

    pub fn secondary_runner(&self, player: &PlayerId) -> Option<&PlanningRunner> {
        self.secondary.as_ref()?.runners.get(player)
    }

    /// A new answer is a different draft: it must be marked ready again.
    pub fn submit_secondary(
        &mut self,
        player: &PlayerId,
        identity: AttemptIdentity,
        option_id: &str,
    ) -> Result<(), PlanningError> {
        self.secondary_round(player)?;
        if self.secondary_applications.contains_key(player) {
            return Err(PlanningError::Submission(SubmissionError::NotWaiting));
        }
        let round = self.secondary.as_mut().expect("round");
        round
            .runners
            .get(player)
            .ok_or(PlanningError::NotStarted)?
            .submit(identity, option_id)
            .map_err(PlanningError::Submission)?;
        round.ready.remove(player);
        Ok(())
    }

    pub fn reset_secondary(
        &mut self,
        player: &PlayerId,
        identity: AttemptIdentity,
    ) -> Result<(), PlanningError> {
        self.secondary_round(player)?;
        if self.secondary_applications.contains_key(player) {
            return Err(PlanningError::Submission(SubmissionError::NotWaiting));
        }
        let runner = self
            .secondary_runner(player)
            .ok_or(PlanningError::NotStarted)?;
        if runner.identity() != identity {
            return Err(PlanningError::Submission(SubmissionError::Retired));
        }
        let game = self.checkpoint.take().ok_or(PlanningError::Unavailable)?();
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        let checkpoint_id = self.checkpoint_id;
        let round = self.secondary.as_mut().expect("round");
        round
            .runners
            .get_mut(player)
            .expect("runner")
            .edit(&game, checkpoint_id, Vec::new());
        round.ready.remove(player);
        Ok(())
    }

    /// Only a draft whose every recorded answer still replays can be made ready.
    pub fn set_secondary_ready(
        &mut self,
        player: &PlayerId,
        identity: AttemptIdentity,
        ready: bool,
    ) -> Result<(), PlanningError> {
        self.secondary_round(player)?;
        let runner = self
            .secondary_runner(player)
            .ok_or(PlanningError::NotStarted)?;
        if runner.identity() != identity {
            return Err(PlanningError::Submission(SubmissionError::Retired));
        }
        if ready
            && runner
                .validated_plan(identity)
                .is_none_or(|plan| plan.recorded_decisions.is_empty())
        {
            return Err(PlanningError::ReplayMismatch);
        }
        let round = self.secondary.as_mut().expect("round");
        if ready {
            round.ready.insert(player.clone());
        } else {
            round.ready.remove(player);
        }
        Ok(())
    }

    pub fn secondary_status(&self, player: &PlayerId, human: bool) -> Option<SecondaryDraftStatus> {
        let round = self.secondary.as_ref()?;
        let application = self
            .secondary_applications
            .get(player)
            .map(|app| app.progress.clone());
        if !round.may_draft(player) && application.is_none() {
            return None;
        }
        let runner = round.runners.get(player);
        Some(SecondaryDraftStatus {
            card: round.card.to_string(),
            played_by: round.primary.clone(),
            window_open: round.unresolved.is_some(),
            seats_before: round.unresolved.as_ref().map(|unresolved| {
                unresolved
                    .iter()
                    .position(|seat| seat == player)
                    .unwrap_or(0)
            }),
            can_start: human && !self.closed && round.may_draft(player),
            has_draft: runner.is_some(),
            identity: runner.map(PlanningRunner::identity),
            ready: round.ready.contains(player),
            application,
        })
    }

    /// The live window has reached a seat. A ready draft that still replays in
    /// full becomes the source of that seat's answers; anything else leaves the
    /// seat its ordinary live prompt. Called before [`Self::application_option`].
    pub fn begin_secondary_application(&mut self, choice: &Choice) {
        let detail = |key: &str| choice.details.get(key).and_then(serde_json::Value::as_str);
        if detail("kind") != Some("strategy_secondary") {
            return;
        }
        let player = &choice.player;
        let Some(round) = self.secondary.as_mut().filter(|round| {
            detail("card") == Some(round.card.as_str())
                && detail("played_by") == Some(round.primary.as_str())
        }) else {
            return;
        };
        // Ready is spent on this one arrival, whether or not the draft applies.
        if !round.ready.remove(player) || self.secondary_applications.contains_key(player) {
            return;
        }
        let Some(runner) = round.runners.get(player) else {
            return;
        };
        let Some(plan) = runner.settled_plan(SETTLE) else {
            return;
        };
        if !plan
            .recorded_decisions
            .first()
            .is_some_and(|decision| decision.offered_option(choice).is_some())
        {
            return;
        }
        let total = plan.recorded_decisions.len();
        self.secondary_applications.insert(
            player.clone(),
            Application {
                decisions: plan.recorded_decisions,
                progress: DraftApplication {
                    applied: 0,
                    total,
                    state: DraftApplicationState::Applying,
                    message: "Applying your secondary draft.".into(),
                },
            },
        );
    }

    pub fn application(&self, player: &PlayerId) -> Option<DraftApplication> {
        self.applications
            .get(player)
            .map(|app| app.progress.clone())
    }

    pub fn edit_movement(
        &mut self,
        player: &PlayerId,
        identity: AttemptIdentity,
    ) -> Result<(), PlanningError> {
        if self.closed || !self.available {
            return Err(PlanningError::Unavailable);
        }
        if self.has_application(player) {
            return Err(PlanningError::Submission(SubmissionError::NotWaiting));
        }
        if !self.runners.contains_key(player) {
            return Err(PlanningError::NotStarted);
        }
        let game = self.checkpoint.take().ok_or(PlanningError::Unavailable)?();
        self.checkpoint = Some(Box::new(game.fork_for_worker()));
        self.runners
            .get_mut(player)
            .expect("runner")
            .reopen_movement(&game, self.checkpoint_id, identity)
            .map_err(PlanningError::Submission)
    }

    pub fn is_applying(&self, player: &PlayerId) -> bool {
        self.applications.get(player).is_some_and(|app| {
            matches!(
                app.progress.state,
                DraftApplicationState::Applying | DraftApplicationState::WaitingForPlayer
            )
        })
    }

    pub fn has_application(&self, player: &PlayerId) -> bool {
        self.applications.contains_key(player)
    }

    pub fn executable_plan(
        &self,
        player: &PlayerId,
        identity: AttemptIdentity,
        choice: &Choice,
    ) -> Result<PlayerPlan, PlanningError> {
        if self.closed || !self.available {
            return Err(PlanningError::Unavailable);
        }
        if self.has_application(player) {
            return Err(PlanningError::Submission(SubmissionError::NotWaiting));
        }
        let runner = self.runners.get(player).ok_or(PlanningError::NotStarted)?;
        if runner.identity() != identity {
            return Err(PlanningError::Submission(SubmissionError::Retired));
        }
        if choice.player != *player
            || !choice
                .context
                .as_ref()
                .is_some_and(|context| context.subtype == "action_menu")
            || choice.option("tactical").is_none()
        {
            return Err(PlanningError::NoActionOpportunity);
        }
        let mut plan = runner
            .validated_plan(identity)
            .ok_or(PlanningError::ReplayMismatch)?;
        // Answers to the planner's start-of-turn windows precede the menu in a
        // draft. Live, those windows were answered before this menu was offered.
        let menu = plan
            .recorded_decisions
            .iter()
            .position(|decision| {
                decision
                    .context
                    .as_ref()
                    .is_some_and(|context| context.subtype == "action_menu")
            })
            .ok_or(PlanningError::ReplayMismatch)?;
        plan.recorded_decisions.drain(..menu);
        let decisions = &plan.recorded_decisions;
        if decisions.len() < 2
            || decisions[0].option_id != "tactical"
            || decisions[0].offered_option(choice).is_none()
            // Every answer is the planner's own, to a question the preview
            // showed them; each is matched against the live offer when replayed.
            || decisions.iter().any(|decision| decision.player != *player)
        {
            return Err(PlanningError::ReplayMismatch);
        }
        Ok(plan)
    }

    pub fn begin_application(
        &mut self,
        player: &PlayerId,
        mut plan: PlayerPlan,
        activation_seq: u32,
    ) {
        // Hypothetical preparation advances scoped counters to invalidate old
        // effects. Bind landing contexts to the upcoming authoritative activation
        // once, while retaining exact live question/option matching thereafter.
        for decision in &mut plan.recorded_decisions {
            if let Some(context) = &mut decision.context
                && context.subtype == "commit_ground_forces"
                && context.invasion_seq.is_some()
            {
                context.invasion_seq = Some(u64::from(activation_seq.saturating_add(1)));
            }
        }
        let total = plan.recorded_decisions.len();
        self.applications.insert(
            player.clone(),
            Application {
                decisions: plan.recorded_decisions,
                progress: DraftApplication {
                    applied: 0,
                    total,
                    state: DraftApplicationState::Applying,
                    message: "Applying draft to the live game.".into(),
                },
            },
        );
    }

    /// Other seats keep their normal input. An unplanned owner question or any
    /// changed instruction stops automation, retaining the unapplied suffix.
    pub fn application_option(&mut self, choice: &Choice) -> Option<ChoiceOption> {
        for (player, app) in self
            .applications
            .iter_mut()
            .chain(&mut self.secondary_applications)
        {
            if !matches!(
                app.progress.state,
                DraftApplicationState::Applying | DraftApplicationState::WaitingForPlayer
            ) {
                continue;
            }
            if *player != choice.player {
                app.progress.state = DraftApplicationState::WaitingForPlayer;
                app.progress.message = "Waiting for another player's live decision.".into();
                continue;
            }
            let expected = &app.decisions[app.progress.applied];
            if let Some(option) = expected.offered_option(choice) {
                app.progress.state = DraftApplicationState::Applying;
                app.progress.message = "Applying draft to the live game.".into();
                return Some(option);
            }
            app.progress.state = DraftApplicationState::NeedsDecision;
            app.progress.message = if expected.context == choice.context {
                "A planned choice is no longer available. Continue in Live; the remaining draft is retained."
            } else {
                "An unplanned live decision is required. Continue in Live; the remaining draft is retained."
            }.into();
        }
        None
    }

    pub fn application_answered(&mut self, choice: &Choice, option: &ChoiceOption) {
        for app in [
            self.applications.get_mut(&choice.player),
            self.secondary_applications.get_mut(&choice.player),
        ]
        .into_iter()
        .flatten()
        {
            if app.progress.state != DraftApplicationState::Applying
                || app.decisions[app.progress.applied]
                    .offered_option(choice)
                    .as_ref()
                    != Some(option)
            {
                continue;
            }
            app.progress.applied += 1;
            if app.progress.applied == app.progress.total {
                app.progress.state = DraftApplicationState::Applied;
                app.progress.message =
                    "Recorded draft choices applied. Continue in Live for any further decisions."
                        .into();
            }
        }
    }

    fn retire_runners(&mut self) {
        for (player, mut runner) in std::mem::take(&mut self.runners) {
            // Save the settled script after the worker has terminated.
            runner.cancel();
            self.saved.insert(player, runner.plan());
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
        self.available = false;
        self.checkpoint = None;
        self.secondary = None;
        self.secondary_applications.clear();
        self.retire_runners();
    }
}
