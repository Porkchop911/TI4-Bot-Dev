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

use crate::{Actor, CriticInput, FactionRow, SparseOption, bundle::CriticMode};

/// §6.3's fixed PPO settings.
///
/// The learning rate is **not** fixed here: §6.3 defers it to M10-036a's six pre-registered pilots
/// over `{1e-4, 3e-4, 1e-3} × {0, 1e-5}`, and picking one now would pre-empt that selection.
#[derive(Debug, Clone, Copy, PartialEq)]
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
    /// The `movement` head's own entropy bonus.
    ///
    /// Movement is the lowest-entropy head on every update, and the measured failure is that seats
    /// concentrate their forces — a settled, confident habit of sending everything to one system.
    /// Raising exploration everywhere would also randomise production and scoring, which are not
    /// the problem; this raises it where the local optimum is.
    ///
    /// Defaults to `entropy`, so leaving it alone changes nothing.
    pub movement_entropy: f64,
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
            movement_entropy: 0.01,
        }
    }
}

impl Settings {
    /// Whether two settings describe the same optimizer.
    ///
    /// Exactly the six values `Adam::new` copies. A run may anneal entropy between updates; it may
    /// not change its learning rate or clip under a retained optimizer, because those are what its
    /// moments were accumulated against.
    #[must_use]
    pub fn optimizer_equivalent(&self, other: Self) -> bool {
        self.learning_rate == other.learning_rate
            && self.beta1 == other.beta1
            && self.beta2 == other.beta2
            && self.eps == other.eps
            && self.weight_decay == other.weight_decay
            && self.grad_clip == other.grad_clip
    }

    /// The entropy coefficient for one head.
    ///
    /// `strategy` gets its own larger bonus (§6.3's `--draft-entropy`): the strategy draft is a
    /// once-per-round decision with long-range consequences, so collapsing it early costs more than
    /// collapsing a tactical head.
    #[must_use]
    pub fn entropy_for(&self, head: &str) -> f64 {
        match head {
            "strategy" => self.strategy_entropy,
            "movement" => self.movement_entropy,
            _ => self.entropy,
        }
    }
}

/// One decision as it was actually played, recorded **before** optimisation begins.
///
/// The critic field is typed: a policy [`SparseOption`] cannot be substituted.
///
/// ```compile_fail,E0308
/// # use ti4_mlp::{CriticInput, SparseOption};
/// let option = SparseOption { columns: vec![1], values: vec![1.0] };
/// let _: CriticInput = option;
/// ```
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
    /// The sampling temperature the behaviour distribution was drawn at.
    ///
    /// Recorded per step rather than assumed, because the ratio `pi_new / pi_behaviour` is only a
    /// ratio if both sides are the same function. The optimiser divides its logits by this before
    /// the softmax; without it the numerator was `softmax(s)` while the denominator was
    /// `softmax(s / T)`, which is two different distributions and not a ratio at all.
    ///
    /// A run at temperature 1.0 is unaffected -- `s / 1.0 == s` -- which is why this was invisible
    /// until a run used any other value, and why that run destroyed a 91% policy.
    pub temperature: f64,
    /// `V(s)` under those same weights.
    pub behaviour_value: Option<f64>,
    /// The accepted return from this decision.
    pub return_to_go: f64,
    /// The option-free critic vector for this position.
    pub critic: Option<CriticInput>,
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
    mode: CriticMode,
    /// The same constants a minibatch uploads, in the `f32` the model is built from.
    ///
    /// Every one of these was rebuilt per minibatch per epoch — the same values narrowed from `f64`
    /// four times an update — although only the *selection* changes between epochs. They are
    /// derived once here, in decision order, and gathered by index when a minibatch is packed.
    constants: Constants,
}

/// Per-decision constants, derived once at freeze and indexed thereafter.
#[derive(Debug)]
struct Constants {
    behaviour: Vec<f32>,
    advantage: Vec<f32>,
    returns: Vec<f32>,
    /// Per-option expansion counts, so a minibatch can size its buffers exactly.
    options: Vec<usize>,
    heads: Vec<i64>,
    rows: Vec<i64>,
}

/// Put one sparse vector into the canonical form `gather_reduce_batch`'s fast path expects.
///
/// Strictly increasing columns, each appearing once, duplicates folded. The fold order is exactly
/// `ordered_pairs`': by column, then by the total order over the f32 bit pattern. That tie-break is
/// not decoration -- summing two duplicate columns' values in the other order gives a different
/// f32 -- so reproducing it here is what makes canonicalising once bit-identical to sorting on
/// every visit, rather than merely close to it.
///
/// Done when a batch is frozen. PPO reads a frozen batch four times per update, and the four reads
/// used to re-sort the same columns each time.
fn canonicalise(option: &mut crate::SparseOption) {
    let mut pairs: Vec<(i64, f32)> = option
        .columns
        .iter()
        .copied()
        .zip(option.values.iter().copied())
        .collect();
    pairs.sort_by(|left, right| {
        left.0.cmp(&right.0).then_with(|| {
            ti4_tensor::total_order_key(left.1).cmp(&ti4_tensor::total_order_key(right.1))
        })
    });
    option.columns.clear();
    option.values.clear();
    for (column, value) in pairs {
        if option.columns.last() == Some(&column) {
            let last = option.values.len() - 1;
            option.values[last] += value;
        } else {
            option.columns.push(column);
            option.values.push(value);
        }
    }
}

impl Constants {
    /// Derive every per-decision constant a minibatch will need, in decision order.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "behaviour quantities narrow to the f32 the model is built from"
    )]
    fn of(steps: &[Step], advantages: &[f64]) -> Self {
        Constants {
            behaviour: steps
                .iter()
                .map(|step| step.behaviour_log_prob as f32)
                .collect(),
            advantage: advantages.iter().map(|value| *value as f32).collect(),
            returns: steps.iter().map(|step| step.return_to_go as f32).collect(),
            options: steps.iter().map(|step| step.options.len()).collect(),
            heads: steps
                .iter()
                .map(|step| i64::try_from(step.head).unwrap_or(0))
                .collect(),
            rows: steps
                .iter()
                .map(|step| i64::try_from(step.row.index()).unwrap_or(0))
                .collect(),
        }
    }
}

impl Batch {
    /// Freeze one update's advantages.
    ///
    /// `A = returns − V_behaviour`, then normalised — both **once**, here. The values are `f64` and
    /// not tensors: there is no graph to detach because none was ever built, which is a stronger
    /// guarantee than calling `.detach()` and hoping every later path respects it.
    ///
    /// # Errors
    /// Returns an error when the batch or any step is empty, malformed, or non-finite.
    pub fn freeze(steps: Vec<Step>, mode: CriticMode) -> Result<Self, String> {
        if steps.is_empty() {
            return Err("a PPO batch is empty".to_owned());
        }
        for (index, step) in steps.iter().enumerate() {
            if step.head >= crate::heads().len()
                || step.options.len() < 2
                || step.chosen >= step.options.len()
                || !step.behaviour_log_prob.is_finite()
                || step.behaviour_log_prob > 0.0
                || !step.return_to_go.is_finite()
                // A temperature of zero divides the logits to infinity and a negative one inverts
                // the preference order. Neither is a distribution the bot could have sampled, so
                // the batch is refused here rather than producing `NaN` four epochs later.
                || !step.temperature.is_finite()
                || step.temperature <= 0.0
                || step.options.iter().any(|option| {
                    option.columns.is_empty()
                        || option.columns.len() != option.values.len()
                        || option.columns.iter().any(|column| *column < 0)
                        || option.values.iter().any(|value| !value.is_finite())
                })
            {
                return Err(format!("PPO step {index} is malformed or non-finite"));
            }
            match mode {
                CriticMode::Shared | CriticMode::Separate => {
                    let value = step.behaviour_value.ok_or_else(|| {
                        format!("PPO step {index} has no behavior value for {mode:?}")
                    })?;
                    let critic = step.critic.as_ref().ok_or_else(|| {
                        format!("PPO step {index} has no critic input for {mode:?}")
                    })?;
                    if !value.is_finite()
                        || critic.sparse().columns.is_empty()
                        || critic.sparse().columns.len() != critic.sparse().values.len()
                        || critic.sparse().columns.iter().any(|column| *column < 0)
                        || critic
                            .sparse()
                            .values
                            .iter()
                            .any(|value| !value.is_finite())
                    {
                        return Err(format!(
                            "PPO step {index} has malformed critic behavior data"
                        ));
                    }
                }
                CriticMode::BatchMean => {
                    if step.behaviour_value.is_some() || step.critic.is_some() {
                        return Err(format!(
                            "PPO step {index} stores unused critic data in batch-mean mode"
                        ));
                    }
                }
            }
        }
        // Canonicalise once, now that every step has been validated. Everything downstream reads
        // these vectors four times per update and never writes them.
        let mut steps = steps;
        for step in &mut steps {
            for option in &mut step.options {
                canonicalise(option);
            }
            if let Some(critic) = step.critic.as_mut() {
                canonicalise(critic.sparse_mut());
            }
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "decision counts are exact in f64"
        )]
        let return_mean =
            steps.iter().map(|step| step.return_to_go).sum::<f64>() / steps.len() as f64;
        // The loop above already refused a missing behaviour value in the critic modes, so this
        // could `expect`. It does not: an `expect` here would make the invariant a panic that a
        // later edit to the validation loop could reach, and the function already returns `Result`,
        // so carrying the failure costs nothing.
        let raw: Vec<f64> = steps
            .iter()
            .enumerate()
            .map(|(index, step)| match mode {
                CriticMode::Shared | CriticMode::Separate => step
                    .behaviour_value
                    .map(|value| step.return_to_go - value)
                    .ok_or_else(|| format!("PPO step {index} has no behaviour value for {mode:?}")),
                CriticMode::BatchMean => Ok(step.return_to_go - return_mean),
            })
            .collect::<Result<Vec<f64>, String>>()?;
        #[expect(
            clippy::cast_precision_loss,
            reason = "decision counts are exact in f64"
        )]
        let count = raw.len().max(1) as f64;
        let mean = raw.iter().sum::<f64>() / count;
        let variance = raw.iter().map(|a| (a - mean).powi(2)).sum::<f64>() / count;
        let deviation = variance.sqrt();
        let advantages: Vec<f64> = raw
            .iter()
            .map(|a| (a - mean) / (deviation + 1e-8))
            .collect();
        if advantages.iter().any(|advantage| !advantage.is_finite()) {
            return Err("normalised PPO advantages are non-finite".to_owned());
        }
        let constants = Constants::of(&steps, &advantages);

        Ok(Self {
            steps,
            advantages,
            mode,
            constants,
        })
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
/// One decision's scored quantities, from the per-decision path.
///
/// Retained for the tests only. `score_minibatch` replaced this path in production, and the
/// finite-difference gradient test now uses it as an **independent oracle**: the numeric side of
/// that comparison is computed here, one decision at a time, while the analytic side comes from the
/// batched implementation. Deleting it would leave the batched padding, gather, and softmax with no
/// reference to disagree with.
#[cfg(test)]
struct Scored {
    /// `log pi_current(chosen | s)`, with a graph.
    log_prob: Tensor,
    /// The decision's entropy, with a graph.
    entropy: Tensor,
}

/// Score one decision under the current weights.
///
/// The softmax is over exactly this decision's options — the segment — so no padding can enter it.
#[cfg(test)]
fn score(actor: &Actor, step: &Step) -> Result<Scored, String> {
    let head = crate::heads()
        .get(step.head)
        .ok_or_else(|| format!("head index {} is out of range", step.head))?;
    let logits = actor
        .logits(&step.options, head, step.row)
        .map_err(|error| format!("policy scoring failed: {error}"))?;
    // Divided by the temperature the behaviour was *drawn* at. `pi_new / pi_behaviour` is a ratio
    // only if both are the same function of the logits; scoring at 1.0 against a behaviour recorded
    // at 0.25 compares two different distributions, and PPO's clip then bounds a quantity that
    // means nothing. At 1.0 this divides by one and nothing changes, which is why the omission was
    // invisible until a run used another value.
    let log_probs = (logits / step.temperature).log_softmax(0, ti4_tensor::Kind::Float);
    let chosen = i64::try_from(step.chosen).map_err(|_| "chosen index does not fit i64")?;
    if chosen >= i64::try_from(step.options.len()).map_err(|_| "option count does not fit i64")? {
        return Err("chosen option is outside the legal set".to_owned());
    }
    let log_prob = log_probs.narrow(0, chosen, 1).squeeze();
    // H = −Σ p log p, over this decision's options only.
    let entropy = -(log_probs.exp() * &log_probs).sum(ti4_tensor::Kind::Float);
    Ok(Scored { log_prob, entropy })
}

/// Everything one scored minibatch hands back: a differentiable loss and undrained telemetry.
///
/// `readings` is `[4, decisions]` — surrogate, ratio, entropy, squared critic error — and stays on
/// the device. Draining it here would cost one host synchronisation per minibatch; `update`
/// concatenates a whole epoch's worth and reads them in one transfer instead.
struct ScoredMinibatch {
    loss: Tensor,
    readings: Tensor,
}

/// Per-epoch telemetry, reduced on the host from a single transfer.
struct Telemetry {
    actor_loss: f64,
    critic_loss: f64,
    kl: f64,
    clipped: usize,
    entropy: BTreeMap<String, (f64, usize)>,
}

/// Reduce a whole epoch's readings, in the order its minibatches were scored.
///
/// `readings` are the per-minibatch `[4, decisions]` blocks; `order` is the shuffled decision order
/// they were scored in, so concatenating along dimension 1 lines the columns up with `order`.
fn drain_epoch(
    readings: &[Tensor],
    order: &[usize],
    batch: &Batch,
    settings: Settings,
) -> Result<Telemetry, String> {
    let joined = Tensor::cat(readings, 1);
    let flat = ti4_tensor::to_vec(&joined.view([-1]))
        .map_err(|error| format!("reading PPO telemetry: {error}"))?;
    let count = order.len();
    if flat.len() != count * 4 {
        return Err(format!(
            "PPO telemetry returned {} values for {count} decisions",
            flat.len()
        ));
    }
    let (actor_values, rest) = flat.split_at(count);
    let (ratios, rest) = rest.split_at(count);
    let (entropies, criticals) = rest.split_at(count);

    let mut telemetry = Telemetry {
        actor_loss: 0.0,
        critic_loss: 0.0,
        kl: 0.0,
        clipped: 0,
        entropy: BTreeMap::new(),
    };
    for (position, index) in order.iter().enumerate() {
        telemetry.actor_loss += f64::from(actor_values[position]);
        telemetry.critic_loss += f64::from(criticals[position]);
        let ratio = f64::from(ratios[position]);
        telemetry.kl += ratio.ln().abs();
        if (ratio - 1.0).abs() > settings.clip_epsilon {
            telemetry.clipped += 1;
        }
        let entry = telemetry
            .entropy
            .entry(head_of(batch, *index).to_owned())
            .or_insert((0.0, 0));
        entry.0 += f64::from(entropies[position]);
        entry.1 += 1;
    }
    Ok(telemetry)
}

/// Score a whole minibatch in one forward pass.
///
/// This function is the reason a PPO update is not launch-bound. The version it replaced scored one
/// decision at a time — one `logits` call, one `log_softmax`, one critic pass, and five scalar reads
/// back to the host *per decision* — then summed 4,096 separate subgraphs before a single
/// `backward`. On CUDA that is thousands of kernel launches and thousands of synchronising stalls
/// per optimizer step, which is why the GPU sat at 40% and the device bought only 1.36x over CPU.
///
/// Decisions have different option counts, so the ragged logits are scattered into a padded
/// `[decisions, widest]` rectangle whose padding is `-inf` before the softmax and zeroed after it.
/// The zeroing is not cosmetic: `exp(-inf) * -inf` is `0 * -inf`, which is `NaN`, and one `NaN`
/// entering the sum poisons the whole minibatch's gradient. `distill::batch_cross_entropy` meets
/// the same hazard the same way.
#[expect(
    clippy::cast_possible_truncation,
    reason = "behaviour quantities narrow to the f32 the model is built from"
)]
#[expect(
    clippy::too_many_lines,
    reason = "one forward pass over a minibatch; splitting it would hide the padding contract \
              that its correctness depends on"
)]
fn score_minibatch(
    actor: &Actor,
    batch: &Batch,
    minibatch: &[usize],
    critic_mode: CriticMode,
    settings: Settings,
) -> Result<ScoredMinibatch, String> {
    let count = minibatch.len();
    if count == 0 {
        return Err("a PPO minibatch is empty".to_owned());
    }

    // ---- flatten every decision's options into one gather ----
    // Sized exactly before filling: these are the largest host allocations in the loop, and their
    // final length is known from the frozen batch's per-decision option counts.
    let expansion: usize = minibatch
        .iter()
        .map(|index| batch.constants.options[*index])
        .sum();
    let mut parts: Vec<(&[i64], &[f32])> = Vec::with_capacity(expansion);
    let mut heads: Vec<i64> = Vec::with_capacity(expansion);
    let mut rows: Vec<i64> = Vec::with_capacity(expansion);
    let mut widest = 0usize;
    for index in minibatch {
        let step = &batch.steps[*index];
        let head = batch.constants.heads[*index];
        let row = batch.constants.rows[*index];
        if step.chosen >= step.options.len() {
            return Err("chosen option is outside the legal set".to_owned());
        }
        widest = widest.max(step.options.len());
        for option in &step.options {
            parts.push((option.columns.as_slice(), option.values.as_slice()));
            heads.push(head);
            rows.push(row);
        }
    }
    let widest = i64::try_from(widest).map_err(|_| "option count does not fit i64")?;
    let rectangle = i64::try_from(count).map_err(|_| "minibatch size does not fit i64")?;

    let flat = actor
        .logits_mixed_parts(&parts, &heads, &rows)
        .map_err(|error| format!("policy scoring failed: {error}"))?;

    // ---- scatter the ragged logits into a padded rectangle ----
    //
    // The rectangle is `[decisions, widest]`, so most of it is padding: option counts average
    // around six and the widest decision in a minibatch sets the width. The earlier version built
    // two host vectors of that full size — an `i64` gather index and a `bool` padding mask — and
    // uploaded both. What is uploaded now is one slot index per *real* option, which is the ragged
    // total rather than the rectangle, and both the `-inf` ground and the mask are made on device.
    //
    // The mask is derived from the slot list rather than from `padded.eq(-inf)`. Testing the values
    // would be shorter and would also swallow a genuine `-inf` logit in a real slot, turning a
    // malformed distribution into a silently zeroed entropy term. Deriving it from the layout keeps
    // "this cell is padding" and "this cell is bad" distinguishable, so the non-finite refusals
    // downstream still fire.
    let mut slots: Vec<i64> = Vec::with_capacity(expansion);
    let mut chosen: Vec<i64> = Vec::with_capacity(count);
    let mut temperatures: Vec<f64> = Vec::with_capacity(count);
    let mut offset = 0i64;
    for (position, index) in minibatch.iter().enumerate() {
        let step = &batch.steps[*index];
        let options = i64::try_from(step.options.len()).map_err(|_| "option count overflow")?;
        let row_start =
            i64::try_from(position).map_err(|_| "minibatch position overflow")? * widest;
        for slot in 0..options {
            slots.push(row_start + slot);
        }
        chosen.push(i64::try_from(step.chosen).map_err(|_| "chosen index overflow")?);
        temperatures.push(step.temperature);
        offset += options;
    }
    debug_assert_eq!(
        offset,
        i64::try_from(expansion).unwrap_or(0),
        "the slot walk and the gather disagree about how many options there are"
    );

    let device = flat.device();
    let cells = rectangle * widest;
    let slot_index = Tensor::from_slice(&slots).to_device(device);
    let padded = Tensor::full(
        [cells],
        f64::NEG_INFINITY,
        (ti4_tensor::Kind::Float, device),
    )
    .index_copy(0, &slot_index, &flat)
    .view([rectangle, widest]);
    let mask = Tensor::ones([cells], (ti4_tensor::Kind::Bool, device))
        .index_fill(0, &slot_index, 0)
        .view([rectangle, widest]);
    // Each row is one decision, and each decision carries the temperature its behaviour was drawn
    // at, so the division is per row rather than global -- a batch may mix them. Padding is
    // `-inf`, and `-inf / t` is still `-inf` for any positive `t`, so the mask below is unaffected.
    let row_temperatures = Tensor::from_slice(&temperatures)
        .to_kind(ti4_tensor::Kind::Float)
        .to_device(device)
        .view([rectangle, 1]);
    let log_probs = (padded / row_temperatures).log_softmax(1, ti4_tensor::Kind::Float);

    // `H = -sum p log p` over each decision's own options. `p` is already zero in the padding
    // (`exp(-inf)`), but `log p` is `-inf` there, so the product is `NaN` until the log is zeroed.
    let safe_log = log_probs.masked_fill(&mask, 0.0);
    let entropy = -(log_probs.exp() * &safe_log).sum_dim_intlist(
        [1i64].as_slice(),
        false,
        ti4_tensor::Kind::Float,
    );

    let chosen_index = Tensor::from_slice(&chosen)
        .to_device(device)
        .view([rectangle, 1]);
    let log_prob = log_probs.gather(1, &chosen_index, false).squeeze_dim(1);

    // ---- the clipped surrogate, per decision, as one vector ----
    // These narrow from f64 to f32 on purpose: every parameter in the model is f32, so a wider
    // constant would only be rounded on first contact with the graph.
    // Advantages and behaviour log-probabilities enter as constants, which is the detach §6.3
    // requires expressed as data rather than as a call.
    let behaviour: Vec<f32> = minibatch
        .iter()
        .map(|index| batch.constants.behaviour[*index])
        .collect();
    let advantage: Vec<f32> = minibatch
        .iter()
        .map(|index| batch.constants.advantage[*index])
        .collect();
    let behaviour = Tensor::from_slice(&behaviour).to_device(device);
    let advantage = Tensor::from_slice(&advantage).to_device(device);

    let ratio = (&log_prob - &behaviour).exp();
    let clipped_ratio = ratio.clamp(1.0 - settings.clip_epsilon, 1.0 + settings.clip_epsilon);
    let actor_term = -(&ratio * &advantage).minimum(&(clipped_ratio * &advantage));

    let coefficients: Vec<f32> = minibatch
        .iter()
        .map(|index| settings.entropy_for(head_of(batch, *index)) as f32)
        .collect();
    let coefficients = Tensor::from_slice(&coefficients).to_device(device);
    let mut term = &actor_term - &entropy * &coefficients;

    // ---- the critic's own path to the trunk, per §6.3, and only in the modes that have one ----
    let critic_term = if matches!(critic_mode, CriticMode::BatchMean) {
        None
    } else {
        let mut critics: Vec<&crate::CriticInput> = Vec::with_capacity(count);
        let mut critic_rows: Vec<i64> = Vec::with_capacity(count);
        for index in minibatch {
            let step = &batch.steps[*index];
            critics.push(
                step.critic
                    .as_ref()
                    .ok_or_else(|| "critic mode has no critic input".to_owned())?,
            );
            critic_rows.push(batch.constants.rows[*index]);
        }
        let value = actor
            .value_batch(&critics, &critic_rows)
            .map_err(|error| format!("critic scoring failed: {error}"))?;
        let returns: Vec<f32> = minibatch
            .iter()
            .map(|index| batch.constants.returns[*index])
            .collect();
        let returns = Tensor::from_slice(&returns).to_device(device);
        let squared = (value - returns).square();
        term += &squared * settings.value_coefficient;
        Some(squared)
    };

    let loss = term.mean(ti4_tensor::Kind::Float);

    // ---- telemetry stays on the device until the epoch ends ----
    let zeros = Tensor::zeros([rectangle], (ti4_tensor::Kind::Float, device));
    let readings = Tensor::stack(
        &[
            actor_term.detach(),
            ratio.detach(),
            entropy.detach(),
            critic_term.map_or(zeros, |squared| squared.detach()),
        ],
        0,
    );

    Ok(ScoredMinibatch { loss, readings })
}

/// The schema head a step belongs to, defaulting the way the per-decision loop did.
fn head_of(batch: &Batch, index: usize) -> &'static str {
    crate::heads()
        .get(batch.steps[index].head)
        .copied()
        .unwrap_or("other")
}

/// The clipped surrogate for one decision.
///
/// `−min( r·A , clip(r, 1−e, 1+e)·A )`, with `A` arriving as a plain `f64` — a constant in the
/// graph, which is the detach §6.3 requires expressed as a type rather than a call.
#[cfg(test)]
fn surrogate(log_prob: &Tensor, behaviour_log_prob: f64, advantage: f64, epsilon: f64) -> Tensor {
    let ratio = (log_prob - behaviour_log_prob).exp();
    let clipped = ratio.clamp(1.0 - epsilon, 1.0 + epsilon);
    let unclipped_term = &ratio * advantage;
    let clipped_term = clipped * advantage;
    -unclipped_term.minimum(&clipped_term)
}

/// Stateful Adam over the exact parameter set selected by the critic mode.
pub struct Adam {
    inner: crate::distill::Adam,
    mode: CriticMode,
    settings: Settings,
}

impl Adam {
    /// Open the selected trainables and create zeroed moments.
    ///
    /// # Errors
    /// Returns an error for invalid settings or a critic mode incompatible with the actor.
    pub fn new(actor: &mut Actor, mode: CriticMode, settings: Settings) -> Result<Self, String> {
        validate_settings(settings)?;
        match mode {
            CriticMode::Shared if actor.separate_critic().is_some() => {
                return Err("shared mode was given an actor with a separate critic".to_owned());
            }
            CriticMode::Separate if actor.separate_critic().is_none() => {
                return Err("separate mode has no separate critic".to_owned());
            }
            CriticMode::BatchMean if actor.separate_critic().is_some() => {
                return Err("batch-mean mode must not carry separate critic tensors".to_owned());
            }
            _ => {}
        }
        actor.open_main_for_training(matches!(mode, CriticMode::Shared));
        if matches!(mode, CriticMode::Separate) {
            actor
                .separate_critic_mut()
                .ok_or_else(|| "separate critic disappeared while opening PPO".to_owned())?
                .open_for_training();
        }
        let parameters = parameters(actor, mode)?;
        let optimizer_settings = crate::distill::Settings {
            learning_rate: settings.learning_rate,
            beta1: settings.beta1,
            beta2: settings.beta2,
            eps: settings.eps,
            weight_decay: settings.weight_decay,
            clip: settings.grad_clip,
            ..crate::distill::Settings::default()
        };
        Ok(Self {
            inner: crate::distill::Adam::new(optimizer_settings, &parameters),
            mode,
            settings,
        })
    }

    fn step(&mut self, actor: &Actor) -> Result<(), String> {
        let mut parameters = parameters(actor, self.mode)?;
        self.inner.step(&mut parameters, 1.0)
    }

    /// Exact moments and step-counter fingerprint.
    ///
    /// # Errors
    /// Returns an error when an optimizer tensor cannot be read.
    pub fn state_fingerprint(&self) -> Result<Vec<u32>, String> {
        self.inner.state_fingerprint()
    }

    /// Move Adam's moments to the optimizer device without resetting its step counter.
    pub fn move_to(&mut self, device: ti4_tensor::Device) {
        self.inner.move_to(device);
    }

    /// How many minibatch steps have advanced the bias-correction cursor.
    #[must_use]
    pub const fn steps(&self) -> i64 {
        self.inner.steps()
    }
}

fn validate_settings(settings: Settings) -> Result<(), String> {
    let positive = [
        settings.learning_rate,
        settings.beta1,
        settings.beta2,
        settings.eps,
        settings.clip_epsilon,
        settings.grad_clip,
        settings.value_coefficient,
    ];
    if settings.epochs == 0
        || settings.minibatch == 0
        || positive
            .iter()
            .any(|value| !value.is_finite() || *value <= 0.0)
        || !settings.weight_decay.is_finite()
        || settings.weight_decay < 0.0
        || !settings.entropy.is_finite()
        || settings.entropy < 0.0
        || !settings.strategy_entropy.is_finite()
        || settings.strategy_entropy < 0.0
        || settings.beta1 >= 1.0
        || settings.beta2 >= 1.0
        || settings.clip_epsilon >= 1.0
    {
        return Err("PPO settings are empty, non-finite, or outside their valid ranges".to_owned());
    }
    Ok(())
}

fn parameters(actor: &Actor, mode: CriticMode) -> Result<Vec<Tensor>, String> {
    let mut parameters = actor.main_parameters(matches!(mode, CriticMode::Shared));
    if matches!(mode, CriticMode::Separate) {
        parameters.extend(
            actor
                .separate_critic()
                .ok_or_else(|| "separate PPO mode has no separate critic".to_owned())?
                .parameters(),
        );
    }
    Ok(parameters)
}

/// Exact trainable-parameter fingerprint for deterministic-update tests and evidence.
///
/// # Errors
/// Returns an error for an invalid critic mode or when a parameter tensor cannot be read.
pub fn parameter_fingerprint(actor: &Actor, mode: CriticMode) -> Result<Vec<u32>, String> {
    let mut bits = Vec::new();
    for tensor in parameters(actor, mode)? {
        bits.extend(
            ti4_tensor::to_vec(&tensor)
                .map_err(|error| format!("reading PPO parameter: {error}"))?
                .iter()
                .map(|value| value.to_bits()),
        );
    }
    Ok(bits)
}

/// One PPO update over a frozen batch.
///
/// Returns one [`EpochStats`] per epoch. The advantages are the batch's, unchanged, in every epoch;
/// `shuffle` only changes the order decisions are visited in.
///
/// # Errors
/// Returns an error for an empty or malformed batch, incompatible settings or critic mode, or any
/// failed model or optimizer operation. Validation occurs before the first mutation.
pub fn update(
    actor: &mut Actor,
    batch: &Batch,
    critic_mode: CriticMode,
    settings: Settings,
    shuffle_seed: u64,
    optimizer: &mut Adam,
) -> Result<Vec<EpochStats>, String> {
    validate_settings(settings)?;
    if batch.is_empty() {
        return Err("a PPO update batch is empty".to_owned());
    }
    if batch.mode != critic_mode {
        return Err("PPO batch critic mode does not match the update".to_owned());
    }
    if optimizer.mode != critic_mode {
        return Err("PPO optimizer critic mode does not match the update".to_owned());
    }
    // Only the settings the optimizer is *made of*. Entropy coefficients are loss terms: they
    // change the gradient's value, never Adam's moments, its step cursor or its clip. Comparing
    // whole `Settings` refused an entropy schedule -- which is the one thing a long run should be
    // able to change, since a bonus paid for keeping mass off the move the policy has learned is
    // best is a floor on its error rate.
    if !optimizer.settings.optimizer_equivalent(settings) {
        return Err("PPO optimizer settings do not match the update".to_owned());
    }
    // Every trainable parameter must still be a leaf that requires a gradient.
    //
    // `backward` populates `.grad` on the leaf a tensor was derived from. If the actor's parameters
    // have been replaced by non-leaf views -- which is exactly what `Tensor::to_device` returns for
    // a tensor that requires a gradient -- the gradients land on the tensors left behind, Adam
    // receives none, and the update applies nothing while every loss in the telemetry looks
    // healthy. The symptom surfaces three layers down as "0 defined gradients"; this names the
    // cause instead.
    for (index, parameter) in parameters(actor, critic_mode)?.iter().enumerate() {
        if !parameter.requires_grad() {
            return Err(format!(
                "PPO parameter {index} does not require a gradient, so this update would train                  nothing"
            ));
        }
        if !parameter.is_leaf() {
            return Err(format!(
                "PPO parameter {index} is not a leaf tensor, so backward would populate the                  gradient of the tensor it was derived from and this update would apply nothing;                  the usual cause is moving the actor to another device after Adam::new opened it"
            ));
        }
    }
    if batch.steps.iter().any(|step| {
        step.options
            .iter()
            .flat_map(|option| &option.columns)
            .chain(
                step.critic
                    .as_ref()
                    .into_iter()
                    .flat_map(|critic| &critic.sparse().columns),
            )
            .any(|column| *column >= actor.capacity())
    }) {
        return Err("a PPO feature column is outside the actor capacity".to_owned());
    }
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

        // One host transfer per *epoch*, not per minibatch and emphatically not per decision. The
        // per-decision version of this loop read five scalars per decision out of the graph; each
        // one drains the CUDA pipeline, and 4,096 decisions x 5 reads is what left the GPU at 40%.
        let mut readings: Vec<Tensor> = Vec::new();
        for minibatch in order.chunks(settings.minibatch) {
            let scored = score_minibatch(actor, batch, minibatch, critic_mode, settings)?;
            scored.loss.backward();
            optimizer.step(actor)?;
            readings.push(scored.readings);
            seen += minibatch.len();
        }

        let telemetry = drain_epoch(&readings, &order, batch, settings)?;
        stats.actor_loss += telemetry.actor_loss;
        stats.critic_loss += telemetry.critic_loss;
        stats.kl += telemetry.kl;
        clipped += telemetry.clipped;
        for (head, (sum, count)) in telemetry.entropy {
            let entry = entropy_sums.entry(head).or_insert((0.0, 0));
            entry.0 += sum;
            entry.1 += count;
        }

        if seen != batch.len() {
            return Err(format!("PPO evaluated {seen} of {} decisions", batch.len()));
        }

        #[expect(
            clippy::cast_precision_loss,
            reason = "decision counts are exact in f64"
        )]
        let denominator = seen as f64;
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
    Ok(out)
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

    /// The same decision as [`step`], recorded the way `batch_mean` mode records one.
    ///
    /// `Batch::freeze` **refuses** a stored behaviour value or critic input in batch-mean mode
    /// rather than ignoring it, so a fixture that carried them would be testing a configuration the
    /// type system does not admit.
    fn batch_mean_step(chosen: usize, options: usize, ret: f64) -> Step {
        let mut built = step(chosen, options, ret, 0.0);
        built.behaviour_value = None;
        built.critic = None;
        built
    }

    /// The importance ratio is 1 under unchanged weights, at **any** sampling temperature.
    ///
    /// This is the invariant PPO rests on: `pi_new / pi_behaviour` compares the same function
    /// before and after an update, so with no update it is exactly 1. It held at temperature 1.0
    /// and nowhere else, because the optimiser scored `softmax(s)` while the bot had recorded
    /// `softmax(s / T)`. Those are the same distribution only when `T == 1`.
    ///
    /// The consequence was not subtle. A run at 0.25 optimised a ratio between two different
    /// distributions, PPO's clip bounded a quantity that meant nothing, and a policy clearing 91%
    /// of openings fell to 2.6% over 650 updates while every health statistic looked normal — they
    /// looked normal because they were computed from the same broken ratio.
    ///
    /// The fixture must be `distinguishable_step`. The first version of this test built its options
    /// from `step`, whose options all score alike; a uniform softmax is temperature-invariant, so
    /// the test passed against the bug it was written to catch. Probed: reverting the division in
    /// `score` gives a log ratio of 4.41 at 0.25, and this test fails while the batched one below
    /// still passes — the two cover separate paths.
    #[test]
    fn the_behaviour_ratio_is_one_under_unchanged_weights_at_any_temperature() {
        let actor = trainable_actor();
        for temperature in [1.0, 0.25, 2.5] {
            let mut recorded = distinguishable_step(1, 4);
            recorded.temperature = temperature;

            // Record the behaviour exactly as `MlpBot` does: one `probabilities` call at the
            // sampling temperature, which is both what it acts on and what it stores.
            let head = crate::heads().first().copied().unwrap_or("other");
            let behaviour = actor
                .probabilities(&recorded.options, head, recorded.row, temperature)
                .expect("probabilities");
            recorded.behaviour_log_prob = behaviour[recorded.chosen].ln();

            let spread = behaviour.iter().copied().fold(f64::NEG_INFINITY, f64::max)
                - behaviour.iter().copied().fold(f64::INFINITY, f64::min);
            assert!(
                spread > 1e-3,
                "temperature {temperature}: the fixture must be non-uniform, or this test cannot\n                 see a temperature mismatch at all (spread {spread})"
            );

            // Score it with the optimiser, weights untouched.
            let scored = score(&actor, &recorded).expect("scored");
            let log_ratio =
                f64::try_from(scored.log_prob).expect("scalar") - recorded.behaviour_log_prob;
            assert!(
                log_ratio.abs() < 1e-6,
                "temperature {temperature}: log ratio {log_ratio}, not zero: the optimiser and\n                 the bot are scoring different distributions"
            );
        }
    }

    /// The same invariant through the **batched** path, which is the one production runs.
    ///
    /// `score` is `#[cfg(test)]`; `score_minibatch` is what `update` calls, and it reaches the
    /// temperature through a per-row `[decisions, 1]` tensor rather than a scalar divide. That is a
    /// second place to get it wrong — a transposed or broadcast-mismatched row vector would divide
    /// the wrong decisions by the wrong values and still produce finite numbers.
    ///
    /// `EpochStats::kl` is mean `|log r|`, so under unchanged weights the first epoch's reading is
    /// the invariant made observable. The batch fits in one minibatch, and a minibatch is scored
    /// before it is stepped, so the first epoch's reading is taken against the weights the
    /// behaviour was recorded under. (A zero learning rate would say this more directly, but
    /// `validate_settings` refuses one.)
    ///
    /// Probed: reverting the batched division gives kl 3.40 at 0.25 against a bound of 1e-4, and the
    /// single-decision test above still passes.
    #[test]
    fn the_batched_path_honours_the_recorded_temperature_too() {
        let actor_for_probabilities = trainable_actor();
        let head = crate::heads().first().copied().unwrap_or("other");

        for temperature in [1.0, 0.25, 2.5] {
            let steps: Vec<Step> = [(1usize, 4usize), (0, 3), (2, 5), (1, 2)]
                .into_iter()
                .map(|(chosen, options)| {
                    let mut recorded = distinguishable_step(chosen, options);
                    recorded.temperature = temperature;
                    recorded.behaviour_value = None;
                    recorded.critic = None;
                    let behaviour = actor_for_probabilities
                        .probabilities(&recorded.options, head, recorded.row, temperature)
                        .expect("probabilities");
                    recorded.behaviour_log_prob = behaviour[recorded.chosen].ln();
                    recorded
                })
                .collect();
            let batch = Batch::freeze(steps, CriticMode::BatchMean).expect("valid batch");

            let mut actor = trainable_actor();
            let settings = Settings {
                epochs: 1,
                minibatch: 64,
                ..Settings::default()
            };
            let mut optimizer =
                Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
            let stats = update(
                &mut actor,
                &batch,
                CriticMode::BatchMean,
                settings,
                7,
                &mut optimizer,
            )
            .expect("update");

            let kl = stats.first().expect("one epoch").kl;
            assert!(
                kl < 1e-4,
                "temperature {temperature}: batched mean |log r| was {kl}, not zero: the optimiser\n                 is scoring a different distribution from the one the bot sampled"
            );
        }
    }

    /// A temperature the softmax cannot be taken at is refused when the batch is frozen.
    ///
    /// Zero divides every logit to an infinity and negative values invert the preference order.
    /// Both produce a distribution no bot could have sampled, so they are caught at the boundary
    /// rather than surfacing as `NaN` gradients partway through the fourth epoch.
    #[test]
    fn a_temperature_that_is_not_a_temperature_is_refused() {
        for bad in [0.0, -0.25, f64::NAN, f64::INFINITY] {
            let mut step = batch_mean_step(0, 2, 1.0);
            step.temperature = bad;
            assert!(
                Batch::freeze(vec![step], CriticMode::BatchMean).is_err(),
                "temperature {bad} was accepted"
            );
        }
        let mut fine = batch_mean_step(0, 2, 1.0);
        fine.temperature = 0.25;
        assert!(
            Batch::freeze(vec![fine], CriticMode::BatchMean).is_ok(),
            "0.25 is a temperature the bot really samples at"
        );
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
            temperature: 1.0,
            behaviour_value: Some(value),
            return_to_go: ret,
            critic: Some(CriticInput::from_sparse(SparseOption {
                columns: vec![7],
                values: vec![1.0],
            })),
        }
    }

    #[test]
    fn advantages_are_frozen_and_normalised_once() {
        let batch = Batch::freeze(
            vec![
                step(0, 2, 3.0, 1.0),
                step(1, 3, 1.0, 1.0),
                step(0, 2, 2.0, 4.0),
            ],
            CriticMode::Shared,
        )
        .expect("valid batch");
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
        let batch = Batch::freeze(
            vec![
                step(0, 2, 2.0, 2.0),
                step(0, 2, 5.0, 5.0),
                step(0, 2, 1.0, 1.0),
            ],
            CriticMode::Shared,
        )
        .expect("valid batch");
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

    /// A step whose options the model can actually tell apart.
    ///
    /// `step` gives every option the same feature value, so their trunk outputs are identical, the
    /// policy is exactly uniform, and the surrogate's gradient at ratio 1 is identically zero —
    /// `d log p_chosen / dw = z_chosen − Σ p_i z_i = 0` when every `z_i` is the same vector. That is
    /// the right fixture for the entropy test, which wants exactly `ln n`, and a useless one for a
    /// gradient check.
    fn distinguishable_step(chosen: usize, options: usize) -> Step {
        Step {
            row: FactionRow::of("sol").expect("roster"),
            head: 0,
            options: (0..options)
                .map(|index| SparseOption {
                    columns: vec![
                        i64::try_from(index).unwrap_or(0) + 1,
                        i64::try_from(index).unwrap_or(0) + 40,
                    ],
                    // `f32::from(u8)` is lossless, so no cast lint and no precision question:
                    // option counts here are single digits.
                    values: vec![
                        1.0 + f32::from(u8::try_from(index).unwrap_or(0)) * 0.7,
                        0.3 - f32::from(u8::try_from(index).unwrap_or(0)) * 0.2,
                    ],
                })
                .collect(),
            chosen,
            // Deliberately not the current policy's log-prob, so the ratio is away from 1 and the
            // clip's branch is exercised rather than sitting on its boundary.
            behaviour_log_prob: -1.4,
            temperature: 1.0,
            behaviour_value: Some(1.0),
            return_to_go: 4.0,
            critic: Some(CriticInput::from_sparse(SparseOption {
                columns: vec![7],
                values: vec![1.0],
            })),
        }
    }

    /// The scalar PPO objective for one step, with no graph — the thing a finite difference
    /// perturbs.
    fn objective(actor: &Actor, step: &Step, advantage: f64, settings: Settings) -> f64 {
        tch::no_grad(|| {
            let scored = score(actor, step).expect("scored");
            let actor_term = surrogate(
                &scored.log_prob,
                step.behaviour_log_prob,
                advantage,
                settings.clip_epsilon,
            );
            let head = crate::heads().get(step.head).copied().unwrap_or("other");
            (actor_term - &scored.entropy * settings.entropy_for(head)).double_value(&[])
        })
    }

    #[test]
    fn the_analytic_gradient_matches_a_finite_difference() {
        // What this does and does not cover, established by probing it.
        //
        // It catches a term present in the objective but missing from the differentiated loss:
        // removing the entropy term from the analytic side alone gives analytic 0.0345 against
        // numeric 0.0371, and the check fails.
        //
        // It does **not** catch a globally flipped surrogate sign, because the finite difference is
        // taken through the same objective — both sides flip together and still agree. A gradient
        // check verifies that the gradient matches the loss, never that the loss is the right loss.
        // `the_surrogate_clips_and_the_clip_binds_on_the_right_side` is what covers that, and the
        // two are not interchangeable.
        let settings = Settings {
            entropy: 0.01,
            ..Settings::default()
        };
        let mut actor = trainable_actor();
        let step = distinguishable_step(0, 3);
        let advantage = 0.75;

        // Analytic.
        let scored = score(&actor, &step).expect("scored");
        let actor_term = surrogate(
            &scored.log_prob,
            step.behaviour_log_prob,
            advantage,
            settings.clip_epsilon,
        );
        let head = crate::heads().first().copied().unwrap_or("other");
        let loss = actor_term - &scored.entropy * settings.entropy_for(head);
        loss.backward();
        let grad = actor.shared_readout().grad();
        assert!(grad.defined(), "no gradient reached the shared readout");
        let analytic = ti4_tensor::to_vec(&grad).expect("vec");

        // Finite difference on a few coordinates of the shared readout. Central difference, so the
        // error is O(h^2) rather than O(h); h = 1e-3 in f32 is the usual compromise between
        // truncation and cancellation.
        let h = 1e-3f64;
        let width = usize::try_from(actor.width()).expect("fits");
        let mut checked = 0usize;
        for coordinate in [0usize, 5, width + 3] {
            let index = i64::try_from(coordinate).expect("fits");
            let base = tch::no_grad(|| {
                ti4_tensor::to_vec(actor.shared_readout())
                    .expect("vec")
                    .clone()
            });
            let mut bump = |delta: f64| -> f64 {
                let mut values = base.clone();
                #[expect(clippy::cast_possible_truncation, reason = "weights are f32")]
                {
                    values[coordinate] += delta as f32;
                }
                let heads = i64::try_from(crate::heads().len()).expect("fits");
                tch::no_grad(|| {
                    *actor.shared_readout_mut() =
                        Tensor::from_slice(&values).view([heads, actor.width()]);
                });
                objective(&actor, &step, advantage, settings)
            };
            let up = bump(h);
            let down = bump(-h);
            let _ = index;
            let numeric = (up - down) / (2.0 * h);
            let exact = f64::from(analytic[coordinate]);

            // Restore before the next coordinate.
            let heads = i64::try_from(crate::heads().len()).expect("fits");
            tch::no_grad(|| {
                *actor.shared_readout_mut() =
                    Tensor::from_slice(&base).view([heads, actor.width()]);
            });

            // Only coordinates with real signal are informative; a zero gradient matches a zero
            // difference trivially.
            if exact.abs() > 1e-4 {
                let relative = (numeric - exact).abs() / exact.abs().max(1e-6);
                assert!(
                    relative < 5e-2,
                    "coordinate {coordinate}: analytic {exact}, numeric {numeric}"
                );
                checked += 1;
            }
        }
        assert!(
            checked > 0,
            "every probed coordinate had a zero gradient, so nothing was compared"
        );
    }

    #[test]
    fn the_same_update_twice_produces_the_same_numbers() {
        // §6.3's deterministic reduction. Two updates from the same start, same batch and same
        // shuffle seed must agree exactly — not nearly.
        let batch = Batch::freeze(
            vec![
                batch_mean_step(0, 2, 3.0),
                batch_mean_step(1, 3, 1.0),
                batch_mean_step(0, 4, 5.0),
                batch_mean_step(2, 3, 2.0),
            ],
            CriticMode::BatchMean,
        )
        .expect("valid batch");
        let run = || {
            let mut actor = trainable_actor();
            let settings = Settings {
                epochs: 2,
                minibatch: 2,
                ..Settings::default()
            };
            let mut optimizer =
                Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
            let before = parameter_fingerprint(&actor, CriticMode::BatchMean).expect("parameters");
            let stats = update(
                &mut actor,
                &batch,
                CriticMode::BatchMean,
                settings,
                7,
                &mut optimizer,
            )
            .expect("update");
            let after = parameter_fingerprint(&actor, CriticMode::BatchMean).expect("parameters");
            let state = optimizer.state_fingerprint().expect("Adam state");
            assert_ne!(after, before, "the PPO update moved no parameter");
            assert!(
                state.iter().any(|bits| *bits != 0),
                "Adam state stayed zero"
            );
            (stats, after, state)
        };
        let first = run();
        let second = run();
        assert_eq!(first.0.len(), second.0.len());
        for (a, b) in first.0.iter().zip(&second.0) {
            assert!(
                (a.actor_loss - b.actor_loss).abs() < f64::EPSILON,
                "actor loss differed: {} against {}",
                a.actor_loss,
                b.actor_loss
            );
            assert!((a.kl - b.kl).abs() < f64::EPSILON, "kl differed");
            assert_eq!(a.entropy, b.entropy, "per-head entropy differed");
        }
        // Non-vacuity: the run must actually have produced numbers.
        assert!(
            first.0.iter().any(|stats| stats.actor_loss != 0.0),
            "every actor loss was zero, so the comparison proves nothing"
        );
        assert_eq!(first.1, second.1, "updated weights differed bit-for-bit");
        assert_eq!(
            first.2, second.2,
            "Adam moments/cursor differed bit-for-bit"
        );
    }

    #[test]
    fn an_actor_whose_parameters_stopped_being_leaves_is_refused() {
        // The defect this guards against cost a CUDA run: `Adam::new` opens the actor's parameters
        // as leaves, a later `to_device` replaces them with non-leaf views, and every gradient then
        // lands on the tensors left behind. On CPU `to_device` is a no-op, so the only way to
        // reproduce it here is to derive a parameter explicitly -- which is the same thing a device
        // move does.
        let batch = Batch::freeze(
            vec![batch_mean_step(0, 2, 3.0), batch_mean_step(1, 3, 1.0)],
            CriticMode::BatchMean,
        )
        .expect("valid batch");
        let settings = Settings {
            epochs: 1,
            minibatch: 2,
            ..Settings::default()
        };
        let mut actor = trainable_actor();
        let mut optimizer =
            Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");

        // Control: the update works before the parameters stop being leaves, so the refusal below
        // is about leaf-ness and not about the fixture.
        update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            settings,
            7,
            &mut optimizer,
        )
        .expect("the actor trains while its parameters are leaves");

        let derived = actor.input() + 0.0;
        assert!(!derived.is_leaf(), "the fixture did not produce a non-leaf");
        *actor.input_mut() = derived;

        let refusal = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            settings,
            7,
            &mut optimizer,
        )
        .expect_err("a non-leaf parameter was accepted and would have trained nothing");
        assert!(
            refusal.contains("is not a leaf tensor"),
            "refused for the wrong reason: {refusal}"
        );
    }

    #[test]
    fn entropy_may_be_annealed_under_a_retained_optimizer_but_the_learning_rate_may_not() {
        // The distinction the guard now draws. Entropy is a loss term; the learning rate is what
        // Adam's moments were accumulated against, and changing it under a retained optimizer
        // would silently reinterpret every one of them.
        let batch = Batch::freeze(
            vec![batch_mean_step(0, 2, 3.0), batch_mean_step(1, 3, 1.0)],
            CriticMode::BatchMean,
        )
        .expect("valid batch");
        let settings = Settings {
            epochs: 1,
            minibatch: 2,
            ..Settings::default()
        };
        let mut actor = trainable_actor();
        let mut optimizer =
            Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");

        let annealed = Settings {
            entropy: settings.entropy / 10.0,
            movement_entropy: 0.0,
            ..settings
        };
        update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            annealed,
            7,
            &mut optimizer,
        )
        .expect("an annealed entropy is accepted");

        let faster = Settings {
            learning_rate: settings.learning_rate * 2.0,
            ..settings
        };
        let refusal = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            faster,
            7,
            &mut optimizer,
        )
        .expect_err("a changed learning rate was accepted under a retained optimizer");
        assert!(
            refusal.contains("optimizer settings do not match"),
            "refused for the wrong reason: {refusal}"
        );
    }

    #[test]
    fn adam_carries_its_moments_across_updates_instead_of_restarting() {
        // F-M10-034-D3. The driver used to construct `Adam::new` inside the update loop, so every
        // update after the first began from zeroed moments and `t = 1`. Adam's bias correction
        // divides by `1 - beta^t`, so a restarted optimiser takes its largest, least-Adam-like step
        // every time — and the loss telemetry looks perfectly healthy while it happens.
        //
        // The test is a falsification: a retained optimiser and a restarted one are run over the
        // same two batches from the same weights, and their results must differ.
        let first = Batch::freeze(
            vec![batch_mean_step(0, 2, 3.0), batch_mean_step(1, 3, 1.0)],
            CriticMode::BatchMean,
        )
        .expect("valid batch");
        let second = Batch::freeze(
            vec![batch_mean_step(1, 3, 4.0), batch_mean_step(0, 4, 2.0)],
            CriticMode::BatchMean,
        )
        .expect("valid batch");
        let settings = Settings {
            epochs: 1,
            minibatch: 2,
            ..Settings::default()
        };

        let mut actor = trainable_actor();
        let mut retained =
            Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
        for batch in [&first, &second] {
            update(
                &mut actor,
                batch,
                CriticMode::BatchMean,
                settings,
                7,
                &mut retained,
            )
            .expect("update");
        }
        let carried = parameter_fingerprint(&actor, CriticMode::BatchMean).expect("parameters");
        assert_eq!(
            retained.steps(),
            2,
            "the retained cursor did not advance twice"
        );

        let mut actor = trainable_actor();
        for batch in [&first, &second] {
            let mut fresh =
                Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
            update(
                &mut actor,
                batch,
                CriticMode::BatchMean,
                settings,
                7,
                &mut fresh,
            )
            .expect("update");
            assert_eq!(
                fresh.steps(),
                1,
                "a fresh optimiser is meant to start at one"
            );
        }
        let restarted = parameter_fingerprint(&actor, CriticMode::BatchMean).expect("parameters");

        assert_ne!(
            carried, restarted,
            "restarting Adam every update produced identical weights, so this test could not have              caught the defect it exists for"
        );
    }

    #[test]
    fn a_batch_mean_baseline_refuses_the_critic_data_it_would_not_use() {
        // F-M10-034-D5. §6.3 says batch-mean mode "does not evaluate/store an unused value". The
        // enforcement is stronger than ignoring such data: `freeze` refuses it, so a driver that
        // evaluated a nominal critic anyway cannot quietly hand over a batch that merely looks
        // right. Both halves are checked, because a rule that only ever fires one way is not one.
        let carries_a_value = Batch::freeze(
            vec![step(0, 2, 3.0, 0.5), step(1, 3, 1.0, 0.25)],
            CriticMode::BatchMean,
        );
        // The specific refusal, not merely "some error": an `is_err` here would also pass if the
        // fixture were malformed for an unrelated reason, which is the failure mode this milestone
        // kept producing.
        let refusal = carries_a_value
            .expect_err("batch-mean accepted a stored behaviour value it is defined not to use");
        assert!(
            refusal.contains("unused critic data in batch-mean mode"),
            "batch-mean refused for the wrong reason: {refusal}"
        );

        // And the advantages it does produce are a function of the returns alone: centre on the
        // batch mean, then normalise. Computed here independently rather than read back.
        let returns = [3.0_f64, 1.0, 5.0, 2.0];
        let batch = Batch::freeze(
            returns
                .iter()
                .enumerate()
                .map(|(index, value)| batch_mean_step(index % 2, 3, *value))
                .collect(),
            CriticMode::BatchMean,
        )
        .expect("valid batch");

        let mean = returns.iter().sum::<f64>() / 4.0;
        let centred: Vec<f64> = returns.iter().map(|value| value - mean).collect();
        let centre = centred.iter().sum::<f64>() / 4.0;
        let deviation = (centred.iter().map(|a| (a - centre).powi(2)).sum::<f64>() / 4.0).sqrt();
        let expected: Vec<f64> = centred
            .iter()
            .map(|a| (a - centre) / (deviation + 1e-8))
            .collect();

        for (got, want) in batch.advantages().iter().zip(&expected) {
            assert!(
                (got - want).abs() < 1e-12,
                "batch-mean advantage {got} is not the centred, normalised return {want}"
            );
        }
        // Non-vacuity: constant advantages would satisfy the loop above by accident.
        assert!(
            expected.iter().any(|a| (a - expected[0]).abs() > 0.5),
            "the fixture's returns did not spread, so the comparison proves nothing"
        );
    }

    #[test]
    fn batch_mean_mode_trains_no_critic() {
        // §6.3: "set critic loss to zero, and do not update/store unused value tensors".
        let mut actor = trainable_actor();
        let batch = Batch::freeze(
            vec![batch_mean_step(0, 2, 3.0), batch_mean_step(1, 2, 1.0)],
            CriticMode::BatchMean,
        )
        .expect("valid batch");

        let settings = Settings {
            epochs: 1,
            minibatch: 8,
            ..Settings::default()
        };
        let value_before = ti4_tensor::to_vec(actor.value_readout()).expect("value");
        let mut optimizer =
            Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
        let stats = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            settings,
            1,
            &mut optimizer,
        )
        .expect("update");
        assert_eq!(stats.len(), 1);
        assert!(
            stats[0].critic_loss.abs() < f64::EPSILON,
            "batch_mean mode reported a critic loss of {}",
            stats[0].critic_loss
        );
        assert_eq!(
            ti4_tensor::to_vec(actor.value_readout()).expect("value"),
            value_before,
            "batch-mean mode moved an unused value tensor"
        );
    }

    #[test]
    fn every_epoch_sees_the_same_advantages_however_the_order_changes() {
        // The freeze, end to end. Four epochs shuffle the records differently; the advantage vector
        // is one object and is never recomputed.
        let batch = Batch::freeze(
            vec![
                batch_mean_step(0, 2, 3.0),
                batch_mean_step(1, 2, 1.0),
                batch_mean_step(0, 3, 5.0),
            ],
            CriticMode::BatchMean,
        )
        .expect("valid batch");
        let before = batch.advantages().to_vec();

        let mut actor = trainable_actor();
        let settings = Settings {
            epochs: 4,
            minibatch: 2,
            ..Settings::default()
        };
        let mut optimizer =
            Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
        let stats = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            settings,
            42,
            &mut optimizer,
        )
        .expect("update");
        assert_eq!(stats.len(), 4, "four epochs did not run");
        assert_eq!(
            batch.advantages(),
            before.as_slice(),
            "the advantages changed during the update"
        );
    }

    #[test]
    fn malformed_batches_and_out_of_capacity_columns_fail_before_any_update() {
        assert!(
            Batch::freeze(Vec::new(), CriticMode::BatchMean).is_err(),
            "an empty batch was accepted"
        );
        let mut invalid = batch_mean_step(0, 2, 1.0);
        invalid.chosen = 2;
        assert!(
            Batch::freeze(
                vec![batch_mean_step(0, 2, 1.0), invalid],
                CriticMode::BatchMean
            )
            .is_err(),
            "one invalid record among valid records was accepted"
        );

        let mut outside = batch_mean_step(0, 2, 1.0);
        outside.options[0].columns[0] = 9_999;
        let batch = Batch::freeze(
            vec![outside, batch_mean_step(1, 2, 2.0)],
            CriticMode::BatchMean,
        )
        .expect("structural batch");
        let mut actor = trainable_actor();
        let settings = Settings {
            epochs: 1,
            minibatch: 1,
            ..Settings::default()
        };
        let mut optimizer =
            Adam::new(&mut actor, CriticMode::BatchMean, settings).expect("optimizer");
        let before = parameter_fingerprint(&actor, CriticMode::BatchMean).expect("parameters");
        let error = update(
            &mut actor,
            &batch,
            CriticMode::BatchMean,
            settings,
            5,
            &mut optimizer,
        )
        .expect_err("out-of-capacity input must fail");
        assert!(error.contains("outside"), "{error}");
        assert_eq!(
            parameter_fingerprint(&actor, CriticMode::BatchMean).expect("parameters"),
            before,
            "validation failed only after mutating parameters"
        );
    }

    #[test]
    fn shared_mode_really_updates_the_value_head() {
        let mut actor = trainable_actor();
        let batch = Batch::freeze(
            vec![distinguishable_step(0, 3), distinguishable_step(1, 3)],
            CriticMode::Shared,
        )
        .expect("batch");
        let settings = Settings {
            epochs: 1,
            minibatch: 2,
            ..Settings::default()
        };
        let mut optimizer = Adam::new(&mut actor, CriticMode::Shared, settings).expect("optimizer");
        let before = ti4_tensor::to_vec(actor.value_readout()).expect("value head");
        let stats = update(
            &mut actor,
            &batch,
            CriticMode::Shared,
            settings,
            11,
            &mut optimizer,
        )
        .expect("update");
        assert!(stats[0].critic_loss > 0.0, "critic loss is vacuous");
        assert_ne!(
            ti4_tensor::to_vec(actor.value_readout()).expect("value head"),
            before,
            "shared PPO did not update the value head"
        );
    }
}
