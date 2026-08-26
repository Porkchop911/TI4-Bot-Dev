//! Multi-teacher factual distillation (M10-032), per MLP plan §6.1.
//!
//! ```text
//! phase 0:  minimise  Σ_f  KL( champion_f(·|s) ‖ mlp(·|s, f) )
//!           over decisions sampled from champion self-play rollouts
//!           no reward signal, supervised only
//! ```
//!
//! Six schema-4 linear champions become one shared trunk with shared + per-faction residual
//! readouts. The trunk is forced to find the representation common to all six.
//!
//! # The objective is a mean of means, and that matters
//!
//! §6.1: "Minimize the mean of six per-faction KL means, so a faction generating more decisions
//! cannot dominate the shared trunk." The corpus is genuinely lopsided — hacan contributes 170,852
//! training decisions against letnev's 115,429 — so a pooled mean would hand hacan 21% of the
//! gradient and letnev 14% purely because hacan's games run longer. Within a faction every captured
//! non-forced decision has equal weight, and heads are not resampled.
//!
//! # Student temperature is fixed at 1.0
//!
//! Teacher probabilities already contain each teacher checkpoint's own temperature. Learning a
//! second one would introduce an unidentifiable second logit scale: any student temperature can be
//! absorbed into the readout weights, so the pair is not jointly determined by the data.

use std::collections::BTreeMap;

use rand::{Rng, SeedableRng};
use ti4_tensor::Tensor;

use crate::{Actor, FactionRow, SparseOption, Width};

/// §6.1's initialisation RNG domain and seed.
pub const INIT_DOMAIN: &str = "mlp-init-v1";
/// §6.1's initialisation seed.
pub const INIT_SEED: u64 = 20_260_821;
/// §6.1's fixed student temperature.
pub const STUDENT_TEMPERATURE: f64 = 1.0;

/// §6.1's optimiser settings, none of them tuned here.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Adam learning rate.
    pub learning_rate: f64,
    /// Adam `beta_1`.
    pub beta1: f64,
    /// Adam `beta_2`.
    pub beta2: f64,
    /// Adam epsilon.
    pub eps: f64,
    /// L2 added to the gradient.
    pub weight_decay: f64,
    /// Decisions per optimiser step.
    pub batch: usize,
    /// Decisions per backward pass. Gradients accumulate across micro-batches, so this changes
    /// peak memory and nothing else: the summed gradient over a batch is the same either way.
    pub micro_batch: usize,
    /// Global gradient-norm clip.
    pub clip: f64,
    /// Hard cap on epochs.
    pub max_epochs: usize,
    /// Stop after this many epochs without an improvement.
    pub patience: usize,
    /// Improvements smaller than this do not count, and ties choose the earlier epoch.
    pub tie: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            learning_rate: 3e-4,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            weight_decay: 1e-5,
            batch: 4_096,
            micro_batch: 512,
            clip: 1.0,
            max_epochs: 20,
            patience: 3,
            tie: 1e-5,
        }
    }
}

/// One decision, compiled to columns so training never touches a feature name.
#[derive(Debug, Clone)]
pub struct Sample {
    /// The faction row this decision conditions on.
    pub row: FactionRow,
    /// The schema-4 head index.
    pub head: usize,
    /// One sparse vector per legal option.
    pub options: Vec<SparseOption>,
    /// The teacher's probability for each option, positionally matched.
    pub teacher: Vec<f64>,
}

impl Sample {
    /// `Σ p log p`, the part of the KL that does not depend on the student.
    ///
    /// Precomputed because it is constant across every epoch and appears in every reported KL.
    fn teacher_entropy_term(&self) -> f64 {
        self.teacher
            .iter()
            .filter(|p| **p > 0.0)
            .map(|p| p * p.ln())
            .sum()
    }
}

/// What one epoch did.
#[derive(Debug, Clone)]
pub struct Epoch {
    /// Which epoch, from 1.
    pub number: usize,
    /// Mean of the six per-faction KL means, on the training shard.
    pub train_kl: f64,
    /// The same on the validation shard.
    pub validation_kl: f64,
    /// Per-faction validation KL, so a single faction collapsing is visible rather than averaged
    /// away.
    pub per_faction: BTreeMap<String, f64>,
    /// Optimiser steps taken so far.
    pub steps: usize,
}

/// The complete §6.1 imitation validation result.
#[derive(Debug, Clone)]
pub struct Validation {
    /// Mean of the six faction KL means.
    pub mean_kl: f64,
    /// Fraction of decisions whose highest-probability option matches the teacher's.
    pub top1_agreement: f64,
    /// Validation KL by faction.
    pub per_faction: BTreeMap<String, f64>,
    /// Validation KL by schema-4 head.
    pub per_head: BTreeMap<String, f64>,
}

/// Initialise a student exactly as §6.1 specifies.
///
/// # Why the RNG is ours and not libtorch's
///
/// §6.1: "A pinned Rust RNG generates f32 values in manifest tensor/name order before copying them
/// into libtorch, so a backend default cannot change initialization." A libtorch initialiser would
/// make the starting weights a property of the linked build rather than of the plan, and every
/// comparison downstream is against a run that started somewhere specific.
///
/// Zero, deliberately, for: every bias, the value head (M10-033 trains it), the critic rows, the
/// objective and ability rows, and the faction residuals and identity embeddings. A residual that
/// starts at zero means the student begins as the shared model and earns any per-faction deviation.
#[must_use]
pub fn initialize(width: Width, capacity: i64, active_rows: &[i64]) -> Actor {
    let mut actor = Actor::zeros(width, capacity);
    let w = width.dim();
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(INIT_SEED);

    // Order matters and is fixed: input rows, then hidden, then shared readout. Drawing them in a
    // different order would produce a different — and equally valid-looking — starting point.
    //
    // The input fan-in is the number of active features in a vector, which §6.1 pins at 32 rather
    // than deriving from the corpus, so the bound does not move when the extractor grows.
    let input_bound = (6.0f64 / 32.0).sqrt();
    let width_units = usize::try_from(w).unwrap_or(0);
    let mut input = vec![0.0f32; usize::try_from(capacity).unwrap_or(0) * width_units];
    for row in active_rows {
        let start = usize::try_from(*row).unwrap_or(0) * width_units;
        for value in &mut input[start..start + width_units] {
            *value = uniform(&mut rng, input_bound);
        }
    }
    *actor.input_mut() = Tensor::from_slice(&input).view([capacity, w]);

    let hidden_bound = (6.0f64 / f64::from(u32::try_from(w).unwrap_or(1))).sqrt();
    let hidden: Vec<f32> = (0..w * w)
        .map(|_| uniform(&mut rng, hidden_bound))
        .collect();
    *actor.hidden_mut() = Tensor::from_slice(&hidden).view([w, w]);

    let heads = i64::try_from(crate::heads().len()).unwrap_or(0);
    let readout_bound = 1.0 / f64::from(u32::try_from(w).unwrap_or(1)).sqrt();
    let readout: Vec<f32> = (0..heads * w)
        .map(|_| uniform(&mut rng, readout_bound))
        .collect();
    *actor.shared_readout_mut() = Tensor::from_slice(&readout).view([heads, w]);

    actor
}

fn uniform(rng: &mut rand_chacha::ChaCha8Rng, bound: f64) -> f32 {
    #[expect(
        clippy::cast_possible_truncation,
        reason = "weights are f32 by manifest"
    )]
    let value = rng.random_range(-bound..bound) as f32;
    value
}

/// Hand-rolled Adam over a named parameter set.
///
/// # Why not `tch`'s optimiser
///
/// It drives a `VarStore`, and the actor holds its tensors directly so a bundle can name each one.
/// Rolling Adam here also means the moments are ordinary tensors this crate owns, which is what
/// M10-035's resume has to serialise — an optimiser whose state lives inside a `VarStore` would
/// have to be taken apart to write it anyway.
pub struct Adam {
    settings: Settings,
    /// First moment per parameter.
    m: Vec<Tensor>,
    /// Second moment per parameter.
    v: Vec<Tensor>,
    /// The step counter. Adam's bias correction is a function of it, so it is state and not a
    /// statistic — a resume that restarted it at zero would take a large first step.
    t: i64,
}

impl Adam {
    /// Zeroed moments shaped like `parameters`.
    #[must_use]
    pub fn new(settings: Settings, parameters: &[Tensor]) -> Self {
        Self {
            settings,
            m: parameters.iter().map(Tensor::zeros_like).collect(),
            v: parameters.iter().map(Tensor::zeros_like).collect(),
            t: 0,
        }
    }

    /// How many steps have been taken.
    #[must_use]
    pub const fn steps(&self) -> i64 {
        self.t
    }

    /// Exact optimizer-state fingerprint for repeatability and resume tests.
    ///
    /// # Errors
    /// Returns an error when an optimizer tensor cannot be read.
    pub fn state_fingerprint(&self) -> Result<Vec<u32>, String> {
        let mut bits = Vec::new();
        for tensor in self.m.iter().chain(&self.v) {
            bits.extend(
                ti4_tensor::to_vec(tensor)
                    .map_err(|error| format!("reading Adam state: {error}"))?
                    .iter()
                    .map(|value| value.to_bits()),
            );
        }
        bits.extend(
            self.t
                .to_le_bytes()
                .chunks_exact(4)
                .map(|word| u32::from_le_bytes([word[0], word[1], word[2], word[3]])),
        );
        Ok(bits)
    }

    /// One update from the gradients currently on `parameters`, then zero them.
    ///
    /// The clip is on the **global** norm across every parameter, not per tensor: clipping each
    /// tensor separately would change the update's direction, not just its length.
    ///
    /// # Errors
    /// Returns an error when gradients are absent or non-finite, or an update cannot be applied.
    pub fn step(&mut self, parameters: &mut [Tensor], scale: f64) -> Result<(), String> {
        let mut total = 0.0f64;
        let mut defined = 0usize;
        for parameter in parameters.iter() {
            let grad = parameter.grad();
            if grad.defined() {
                defined += 1;
                total += (&grad * scale)
                    .square()
                    .sum(ti4_tensor::Kind::Float)
                    .double_value(&[]);
            }
        }
        if defined == 0 || !total.is_finite() {
            return Err(format!(
                "Adam received {defined} defined gradients with squared norm {total}"
            ));
        }
        let norm = total.sqrt();
        let clip = if norm > self.settings.clip && norm > 0.0 {
            self.settings.clip / norm
        } else {
            1.0
        };

        self.t += 1;
        let bias1 = 1.0
            - self
                .settings
                .beta1
                .powi(i32::try_from(self.t).unwrap_or(i32::MAX));
        let bias2 = 1.0
            - self
                .settings
                .beta2
                .powi(i32::try_from(self.t).unwrap_or(i32::MAX));

        for (index, parameter) in parameters.iter_mut().enumerate() {
            let raw = parameter.grad();
            if !raw.defined() {
                continue;
            }
            let grad = tch::no_grad(|| {
                // Classic Adam: L2 is added to the gradient, which is what "weight_decay" means
                // for Adam as §6.1 names it. AdamW's decoupled form is a different optimiser and
                // would need its own decision.
                &raw * (scale * clip) + &*parameter * self.settings.weight_decay
            });
            tch::no_grad(|| {
                self.m[index] =
                    &self.m[index] * self.settings.beta1 + &grad * (1.0 - self.settings.beta1);
                self.v[index] = &self.v[index] * self.settings.beta2
                    + grad.square() * (1.0 - self.settings.beta2);
                let m_hat = &self.m[index] / bias1;
                let v_hat = &self.v[index] / bias2;
                let update =
                    m_hat * self.settings.learning_rate / (v_hat.sqrt() + self.settings.eps);
                let _ = parameter.subtract_(&update);
            });
        }
        for parameter in parameters.iter_mut() {
            parameter.zero_grad();
        }
        Ok(())
    }
}

/// Cross-entropy per decision for an arbitrary mixed minibatch, in one forward pass.
///
/// # Why the padding is `-inf` and then masked
///
/// Decisions have different option counts, so the flat logits are scattered into a `[decisions,
/// widest]` matrix to take one batched softmax. The empty slots are filled with `-inf`, which is
/// exact rather than approximate: `exp(-inf)` is zero, so a padded slot takes no probability mass
/// and cannot flatter the model.
///
/// The log-probabilities at those slots are then `-inf`, and the teacher's probability is `0`. Their
/// product is `0 * -inf = NaN`, which would silently poison the whole batch — so the padded slots
/// are zeroed before the multiply. Both steps are needed: `-inf` for a correct softmax, zeroing for
/// a correct product.
///
/// # Errors
/// Anything the gather or the readout raises, and a head index outside the schema.
fn batch_cross_entropy(actor: &Actor, samples: &[&Sample]) -> Result<Tensor, String> {
    if samples.is_empty() {
        return Err("cross-entropy batch is empty".to_owned());
    }
    let widest = samples
        .iter()
        .map(|sample| sample.options.len())
        .max()
        .unwrap_or(0);
    if widest == 0 {
        return Err("every decision in the batch has no options".to_owned());
    }
    let decisions = samples.len();

    let mut flat: Vec<SparseOption> = Vec::new();
    let mut heads_idx: Vec<i64> = Vec::new();
    let mut rows_idx: Vec<i64> = Vec::new();
    let mut slots: Vec<i64> = Vec::new();
    let mut teacher = vec![0.0f32; decisions * widest];
    let mut padded = vec![1u8; decisions * widest];

    for (index, sample) in samples.iter().enumerate() {
        if crate::heads().get(sample.head).is_none() {
            return Err(format!("head index {} is out of range", sample.head));
        }
        if sample.teacher.len() != sample.options.len() {
            return Err(format!(
                "decision {index}: {} options against {} teacher probabilities",
                sample.options.len(),
                sample.teacher.len()
            ));
        }
        for (slot, option) in sample.options.iter().enumerate() {
            flat.push(option.clone());
            heads_idx.push(i64::try_from(sample.head).unwrap_or(0));
            rows_idx.push(i64::try_from(sample.row.index()).unwrap_or(0));
            let cell = index * widest + slot;
            slots.push(i64::try_from(cell).unwrap_or(0));
            #[expect(
                clippy::cast_possible_truncation,
                reason = "probabilities are f32-scale"
            )]
            {
                teacher[cell] = sample.teacher[slot] as f32;
            }
            padded[cell] = 0;
        }
    }

    let logits = actor
        .logits_mixed(&flat, &heads_idx, &rows_idx)
        .map_err(|error| format!("mixed logits: {error}"))?;
    let device = logits.device();

    let rows = i64::try_from(decisions).unwrap_or(0);
    let columns = i64::try_from(widest).unwrap_or(0);
    let scattered = Tensor::full(
        [rows * columns],
        f64::NEG_INFINITY,
        (ti4_tensor::Kind::Float, device),
    )
    .index_copy(0, &Tensor::from_slice(&slots).to_device(device), &logits)
    .view([rows, columns]);

    let log_q = scattered.log_softmax(1, ti4_tensor::Kind::Float);
    let mask = Tensor::from_slice(&padded)
        .to_device(device)
        .view([rows, columns])
        .to_kind(ti4_tensor::Kind::Bool);
    let log_q = log_q.masked_fill(&mask, 0.0);
    let p = Tensor::from_slice(&teacher)
        .to_device(device)
        .view([rows, columns]);

    Ok(-(p * log_q).sum_dim_intlist([1i64].as_slice(), false, ti4_tensor::Kind::Float))
}

/// Every §6.1 imitation metric in one pass: mean KL, top-1 agreement, and KL by faction and head.
///
/// # Errors
/// Returns an error for an empty or malformed sample set or a failed model evaluation.
pub fn validation_metrics(actor: &Actor, samples: &[Sample]) -> Result<Validation, String> {
    if samples.is_empty() {
        return Err("evaluation samples are empty".to_owned());
    }
    let mut sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut head_sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
    let mut agreements = 0usize;
    // Batched like training, and for the same reason: this is a full pass over the validation shard
    // once per epoch, so grouping it by `(row, head)` would issue the same storm of tiny kernels.
    let flat: Vec<&Sample> = samples.iter().collect();
    tch::no_grad(|| -> Result<(), String> {
        for micro in flat.chunks(2_048) {
            let crosses = batch_cross_entropy(actor, micro)?;
            let choices = batch_choices(actor, micro)?;
            let values = ti4_tensor::to_vec(&crosses)
                .map_err(|error| format!("reading validation cross-entropy: {error}"))?;
            if values.len() != micro.len() || choices.len() != micro.len() {
                return Err(format!(
                    "validation returned {} cross-entropies and {} choices for {} decisions",
                    values.len(),
                    choices.len(),
                    micro.len()
                ));
            }
            for ((sample, cross), choice) in micro.iter().zip(&values).zip(&choices) {
                let kl = f64::from(*cross) + sample.teacher_entropy_term();
                let entry = sums
                    .entry(crate::FACTION_ROSTER[sample.row.index()].to_owned())
                    .or_insert((0.0, 0));
                entry.0 += kl;
                entry.1 += 1;
                let head = crate::heads()
                    .get(sample.head)
                    .ok_or_else(|| format!("head index {} is out of range", sample.head))?;
                let head_entry = head_sums.entry((*head).to_owned()).or_insert((0.0, 0));
                head_entry.0 += kl;
                head_entry.1 += 1;
                let teacher_choice = sample
                    .teacher
                    .iter()
                    .enumerate()
                    .max_by(|left, right| left.1.total_cmp(right.1))
                    .map(|(index, _)| index)
                    .ok_or_else(|| "teacher distribution is empty".to_owned())?;
                agreements += usize::from(*choice == teacher_choice);
            }
        }
        Ok(())
    })?;
    let evaluated: usize = sums.values().map(|(_, count)| *count).sum();
    if evaluated != samples.len() {
        return Err(format!(
            "evaluated {evaluated} of {} validation samples",
            samples.len()
        ));
    }
    let per_faction: BTreeMap<String, f64> = sums
        .into_iter()
        .map(|(faction, (sum, count))| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "decision counts are exact in f64"
            )]
            let mean = if count == 0 { 0.0 } else { sum / count as f64 };
            (faction, mean)
        })
        .collect();
    let per_head = head_sums
        .into_iter()
        .map(|(head, (sum, count))| {
            #[expect(
                clippy::cast_precision_loss,
                reason = "decision counts are exact in f64"
            )]
            let mean = sum / count as f64;
            (head, mean)
        })
        .collect();
    #[expect(
        clippy::cast_precision_loss,
        reason = "decision counts are exact in f64"
    )]
    let top1_agreement = agreements as f64 / samples.len() as f64;
    Ok(Validation {
        mean_kl: mean_of_means(&per_faction),
        top1_agreement,
        per_faction,
        per_head,
    })
}

/// The student's chosen option index per decision, for top-1 agreement.
///
/// Separate from [`batch_cross_entropy`] because it needs no graph and the argmax is taken over
/// each decision's own options — the padded slots hold `-inf` and so can never win.
///
/// # Errors
/// Anything the readout raises.
fn batch_choices(actor: &Actor, samples: &[&Sample]) -> Result<Vec<usize>, String> {
    let widest = samples
        .iter()
        .map(|sample| sample.options.len())
        .max()
        .unwrap_or(0);
    if widest == 0 {
        return Err("every decision in the batch has no options".to_owned());
    }
    let mut flat: Vec<SparseOption> = Vec::new();
    let mut heads_idx: Vec<i64> = Vec::new();
    let mut rows_idx: Vec<i64> = Vec::new();
    let mut slots: Vec<i64> = Vec::new();
    for (index, sample) in samples.iter().enumerate() {
        for (slot, option) in sample.options.iter().enumerate() {
            flat.push(option.clone());
            heads_idx.push(i64::try_from(sample.head).unwrap_or(0));
            rows_idx.push(i64::try_from(sample.row.index()).unwrap_or(0));
            slots.push(i64::try_from(index * widest + slot).unwrap_or(0));
        }
    }
    let logits = actor
        .logits_mixed(&flat, &heads_idx, &rows_idx)
        .map_err(|error| format!("mixed logits: {error}"))?;
    let device = logits.device();
    let rows = i64::try_from(samples.len()).unwrap_or(0);
    let columns = i64::try_from(widest).unwrap_or(0);
    let scattered = Tensor::full(
        [rows * columns],
        f64::NEG_INFINITY,
        (ti4_tensor::Kind::Float, device),
    )
    .index_copy(0, &Tensor::from_slice(&slots).to_device(device), &logits)
    .view([rows, columns]);
    let picked = scattered.argmax(1, false);
    let picked = Vec::<i64>::try_from(picked.to_device(ti4_tensor::Device::Cpu))
        .map_err(|error| format!("reading argmax: {error}"))?;
    Ok(picked
        .into_iter()
        .map(|index| usize::try_from(index).unwrap_or(0))
        .collect())
}

/// Mean KL by faction, retained as the narrow compatibility view used by existing callers.
///
/// # Errors
/// Returns an error for an empty or malformed sample set or a failed model evaluation.
pub fn evaluate(actor: &Actor, samples: &[Sample]) -> Result<BTreeMap<String, f64>, String> {
    Ok(validation_metrics(actor, samples)?.per_faction)
}

/// The mean of the per-faction means. The objective §6.1 names.
#[must_use]
pub fn mean_of_means(per_faction: &BTreeMap<String, f64>) -> f64 {
    if per_faction.is_empty() {
        return 0.0;
    }
    #[expect(clippy::cast_precision_loss, reason = "six factions")]
    let count = per_faction.len() as f64;
    per_faction.values().sum::<f64>() / count
}

/// Which faction rows distillation may move.
///
/// §6.1: "The six training factions' residuals and embeddings **are trainable during distillation**;
/// untrained faction rows remain zero." The other twenty-seven rows have no data, so any value they
/// acquired would be noise the bundle then carries and a later run inherits.
fn trainable_rows(samples: &[Sample]) -> Vec<i64> {
    let mut rows: Vec<i64> = samples
        .iter()
        .map(|sample| i64::try_from(sample.row.index()).unwrap_or(0))
        .collect();
    rows.sort_unstable();
    rows.dedup();
    rows
}

/// Zero every faction row the corpus never trains on.
///
/// Applied after each step rather than trusted to the gradient being zero: weight decay acts on a
/// parameter whether or not it received a gradient, so an untouched row would drift without this.
fn hold_untrained_rows_at_zero(actor: &mut Actor, trainable: &[i64]) {
    let rows = i64::try_from(crate::FACTION_ROSTER.len()).unwrap_or(0);
    tch::no_grad(|| {
        for row in 0..rows {
            if trainable.contains(&row) {
                continue;
            }
            let _ = actor.delta_mut().get(row).zero_();
            let _ = actor.b_delta_mut().get(row).zero_();
            let _ = actor.embedding_mut().get(row).zero_();
        }
    });
}

/// A copy of every tensor that distillation moves, for retaining the best epoch.
fn snapshot(actor: &Actor) -> Vec<Tensor> {
    tch::no_grad(|| {
        [
            actor.input(),
            actor.b1(),
            actor.hidden(),
            actor.b2(),
            actor.shared_readout(),
            actor.b_shared(),
            actor.delta(),
            actor.b_delta(),
            actor.embedding(),
        ]
        .iter()
        .map(|tensor| tensor.detach().copy())
        .collect()
    })
}

fn restore(actor: &mut Actor, state: &[Tensor]) {
    tch::no_grad(|| {
        *actor.input_mut() = state[0].detach().copy().set_requires_grad(true);
        *actor.b1_mut() = state[1].detach().copy().set_requires_grad(true);
        *actor.hidden_mut() = state[2].detach().copy().set_requires_grad(true);
        *actor.b2_mut() = state[3].detach().copy().set_requires_grad(true);
        *actor.shared_readout_mut() = state[4].detach().copy().set_requires_grad(true);
        *actor.b_shared_mut() = state[5].detach().copy().set_requires_grad(true);
        *actor.delta_mut() = state[6].detach().copy().set_requires_grad(true);
        *actor.b_delta_mut() = state[7].detach().copy().set_requires_grad(true);
        *actor.embedding_mut() = state[8].detach().copy().set_requires_grad(true);
    });
}

/// Make every distilled parameter require a gradient.
fn open_for_training(actor: &mut Actor) {
    // One at a time: an array of `&mut` borrows of the same actor is two mutable borrows at once.
    macro_rules! open {
        ($accessor:ident) => {{
            let opened = actor.$accessor().detach().copy().set_requires_grad(true);
            *actor.$accessor() = opened;
        }};
    }
    open!(input_mut);
    open!(b1_mut);
    open!(hidden_mut);
    open!(b2_mut);
    open!(shared_readout_mut);
    open!(b_shared_mut);
    open!(delta_mut);
    open!(b_delta_mut);
    open!(embedding_mut);
}

fn parameters(actor: &Actor) -> Vec<Tensor> {
    [
        actor.input(),
        actor.b1(),
        actor.hidden(),
        actor.b2(),
        actor.shared_readout(),
        actor.b_shared(),
        actor.delta(),
        actor.b_delta(),
        actor.embedding(),
    ]
    .iter()
    .map(|tensor| (*tensor).shallow_clone())
    .collect()
}

/// What a completed distillation produced.
#[derive(Debug, Clone)]
pub struct Distillation {
    /// Every epoch that ran, in order.
    pub epochs: Vec<Epoch>,
    /// The epoch whose weights were retained.
    pub selected: usize,
    /// Why the run ended.
    pub stopped: String,
    /// L2 distance between the retained weights and the initialisation.
    ///
    /// Reported because a training loop that silently applies no update still produces a full,
    /// plausible-looking table of KLs. `Adam::step` skips any parameter whose gradient is
    /// undefined, so a plumbing mistake anywhere between the loss and the leaf tensors would look
    /// exactly like a run that simply did not learn much. This number distinguishes the two, and a
    /// caller should refuse a distillation reporting zero.
    pub parameter_movement: f64,
}

/// L2 distance between two parameter snapshots.
fn distance(before: &[Tensor], after: &[Tensor]) -> f64 {
    tch::no_grad(|| {
        before
            .iter()
            .zip(after)
            .map(|(left, right)| {
                (right - left)
                    .square()
                    .sum(ti4_tensor::Kind::Float)
                    .double_value(&[])
            })
            .sum::<f64>()
            .sqrt()
    })
}

/// Run phase 0.
///
/// Retains the **earliest** epoch reaching the minimum validation KL — ties within
/// `settings.tie` choose the earlier one — and stops after `settings.patience` epochs without an
/// improvement. Earliest rather than latest because two epochs that fit equally well are not
/// equally good: the earlier one got there with fewer updates and is the less over-fitted of the
/// two.
///
/// `progress` is called after each epoch so a long run reports as it goes rather than at the end.
///
/// # Errors
/// Returns an error for invalid settings or samples, or any failed model or optimizer operation.
#[expect(
    clippy::too_many_lines,
    reason = "one epoch loop: shuffle, batches, evaluation and selection read in the order they run"
)]
pub fn train(
    actor: &mut Actor,
    train_samples: &[Sample],
    validation_samples: &[Sample],
    settings: Settings,
    mut progress: impl FnMut(&Epoch),
) -> Result<Distillation, String> {
    if settings.max_epochs == 0
        || settings.batch == 0
        || settings.micro_batch == 0
        || train_samples.is_empty()
        || validation_samples.is_empty()
    {
        return Err("distillation settings or sample splits are empty".to_owned());
    }
    open_for_training(actor);
    let start = snapshot(actor);
    let trainable = trainable_rows(train_samples);
    let mut adam = Adam::new(settings, &parameters(actor));

    let mut epochs: Vec<Epoch> = Vec::new();
    let mut best: Option<(usize, f64, Vec<Tensor>)> = None;
    let mut since_improvement = 0usize;
    let mut stopped = format!("reached the {} epoch cap", settings.max_epochs);

    // Pinned shuffle domain: the order decisions are visited in is part of the run's identity, so
    // it comes from a seeded stream rather than from however the corpus happened to be laid out.
    let mut shuffle = rand_chacha::ChaCha8Rng::seed_from_u64(INIT_SEED ^ 0x5348_5546_464C_4521);

    for number in 1..=settings.max_epochs {
        let mut order: Vec<usize> = (0..train_samples.len()).collect();
        for index in (1..order.len()).rev() {
            order.swap(index, shuffle.random_range(0..=index));
        }

        let mut train_sums: BTreeMap<String, (f64, usize)> = BTreeMap::new();
        for batch in order.chunks(settings.batch) {
            // The scale that turns a sum of per-decision KLs into the mean of six per-faction
            // means. Computed per batch from that batch's own composition, so a batch that happens
            // to contain more hacan decisions does not thereby weight hacan more.
            let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
            for index in batch {
                *counts.entry(train_samples[*index].row.index()).or_default() += 1;
            }
            #[expect(
                clippy::cast_precision_loss,
                reason = "decision counts are exact in f64"
            )]
            let factions = counts.len() as f64;

            // No grouping by `(row, head)`. `logits_mixed` takes per-option head and faction
            // indices, so an entire micro-batch is one gather, one trunk and one row-wise readout
            // however many factions and heads it spans.
            //
            // The previous version split each batch into up to 6 x 14 groups and issued a separate
            // forward and backward for each. On CPU that wasted dispatches; on CUDA it was fatal —
            // an epoch took 343.9 s against the CPU's 135.9 s, because every group was a handful of
            // tiny kernel launches and launch overhead dominated the arithmetic.
            //
            // The summed gradient is unchanged: addition is associative over the graph, and the
            // per-decision weights below are the same numbers applied to the same terms.
            for micro in batch.chunks(settings.micro_batch) {
                let samples: Vec<&Sample> =
                    micro.iter().map(|index| &train_samples[*index]).collect();
                let crosses = batch_cross_entropy(actor, &samples)?;

                // The weights as a vector, so the whole micro-batch is scaled in one operation
                // rather than one multiply per decision.
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "decision counts are exact in f64"
                )]
                let weights: Vec<f32> = samples
                    .iter()
                    .map(|sample| {
                        (1.0 / (factions
                            * counts.get(&sample.row.index()).copied().unwrap_or(1) as f64))
                            as f32
                    })
                    .collect();
                let weight_tensor = Tensor::from_slice(&weights).to_device(crosses.device());

                // Reported before scaling: the KL a reader sees is the decision's own, not its
                // contribution to the objective.
                let reported = tch::no_grad(|| ti4_tensor::to_vec(&crosses))
                    .map_err(|error| format!("reading the batch cross-entropy: {error}"))?;
                if reported.len() != samples.len() {
                    return Err(format!(
                        "cross-entropy returned {} values for {} decisions",
                        reported.len(),
                        samples.len()
                    ));
                }
                for (sample, cross) in samples.iter().zip(&reported) {
                    let entry = train_sums
                        .entry(crate::FACTION_ROSTER[sample.row.index()].to_owned())
                        .or_insert((0.0, 0));
                    entry.0 += f64::from(*cross) + sample.teacher_entropy_term();
                    entry.1 += 1;
                }

                (crosses * weight_tensor)
                    .sum(ti4_tensor::Kind::Float)
                    .backward();
            }
            let mut params = parameters(actor);
            let before_steps = adam.steps();
            adam.step(&mut params, 1.0)?;
            if adam.steps() != before_steps + 1 {
                return Err("Adam did not apply the completed batch gradient".to_owned());
            }
            hold_untrained_rows_at_zero(actor, &trainable);
        }

        let per_faction = evaluate(actor, validation_samples)?;
        let validation_kl = mean_of_means(&per_faction);
        let train_kl = mean_of_means(
            &train_sums
                .into_iter()
                .map(|(faction, (sum, count))| {
                    #[expect(
                        clippy::cast_precision_loss,
                        reason = "decision counts are exact in f64"
                    )]
                    let mean = if count == 0 { 0.0 } else { sum / count as f64 };
                    (faction, mean)
                })
                .collect(),
        );

        let epoch = Epoch {
            number,
            train_kl,
            validation_kl,
            per_faction,
            steps: usize::try_from(adam.steps()).unwrap_or(0),
        };
        progress(&epoch);
        epochs.push(epoch);

        // Strictly better by more than the tie band, so an epoch that merely equals the incumbent
        // never displaces it.
        let improved = best
            .as_ref()
            .is_none_or(|(_, incumbent, _)| validation_kl < incumbent - settings.tie);
        if improved {
            best = Some((number, validation_kl, snapshot(actor)));
            since_improvement = 0;
        } else {
            since_improvement += 1;
            if since_improvement >= settings.patience {
                stopped = format!("{} epochs without improvement", settings.patience);
                break;
            }
        }
    }

    let (selected, _, state) =
        best.ok_or_else(|| "distillation completed without a selectable epoch".to_owned())?;
    restore(actor, &state);
    hold_untrained_rows_at_zero(actor, &trainable);
    let parameter_movement = distance(&start, &snapshot(actor));
    Ok(Distillation {
        epochs,
        selected,
        stopped,
        parameter_movement,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vocabulary() -> ti4_policy::vocabulary::Vocabulary {
        ti4_policy::vocabulary::Vocabulary::build((0..64).map(|n| format!("option:name{n}")))
            .expect("builds")
    }

    fn sample(row: &str, teacher: Vec<f64>) -> Sample {
        Sample {
            row: FactionRow::of(row).expect("roster"),
            head: 0,
            options: teacher
                .iter()
                .enumerate()
                .map(|(index, _)| SparseOption {
                    columns: vec![i64::try_from(index).unwrap_or(0) + 1],
                    values: vec![1.0],
                })
                .collect(),
            teacher,
        }
    }

    #[test]
    fn initialisation_is_a_pure_function_of_the_pinned_seed() {
        let vocabulary = vocabulary();
        let capacity = i64::try_from(vocabulary.capacity()).expect("fits");
        let active: Vec<i64> = (0..capacity).collect();

        let first = initialize(Width::W128, capacity, &active);
        let second = initialize(Width::W128, capacity, &active);
        assert_eq!(
            ti4_tensor::to_vec(first.input()).expect("vec"),
            ti4_tensor::to_vec(second.input()).expect("vec"),
            "two initialisations from the same pinned seed disagree"
        );
        // Non-vacuity: an all-zero table would satisfy the equality above trivially.
        assert!(
            ti4_tensor::to_vec(first.input())
                .expect("vec")
                .iter()
                .any(|value| *value != 0.0),
            "the initialised input table is all zeros"
        );
    }

    #[test]
    fn everything_section_six_one_says_starts_at_zero_does() {
        let first = initialize(Width::W128, 4_096, &[1, 2, 3]);
        for (name, tensor) in [
            ("b1", first.b1()),
            ("b2", first.b2()),
            ("b_shared", first.b_shared()),
            ("delta", first.delta()),
            ("b_delta", first.b_delta()),
            ("embedding", first.embedding()),
            ("w_value", first.value_readout()),
            ("b_value", first.b_value()),
        ] {
            let values = ti4_tensor::to_vec(tensor).expect("vec");
            assert!(
                values.iter().all(|value| *value == 0.0),
                "{name} did not start at zero"
            );
        }
    }

    #[test]
    fn an_inactive_input_row_stays_zero_while_an_active_one_does_not() {
        // The masking is the point: a row no feature reaches must not acquire a weight, or the
        // model has parameters nothing can ever train and the bundle carries noise.
        let actor = initialize(Width::W128, 4_096, &[7]);
        let table = ti4_tensor::to_vec(actor.input()).expect("vec");
        let width = 128;
        assert!(
            table[7 * width..8 * width].iter().any(|v| *v != 0.0),
            "the active row was not initialised"
        );
        assert!(
            table[8 * width..9 * width].iter().all(|v| *v == 0.0),
            "an inactive row was initialised"
        );
    }

    #[test]
    fn the_objective_is_a_mean_of_per_faction_means_not_a_pooled_mean() {
        // The distinction §6.1 turns on. One faction with many decisions and a different KL must
        // not pull the objective toward itself.
        let lopsided: BTreeMap<String, f64> = [("sol".to_owned(), 1.0), ("letnev".to_owned(), 3.0)]
            .into_iter()
            .collect();
        assert!((mean_of_means(&lopsided) - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_perfect_student_scores_zero_kl_and_a_wrong_one_scores_more() {
        // Non-vacuity for the KL itself: it must be zero exactly when the student matches, and
        // larger otherwise, or every training number below means nothing.
        let vocabulary = vocabulary();
        let capacity = i64::try_from(vocabulary.capacity()).expect("fits");
        // A zero actor gives uniform logits, so a uniform teacher is matched exactly.
        let actor = Actor::zeros(Width::W128, capacity);

        let uniform = sample("sol", vec![0.5, 0.5]);
        let matched = evaluate(&actor, std::slice::from_ref(&uniform)).expect("evaluates");
        assert!(
            mean_of_means(&matched).abs() < 1e-6,
            "a matched student did not score zero: {matched:?}"
        );

        let skewed = sample("sol", vec![0.9, 0.1]);
        let mismatched = evaluate(&actor, std::slice::from_ref(&skewed)).expect("evaluates");
        assert!(
            mean_of_means(&mismatched) > 0.1,
            "a mismatched student scored as if matched: {mismatched:?}"
        );

        let aligned = sample("sol", vec![0.9, 0.1]);
        let opposite = sample("sol", vec![0.1, 0.9]);
        let metrics = validation_metrics(&actor, &[aligned, opposite]).expect("metrics");
        assert_eq!(metrics.per_head.len(), 1, "head accounting is empty");
        assert!(metrics.per_head.values().all(|kl| *kl > 0.0));
        assert!(
            (metrics.top1_agreement - 0.5).abs() < f64::EPSILON,
            "top-1 accounting did not distinguish a matching and mismatching argmax"
        );
    }
}
