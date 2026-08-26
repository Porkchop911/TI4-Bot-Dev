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
    actor: Actor,
    vocabulary: Vocabulary,
    row: FactionRow,
    temperature: f64,
    rng: rand_chacha::ChaCha8Rng,
    counters: Arc<Counters>,
    /// PPO steps, when recording. Behind a shared handle for the same reason the linear bot's
    /// trajectory is: the bot is moved into the decision table and cannot be borrowed back out.
    records: std::rc::Rc<std::cell::RefCell<Vec<PpoRecord>>>,
    ppo_mode: Option<crate::bundle::CriticMode>,
    baseline: ti4_policy::progress::Baseline,
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
}

impl MlpBot {
    /// Seat an actor behind a vocabulary.
    #[must_use]
    pub fn new(actor: Actor, vocabulary: Vocabulary, row: FactionRow, stream: u64) -> Self {
        Self {
            actor,
            vocabulary,
            row,
            temperature: 1.0,
            rng: rand_chacha::ChaCha8Rng::seed_from_u64(stream),
            counters: Arc::new(Counters::default()),
            records: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
            ppo_mode: None,
            baseline: ti4_policy::progress::Baseline::default(),
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
    #[must_use]
    pub const fn at_temperature(mut self, temperature: f64) -> Self {
        self.temperature = temperature;
        self
    }

    /// The actor, for a caller that wants to inspect or load weights.
    #[must_use]
    pub const fn actor_mut(&mut self) -> &mut Actor {
        &mut self.actor
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
            // re-hash per feature (M09-029).
            if self.vocabulary.is_assigned_key(*key) {
                self.counters.assigned.fetch_add(1, Ordering::Relaxed);
            } else {
                self.counters.oov.fetch_add(1, Ordering::Relaxed);
            }
            let column = self.vocabulary.column_of_key(*key);
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
    fn record(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
        mode: crate::bundle::CriticMode,
        probabilities: &[f64],
        options: Vec<SparseOption>,
        head_index: usize,
        chosen: usize,
    ) -> Result<(), IllegalChoice> {
        // The behaviour quantities, taken here because here is the only place they exist. The
        // critic vector comes from the same bound capability the policy used, so a PPO batch
        // cannot acquire a value input the inference path would refuse.
        let (critic, behaviour_value) = if matches!(mode, crate::bundle::CriticMode::BatchMean) {
            (None, None)
        } else {
            let vector = ti4_policy::critic::critic_vector(
                seen,
                ti4_policy::critic::CriticFeatures::full(),
            );
            let critic = crate::CriticInput::new(&vector, &self.vocabulary);
            let value = self
                .actor
                .value(&critic, self.row)
                .map_err(|error| self.refuse(choice, format!("critic inference: {error}")))?;
            if !value.is_finite() {
                return Err(self.refuse(choice, "critic returned a non-finite value".to_owned()));
            }
            (Some(critic), Some(value))
        };
        let probability = probabilities.get(chosen).copied().ok_or_else(|| {
            self.refuse(choice, "sampled option has no behavior probability".to_owned())
        })?;
        if !probability.is_finite() || probability <= 0.0 {
            return Err(self.refuse(
                choice,
                format!("sampled option has invalid behavior probability {probability}"),
            ));
        }
        self.records.borrow_mut().push(PpoRecord {
            progress: ti4_policy::progress::measure(
                seen.observed(),
                &choice.player,
                self.baseline,
            ),
            step: crate::ppo::Step {
            row: self.row,
            head: head_index,
            options,
            chosen,
            behaviour_log_prob: probability.ln(),
            behaviour_value,
            return_to_go: 0.0,
            critic,
            },
        });

        Ok(())
    }

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
        let held = seen.held_secret_progress();
        let vectors = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            &choice.player,
            &held,
        );
        let options: Vec<SparseOption> = vectors
            .iter()
            .map(|vector| self.sparse_from(vector))
            .collect::<Result<_, _>>()
            .map_err(|reason| self.refuse(choice, reason))?;
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

        let head = Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        // `head_index` is fallible and returns `Result`; the schema is fixed, so a miss here is a
        // build inconsistency rather than a runtime condition — but it refuses rather than
        // defaulting to head 0, which would train the wrong readout (F-M10-034-D2).
        let head_index = Actor::head_index(head).map_err(|error| {
            self.refuse(
                choice,
                format!("resolved MLP head {head} is not in the schema: {error}"),
            )
        })?;
        let probabilities =
            match self
                .actor
                .probabilities(&options, head, self.row, self.temperature)
            {
                Ok(probabilities) => probabilities,
                Err(error) => {
                    // A model refusal is a failed game step, not a legal-looking move plus a side
                    // channel a caller may forget to inspect. The counter remains useful evidence, but
                    // correctness no longer depends on consuming it.
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
            };
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
        let draw: f64 = self.rng.random_range(0.0..1.0);
        let mut cumulative = 0.0;
        let mut chosen = choice.options.len() - 1;
        for (index, probability) in probabilities.iter().enumerate() {
            cumulative += *probability;
            if draw < cumulative {
                chosen = index;
                break;
            }
        }

        // Forced decisions are not recorded. With one legal option the policy's probability is
        // 1.0 whatever it believes, so the ratio is identically 1 and the surrogate's gradient is
        // identically zero — it would contribute nothing but weight to the per-batch means. The
        // teacher corpus drops them for the same reason, and `Batch::freeze` refuses them outright.
        if let Some(mode) = self.ppo_mode
            && choice.options.len() >= 2
        {
            self.record(choice, seen, mode, &probabilities, options, head_index, chosen)?;
        }

        Ok(choice.options[chosen].clone())
    }
}
