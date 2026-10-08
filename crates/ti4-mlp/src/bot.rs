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
//! # Hidden information
//!
//! Features come from `projection::mlp_choice_features` against the bound `SeatObservation` the
//! engine hands a decider, so this sees the acting seat's own secrets and nothing else — the
//! boundary M09-021 established and M09-023 proved across every feature set.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use rand::{Rng, SeedableRng};
use ti4_engine::choice::{Choice, ChoiceOption, Decider, IllegalChoice, SeatObservation};
use ti4_policy::vocabulary::Vocabulary;

use crate::{Actor, FactionRow, SparseOption};

/// The option kind of a fleet decision's synthetic options.
pub const PACKAGE_KIND: &str = "package";

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
    actor: Option<std::rc::Rc<Actor>>,
    /// Experimental bounded cross-game scorer. Absent in the ordinary CPU path.
    gpu: Option<crate::gpu_batch::GpuInferenceClient>,
    vocabulary: Vocabulary,
    /// Resolved `FeatureKey -> (column, assigned)`, memoised for this bot.
    ///
    /// The same feature names recur constantly — across the options of one decision, across the
    /// decisions of one game, and across every game this bot plays — while the answer for a given
    /// key never changes, so the ordered-index walk is repeated work rather than a lookup that had
    /// to happen.
    ///
    /// Safe because the vocabulary cannot move underneath it: `MlpBot` owns it by value, nothing
    /// hands out `&mut` to it, and the MLP path never calls `Vocabulary::append` — a published
    /// generation is immutable and a new one produces a new bundle and a new bot. If that ever
    /// changes, this cache has to be cleared with it; that is the whole invariant.
    ///
    /// Counters are still incremented per *occurrence*, not per distinct key, so `assigned` and
    /// `oov` read exactly as they did before.
    resolved: std::collections::HashMap<ti4_policy::intern::FeatureKey, (usize, bool)>,
    row: FactionRow,
    temperature: f64,
    rng: rand_chacha::ChaCha8Rng,
    counters: Arc<Counters>,
    /// PPO steps, when recording. Behind a shared handle for the same reason the linear bot's
    /// trajectory is: the bot is moved into the decision table and cannot be borrowed back out.
    records: std::rc::Rc<std::cell::RefCell<Vec<PpoRecord>>>,
    ppo_mode: Option<crate::bundle::CriticMode>,
    baseline: ti4_policy::progress::Baseline,
    /// One note per recorded step, pushed with it: what the trainer charges wasted activations
    /// against. Kept here rather than rebuilt from the engine's prompts, because fleet decisions
    /// have no engine prompt and planned steps have no record.
    notes: std::rc::Rc<std::cell::RefCell<Vec<crate::positive_corpus::Note>>>,
    /// The fleet this seat chose at its last activation (fact version 7), while it is being moved.
    plan: Option<PlanSlot>,
    /// What the fleet decisions and plans did, for a reviewer.
    trace: std::rc::Rc<std::cell::RefCell<Vec<PlanTrace>>>,
    /// This seat's power map, kept as state and refreshed only where the board changed.
    power_cache: std::cell::RefCell<ti4_policy::power_map::PowerCache>,
    /// Every seat's power map as it stood at this seat's latest activation decision.
    last_table: std::cell::RefCell<Option<ti4_policy::power_facts::TablePower>>,
}

/// A fleet being carried out, tied to the activation it was chosen for.
#[derive(Debug, Clone)]
struct PlanSlot {
    player: ti4_model::id::PlayerId,
    system: ti4_model::id::SystemId,
    execution: ti4_policy::tactical_plan::Execution,
}

/// One fleet on a decision's menu, as a trace shows it: label, probability, facts.
pub type PackageOptionTrace = (String, f64, Vec<(String, f64)>);

/// One entry of a bot's plan trace.
#[derive(Debug, Clone)]
pub enum PlanTrace {
    /// A fleet decision: the menu, what the model made of it, and the pick.
    Package {
        player: String,
        system: String,
        /// The menu, the manual option last.
        options: Vec<PackageOptionTrace>,
        chosen: usize,
    },
    /// An engine prompt answered by the plan rather than the model.
    Planned {
        player: String,
        prompt: String,
        chosen: String,
    },
    /// The plan stopped before it was finished.
    Stopped { player: String, reason: String },
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
        let actor = std::rc::Rc::clone(actor);
        Self {
            actor: Some(actor),
            gpu: None,
            vocabulary,
            resolved: std::collections::HashMap::new(),
            row,
            temperature: 1.0,
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(stream),
            counters: Arc::new(Counters::default()),
            records: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            ppo_mode: None,
            baseline: ti4_policy::progress::Baseline::default(),
            notes: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            plan: None,
            trace: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            power_cache: std::cell::RefCell::new(ti4_policy::power_map::PowerCache::default()),
            last_table: std::cell::RefCell::new(None),
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
        Self {
            actor: None,
            gpu: Some(gpu),
            vocabulary,
            resolved: std::collections::HashMap::new(),
            row,
            temperature: 1.0,
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(stream),
            counters: Arc::new(Counters::default()),
            records: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            ppo_mode: None,
            baseline: ti4_policy::progress::Baseline::default(),
            notes: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            plan: None,
            trace: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            power_cache: std::cell::RefCell::new(ti4_policy::power_map::PowerCache::default()),
            last_table: std::cell::RefCell::new(None),
        }
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

    /// Turn one option's feature vector into dense columns.
    ///
    /// A name with no column of its own is **not dropped**: `column_of` routes it to its family's
    /// out-of-vocabulary column, or the global one. Dropping would make an unknown option word
    /// indistinguishable from its absence.
    fn sparse_from(
        &mut self,
        vector: &ti4_policy::features::FeatureVector,
    ) -> Result<SparseOption, String> {
        let mut columns = Vec::with_capacity(vector.len());
        let mut values = Vec::with_capacity(vector.len());
        for (key, value) in vector {
            // Keyed lookups throughout: resolving the name here cost a lock, an allocation and a
            // re-hash per feature (M09-029). Both answers now come from one memoised probe rather
            // than two walks of the ordered index -- see `resolved` and `Vocabulary::resolve_key`.
            let (column, assigned) = match self.resolved.get(key) {
                Some(hit) => *hit,
                None => {
                    let answer = self.vocabulary.resolve_key(*key);
                    self.resolved.insert(*key, answer);
                    answer
                }
            };
            if assigned {
                self.counters.assigned.fetch_add(1, Ordering::Relaxed);
            } else {
                self.counters.oov.fetch_add(1, Ordering::Relaxed);
            }
            columns.push(
                i64::try_from(column)
                    .map_err(|_| format!("feature column {column} does not fit i64"))?,
            );
            #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
            let value = *value as f32;
            if !value.is_finite() {
                return Err("a projected feature is not finite f32".to_owned());
            }
            values.push(value);
        }
        Ok(SparseOption { columns, values })
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
    pub const fn from_setup(mut self, baseline: ti4_policy::progress::Baseline) -> Self {
        self.baseline = baseline;
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

    /// Answer from the current plan when this prompt is the plan's; drop a plan that no longer
    /// applies. `None` hands the prompt to the model.
    fn planned(&mut self, choice: &Choice, seen: &SeatObservation<'_>) -> Option<ChoiceOption> {
        let slot = self.plan.as_mut()?;
        let starts_over = choice.options.iter().any(|option| {
            option.kind == ti4_engine::tactical::ACTIVATE_KIND
                || option.id == ti4_engine::game::TACTICAL_ACTION_ID
        });
        let elsewhere = seen.observed().active_system() != Some(&slot.system);
        if starts_over || (elsewhere && choice.player == slot.player) {
            self.plan = None;
            return None;
        }
        if choice.player != slot.player || !ti4_policy::tactical_plan::Execution::handles(choice) {
            return None;
        }
        let answer = slot.execution.answer(choice);
        let player = slot.player.to_string();
        match (&answer, &slot.execution.state) {
            (Some(id), state) => {
                let option = choice
                    .options
                    .iter()
                    .find(|option| &option.id == id)?
                    .clone();
                self.counters.planned.fetch_add(1, Ordering::Relaxed);
                self.trace.borrow_mut().push(PlanTrace::Planned {
                    player,
                    prompt: choice.prompt.clone(),
                    chosen: option.id.clone(),
                });
                if *state == ti4_policy::tactical_plan::PlanState::Complete {
                    self.plan = None;
                }
                Some(option)
            }
            (None, ti4_policy::tactical_plan::PlanState::NotOffered(reason)) => {
                self.counters.plans_stopped.fetch_add(1, Ordering::Relaxed);
                self.trace.borrow_mut().push(PlanTrace::Stopped {
                    player,
                    reason: reason.clone(),
                });
                self.plan = None;
                None
            }
            (None, _) => None,
        }
    }

    /// Fact version 7: after an activation, sample which candidate fleet to send and keep it as
    /// the plan. A recorded step of the movement head, with its own options and probability.
    #[expect(
        clippy::too_many_lines,
        reason = "one decision: menu, features, sampling, record, plan"
    )]
    fn choose_package(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        system: &ti4_model::id::SystemId,
        lap: &mut crate::perf::Lap,
    ) -> Result<(), IllegalChoice> {
        let Some(actor) = self.actor.clone() else {
            return Ok(());
        };
        let Some(predictor) = actor.battle_predictor() else {
            return Ok(());
        };
        if !ti4_policy::battle::samples_packages(predictor.feature_version) {
            return Ok(());
        }
        let menu = ti4_policy::tactical_plan::packages(
            seen.observed(),
            &choice.player,
            system,
            Some(predictor),
        );
        if menu.is_empty() {
            return Ok(());
        }
        // The menu, then building the fleet step by step.
        let mut options: Vec<ChoiceOption> = menu
            .iter()
            .enumerate()
            .map(|(index, package)| {
                ChoiceOption::labelled(
                    format!("package|{index}"),
                    PACKAGE_KIND,
                    format!(
                        "{}: {} ships, {} carried",
                        package.strategy.label(),
                        package.moves.len(),
                        package.loads.len()
                    ),
                )
            })
            .collect();
        options.push(ChoiceOption::labelled(
            "package|manual",
            PACKAGE_KIND,
            "build the fleet step by step",
        ));
        let synthetic = Choice::new(choice.player.clone(), "movement", options);
        let held = seen.held_secret_progress();
        let vectors = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            &synthetic,
            &synthetic.player,
            &held,
            self.baseline,
        );
        let facts: Vec<Vec<(&'static str, f64)>> = menu
            .iter()
            .map(|package| ti4_policy::battle::package_facts(Some(package)))
            .chain(std::iter::once(ti4_policy::battle::package_facts(None)))
            .collect();
        let vectors = ti4_policy::battle::append_facts(vectors, &facts);
        let sparse: Vec<SparseOption> = vectors
            .iter()
            .map(|vector| self.sparse_from(vector))
            .collect::<Result<_, _>>()
            .map_err(|reason| self.refuse(choice, reason))?;
        let requested = ti4_policy::learned::decision_head(&synthetic);
        let head = actor.resolve_layout_head(requested);
        let head_index = actor
            .layout_head_index(head)
            .map_err(|error| self.refuse(choice, format!("package head {head}: {error}")))?;
        let head = head.to_owned();
        let probabilities = actor
            .probabilities(&sparse, &head, self.row, self.temperature)
            .map_err(|error| self.refuse(choice, format!("package head {head}: {error}")))?;
        if probabilities.len() != synthetic.options.len()
            || probabilities.iter().any(|p| !p.is_finite() || *p < 0.0)
        {
            return Err(self.refuse(choice, "malformed package distribution".to_owned()));
        }
        self.counters
            .package_decisions
            .fetch_add(1, Ordering::Relaxed);
        let chosen = self.sample(&probabilities);
        let manual = chosen == menu.len();
        self.trace.borrow_mut().push(PlanTrace::Package {
            player: choice.player.to_string(),
            system: system.to_string(),
            options: synthetic
                .options
                .iter()
                .zip(&probabilities)
                .zip(&facts)
                .map(|((option, p), facts)| {
                    (
                        option.label.clone(),
                        *p,
                        facts.iter().map(|(n, v)| ((*n).to_owned(), *v)).collect(),
                    )
                })
                .collect(),
            chosen,
        });
        if let Some(mode) = self.ppo_mode
            && synthetic.options.len() >= 2
        {
            self.record(
                &synthetic,
                seen,
                mode,
                &probabilities,
                sparse,
                head_index,
                chosen,
                lap,
            )?;
            let moves_nothing = manual || menu[chosen].moves.is_empty();
            let label = if manual {
                "manual"
            } else {
                menu[chosen].strategy.label()
            };
            self.note(requested, label, moves_nothing);
        }
        self.plan = (!manual).then(|| PlanSlot {
            player: choice.player.clone(),
            system: system.clone(),
            execution: ti4_policy::tactical_plan::Execution::new(menu[chosen].clone()),
        });
        Ok(())
    }

    fn sample(&mut self, probabilities: &[f64]) -> usize {
        let draw: f64 = self.rng.random_range(0.0..1.0);
        let mut cumulative = 0.0;
        for (index, probability) in probabilities.iter().enumerate() {
            cumulative += *probability;
            if draw < cumulative {
                return index;
            }
        }
        probabilities.len() - 1
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
    #[expect(
        clippy::too_many_arguments,
        reason = "the behaviour quantities are only jointly meaningful; splitting them into a                   struct would move the coupling rather than remove it"
    )]
    /// Whether this bundle's critic carries the power-projection facts: only when its vocabulary
    /// places them, so a bundle without them sees exactly the value input it always did.
    fn critic_wants_power(&self) -> bool {
        self.vocabulary
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
        (reward || self.critic_wants_power()).then(|| {
            self.power_cache
                .borrow_mut()
                .summary(seen.observed(), player)
        })
    }

    /// The power summary for the critic alone.
    fn critic_power_summary(
        &self,
        seen: &SeatObservation<'_>,
        player: &ti4_model::id::PlayerId,
    ) -> Option<ti4_policy::power_map::Summary> {
        self.critic_wants_power().then(|| {
            self.power_cache
                .borrow_mut()
                .summary(seen.observed(), player)
        })
    }

    /// The power summary for a projection reward alone.
    fn projection_summary(
        &self,
        seen: &SeatObservation<'_>,
        player: &ti4_model::id::PlayerId,
    ) -> Option<ti4_policy::power_map::Summary> {
        ti4_policy::progress::MEASURE_PROJECTION
            .load(std::sync::atomic::Ordering::Relaxed)
            .then(|| {
                self.power_cache
                    .borrow_mut()
                    .summary(seen.observed(), player)
            })
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
        let mut progress = ti4_policy::progress::measure(seen.observed(), player, self.baseline);
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
            let critic = crate::CriticInput::new(&vector, &self.vocabulary);
            lap.mark(crate::perf::Stage::CriticFeatures);
            let value = self
                .actor
                .as_ref()
                .ok_or_else(|| self.refuse(choice, "CPU MLP bot has no actor".to_owned()))?
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

    #[expect(
        clippy::too_many_lines,
        reason = "one decision, from features to the sampled option"
    )]
    fn decide(
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
        if let Some(option) = self.planned(choice, seen) {
            return Ok(option);
        }
        let held = seen.held_secret_progress();
        // The seat's own setup baseline goes in with the features: the opening-progress facts are
        // deltas against it, and a bot that passed a default would report absolute holdings as
        // gains — wrong in the flattering direction, and invisible.
        let vectors = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            &choice.player,
            &held,
            self.baseline,
        );
        // An arena-capable actor adds each movement option's battle facts; any other actor sees
        // exactly the vectors it always did.
        let vectors = match self
            .actor
            .as_ref()
            .and_then(|actor| actor.battle_predictor())
        {
            Some(predictor) => {
                let facts = ti4_policy::battle::decision_facts(
                    seen.observed(),
                    choice,
                    &choice.player,
                    predictor,
                );
                ti4_policy::battle::append_facts(vectors, &facts)
            }
            None => vectors,
        };
        // Deal values on diplomacy options, for a bundle whose vocabulary places them (migrated
        // with the names appended); any other bundle sees exactly the vectors it always did.
        let vectors = if self
            .vocabulary
            .is_assigned(ti4_policy::deal_value::FACT_SCORE)
        {
            let facts = ti4_policy::deal_value::deal_facts(seen.observed(), choice);
            ti4_policy::battle::append_facts(vectors, &facts)
        } else {
            vectors
        };
        // Power projection per option, for a bundle whose vocabulary places the names (migrated
        // with them appended); any other bundle sees exactly the vectors it always did. The whole
        // table's projection is computed at the activation decision and reused for the moves of
        // that action, which see the board as it stood when the system was chosen.
        let vectors = if self
            .vocabulary
            .is_assigned(ti4_policy::power_facts::NAMES[0])
        {
            let summary = self
                .power_cache
                .borrow_mut()
                .summary(seen.observed(), &choice.player);
            let needs_table = choice.options.iter().any(|option| {
                option.kind == ti4_engine::tactical::ACTIVATE_KIND
                    || option.kind == ti4_engine::tactical::MOVE_KIND
            });
            if needs_table && (seen.active_system().is_none() || self.last_table.borrow().is_none())
            {
                *self.last_table.borrow_mut() =
                    Some(ti4_policy::power_facts::TablePower::of(seen.observed()));
            }
            let table = self.last_table.borrow();
            let facts = ti4_policy::power_facts::decision_facts(
                seen.observed(),
                choice,
                &choice.player,
                &summary,
                if needs_table { table.as_ref() } else { None },
            );
            ti4_policy::battle::append_facts(vectors, &facts)
        } else {
            vectors
        };
        lap.mark(crate::perf::Stage::Features);
        let options: Vec<SparseOption> = vectors
            .iter()
            .map(|vector| self.sparse_from(vector))
            .collect::<Result<_, _>>()
            .map_err(|reason| self.refuse(choice, reason))?;
        lap.mark(crate::perf::Stage::Vocabulary);
        if options.len() != choice.options.len() {
            return Err(self.refuse(
                choice,
                format!(
                    "MLP projection produced {} vectors for {} legal options",
                    options.len(),
                    choice.options.len()
                ),
            ));
        }

        let requested_head = ti4_policy::learned::decision_head(choice);
        // `head_index` is fallible and returns `Result`; the schema is fixed, so a miss here is a
        // build inconsistency rather than a runtime condition — but it refuses rather than
        // defaulting to head 0, which would train the wrong readout (F-M10-034-D2).
        let (head, head_index) = if let Some(actor) = &self.actor {
            let head = actor.resolve_layout_head(requested_head);
            let index = actor.layout_head_index(head).map_err(|error| {
                self.refuse(
                    choice,
                    format!("resolved MLP head {head} is not in the schema: {error}"),
                )
            })?;
            (head.to_owned(), index)
        } else if let Some(gpu) = &self.gpu {
            let head = gpu.resolve_layout_head(requested_head);
            let index = gpu.layout_head_index(head).map_err(|error| {
                self.refuse(
                    choice,
                    format!("resolved GPU MLP head {head} is not in the schema: {error}"),
                )
            })?;
            (head.to_owned(), index)
        } else {
            return Err(self.refuse(choice, "MLP bot has no inference backend".to_owned()));
        };
        // The CPU path intentionally retains its original ordering: it only builds a critic input
        // after sampling. The GPU service needs actor and critic inputs together to batch both.
        let gpu_critic = if self.gpu.is_some()
            && self.ppo_mode.is_some()
            && choice.options.len() >= 2
            && !matches!(self.ppo_mode, Some(crate::bundle::CriticMode::BatchMean))
        {
            let power = self.critic_power_summary(seen, &choice.player);
            let vector = self.critic_vector(seen, power.as_ref());
            lap.mark(crate::perf::Stage::CriticFeatures);
            Some(crate::CriticInput::new(&vector, &self.vocabulary))
        } else {
            None
        };
        let mut gpu_value = None;
        let probabilities = if let Some(actor) = &self.actor {
            match actor.probabilities(&options, &head, self.row, self.temperature) {
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
        } else if let Some(gpu) = &self.gpu {
            match gpu.score(
                options.clone(),
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
        if probabilities.len() != choice.options.len()
            || probabilities
                .iter()
                .any(|probability| !probability.is_finite() || *probability < 0.0)
        {
            return Err(self.refuse(
                choice,
                "MLP returned a malformed probability distribution".to_owned(),
            ));
        }
        self.counters.decisions.fetch_add(1, Ordering::Relaxed);

        // Sample. The cumulative walk is the same shape the linear bot uses, so a comparison
        // between them is about the policy rather than about the sampler.
        let chosen = self.sample(&probabilities);

        lap.mark(crate::perf::Stage::Sampling);

        // Forced decisions are not recorded. With one legal option the policy's probability is
        // 1.0 whatever it believes, so the ratio is identically 1 and the surrogate's gradient is
        // identically zero — it would contribute nothing but weight to the per-batch means. The
        // teacher corpus drops them for the same reason, and `Batch::freeze` refuses them outright.
        if let Some(mode) = self.ppo_mode
            && choice.options.len() >= 2
        {
            if self.gpu.is_some() {
                self.record_gpu(
                    choice,
                    seen,
                    mode,
                    &probabilities,
                    options,
                    head_index,
                    chosen,
                    gpu_critic,
                    gpu_value,
                    &mut lap,
                )?;
            } else {
                self.record(
                    choice,
                    seen,
                    mode,
                    &probabilities,
                    options,
                    head_index,
                    chosen,
                    &mut lap,
                )?;
            }
            let taken = &choice.options[chosen];
            self.note(requested_head, &taken.id, taken.is_decline());
        }

        let taken = choice.options[chosen].clone();
        if taken.kind == ti4_engine::tactical::ACTIVATE_KIND {
            self.plan = None;
            let system = ti4_model::id::SystemId::new(taken.id.clone());
            self.choose_package(choice, seen, &system, &mut lap)?;
        }
        Ok(taken)
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
}
