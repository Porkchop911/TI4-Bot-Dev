//! An MLP decider: features → columns → logits → a sampled legal option.
//!
//! This is the inference path the branch exists to build, at its smallest honest size. It takes the
//! vocabulary M09-024b2 produced and the actor M09-026 defined, and answers a real engine choice.
//!
//! # What it is not, yet
//!
//! There is no checkpoint format here — loading and saving a trained model is M09-028's schema-6
//! bundle. This takes an actor it is handed. With a zero-initialised actor every logit is identical
//! and the policy is uniform over the legal set, which is the correct behaviour for an untrained
//! model and is exactly what §7.1's legality smoke wants: proof that the MLP can *choose* legally
//! before anything is trained.
//!
//! # The pipeline
//!
//! Everything between the observation and the drawn option but the forward pass is the shared
//! decision pipeline of `ti4_policy::mlp` (features, columns, head, softmax, draw, fleet plan),
//! the same that the hosts of the game run on `ti4-infer`. This bot scores its decisions on the
//! actor on libtorch, on the actor packed for `ti4-infer` (`crate::infer`, the rollouts of
//! training), or on the GPU batch client, and records the PPO steps between the calls of the
//! pipeline.
//!
//! # Hidden information
//!
//! Features come from `projection::mlp_choice_features` against the bound `SeatObservation` the
//! engine hands a decider, so this sees the acting seat's own secrets and nothing else — the
//! boundary M09-021 established and M09-023 proved across every feature set.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rand::SeedableRng;
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_policy::mlp::{Features, ModelInfo, Pipeline, Prepared, Scorer};
use ti4_policy::vocabulary::Vocabulary;

use crate::{Actor, FactionRow, SparseOption};

pub use ti4_policy::mlp::{PACKAGE_KIND, PackageOptionTrace, PlanTrace};

/// A campaign's inference status, which cannot be discarded to obtain a success.
///
/// Handed out by [`MlpBot::seat`] and consumed by [`InferenceStatus::into_result`]. An earlier
/// version exposed a public counter and relied on each caller remembering to read it — the smoke
/// did, and nothing made the next training or profile entry point do the same, so a campaign could
/// report a successful game while every model call had failed (F-M09-026-9).
///
/// This type is `#[must_use]` and its only accessor returns a `Result`, so the success path cannot
/// be reached without the failure count having been looked at.
#[must_use = "an inference status that is never consumed hides model failures"]
pub struct InferenceStatus {
    counters: Arc<Counters>,
}

impl InferenceStatus {
    /// The campaign result: `Ok` with the decisions answered, or the failure count.
    ///
    /// # Errors
    /// [`InferenceFailed`] if any decision fell back, or if the model answered nothing at all — a
    /// run in which the model was never consulted is not a successful model run.
    pub fn into_result(self) -> Result<usize, InferenceFailed> {
        let decisions = self.counters.decisions.load(Ordering::Relaxed);
        let fallbacks = self.counters.fallbacks.load(Ordering::Relaxed);
        if fallbacks > 0 || decisions == 0 {
            return Err(InferenceFailed {
                decisions,
                fallbacks,
            });
        }
        Ok(decisions)
    }

    /// The raw counters, for reporting alongside the result.
    #[must_use]
    pub fn counters(&self) -> &Counters {
        &self.counters
    }
}

/// A campaign in which the model did not answer every decision it was given.
#[derive(Debug, Clone, Copy, thiserror::Error)]
#[error("model answered {decisions} decisions with {fallbacks} fallbacks")]
pub struct InferenceFailed {
    /// Decisions the model answered.
    pub decisions: usize,
    /// Decisions that fell back to a legal guess.
    pub fallbacks: usize,
}

/// A decider that scores every legal option with the MLP and samples from the result.
///
/// **`MlpBot` does not implement `Decider`.** It cannot be boxed and seated directly, because the
/// only type that does implement it is private and is produced solely by [`MlpBot::seat`], which
/// hands back an [`InferenceStatus`] alongside it. That is what makes fail-closed behaviour a
/// property of the API rather than of each caller remembering (F-M09-026-10): reporting a
/// successful campaign without consuming the status is not something a caller can express.
pub struct MlpBot {
    /// What scores the decisions: the actor on libtorch, the actor packed for `ti4-infer`, or the
    /// GPU batch client.
    inference: Inference,
    /// The shared decision pipeline (`ti4_policy::mlp`): the features, the columns of the
    /// vocabulary, the head, the softmax and the draw, and the seat's fleet plan and power maps.
    /// Play and the PPO records both go through it, so the features a step is trained on are the
    /// features it was played with.
    pipeline: Pipeline,
    row: FactionRow,
    temperature: f64,
    rng: rand_chacha::ChaCha8Rng,
    counters: Arc<Counters>,
    /// PPO steps, when recording. Behind a shared handle for the same reason the linear bot's
    /// trajectory is: the bot is moved into the decision table and cannot be borrowed back out.
    records: std::rc::Rc<std::cell::RefCell<Vec<PpoRecord>>>,
    ppo_mode: Option<crate::bundle::CriticMode>,
    /// One note per recorded step, pushed with it: what the trainer charges wasted activations
    /// against. Kept here rather than rebuilt from the engine's prompts, because fleet decisions
    /// have no engine prompt and planned steps have no record.
    notes: std::rc::Rc<std::cell::RefCell<Vec<crate::positive_corpus::Note>>>,
    /// What the fleet decisions and plans did, for a reviewer.
    trace: std::rc::Rc<std::cell::RefCell<Vec<PlanTrace>>>,
}

/// The model behind a bot.
#[derive(Clone)]
enum Inference {
    /// The actor on libtorch, on this thread.
    Tch(std::rc::Rc<Actor>),
    /// The actor packed for `ti4-infer` ([`crate::infer::InferModel`]), shared across threads.
    Infer(Arc<crate::infer::InferModel>),
    /// Experimental bounded cross-game scorer on a GPU.
    Gpu(crate::gpu_batch::GpuInferenceClient),
}

impl Inference {
    /// The model of the decision pipeline.
    fn info(&self) -> &dyn ModelInfo {
        match self {
            Self::Tch(actor) => &**actor,
            Self::Infer(model) => &**model,
            Self::Gpu(client) => client,
        }
    }

    /// What scores logits on this thread; the GPU client returns probabilities instead.
    fn scorer(&self) -> Option<&dyn Scorer> {
        match self {
            Self::Tch(actor) => Some(&**actor),
            Self::Infer(model) => Some(&**model),
            Self::Gpu(_) => None,
        }
    }

    /// `V(s)` of a recorded decision, on the CPU backends.
    fn value(&self, critic: &crate::CriticInput, row: FactionRow) -> Result<f64, String> {
        match self {
            Self::Tch(actor) => actor.value(critic, row).map_err(|error| error.to_string()),
            Self::Infer(model) => model.value(critic, row).map_err(|error| error.to_string()),
            Self::Gpu(_) => Err("the GPU client records its own value".to_owned()),
        }
    }
}

/// One PPO decision and the progress snapshot taken at that exact decision.
///
/// Keeping these in one record makes index alignment structural: neither side can be pushed or
/// drained without the other.
#[derive(Debug, Clone)]
pub struct PpoRecord {
    /// Behavior-policy inputs and outputs recorded before optimization.
    pub step: crate::ppo::Step,
    /// Shaped-reward state measured against the exact post-deployment baseline.
    pub progress: ti4_policy::progress::Progress,
}

/// What a run saw, readable while the bot is inside a table.
#[derive(Debug, Default)]
pub struct Counters {
    /// Decisions answered by the model.
    pub decisions: AtomicUsize,
    /// Decisions the model **failed** to answer. The name is retained from the first implementation,
    /// but failures now propagate through [`IllegalChoice::DeciderFailed`] and no legal guess is
    /// returned.
    ///
    /// Any non-zero value invalidates the campaign that produced it. An earlier version caught
    /// every actor error and made a random legal choice with no counter at all, so a run in which
    /// every model call failed exited successfully (F-M09-026-3).
    pub fallbacks: AtomicUsize,
    /// Feature names that fell to an out-of-vocabulary column.
    pub oov: AtomicUsize,
    /// Feature names that found a column of their own.
    pub assigned: AtomicUsize,
    /// Fleet decisions sampled (fact version 7).
    pub package_decisions: AtomicUsize,
    /// Engine prompts answered by a chosen fleet's plan.
    pub planned: AtomicUsize,
    /// Plans that stopped before they finished.
    pub plans_stopped: AtomicUsize,
}

impl MlpBot {
    /// Seat an actor behind a vocabulary.
    #[must_use]
    pub fn new(actor: Actor, vocabulary: Vocabulary, row: FactionRow, stream: u64) -> Self {
        Self::sharing(&std::rc::Rc::new(actor), vocabulary, row, stream)
    }

    /// A bot that reads an actor someone else owns.
    ///
    /// Inference never mutates the model, so the six seats of a game — and every game one rollout
    /// worker plays — can read one copy. [`Self::new`] gives each bot its own, which for a PPO
    /// update meant 96 games x 6 seats of deep tensor copies per update on top of one per worker:
    /// gigabytes of pure allocation churn to produce identical read-only weights.
    ///
    /// `Rc` rather than `Arc` on purpose. `tch::Tensor` is `Send` but not `Sync`, so an actor must
    /// not be shared *across* threads; confining the handle to one thread is the property that
    /// makes this safe, and `Rc` is that property written down.
    #[must_use]
    pub fn sharing(
        actor: &std::rc::Rc<Actor>,
        vocabulary: Vocabulary,
        row: FactionRow,
        stream: u64,
    ) -> Self {
        Self::with_inference(
            Inference::Tch(std::rc::Rc::clone(actor)),
            vocabulary,
            row,
            stream,
        )
    }

    /// A bot that scores on `ti4-infer`: the actor of a rollout packed in memory
    /// ([`crate::infer::InferModel::from_actor`]). One model serves every seat on every thread.
    /// It records the same PPO steps as a bot on the actor; their values come from the value head
    /// of the model, so a separate critic cannot record (the rollouts refuse that pair up front).
    #[must_use]
    pub fn on_infer(
        model: &Arc<crate::infer::InferModel>,
        vocabulary: Vocabulary,
        row: FactionRow,
        stream: u64,
    ) -> Self {
        Self::with_inference(Inference::Infer(Arc::clone(model)), vocabulary, row, stream)
    }

    fn with_inference(
        inference: Inference,
        vocabulary: Vocabulary,
        row: FactionRow,
        stream: u64,
    ) -> Self {
        Self {
            inference,
            pipeline: Pipeline::new(Arc::new(vocabulary), row.index()),
            row,
            temperature: 1.0,
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(stream),
            counters: Arc::new(Counters::default()),
            records: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            ppo_mode: None,
            notes: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            trace: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        }
    }

    /// A bot whose actor and optional critic inference is synchronously batched by one GPU owner.
    ///
    /// Feature projection, vocabulary lookup, sampling stream, progress measurement and PPO
    /// records remain owned by this seat. The client only receives immutable sparse inputs and
    /// returns the behaviour probabilities and value that this bot records.
    #[must_use]
    pub fn batched(
        gpu: crate::gpu_batch::GpuInferenceClient,
        vocabulary: Vocabulary,
        row: FactionRow,
        stream: u64,
    ) -> Self {
        Self::with_inference(Inference::Gpu(gpu), vocabulary, row, stream)
    }

    /// Hand the bot to a table and keep the status that must be consumed.
    ///
    /// The only way to obtain a boxed `MlpBot`, so no caller can seat one and forget that the model
    /// might have failed.
    pub fn seat(self) -> (Box<dyn Decider>, InferenceStatus) {
        let status = InferenceStatus {
            counters: Arc::clone(&self.counters),
        };
        (Box::new(SeatedBot(self)), status)
    }

    /// Play at a different temperature.
    ///
    /// This project uses three, and they are not interchangeable — see
    /// `plans/MLP_TEMPERATURE_REGIME.md`:
    ///
    /// - **0.25** measures a trained policy (near-greedy, so a number reflects the weights rather
    ///   than the draw). The default in `space_station_reliance`, `failed_openings` and
    ///   `empty_activations`.
    /// - **1.0** trains. PPO's importance ratio is computed against the distribution actions were
    ///   drawn from, and this bot has one `probabilities()` call, so acting and recording share
    ///   whatever is set here — set it for a PPO run only if you mean the ratio to change with it.
    /// - **2.5 and hotter** searches for lines the policy underrates (`opening_reachability`,
    ///   `rescue_imitation`).
    ///
    /// Training exploration is tuned with `--movement-entropy`, not with this. Every PPO run
    /// before 2026-09-01 used `--movement-entropy 0.05` and no temperature at all.
    #[must_use]
    pub const fn at_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }
}

/// The private decider. Boxed only by [`MlpBot::seat`], so obtaining one always yields the status.
struct SeatedBot(MlpBot);

impl Decider for SeatedBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // An MLP cannot score without its bound observation. Returning a random legal choice here
        // would recreate the same apparent-success hole as an actor error, just through the
        // position-free Decider method.
        self.0.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
        Err(IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason: "MLP inference requires a bound seat observation".to_owned(),
        })
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        self.0.decide(choice, seen)
    }
}

impl MlpBot {
    /// Record every decision as a PPO [`crate::ppo::Step`], for an update built from self-play.
    ///
    /// Off by default. §6.3 requires the behaviour log-probability and behaviour value to be stored
    /// **before** optimisation, and the only moment they exist is the decision itself: once the
    /// weights move, `V(s)` is a different number and the ratio `r` would no longer be measured
    /// against the policy that actually played.
    #[must_use]
    pub const fn recording_ppo(mut self, mode: crate::bundle::CriticMode) -> Self {
        self.ppo_mode = Some(mode);
        self
    }

    /// Set the exact post-deployment progress baseline supplied by the rollout.
    #[must_use]
    pub fn from_setup(mut self, baseline: ti4_policy::progress::Baseline) -> Self {
        self.pipeline.set_baseline(baseline);
        self
    }

    /// A handle on the aligned records this bot produces. Taken before seating, read after the game.
    #[must_use]
    pub fn ppo_records(&self) -> std::rc::Rc<std::cell::RefCell<Vec<PpoRecord>>> {
        std::rc::Rc::clone(&self.records)
    }

    /// One note per record, in record order: the decision's head, the option taken, and whether it
    /// was a decline. A fleet decision's note is a movement note (declined when it moves nothing).
    #[must_use]
    pub fn ppo_notes(&self) -> std::rc::Rc<std::cell::RefCell<Vec<crate::positive_corpus::Note>>> {
        std::rc::Rc::clone(&self.notes)
    }

    /// What this bot's fleet decisions and plans did.
    #[must_use]
    pub fn plan_trace(&self) -> std::rc::Rc<std::cell::RefCell<Vec<PlanTrace>>> {
        std::rc::Rc::clone(&self.trace)
    }

    fn note(&self, head: &str, chosen: &str, declined: bool) {
        self.notes.borrow_mut().push(crate::positive_corpus::Note {
            head: crate::capture_head(head).to_owned(),
            chosen: chosen.to_owned(),
            declined,
        });
    }

    /// The fleet decision after an activation (fact version 7): the pipeline's menu of candidate
    /// fleets, scored by the actor and drawn from; the pick becomes the seat's plan. A recorded
    /// step of the movement head, with its own options and probability.
    fn choose_package(
        &mut self,
        scorer: &dyn Scorer,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        fleet: Prepared,
        lap: &mut crate::perf::Lap,
    ) -> Result<(), IllegalChoice> {
        self.count_columns(&fleet);
        let head = fleet.head;
        let probabilities = scorer
            .logits(fleet.faction, fleet.head_index, &fleet.options)
            .and_then(|logits| ti4_policy::mlp::probabilities(&logits, self.temperature))
            .map_err(|error| self.refuse(choice, format!("package head {head}: {error}")))?;
        let picked = self
            .pipeline
            .pick_with(&fleet, probabilities, &mut self.rng)
            .map_err(|_| self.refuse(choice, "malformed package distribution".to_owned()))?;
        if let Some(mode) = self.ppo_mode
            && fleet.choices.len() >= 2
        {
            let synthetic = Choice::new(choice.player.clone(), "movement", fleet.choices.clone());
            let options = fleet
                .options
                .iter()
                .map(|columns| SparseOption::from_columns(columns))
                .collect();
            self.record(
                &synthetic,
                seen,
                mode,
                &picked.probabilities,
                options,
                fleet.head_index,
                picked.index,
                lap,
            )?;
            self.note(fleet.requested_head, &picked.label, picked.declined);
        }
        Ok(())
    }

    /// Count the feature names of a prepared decision that found a column, and those that fell
    /// to an out-of-vocabulary one.
    fn count_columns(&self, prepared: &Prepared) {
        self.counters
            .assigned
            .fetch_add(prepared.assigned, Ordering::Relaxed);
        self.counters.oov.fetch_add(prepared.oov, Ordering::Relaxed);
    }

    /// Move the pipeline's plan trace into the reviewer's, counting each entry.
    fn sync_trace(&mut self) {
        let entries = self.pipeline.take_trace();
        for entry in &entries {
            let counter = match entry {
                PlanTrace::Package { .. } => &self.counters.package_decisions,
                PlanTrace::Planned { .. } => &self.counters.planned,
                PlanTrace::Stopped { .. } => &self.counters.plans_stopped,
            };
            counter.fetch_add(1, Ordering::Relaxed);
        }
        self.trace.borrow_mut().extend(entries);
    }

    fn refuse(&self, choice: &Choice, reason: String) -> IllegalChoice {
        self.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
        IllegalChoice::DeciderFailed {
            player: choice.player.clone(),
            prompt: choice.prompt.clone(),
            reason,
        }
    }

    /// Record one decision as a PPO [`crate::ppo::Step`], at the only moment its behaviour
    /// quantities exist.
    ///
    /// §6.3 requires the behaviour log-probability and behaviour value to be stored **before**
    /// optimisation: once the weights move, `V(s)` is a different number and the ratio `r` would no
    /// longer be measured against the policy that actually played.
    ///
    /// Every failure here refuses the decision rather than skipping the record. A skipped record is
    /// a batch that is quietly smaller and biased toward whichever states happened not to fail, and
    /// no downstream check could see it (F-M10-034-D2).
    /// Whether this bundle's critic carries the power-projection facts: only when its vocabulary
    /// places them, so a bundle without them sees exactly the value input it always did.
    fn critic_wants_power(&self) -> bool {
        self.pipeline
            .vocabulary()
            .is_assigned(ti4_policy::critic::POWER_FACTS[0])
    }

    /// The power summary when either the critic or a projection reward needs it, computed once.
    fn power_summary(
        &self,
        seen: &SeatObservation<'_>,
        player: &ti4_model::id::PlayerId,
    ) -> Option<ti4_policy::power_map::Summary> {
        let reward =
            ti4_policy::progress::MEASURE_PROJECTION.load(std::sync::atomic::Ordering::Relaxed);
        (reward || self.critic_wants_power())
            .then(|| self.pipeline.power_summary(seen.observed(), player))
    }

    /// The power summary for the critic alone.
    fn critic_power_summary(
        &self,
        seen: &SeatObservation<'_>,
        player: &ti4_model::id::PlayerId,
    ) -> Option<ti4_policy::power_map::Summary> {
        self.critic_wants_power()
            .then(|| self.pipeline.power_summary(seen.observed(), player))
    }

    /// The power summary for a projection reward alone.
    fn projection_summary(
        &self,
        seen: &SeatObservation<'_>,
        player: &ti4_model::id::PlayerId,
    ) -> Option<ti4_policy::power_map::Summary> {
        ti4_policy::progress::MEASURE_PROJECTION
            .load(std::sync::atomic::Ordering::Relaxed)
            .then(|| self.pipeline.power_summary(seen.observed(), player))
    }

    fn critic_vector(
        &self,
        seen: &SeatObservation<'_>,
        power: Option<&ti4_policy::power_map::Summary>,
    ) -> ti4_policy::critic::CriticVector {
        ti4_policy::critic::critic_vector_with(
            seen,
            ti4_policy::critic::CriticFeatures::full(),
            power.filter(|_| self.critic_wants_power()),
        )
    }

    fn progress(
        &self,
        seen: &SeatObservation<'_>,
        player: &ti4_model::id::PlayerId,
        power: Option<&ti4_policy::power_map::Summary>,
    ) -> ti4_policy::progress::Progress {
        let mut progress =
            ti4_policy::progress::measure(seen.observed(), player, self.pipeline.baseline());
        if ti4_policy::progress::MEASURE_PROJECTION.load(std::sync::atomic::Ordering::Relaxed) {
            if let Some(power) = power {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "opportunity is a few hundred at most"
                )]
                {
                    progress.projection_permille = (power.opportunity * 1000.0).round() as i64;
                }
            }
        }
        progress
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the behaviour quantities are only jointly meaningful; splitting them into a struct would move the coupling rather than remove it"
    )]
    fn record(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        mode: crate::bundle::CriticMode,
        probabilities: &[f64],
        options: Vec<SparseOption>,
        head_index: usize,
        chosen: usize,
        lap: &mut crate::perf::Lap,
    ) -> Result<(), IllegalChoice> {
        // The behaviour quantities, taken here because here is the only place they exist. The
        // critic vector comes from the same bound capability the policy used, so a PPO batch
        // cannot acquire a value input the inference path would refuse.
        let power = self.power_summary(seen, &choice.player);
        let (critic, behaviour_value) = if matches!(mode, crate::bundle::CriticMode::BatchMean) {
            (None, None)
        } else {
            let vector = self.critic_vector(seen, power.as_ref());
            let critic = crate::CriticInput::new(&vector, self.pipeline.vocabulary());
            lap.mark(crate::perf::Stage::CriticFeatures);
            let value = self
                .inference
                .value(&critic, self.row)
                .map_err(|error| self.refuse(choice, format!("critic inference: {error}")))?;
            if !value.is_finite() {
                return Err(self.refuse(choice, "critic returned a non-finite value".to_owned()));
            }
            lap.mark(crate::perf::Stage::CriticForward);
            (Some(critic), Some(value))
        };
        let probability = probabilities.get(chosen).copied().ok_or_else(|| {
            self.refuse(
                choice,
                "sampled option has no behavior probability".to_owned(),
            )
        })?;
        if !probability.is_finite() || probability <= 0.0 {
            return Err(self.refuse(
                choice,
                format!("sampled option has invalid behavior probability {probability}"),
            ));
        }
        self.records.borrow_mut().push(PpoRecord {
            progress: self.progress(seen, &choice.player, power.as_ref()),
            step: crate::ppo::Step {
                row: self.row,
                head: head_index,
                options,
                chosen,
                behaviour_log_prob: probability.ln(),
                temperature: self.temperature,
                behaviour_value,
                return_to_go: 0.0,
                critic,
            },
        });
        lap.mark(crate::perf::Stage::Record);

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "a GPU response is recorded at the same decision boundary as its complete PPO inputs"
    )]
    fn record_gpu(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        mode: crate::bundle::CriticMode,
        probabilities: &[f64],
        options: Vec<SparseOption>,
        head_index: usize,
        chosen: usize,
        critic: Option<crate::CriticInput>,
        behaviour_value: Option<f64>,
        lap: &mut crate::perf::Lap,
    ) -> Result<(), IllegalChoice> {
        let power = self.projection_summary(seen, &choice.player);
        let probability = probabilities.get(chosen).copied().ok_or_else(|| {
            self.refuse(
                choice,
                "GPU sampled option has no behavior probability".to_owned(),
            )
        })?;
        if !probability.is_finite() || probability <= 0.0 {
            return Err(self.refuse(
                choice,
                format!("GPU sampled option has invalid behavior probability {probability}"),
            ));
        }
        match mode {
            crate::bundle::CriticMode::BatchMean => {
                if critic.is_some() || behaviour_value.is_some() {
                    return Err(self.refuse(
                        choice,
                        "batch-mean GPU request returned a critic".to_owned(),
                    ));
                }
            }
            _ => {
                if critic.is_none() || !behaviour_value.is_some_and(f64::is_finite) {
                    return Err(self.refuse(
                        choice,
                        "GPU request omitted a finite critic value".to_owned(),
                    ));
                }
            }
        }
        self.records.borrow_mut().push(PpoRecord {
            progress: self.progress(seen, &choice.player, power.as_ref()),
            step: crate::ppo::Step {
                row: self.row,
                head: head_index,
                options,
                chosen,
                behaviour_log_prob: probability.ln(),
                temperature: self.temperature,
                behaviour_value,
                return_to_go: 0.0,
                critic,
            },
        });
        lap.mark(crate::perf::Stage::CriticForward);
        lap.mark(crate::perf::Stage::Record);
        Ok(())
    }

    fn decide(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        let decided = self.decide_untraced(choice, seen);
        self.sync_trace();
        decided
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one decision, from features to the sampled option"
    )]
    fn decide_untraced(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        if choice.options.is_empty() {
            return Err(IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            });
        }
        let mut lap = crate::perf::Lap::start();
        // A handle (an `Rc`, an `Arc` or the client), so that the bot can be borrowed mutably
        // while the model scores.
        let inference = self.inference.clone();
        let model = inference.info();
        let scorer = inference.scorer();
        let gpu = match &inference {
            Inference::Gpu(client) => Some(client),
            Inference::Tch(_) | Inference::Infer(_) => None,
        };
        // The features of the shared pipeline (`ti4_policy::mlp`), or the answer of the seat's
        // fleet plan. The columns are resolved there too, so the vocabulary stage of a profile is
        // counted with the features.
        let prepared = match self.pipeline.features(model, choice, seen) {
            Ok(Features::Planned(option)) => return Ok(option),
            Ok(Features::Score(prepared)) => prepared,
            Err(error) => return Err(self.refuse(choice, error.to_string())),
        };
        lap.mark(crate::perf::Stage::Features);
        self.count_columns(&prepared);
        lap.mark(crate::perf::Stage::Vocabulary);
        let head = prepared.head;
        let head_index = prepared.head_index;
        // The CPU path intentionally retains its original ordering: it only builds a critic input
        // after sampling. The GPU service needs actor and critic inputs together to batch both.
        let gpu_critic = if gpu.is_some()
            && self.ppo_mode.is_some()
            && choice.options.len() >= 2
            && !matches!(self.ppo_mode, Some(crate::bundle::CriticMode::BatchMean))
        {
            let power = self.critic_power_summary(seen, &choice.player);
            let vector = self.critic_vector(seen, power.as_ref());
            lap.mark(crate::perf::Stage::CriticFeatures);
            Some(crate::CriticInput::new(&vector, self.pipeline.vocabulary()))
        } else {
            None
        };
        let sparse = || -> Vec<SparseOption> {
            prepared
                .options
                .iter()
                .map(|columns| SparseOption::from_columns(columns))
                .collect()
        };
        let mut gpu_value = None;
        let probabilities = if let Some(scorer) = scorer {
            match scorer
                .logits(prepared.faction, head_index, &prepared.options)
                .and_then(|logits| ti4_policy::mlp::probabilities(&logits, self.temperature))
            {
                Ok(probabilities) => probabilities,
                Err(error) => {
                    self.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
                    eprintln!(
                        "MLP inference failed on head {head} ({error}); refusing the decision"
                    );
                    return Err(IllegalChoice::DeciderFailed {
                        player: choice.player.clone(),
                        prompt: choice.prompt.clone(),
                        reason: format!("MLP head {head}: {error}"),
                    });
                }
            }
        } else if let Some(gpu) = gpu {
            match gpu.score(
                sparse(),
                head_index,
                self.row,
                self.temperature,
                gpu_critic.clone(),
            ) {
                Ok((probabilities, value)) => {
                    gpu_value = value;
                    probabilities
                }
                Err(error) => {
                    return Err(self.refuse(choice, format!("GPU MLP head {head}: {error}")));
                }
            }
        } else {
            return Err(self.refuse(choice, "MLP bot has no inference backend".to_owned()));
        };
        lap.mark(crate::perf::Stage::Forward);

        // Sample. The cumulative walk is the same shape the linear bot uses, so a comparison
        // between them is about the policy rather than about the sampler.
        let picked = self
            .pipeline
            .pick_with(&prepared, probabilities, &mut self.rng)
            .map_err(|_| {
                self.refuse(
                    choice,
                    "MLP returned a malformed probability distribution".to_owned(),
                )
            })?;
        self.counters.decisions.fetch_add(1, Ordering::Relaxed);

        lap.mark(crate::perf::Stage::Sampling);

        // Forced decisions are not recorded. With one legal option the policy's probability is
        // 1.0 whatever it believes, so the ratio is identically 1 and the surrogate's gradient is
        // identically zero — it would contribute nothing but weight to the per-batch means. The
        // teacher corpus drops them for the same reason, and `Batch::freeze` refuses them outright.
        if let Some(mode) = self.ppo_mode
            && choice.options.len() >= 2
        {
            if gpu.is_some() {
                self.record_gpu(
                    choice,
                    seen,
                    mode,
                    &picked.probabilities,
                    sparse(),
                    head_index,
                    picked.index,
                    gpu_critic,
                    gpu_value,
                    &mut lap,
                )?;
            } else {
                self.record(
                    choice,
                    seen,
                    mode,
                    &picked.probabilities,
                    sparse(),
                    head_index,
                    picked.index,
                    &mut lap,
                )?;
            }
            self.note(prepared.requested_head, &picked.label, picked.declined);
        }

        let fleet = self
            .pipeline
            .fleet(model, choice, seen, &picked)
            .map_err(|error| self.refuse(choice, error.to_string()))?;
        if let (Some(fleet), Some(scorer)) = (fleet, scorer) {
            self.choose_package(scorer, choice, seen, fleet, &mut lap)?;
        }
        Ok(picked.option)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Six bots reading one actor keep the state that must stay theirs.
    ///
    /// Sharing the model is safe because inference does not mutate it, but the *bot* owns three
    /// things that must not be shared: its sampling stream, its counters, and its PPO record
    /// buffer. If `sharing` ever aliased one of those, seats would sample identically or record
    /// into each other's batches, and the loss telemetry would look entirely normal while it
    /// happened.
    #[test]
    fn seats_sharing_one_actor_do_not_share_their_streams_counters_or_records() {
        let actor = std::rc::Rc::new(Actor::zeros(crate::Width::W128, 4_096));
        let vocabulary = ti4_policy::vocabulary::Vocabulary::build(["seat-state:vp"])
            .expect("a one-name vocabulary");
        let row = FactionRow::of("sol").expect("roster");

        let seats: Vec<MlpBot> = (0..6)
            .map(|seat| MlpBot::sharing(&actor, vocabulary.clone(), row, 1_000 + seat))
            .collect();

        // The actor really is one object, not six copies that happen to be equal.
        assert_eq!(
            std::rc::Rc::strong_count(&actor),
            7,
            "six seats and the local handle should hold the same actor"
        );

        // Distinct record buffers: writing through one must not be visible through another.
        seats[0].records.borrow_mut().push(PpoRecord {
            progress: ti4_policy::progress::Progress::default(),
            step: crate::ppo::Step {
                row,
                head: 0,
                options: Vec::new(),
                chosen: 0,
                behaviour_log_prob: 0.0,
                temperature: 1.0,
                behaviour_value: None,
                return_to_go: 0.0,
                critic: None,
            },
        });
        assert_eq!(seats[0].records.borrow().len(), 1);
        for seat in &seats[1..] {
            assert!(
                seat.records.borrow().is_empty(),
                "a record written by one seat was visible to another"
            );
        }

        // Distinct counters.
        seats[0].counters.decisions.fetch_add(1, Ordering::Relaxed);
        assert_eq!(seats[0].counters.decisions.load(Ordering::Relaxed), 1);
        for seat in &seats[1..] {
            assert_eq!(
                seat.counters.decisions.load(Ordering::Relaxed),
                0,
                "counters are shared between seats"
            );
        }

        // Distinct sampling streams: seeded per seat, so no two draw the same sequence.
        let draws: Vec<u64> = (0..6u64)
            .map(|seat| {
                let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(1_000 + seat);
                rand::Rng::random::<u64>(&mut rng)
            })
            .collect();
        let mut unique = draws.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(
            unique.len(),
            draws.len(),
            "two seats share a sampling stream"
        );
    }

    /// The card-text families are on exactly when the vocabulary places every one of their names.
    /// An older bundle places none and must keep its input; a partly migrated one is refused.
    #[test]
    fn card_tags_are_on_only_for_a_vocabulary_that_places_every_name() {
        let actor = std::rc::Rc::new(Actor::zeros(crate::Width::W128, 4_096));
        let row = FactionRow::of("sol").expect("roster");
        let names = ti4_policy::projection::card_tag_names();
        assert!(!names.is_empty());

        let old = ti4_policy::vocabulary::Vocabulary::build(["seat-state:vp"]).expect("builds");
        assert!(
            !MlpBot::sharing(&actor, old.clone(), row, 1)
                .pipeline
                .card_tags()
        );

        let mut partial = old.clone();
        partial
            .append_reallocating(names.iter().skip(1))
            .expect("appends");
        assert!(
            !MlpBot::sharing(&actor, partial, row, 1)
                .pipeline
                .card_tags()
        );

        let mut migrated = old;
        migrated.append_reallocating(names.iter()).expect("appends");
        assert!(
            MlpBot::sharing(&actor, migrated, row, 1)
                .pipeline
                .card_tags()
        );
    }
}
