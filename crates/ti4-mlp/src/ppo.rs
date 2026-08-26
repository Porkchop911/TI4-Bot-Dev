//! MLP PPO (M10-034), per MLP plan §6.3.
//!
//! # The two properties that fail quietly
//!
//! §6.3 says of the advantage: *"Two properties, both of which have to be stated or they will be got
//! wrong."* Both are the same shape of bug — the run trains, the numbers move, the objective is not
//! the one that was intended — so both are asserted here rather than described.
//!
//! **Detached.** `A = (returns − V_rollout).detach()`. Without the detach the actor loss
//! back-propagates through the advantage into the critic, and the policy gradient acquires a term
//! that trains `V` to make its own surrogate look better. [`Batch::advantages`] holds no graph at
//! all, which is why `advantages_are_frozen_and_normalised_once` can check it directly.
//!
//! **Frozen across epochs.** `A` is computed once from the behaviour weights and reused unchanged
//! for all four epochs, and normalised at that same moment. Recomputing it per epoch would move the
//! objective underneath the ratio `r`, so the importance-sampling correction the clip exists to
//! bound would no longer refer to a fixed target; re-normalising per epoch reintroduces exactly the
//! drift the freeze removes.
//!
//! The critic still trains every epoch, against `returns`, through its own head — the path that is
//! *supposed* to update it.
//!
//! # Ragged option sets
//!
//! Decisions have different numbers of legal options, so a minibatch's logits are flattened with
//! segment offsets rather than padded to a rectangle. §6.3: "so padding can never enter
//! softmax/entropy". A padded row would contribute a real probability to a softmax over options
//! that do not exist, and would add its own term to the entropy bonus — a policy could then raise
//! its entropy reward by being uncertain about options it was never offered.

use std::collections::BTreeMap;

use ti4_tensor::Tensor;

use crate::{Actor, FactionRow, SparseOption, bundle::CriticMode};

/// §6.3's fixed PPO settings.
///
/// The learning rate is **not** fixed here: §6.3 defers it to M10-036a's six pre-registered pilots
/// over `{1e-4, 3e-4, 1e-3} × {0, 1e-5}`, and picking one now would pre-empt that selection.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Adam learning rate. Chosen by M10-036a, not here.
    pub learning_rate: f64,
    /// Adam weight decay. Also M10-036a's.
    pub weight_decay: f64,
    /// Adam `beta_1`.
    pub beta1: f64,
    /// Adam `beta_2`.
    pub beta2: f64,
    /// Adam epsilon. Named rather than left to a framework default, per §6.3.
    pub eps: f64,
    /// The surrogate's clip range.
    pub clip_epsilon: f64,
    /// Global gradient-norm clip.
    pub grad_clip: f64,
    /// `K`, the number of epochs over each update's data.
    pub epochs: usize,
    /// Decisions per minibatch.
    pub minibatch: usize,
    /// Value-loss coefficient. Fixed at 0.5: a large one destabilises the shared trunk.
    pub value_coefficient: f64,
    /// Entropy bonus for every head except `strategy`.
    pub entropy: f64,
    /// The `strategy` head's larger entropy bonus.
    pub strategy_entropy: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            learning_rate: 3e-4,
            weight_decay: 1e-5,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            clip_epsilon: 0.2,
            grad_clip: 1.0,
            epochs: 4,
            minibatch: 4_096,
            value_coefficient: 0.5,
            entropy: 0.01,
            strategy_entropy: 0.10,
        }
    }
}

impl Settings {
    /// The entropy coefficient for one head.
    ///
    /// `strategy` gets its own larger bonus (§6.3's `--draft-entropy`): the strategy draft is a
    /// once-per-round decision with long-range consequences, so collapsing it early costs more than
    /// collapsing a tactical head.
    #[must_use]
    pub fn entropy_for(&self, head: &str) -> f64 {
        if head == "strategy" {
            self.strategy_entropy
        } else {
            self.entropy
        }
    }
}

/// One decision as it was actually played, recorded **before** optimisation begins.
#[derive(Debug, Clone)]
pub struct Step {
    /// The faction row that acted.
    pub row: FactionRow,
    /// The head index.
    pub head: usize,
    /// Every legal option's features.
    pub options: Vec<SparseOption>,
    /// Which option was taken.
    pub chosen: usize,
    /// `log pi_behaviour(chosen | s)`, under the weights that played the game.
    pub behaviour_log_prob: f64,
    /// `V(s)` under those same weights.
    pub behaviour_value: f64,
    /// The accepted return from this decision.
    pub return_to_go: f64,
    /// The option-free critic vector for this position.
    pub critic: SparseOption,
}

/// One update's data, with the advantage already frozen.
///
/// Constructing this is the moment the advantage is computed and normalised, and it happens exactly
/// once — which is what makes "frozen across epochs" a property of the type rather than of the
/// caller remembering.
#[derive(Debug)]
pub struct Batch {
    steps: Vec<Step>,
    /// `normalise(returns − V_behaviour)`, computed once. Plain `f64`, holding no graph.
    advantages: Vec<f64>,
}

impl Batch {
    /// Freeze one update's advantages.
    ///
    /// `A = returns − V_behaviour`, then normalised — both **once**, here. The values are `f64` and
    /// not tensors: there is no graph to detach because none was ever built, which is a stronger
    /// guarantee than calling `.detach()` and hoping every later path respects it.
    #[must_use]
    pub fn freeze(steps: Vec<Step>) -> Self {
        let raw: Vec<f64> = steps
            .iter()
            .map(|step| step.return_to_go - step.behaviour_value)
            .collect();
        #[expect(
            clippy::cast_precision_loss,
            reason = "decision counts are exact in f64"
        )]
        let count = raw.len().max(1) as f64;
        let mean = raw.iter().sum::<f64>() / count;
        let variance = raw.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / count;
        let deviation = variance.sqrt();
        let advantages = raw
            .iter()
            .map(|a| (a - mean) / (deviation + 1e-8))
            .collect();
        Self { steps, advantages }
    }

    /// The recorded decisions.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    /// The frozen advantages, in step order.
    #[must_use]
    pub fn advantages(&self) -> &[f64] {
        &self.advantages
    }

    /// How many decisions this update carries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether the update is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// What one epoch of one update did.
#[derive(Debug, Clone, Default)]
pub struct EpochStats {
    /// Mean clipped surrogate loss.
    pub actor_loss: f64,
    /// Mean value loss. Zero in `batch_mean` mode, where no critic is trained.
    pub critic_loss: f64,
    /// Mean entropy, by head — mandatory telemetry per §6.3.
    pub entropy: BTreeMap<String, f64>,
    /// Mean `|log r|`, an estimator of the policy's movement from the behaviour weights.
    pub kl: f64,
    /// Fraction of decisions whose ratio was outside the clip range.
    pub clipped_fraction: f64,
}

/// The per-decision quantities one forward pass produces.
struct Scored {
    /// `log pi_current(chosen | s)`, with a graph.
    log_prob: Tensor,
    /// The decision's entropy, with a graph.
    entropy: Tensor,
}

/// Score one decision under the current weights.
///
/// The softmax is over exactly this decision's options — the segment — so no padding can enter it.
fn score(actor: &Actor, step: &Step) -> Option<Scored> {
    let head = crate::heads().get(step.head)?;
    let logits = actor.logits(&step.options, head, step.row).ok()?;
    let log_probs = logits.log_softmax(0, ti4_tensor::Kind::Float);
    let chosen = i64::try_from(step.chosen).ok()?;
    if chosen >= i64::try_from(step.options.len()).ok()? {
        return None;
    }
    let log_prob = log_probs.narrow(0, chosen, 1).squeeze();
    // H = −Σ p log p, over this decision's options only.
    let entropy = -(log_probs.exp() * &log_probs).sum(ti4_tensor::Kind::Float);
    Some(Scored { log_prob, entropy })
}

/// The clipped surrogate for one decision.
///
/// `−min( r·A , clip(r, 1−e, 1+e)·A )`, with `A` arriving as a plain `f64` — a constant in the
/// graph, which is the detach §6.3 requires expressed as a type rather than a call.
fn surrogate(log_prob: &Tensor, behaviour_log_prob: f64, advantage: f64, epsilon: f64) -> Tensor {
    let ratio = (log_prob - behaviour_log_prob).exp();
    let clipped = ratio.clamp(1.0 - epsilon, 1.0 + epsilon);
    let unclipped_term = &ratio * advantage;
    let clipped_term = clipped * advantage;
    -unclipped_term.minimum(&clipped_term)
}

/// One PPO update over a frozen batch.
///
/// Returns one [`EpochStats`] per epoch. The advantages are the batch's, unchanged, in every epoch;
/// `shuffle` only changes the order decisions are visited in.
///
/// # Panics
/// If `settings.epochs` is zero, which would make an update that trains nothing.
pub fn update(
    actor: &mut Actor,
    batch: &Batch,
    critic_mode: CriticMode,
    settings: Settings,
    shuffle_seed: u64,
    mut step_optimiser: impl FnMut(&mut Actor),
) -> Vec<EpochStats> {
    assert!(
        settings.epochs > 0,
        "an update with no epochs trains nothing"
    );
    let mut out = Vec::with_capacity(settings.epochs);

    for epoch in 0..settings.epochs {
        // A domain-separated deterministic shuffle per epoch, per §6.3. Record order changes; the
        // advantages do not.
        let mut order: Vec<usize> = (0..batch.len()).collect();
        let mut rng = <rand_chacha::ChaCha8Rng as rand::SeedableRng>::seed_from_u64(
            shuffle_seed ^ (0x5050_4F5F_4550_0000 | epoch as u64),
        );
        for index in (1..order.len()).rev() {
            order.swap(index, rand::Rng::random_range(&mut rng, 0..=index));
        }

        let mut stats = EpochStats::default();
        let mut entropy_sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        let mut seen = 0usize;
        let mut clipped = 0usize;

        for minibatch in order.chunks(settings.minibatch) {
            let mut loss: Option<Tensor> = None;
            for index in minibatch {
                let step = &batch.steps[*index];
                let advantage = batch.advantages[*index];
                let Some(scored) = score(actor, step) else {
                    continue;
                };

                let actor_term = surrogate(
                    &scored.log_prob,
                    step.behaviour_log_prob,
                    advantage,
                    settings.clip_epsilon,
                );
                let head = crate::heads().get(step.head).copied().unwrap_or("other");
                let entropy_coefficient = settings.entropy_for(head);
                let mut term = &actor_term - &scored.entropy * entropy_coefficient;

                // The critic's own path to the trunk, per §6.3, and only in the modes that have one.
                if !matches!(critic_mode, CriticMode::BatchMean) {
                    let input = crate::CriticInput::from_sparse(step.critic.clone());
                    if let Ok(value) = actor.value_tensor(&input, step.row) {
                        let critic_term = (value - step.return_to_go).square().squeeze();
                        stats.critic_loss += tch::no_grad(|| critic_term.double_value(&[]));
                        term += critic_term * settings.value_coefficient;
                    }
                }

                tch::no_grad(|| {
                    let ratio = (&scored.log_prob - step.behaviour_log_prob)
                        .exp()
                        .double_value(&[]);
                    if (ratio - 1.0).abs() > settings.clip_epsilon {
                        clipped += 1;
                    }
                    stats.kl += (ratio.ln()).abs();
                    stats.actor_loss += actor_term.double_value(&[]);
                    let entry = entropy_sums.entry(head.to_owned()).or_insert((0.0, 0));
                    entry.0 += scored.entropy.double_value(&[]);
                    entry.1 += 1;
                });
                seen += 1;

                loss = Some(loss.map_or_else(|| term.shallow_clone(), |sum| sum + &term));
            }
            if let Some(loss) = loss {
                #[expect(clippy::cast_precision_loss, reason = "minibatch sizes are small")]
                let mean = loss / minibatch.len() as f64;
                mean.backward();
                step_optimiser(actor);
            }
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "decision counts are exact in f64"
        )]
        let denominator = seen.max(1) as f64;
        stats.actor_loss /= denominator;
        stats.critic_loss /= denominator;
        stats.kl /= denominator;
        #[expect(
            clippy::cast_precision_loss,
            reason = "decision counts are exact in f64"
        )]
        {
            stats.clipped_fraction = clipped as f64 / denominator;
        }
        stats.entropy = entropy_sums
            .into_iter()
            .map(|(head, (sum, count))| {
                #[expect(clippy::cast_precision_loss, reason = "counts are exact in f64")]
                let mean = sum / count.max(1) as f64;
                (head, mean)
            })
            .collect();
        out.push(stats);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Width;

    /// An actor whose parameters carry gradients, as a real update requires.
    fn trainable_actor() -> Actor {
        let mut actor = Actor::zeros(Width::W128, 4_096);
        let input = actor
            .input()
            .f_add_scalar(0.05)
            .expect("add")
            .set_requires_grad(true);
        *actor.input_mut() = input;
        let hidden = actor
            .hidden()
            .f_add_scalar(0.02)
            .expect("add")
            .set_requires_grad(true);
        *actor.hidden_mut() = hidden;
        let readout = actor
            .shared_readout()
            .f_add_scalar(0.1)
            .expect("add")
            .set_requires_grad(true);
        *actor.shared_readout_mut() = readout;
        actor
    }

    fn step(chosen: usize, options: usize, ret: f64, value: f64) -> Step {
        Step {
            row: FactionRow::of("sol").expect("roster"),
            head: 0,
            options: (0..options)
                .map(|index| SparseOption {
                    columns: vec![i64::try_from(index).unwrap_or(0) + 1],
                    values: vec![1.0],
                })
                .collect(),
            chosen,
            behaviour_log_prob: -(f64::from(u32::try_from(options).unwrap_or(1))).ln(),
            behaviour_value: value,
            return_to_go: ret,
            critic: SparseOption {
                columns: vec![7],
                values: vec![1.0],
            },
        }
    }

    #[test]
    fn advantages_are_frozen_and_normalised_once() {
        let batch = Batch::freeze(vec![
            step(0, 2, 3.0, 1.0),
            step(1, 3, 1.0, 1.0),
            step(0, 2, 2.0, 4.0),
        ]);
        let advantages = batch.advantages().to_vec();

        // Normalised: mean zero, unit deviation.
        let mean = advantages.iter().sum::<f64>() / 3.0;
        assert!(mean.abs() < 1e-9, "advantages are not centred: {mean}");
        let deviation = (advantages.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / 3.0).sqrt();
        assert!(
            (deviation - 1.0).abs() < 1e-6,
            "not unit scale: {deviation}"
        );

        // And they are plain numbers. There is no graph to leak through, which is the detach
        // requirement met by construction rather than by remembering to call `.detach()`.
        assert_eq!(batch.advantages(), advantages.as_slice());
    }

    #[test]
    fn the_advantage_uses_the_behaviour_value_not_the_current_one() {
        // `return − V_behaviour`, so a batch whose returns all equal their behaviour values has no
        // signal at all — whatever the current critic would now say about those positions.
        let batch = Batch::freeze(vec![
            step(0, 2, 2.0, 2.0),
            step(0, 2, 5.0, 5.0),
            step(0, 2, 1.0, 1.0),
        ]);
        for advantage in batch.advantages() {
            assert!(
                advantage.abs() < 1e-6,
                "a zero-signal batch produced advantage {advantage}"
            );
        }
    }

    #[test]
    fn the_surrogate_clips_and_the_clip_binds_on_the_right_side() {
        let logit = Tensor::from_slice(&[0.0f32]).squeeze();
        // ratio = exp(0 − log 1) = 1, inside the range: unclipped.
        let inside = surrogate(&logit, 0.0, 2.0, 0.2).double_value(&[]);
        assert!(
            (inside + 2.0).abs() < 1e-6,
            "unclipped surrogate was {inside}"
        );

        // A large positive advantage with a ratio far above 1+e must be clipped to 1.2·A.
        let high = Tensor::from_slice(&[1.0f32]).squeeze();
        let clipped = surrogate(&high, 0.0, 2.0, 0.2).double_value(&[]);
        assert!(
            (clipped + 2.4).abs() < 1e-4,
            "expected the clip to bind at 1.2 x A = 2.4, got {clipped}"
        );

        // With a *negative* advantage the same ratio must NOT be clipped. `min` picks the more
        // negative term, which is the unclipped one — and that asymmetry is the point: an action
        // that turned out badly and became much more likely must keep being punished in full, or
        // the clip would shelter exactly the update PPO most wants to undo.
        //
        //   ratio = e = 2.71828,  A = -2
        //   unclipped = -5.43657,  clipped = 1.2 * -2 = -2.4,  min = -5.43657
        //   surrogate = +5.43657, the unclipped magnitude
        let negative = surrogate(&high, 0.0, -2.0, 0.2).double_value(&[]);
        let unclipped_value = 2.0 * std::f64::consts::E;
        assert!(
            (negative - unclipped_value).abs() < 1e-4,
            "the negative-advantage branch was clipped: {negative} against {unclipped_value}"
        );
        // And the clipped alternative really was different, or "not clipped" says nothing.
        assert!((unclipped_value - 2.4).abs() > 1.0);
    }

    #[test]
    fn entropy_is_over_a_decisions_own_options_and_never_a_padded_rectangle() {
        // Ragged by construction: two options and five. A padded implementation would give both the
        // same option count, and the two-option decision's entropy would include three phantom
        // options it could raise its bonus by being uncertain about.
        let actor = Actor::zeros(Width::W128, 4_096);
        let narrow = score(&actor, &step(0, 2, 0.0, 0.0)).expect("scored");
        let wide = score(&actor, &step(0, 5, 0.0, 0.0)).expect("scored");

        // A zero actor is uniform, so entropy is exactly ln(n) for n options.
        let narrow_entropy = narrow.entropy.double_value(&[]);
        let wide_entropy = wide.entropy.double_value(&[]);
        assert!(
            (narrow_entropy - 2.0f64.ln()).abs() < 1e-5,
            "two options gave entropy {narrow_entropy}, expected ln 2"
        );
        assert!(
            (wide_entropy - 5.0f64.ln()).abs() < 1e-5,
            "five options gave entropy {wide_entropy}, expected ln 5"
        );
    }

    #[test]
    fn the_strategy_head_keeps_its_own_larger_entropy_bonus() {
        let settings = Settings::default();
        assert!((settings.entropy_for("strategy") - 0.10).abs() < f64::EPSILON);
        assert!((settings.entropy_for("production") - 0.01).abs() < f64::EPSILON);
        // And the two really differ, or the distinction is decorative.
        assert!(settings.entropy_for("strategy") > settings.entropy_for("production"));
    }

    #[test]
    fn batch_mean_mode_trains_no_critic() {
        // §6.3: "set critic loss to zero, and do not update/store unused value tensors".
        let mut actor = trainable_actor();
        let batch = Batch::freeze(vec![step(0, 2, 3.0, 1.0), step(1, 2, 1.0, 2.0)]);

        let stats = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            Settings {
                epochs: 1,
                minibatch: 8,
                ..Settings::default()
            },
            1,
            |_| {},
        );
        assert_eq!(stats.len(), 1);
        assert!(
            stats[0].critic_loss.abs() < f64::EPSILON,
            "batch_mean mode reported a critic loss of {}",
            stats[0].critic_loss
        );
    }

    #[test]
    fn every_epoch_sees_the_same_advantages_however_the_order_changes() {
        // The freeze, end to end. Four epochs shuffle the records differently; the advantage vector
        // is one object and is never recomputed.
        let batch = Batch::freeze(vec![
            step(0, 2, 3.0, 1.0),
            step(1, 2, 1.0, 2.0),
            step(0, 3, 5.0, 0.5),
        ]);
        let before = batch.advantages().to_vec();

        let mut actor = trainable_actor();
        let stats = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            Settings {
                epochs: 4,
                minibatch: 2,
                ..Settings::default()
            },
            42,
            |_| {},
        );
        assert_eq!(stats.len(), 4, "four epochs did not run");
        assert_eq!(
            batch.advantages(),
            before.as_slice(),
            "the advantages changed during the update"
        );
    }
}
