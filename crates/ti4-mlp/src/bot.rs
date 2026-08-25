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
pub struct MlpBot {
    actor: Actor,
    vocabulary: Vocabulary,
    row: FactionRow,
    temperature: f64,
    rng: rand_chacha::ChaCha8Rng,
    counters: Arc<Counters>,
}

/// What a run saw, readable while the bot is inside a table.
#[derive(Debug, Default)]
pub struct Counters {
    /// Decisions answered by the model.
    pub decisions: AtomicUsize,
    /// Decisions the model **failed** to answer, where the bot fell back to a legal guess.
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
        (Box::new(self), status)
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
    fn sparse_from(&mut self, vector: &ti4_policy::features::FeatureVector) -> SparseOption {
        let mut columns = Vec::with_capacity(vector.len());
        let mut values = Vec::with_capacity(vector.len());
        for (key, value) in vector {
            let name = ti4_policy::intern::name_of(*key);
            if self.vocabulary.is_assigned(&name) {
                self.counters.assigned.fetch_add(1, Ordering::Relaxed);
            } else {
                self.counters.oov.fetch_add(1, Ordering::Relaxed);
            }
            let column = self.vocabulary.column_of(&name);
            columns.push(i64::try_from(column).unwrap_or(0));
            #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
            values.push(*value as f32);
        }
        SparseOption { columns, values }
    }
}

impl Decider for MlpBot {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        // No position offered. Uniform over the legal set rather than a fixed index, so a decider
        // without a view does not silently bias every such decision to the first option.
        choice
            .options
            .get(self.rng.random_range(0..choice.options.len().max(1)))
            .cloned()
            .ok_or_else(|| IllegalChoice::NoOptions {
                player: choice.player.clone(),
                prompt: choice.prompt.clone(),
            })
    }

    fn choose_seeing(
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
            .collect();

        let head = Actor::resolve_head(ti4_policy::learned::decision_head(choice));
        let probabilities = match self.actor.probabilities(
            &options,
            head,
            self.row,
            self.temperature,
        ) {
            Ok(probabilities) => probabilities,
            Err(error) => {
                // A model refusal must not become an apparent success. The game still needs a legal
                // answer, so one is given — but the failure is counted and named, and any campaign
                // that sees a non-zero fallback count is invalid.
                self.counters.fallbacks.fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "MLP inference failed on head {head} ({error}); falling back to a legal guess"
                );
                return self.choose(choice);
            }
        };
        self.counters.decisions.fetch_add(1, Ordering::Relaxed);

        // Sample. The cumulative walk is the same shape the linear bot uses, so a comparison
        // between them is about the policy rather than about the sampler.
        let draw: f64 = self.rng.random_range(0.0..1.0);
        let mut cumulative = 0.0;
        for (index, probability) in probabilities.iter().enumerate() {
            cumulative += *probability;
            if draw < cumulative {
                return Ok(choice.options[index].clone());
            }
        }
        Ok(choice.options[choice.options.len() - 1].clone())
    }
}
