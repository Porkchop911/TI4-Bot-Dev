//! A disposable engine worker with one publication gate.
//!
//! `Game::step` can ask several questions before returning, and can catch an
//! input error. The shared terminal latch, rather than the step's return value,
//! decides whether its state is acceptable. Only completed, validated steps
//! become checkpoints. An offer inside a step is just a safe publication.
//!
//! Start from a completed live-step checkpoint, receive a `SafeOffer`, and send
//! its identity and one offered option ID to `submit`. `plan` returns the retained
//! answers even after an uncertainty stop. `refresh` uses a newer checkpoint;
//! `edit` replaces the script before reconstructing it. Neither installs any
//! answers in the live game.

use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation, Table};
use ti4_engine::game::Game;
use ti4_engine::observation::ExecutionObservation;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;

use super::{DecisionRecording, RecordedDecision, RecordingDecider};
use crate::projection::{project_game_view, project_game_view_full, project_pending_choice};
use crate::protocol::status::ViewerRole;
use crate::protocol::view::GameView;

pub use ti4_host::planning::{
    AttemptIdentity, FailureCategory, PlanningEnvelope, PlanningUpdate, PlayerPlan, Progress,
    SubmissionError,
};
pub use ti4_view::planning::audit::{
    ASSUMPTION, PlanScope, SECONDARY_ASSUMPTION, SafePublication, StopReason,
};
use ti4_view::planning::audit::{audit_offer, audit_secondary_offer, validate_knowledge};
use ti4_view::planning::fork::{TACTICAL_COMPLETE, activity_stop, guard_fork};

enum Input {
    Answer(String),
    Cancel,
}

struct Shared {
    scope: PlanScope,
    publication_id: u64,
    plan: PlayerPlan,
    generation: u64,
    retired: bool,
    terminal: bool,
    pending: Option<Choice>,
    reserved_request_id: Option<String>,
    last_safe: Option<SafePublication>,
    progress: Progress,
    output: mpsc::Sender<PlanningEnvelope>,
    subscribers: Vec<mpsc::SyncSender<PlanningEnvelope>>,
    latest: Option<PlanningEnvelope>,
}

impl Shared {
    fn identity(&self) -> AttemptIdentity {
        AttemptIdentity {
            checkpoint_id: self.plan.base_checkpoint_id,
            plan_revision: self.plan.revision,
            generation_id: self.generation,
        }
    }

    fn active(&self, generation: u64) -> bool {
        !self.retired && !self.terminal && self.generation == generation
    }

    fn publish(&mut self, update: PlanningUpdate) {
        if self.retired {
            return;
        }
        self.progress.recorded_answers = self.plan.recorded_decisions.len();
        self.publication_id += 1;
        let envelope = PlanningEnvelope {
            publication_id: self.publication_id,
            identity: self.identity(),
            reset_revision: self.plan.reset_revision,
            editing_movement: self.plan.editing_movement,
            movement_edit_revision: self.plan.movement_edit_revision,
            awaiting_answer: self.pending.is_some(),
            assumptions: match self.scope {
                PlanScope::Tactical => vec![ASSUMPTION.to_owned()],
                PlanScope::Secondary { .. } => {
                    vec![ASSUMPTION.to_owned(), SECONDARY_ASSUMPTION.to_owned()]
                }
            },
            progress: self.progress.clone(),
            recorded_request_ids: self.plan.recorded_request_ids.clone(),
            recorded_decisions: self.plan.recorded_decisions.clone(),
            update,
        };
        self.latest = Some(envelope.clone());
        self.subscribers
            .retain(|tx| tx.try_send(envelope.clone()).is_ok());
        let _ = self.output.send(envelope);
    }

    fn stop(&mut self, reason: StopReason) {
        self.terminal = true;
        self.pending = None;
        self.publish(PlanningUpdate::Stopped {
            reason,
            last_safe_publication: self.last_safe.clone(),
        });
    }

    fn fail(&mut self, category: FailureCategory) {
        self.terminal = true;
        self.pending = None;
        self.publish(PlanningUpdate::Failed(category));
    }
}

/// The controller owns the script; the worker owns only a disposable copy.
/// Refresh joins the old worker before starting a new one. Nothing here has a
/// live session publisher, authoritative storage, or another player's inbox.
pub struct PlanningRunner {
    player: PlayerId,
    shared: Arc<Mutex<Shared>>,
    inbox: mpsc::Sender<Input>,
    output: mpsc::Receiver<PlanningEnvelope>,
    worker: Option<JoinHandle<()>>,
    step_bound: usize,
}

impl PlanningRunner {
    /// Content is shared for the worker's lifetime, as in the live server. The
    /// supplied Game must be at a completed step, not paused inside a decider.
    pub fn start(
        checkpoint: &Game<'static>,
        player: PlayerId,
        plan: PlayerPlan,
        step_bound: usize,
    ) -> Self {
        Self::start_scoped(checkpoint, player, PlanScope::Tactical, plan, step_bound)
    }

    /// As [`Self::start`], previewing whatever `scope` names.
    pub fn start_scoped(
        checkpoint: &Game<'static>,
        player: PlayerId,
        scope: PlanScope,
        plan: PlayerPlan,
        step_bound: usize,
    ) -> Self {
        let (output_tx, output) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared {
            scope,
            publication_id: 0,
            plan,
            generation: 1,
            retired: false,
            terminal: false,
            pending: None,
            reserved_request_id: None,
            last_safe: None,
            progress: Progress {
                recorded_answers: 0,
                replayed: 0,
                remaining: 0,
                completed_steps: 0,
                nested_answers_since_checkpoint: 0,
            },
            output: output_tx,
            subscribers: Vec::new(),
            latest: None,
        }));
        let (inbox, worker) = spawn(
            checkpoint.fork_for_worker(),
            player.clone(),
            shared.clone(),
            step_bound,
        );
        Self {
            player,
            shared,
            inbox,
            output,
            worker: Some(worker),
            step_bound,
        }
    }

    #[must_use]
    pub fn plan(&self) -> PlayerPlan {
        self.shared.lock().expect("planning lock").plan.clone()
    }

    /// The offer's full identity rejects stale answers, including answers to a
    /// previous revision in the same generation. No default option is supplied.
    pub fn submit(
        &self,
        identity: AttemptIdentity,
        option_id: &str,
    ) -> Result<(), SubmissionError> {
        self.submit_with_request_id(identity, option_id, None)
    }

    pub fn submit_with_request_id(
        &self,
        identity: AttemptIdentity,
        option_id: &str,
        request_id: Option<&str>,
    ) -> Result<(), SubmissionError> {
        let mut shared = self.shared.lock().expect("planning lock");
        if !shared.active(identity.generation_id) || shared.identity() != identity {
            return Err(SubmissionError::Retired);
        }
        let pending = shared.pending.as_ref().ok_or(SubmissionError::NotWaiting)?;
        if pending.option(option_id).is_none() {
            return Err(SubmissionError::UnknownOption);
        }
        // Taking the pending offer reserves this answer. A duplicate submission
        // cannot queue another answer for the following question.
        shared.pending = None;
        shared.reserved_request_id = request_id.map(str::to_owned);
        self.inbox
            .send(Input::Answer(option_id.to_owned()))
            .map_err(|_| SubmissionError::Retired)
    }

    /// Also filters queued output: retirement cannot leave old buffers visible.
    pub fn recv_timeout(
        &self,
        timeout: Duration,
    ) -> Result<PlanningEnvelope, mpsc::RecvTimeoutError> {
        let deadline = Instant::now() + timeout;
        loop {
            let envelope = self
                .output
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))?;
            let shared = self.shared.lock().expect("planning lock");
            if !shared.retired && envelope.identity.generation_id == shared.generation {
                return Ok(envelope);
            }
        }
    }

    /// Discards queued updates from retired generations without blocking.
    pub fn try_recv(&self) -> Result<PlanningEnvelope, mpsc::TryRecvError> {
        loop {
            let envelope = self.output.try_recv()?;
            let shared = self.shared.lock().expect("planning lock");
            if !shared.retired && envelope.identity.generation_id == shared.generation {
                return Ok(envelope);
            }
        }
    }

    /// Subscribe under the publication lock so reconnect sees the latest update
    /// followed only by newer publications. Slow consumers are disconnected.
    pub(crate) fn subscribe(&self) -> mpsc::Receiver<PlanningEnvelope> {
        let (tx, rx) = mpsc::sync_channel(128);
        let mut shared = self.shared.lock().expect("planning lock");
        if let Some(mut envelope) = shared.latest.clone() {
            envelope.awaiting_answer =
                shared.pending.is_some() && envelope.identity == shared.identity();
            let _ = tx.try_send(envelope);
        }
        shared.subscribers.push(tx);
        rx
    }

    pub(crate) fn is_current_attempt(&self, identity: AttemptIdentity) -> bool {
        let shared = self.shared.lock().expect("planning lock");
        !shared.retired
            && shared.identity().checkpoint_id == identity.checkpoint_id
            && shared.generation == identity.generation_id
    }

    pub fn cancel(&mut self) {
        // Publication and retirement hold the same lock. A worker cannot pass
        // the generation check and then publish after this retirement.
        self.shared.lock().expect("planning lock").retired = true;
        let _ = self.inbox.send(Input::Cancel);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub(crate) fn identity(&self) -> AttemptIdentity {
        self.shared.lock().expect("planning lock").identity()
    }

    /// A refreshed, fully matched prefix is executable even if the preview stopped
    /// at the next unsupported or uncertain step. Failed/mismatched previews aren't.
    pub(crate) fn validated_plan(&self, identity: AttemptIdentity) -> Option<PlayerPlan> {
        let shared = self.shared.lock().expect("planning lock");
        let latest = shared.latest.as_ref()?;
        if shared.retired
            || shared.plan.editing_movement
            || shared.plan.replacing_movement
            || shared.identity() != identity
            || latest.identity != identity
            || latest.progress.remaining != 0
            || (latest.awaiting_answer && shared.pending.is_none())
            || matches!(
                latest.update,
                PlanningUpdate::Preparing
                    | PlanningUpdate::Failed(_)
                    | PlanningUpdate::Stopped {
                        reason: StopReason::ReplayMismatch,
                        ..
                    }
            )
            // A followed secondary may end in a draw. Its recorded prefix still
            // matched in full, and only those answers are ever replayed live.
            || (shared.scope == PlanScope::Tactical
                && matches!(
                    latest.update,
                    PlanningUpdate::Stopped {
                        reason: StopReason::KnowledgeChanged,
                        ..
                    }
                ))
        {
            return None;
        }
        Some(shared.plan.clone())
    }

    /// The validated plan once a rebuilt attempt has finished replaying, waiting
    /// up to `timeout` for it. A refresh restarts the worker, and its replay is
    /// what says whether the retained answers still fit the newer position.
    pub(crate) fn settled_plan(&self, timeout: Duration) -> Option<PlayerPlan> {
        let deadline = Instant::now() + timeout;
        loop {
            let (identity, settled) = {
                let shared = self.shared.lock().expect("planning lock");
                (
                    shared.identity(),
                    shared.retired || shared.terminal || shared.pending.is_some(),
                )
            };
            if settled {
                return self.validated_plan(identity);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    /// Replay the whole retained script on a fresh hypothetical turn. Answers
    /// within the old worker's unfinished step are included in that script.
    pub fn refresh(&mut self, checkpoint: &Game<'static>, checkpoint_id: u64) {
        self.rebuild(checkpoint, checkpoint_id, None);
    }

    pub fn edit(
        &mut self,
        checkpoint: &Game<'static>,
        checkpoint_id: u64,
        decisions: Vec<RecordedDecision>,
    ) {
        self.rebuild(checkpoint, checkpoint_id, Some(decisions));
    }

    pub(crate) fn reopen_movement(
        &mut self,
        checkpoint: &Game<'static>,
        checkpoint_id: u64,
        identity: AttemptIdentity,
    ) -> Result<(), SubmissionError> {
        {
            let mut shared = self.shared.lock().expect("planning lock");
            if shared.retired || shared.identity() != identity {
                return Err(SubmissionError::Retired);
            }
            // Do not interrupt an answer already reserved by another connection.
            if shared.plan.editing_movement || (!shared.terminal && shared.pending.is_none()) {
                return Err(SubmissionError::NotWaiting);
            }
            if shared.plan.movement_start().is_none() {
                return Err(SubmissionError::NotWaiting);
            }
            shared.retired = true;
            shared.plan.editing_movement = true;
            shared.plan.movement_edit_revision += 1;
        }
        self.rebuild(checkpoint, checkpoint_id, None);
        Ok(())
    }

    fn rebuild(
        &mut self,
        checkpoint: &Game<'static>,
        checkpoint_id: u64,
        decisions: Option<Vec<RecordedDecision>>,
    ) {
        self.cancel();
        // Captured test output from retired attempts has no further use.
        while self.output.try_recv().is_ok() {}
        {
            let mut shared = self.shared.lock().expect("planning lock");
            if let Some(decisions) = decisions {
                shared.plan.reset_revision += 1;
                shared.plan.editing_movement = false;
                shared.plan.revalidation_start = None;
                shared.plan.replacing_movement = false;
                shared.plan.recorded_decisions = decisions;
                shared.plan.recorded_request_ids.clear();
            }
            shared.plan.revision += 1;
            shared.plan.base_checkpoint_id = checkpoint_id;
            shared.generation += 1;
            shared.retired = false;
            shared.terminal = false;
            shared.pending = None;
            shared.reserved_request_id = None;
            shared.last_safe = None;
            shared.latest = None;
            shared.progress = Progress {
                recorded_answers: 0,
                replayed: 0,
                remaining: 0,
                completed_steps: 0,
                nested_answers_since_checkpoint: 0,
            };
            shared.publish(PlanningUpdate::Preparing);
        }
        let (inbox, worker) = spawn(
            checkpoint.fork_for_worker(),
            self.player.clone(),
            self.shared.clone(),
            self.step_bound,
        );
        self.inbox = inbox;
        self.worker = Some(worker);
    }
}

impl Drop for PlanningRunner {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct Gate {
    shared: Arc<Mutex<Shared>>,
    scope: PlanScope,
    generation: u64,
    player: PlayerId,
    observation: ExecutionObservation,
    baseline: GameView,
}

impl Gate {
    fn check(&self, shared: &mut Shared) -> bool {
        if !shared.active(self.generation) {
            return false;
        }
        let reason = activity_stop(&self.observation);
        if let Some(reason) = reason {
            shared.stop(reason);
            return false;
        }
        true
    }

    fn publication(
        &self,
        state: &GameState,
        choice: Option<&Choice>,
        // Never published, and no boundary of their own: dice and draws latch
        // uncertainty, combat and new identities fail the knowledge check, and
        // another seat's question stops the preview.
        _events: &[String],
    ) -> Result<SafePublication, StopReason> {
        if choice.is_some_and(|choice| choice.player != self.player) {
            return Err(StopReason::OtherPlayerRequired);
        }
        let enabled_options = choice
            .map(|choice| match &self.scope {
                PlanScope::Tactical => audit_offer(choice),
                PlanScope::Secondary { primary, card } => {
                    audit_secondary_offer(choice, primary, card)
                }
            })
            .transpose()?
            .unwrap_or_default();
        let viewer = ViewerRole::Player(self.player.clone());
        let projected_choice =
            project_pending_choice(&viewer, choice.map(|choice| (choice, "planning"))).map(
                |pending| {
                    let mut choice = pending.choice;
                    // Unavailable component/contact options have not been audited. Do
                    // not publish their labels, payloads or previews just to disable them.
                    choice
                        .options
                        .retain(|option| enabled_options.contains(&option.id));
                    choice
                },
            );
        let publication = SafePublication {
            position: project_game_view_full(state, &viewer, &[], projected_choice.as_ref(), &[]),
            choice: projected_choice,
            events: Vec::new(),
        };
        validate_knowledge(&self.baseline, &publication, &self.player, &self.scope)?;
        Ok(publication)
    }
}

struct InboxDecider {
    inbox: mpsc::Receiver<Input>,
    shared: Arc<Mutex<Shared>>,
    generation: u64,
}

/// Resolver::emit can ask without a state observation. Give that second table
/// an explicit stop source too, rather than leaving its first-option default.
struct UnobservedDecider {
    gate: Arc<Gate>,
}

impl Decider for UnobservedDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        let mut shared = self.gate.shared.lock().expect("planning lock");
        if self.gate.check(&mut shared) {
            shared.stop(if choice.player == self.gate.player {
                StopReason::UnsupportedOffer
            } else {
                StopReason::OtherPlayerRequired
            });
        }
        Err(refused(choice))
    }
}

impl Decider for InboxDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        match self.inbox.recv() {
            Ok(Input::Answer(id))
                if self
                    .shared
                    .lock()
                    .expect("planning lock")
                    .active(self.generation) =>
            {
                choice.option(&id).cloned().ok_or_else(|| refused(choice))
            }
            _ => Err(refused(choice)),
        }
    }
}

struct PlanningDecider {
    gate: Arc<Gate>,
    // The callback buffer is consumed once. Plain choose() cannot reuse an
    // earlier question's state and pretend it is a fresh observation.
    buffer: Arc<Mutex<Option<GameState>>>,
    cursor: usize,
    recorder: RecordingDecider,
    recording: DecisionRecording,
}

impl PlanningDecider {
    fn answer(&mut self, choice: &Choice, observed: bool) -> Result<ChoiceOption, IllegalChoice> {
        let state = self.buffer.lock().expect("offer buffer lock").take();
        let mut replay_answer = None;
        {
            let mut shared = self.gate.shared.lock().expect("planning lock");
            if !self.gate.check(&mut shared) {
                return Err(refused(choice));
            }
            let publication = if observed {
                state
                    .as_ref()
                    .ok_or(StopReason::UnsupportedOffer)
                    .and_then(|state| self.gate.publication(state, Some(choice), &[]))
            } else {
                Err(StopReason::UnsupportedOffer)
            };
            match publication {
                Ok(publication) => {
                    let editing = shared.plan.editing_movement
                        && shared.plan.movement_start() == Some(self.cursor);
                    let inserting = shared.plan.replacing_movement
                        && shared.plan.revalidation_start == Some(self.cursor);
                    if !editing && !inserting {
                        if let Some(expected) = shared.plan.recorded_decisions.get(self.cursor) {
                            replay_answer = publication
                                .choice
                                .as_ref()
                                .and_then(|choice| expected.replay_option(choice));
                            if replay_answer.is_none()
                                && !shared
                                    .plan
                                    .revalidation_start
                                    .is_some_and(|start| self.cursor >= start)
                            {
                                shared.last_safe = Some(publication);
                                shared.stop(StopReason::ReplayMismatch);
                                return Err(refused(choice));
                            }
                        }
                    }
                    shared.last_safe = Some(publication.clone());
                    shared.pending = if replay_answer.is_some() {
                        None
                    } else {
                        publication.choice.clone()
                    };
                    shared.publish(PlanningUpdate::SafeOffer(publication));
                }
                Err(reason) => {
                    shared.stop(reason);
                    return Err(refused(choice));
                }
            }
        }
        let replaying = replay_answer.is_some();
        let answer = if let Some(answer) = replay_answer {
            Ok(answer)
        } else {
            self.recorder.choose(choice)
        };
        let mut shared = self.gate.shared.lock().expect("planning lock");
        if !self.gate.check(&mut shared) {
            return Err(refused(choice));
        }
        let answer = match answer {
            Ok(answer) => answer,
            Err(_) => {
                if replaying {
                    shared.stop(StopReason::ReplayMismatch);
                } else {
                    shared.fail(FailureCategory::Engine);
                }
                return Err(refused(choice));
            }
        };
        if !shared
            .last_safe
            .as_ref()
            .and_then(|offer| offer.choice.as_ref())
            .is_some_and(|choice| choice.option(&answer.id).is_some())
        {
            shared.stop(StopReason::UnsupportedOffer);
            return Err(refused(choice));
        }
        if replaying {
            shared.plan.recorded_decisions[self.cursor] =
                RecordedDecision::from_answer(choice, &answer);
        } else {
            if shared.plan.editing_movement {
                // Replace only when the new answer has passed the publication gate.
                // Opening/reloading the editor never loses the original movement.
                let start = shared.plan.movement_start().expect("editable movement");
                let end = shared.plan.recorded_decisions[start..]
                    .iter()
                    .position(|decision| {
                        !decision.context.as_ref().is_some_and(|context| {
                            matches!(context.subtype.as_str(), "movement_step" | "load_cargo")
                        })
                    })
                    .map_or(shared.plan.recorded_decisions.len(), |offset| {
                        start + offset
                    });
                shared.plan.recorded_decisions.drain(start..end);
                shared.plan.revalidation_start = Some(start);
                shared.plan.replacing_movement = true;
                shared.plan.recorded_request_ids.clear();
                shared.plan.editing_movement = false;
            }
            let recorded = self
                .recording
                .lock()
                .expect("recording lock")
                .last()
                .expect("recorded answer")
                .clone();
            if shared.plan.replacing_movement && shared.plan.revalidation_start == Some(self.cursor)
            {
                shared.plan.recorded_decisions.insert(self.cursor, recorded);
                shared.plan.revalidation_start = Some(self.cursor + 1);
                if answer.id == "done_moving" {
                    shared.plan.replacing_movement = false;
                }
            } else if self.cursor < shared.plan.recorded_decisions.len() {
                if shared.plan.recorded_decisions[self.cursor].matches_replay_question(choice) {
                    shared.plan.recorded_decisions[self.cursor] = recorded;
                } else {
                    // An extra fresh question (for example, a higher payment)
                    // needs an inserted answer, not removal of the next intent.
                    shared.plan.recorded_decisions.insert(self.cursor, recorded);
                }
            } else {
                shared.plan.recorded_decisions.push(recorded);
            }
            shared.plan.revision += 1;
            if let Some(request_id) = shared.reserved_request_id.take() {
                shared.plan.recorded_request_ids.push(request_id);
            }
        }
        self.cursor += 1;
        shared.progress.replayed = self.cursor;
        let replay_end = if shared.plan.editing_movement {
            shared.plan.movement_start().expect("editable movement")
        } else {
            shared.plan.recorded_decisions.len()
        };
        shared.progress.remaining = replay_end.saturating_sub(self.cursor);
        // The answer survives even if applying it rolls a rift or draws a card.
        shared.progress.nested_answers_since_checkpoint += 1;
        shared.pending = None;
        Ok(answer)
    }
}

impl Decider for PlanningDecider {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, false)
    }
    fn choose_seeing(
        &mut self,
        choice: &Choice,
        _seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.answer(choice, true)
    }
}

fn refused(choice: &Choice) -> IllegalChoice {
    IllegalChoice::DeciderFailed {
        player: choice.player.clone(),
        prompt: choice.prompt.clone(),
        reason: "disposable execution stopped".to_owned(),
    }
}

fn spawn(
    make_game: impl FnOnce() -> Game<'static> + Send + 'static,
    player: PlayerId,
    shared: Arc<Mutex<Shared>>,
    step_bound: usize,
) -> (mpsc::Sender<Input>, JoinHandle<()>) {
    let (inbox_tx, inbox) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        let mut game = make_game();
        let observation = guard_fork(&mut game, &player);
        let (generation, scope) = {
            let shared = shared.lock().expect("planning lock");
            (shared.generation, shared.scope.clone())
        };
        let baseline = project_game_view(&game.state, &ViewerRole::Player(player.clone()));
        let gate = Arc::new(Gate {
            shared: shared.clone(),
            scope,
            generation,
            player: player.clone(),
            observation,
            baseline,
        });
        *game.timing.table_mut() =
            Table::with_default(Box::new(UnobservedDecider { gate: gate.clone() }));
        let script = {
            let shared = shared.lock().expect("planning lock");
            let mut script = shared.plan.recorded_decisions.clone();
            if shared.plan.editing_movement {
                script.truncate(shared.plan.movement_start().expect("editable movement"));
            }
            script
        };
        shared.lock().expect("planning lock").progress.remaining = script.len();
        let (recorder, recording) = RecordingDecider::new(Box::new(InboxDecider {
            inbox,
            shared: shared.clone(),
            generation,
        }));
        let buffer = Arc::new(Mutex::new(None));
        game.table = Table::with_default(Box::new(PlanningDecider {
            gate: gate.clone(),
            buffer: buffer.clone(),
            cursor: 0,
            recorder,
            recording,
        }));
        game.table.on_observed_offer(move |_, state| {
            *buffer.lock().expect("offer buffer lock") = Some(state.clone());
        });
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run(game, player, &gate, step_bound)
        }));
        if result.is_err() {
            let mut shared = shared.lock().expect("planning lock");
            if gate.check(&mut shared) {
                shared.fail(FailureCategory::Worker);
            }
        }
    });
    (inbox_tx, worker)
}

fn run(mut game: Game<'static>, player: PlayerId, gate: &Gate, step_bound: usize) {
    let prepared = match &gate.scope {
        PlanScope::Tactical => game.prepare_hypothetical_turn(&player),
        PlanScope::Secondary { primary, card } => {
            game.prepare_hypothetical_secondary(&player, primary, card)
        }
    };
    if let Err(error) = prepared {
        tracing::debug!(?error, "planning preparation failed");
        let mut shared = gate.shared.lock().expect("planning lock");
        if gate.check(&mut shared) {
            shared.fail(FailureCategory::Preparation);
        }
        return;
    }
    // Keep a real driver copy only at a completed-step boundary. The nested
    // offer callback must never try to fork a suspended Rust call stack.
    let mut last_checkpoint = game.fork();
    let mut event_cursor = game.events.len();
    for _ in 0..step_bound {
        {
            let mut shared = gate.shared.lock().expect("planning lock");
            if !gate.check(&mut shared) {
                return;
            }
        }
        let result = game.step();
        let mut shared = gate.shared.lock().expect("planning lock");
        if !gate.check(&mut shared) {
            return;
        }
        if let Some(error) = result.error {
            tracing::debug!(?error, "disposable planning step failed");
            shared.fail(FailureCategory::Engine);
            return;
        }
        let publication = match gate.publication(&game.state, None, &game.events[event_cursor..]) {
            Ok(publication) => publication,
            Err(reason) => {
                shared.stop(reason);
                return;
            }
        };
        let complete = match &gate.scope {
            PlanScope::Tactical => (TACTICAL_COMPLETE, StopReason::MovementComplete),
            PlanScope::Secondary { .. } => (
                ti4_engine::game::SECONDARY_PREVIEW_COMPLETE,
                StopReason::SecondaryComplete,
            ),
        };
        let movement_complete = game.events[event_cursor..]
            .iter()
            .any(|event| event == complete.0);
        event_cursor = game.events.len();
        shared.progress.completed_steps += 1;
        shared.progress.nested_answers_since_checkpoint = 0;
        // Replace only after validation. The previous copy remains valid even
        // when a nested answer stops the next step before it returns.
        drop(last_checkpoint);
        last_checkpoint = game.fork();
        shared.last_safe = Some(publication.clone());
        shared.publish(PlanningUpdate::SafeStep(publication));
        if movement_complete {
            shared.stop(complete.1);
            return;
        }
    }
    let mut shared = gate.shared.lock().expect("planning lock");
    if gate.check(&mut shared) {
        shared.stop(StopReason::StepLimit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ti4_content::ContentStore;
    use ti4_engine::choice::Scripted;
    use ti4_engine::decision_context::DecisionSource;
    use ti4_engine::preview::Quantity;
    use ti4_model::id::ActionCardId;

    fn gate(state: &GameState) -> (Arc<Gate>, mpsc::Receiver<PlanningEnvelope>) {
        let (output, receiver) = mpsc::channel();
        let shared = Arc::new(Mutex::new(Shared {
            scope: PlanScope::Tactical,
            publication_id: 0,
            plan: PlayerPlan::new(1),
            generation: 1,
            retired: false,
            terminal: false,
            pending: None,
            reserved_request_id: None,
            last_safe: None,
            progress: Progress {
                recorded_answers: 0,
                replayed: 0,
                remaining: 0,
                completed_steps: 0,
                nested_answers_since_checkpoint: 0,
            },
            output,
            subscribers: Vec::new(),
            latest: None,
        }));
        (
            Arc::new(Gate {
                shared,
                scope: PlanScope::Tactical,
                generation: 1,
                player: PlayerId::new("b"),
                observation: ExecutionObservation::default(),
                baseline: project_game_view(state, &ViewerRole::Player(PlayerId::new("b"))),
            }),
            receiver,
        )
    }

    #[test]
    fn actual_envelope_allows_public_changes_and_known_consumption_but_not_new_identities() {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        state.player_mut(&PlayerId::new("b")).unwrap().action_cards =
            vec![ActionCardId::new("sabo1")];
        let (gate, _) = gate(&state);
        let player = state.player_mut(&PlayerId::new("b")).unwrap();
        player.trade_goods += 2;
        player.action_cards.clear();
        assert!(gate.publication(&state, None, &[]).is_ok());
        state
            .player_mut(&PlayerId::new("b"))
            .unwrap()
            .action_cards
            .push(ActionCardId::new("sabo2"));
        assert_eq!(
            gate.publication(&state, None, &[]),
            Err(StopReason::KnowledgeChanged)
        );
    }

    #[test]
    fn plain_choose_cannot_reuse_a_buffer_or_consume_a_recorded_answer() {
        let mut state = ti4_engine::fixtures::game(&["a", "b"]);
        state.phase = ti4_model::state::Phase::Action;
        state.active = Some(PlayerId::new("b"));
        let game = Game::new(state, ContentStore::embedded())
            .with_galaxy(ti4_engine::fixtures::plain_hub().galaxy);
        let choice = game.legal_options().unwrap();
        let expected = RecordedDecision::from_answer(&choice, choice.option("tactical").unwrap());
        let (gate, output) = gate(&game.state);
        gate.shared
            .lock()
            .unwrap()
            .plan
            .recorded_decisions
            .push(expected);
        let (recorder, recording) = RecordingDecider::new(Box::new(Scripted::new(["tactical"])));
        let buffer = Arc::new(Mutex::new(Some(game.state.clone())));
        let mut decider = PlanningDecider {
            gate,
            buffer: buffer.clone(),
            cursor: 0,
            recorder,
            recording,
        };
        assert!(decider.choose(&choice).is_err());
        assert!(buffer.lock().unwrap().is_none());
        assert_eq!(decider.cursor, 0);
        assert!(matches!(
            output.try_recv().unwrap().update,
            PlanningUpdate::Stopped {
                reason: StopReason::UnsupportedOffer,
                ..
            }
        ));
        assert!(output.try_recv().is_err());
    }

    #[test]
    fn a_familiar_move_does_not_admit_a_card_derived_preview() {
        let choice = Choice::new(
            PlayerId::new("b"),
            "movement",
            vec![ChoiceOption::new("move|19|0", "move").previewed(
                ti4_engine::preview::Preview::certain(vec![ti4_engine::preview::Delta::new(
                    Quantity::ActionCardsHeld,
                    0,
                    1,
                )]),
            )],
        )
        .contextualized(ti4_engine::decision_context::DecisionContext::new(
            PlayerId::new("b"),
            DecisionSource::Rule("89.2".into()),
            "movement_step",
            ti4_model::state::Phase::Action,
            1,
        ));
        assert_eq!(audit_offer(&choice), Err(StopReason::UnsupportedOffer));
    }
}
