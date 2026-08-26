//! Critic warm-up (M10-033), per MLP plan §6.2.
//!
//! # The distilled actor does not train against a random critic
//!
//! PPO's advantage is `return − V(s)`. If `V` is noise at the start of PPO, every advantage is
//! noise, and the policy gradient spends its first updates unlearning the imitation that
//! distillation just bought. So `V` is fitted first, on the returns already captured in the corpus.
//!
//! # What "actor frozen" has to mean, and why the definition is the package
//!
//! §6.2 is unusually explicit: *"'Actor frozen' includes the shared `W2`, biases, all policy input
//! rows, readouts, residuals, and embeddings; otherwise a nominal critic warm-up would silently
//! destroy imitation."*
//!
//! The critic shares the trunk. Left alone, the value loss would happily flow gradient into `W2`,
//! `b1`, `b2` and the embedding — all of which the policy also reads — and the actor would drift
//! while every critic metric improved. Nothing in the warm-up's own numbers would look wrong.
//!
//! So exactly two things train: the **`critic-state:` input rows**, which no policy vector ever
//! gathers, and the **value head**, which no policy readout touches. Both are disjoint from the
//! policy path, which is what makes the guarantee available at all.
//!
//! And it is checked rather than argued: [`WarmUp::logits_unchanged`] compares policy logits
//! **bit for bit** across the whole warm-up. Not within a tolerance — the claim is that the policy
//! computation reads none of the changed parameters, and that claim predicts identical bits.

use std::collections::BTreeSet;

use ti4_tensor::Tensor;

use crate::{Actor, FactionRow, SparseOption, distill::Sample};

/// §6.2's optimiser settings.
#[derive(Debug, Clone, Copy)]
pub struct Settings {
    /// Adam learning rate. Higher than distillation's: the value head starts at zero and has one
    /// scalar output to fit rather than a distribution to match.
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
    /// Decisions per backward pass.
    pub micro_batch: usize,
    /// Global gradient-norm clip.
    pub clip: f64,
    /// Hard cap on epochs.
    pub max_epochs: usize,
    /// The explained variance a warm-up must reach to be selected.
    pub threshold: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            learning_rate: 1e-3,
            beta1: 0.9,
            beta2: 0.999,
            eps: 1e-8,
            weight_decay: 1e-5,
            batch: 4_096,
            micro_batch: 256,
            clip: 1.0,
            max_epochs: 20,
            threshold: 0.10,
        }
    }
}

/// One position's critic vector and the return it should predict.
#[derive(Debug, Clone)]
pub struct CriticSample {
    /// The faction row, for the identity embedding.
    pub row: FactionRow,
    /// The option-free critic vector. Private: it may only be built through [`Self::new`], which
    /// checks the namespace.
    critic: SparseOption,
    /// The accepted four-round return.
    pub target: f64,
}

/// Why a captured record could not become a critic sample.
#[derive(Debug, thiserror::Error)]
pub enum SampleError {
    /// A name outside the critic namespace was offered.
    #[error("{name} is not in the {} namespace", ti4_policy::critic::CRITIC_FAMILY)]
    NotCritic {
        /// The offending name.
        name: String,
    },
    /// The vector was empty, which is a malformed capture rather than a legal position.
    #[error("the critic vector is empty")]
    Empty,
}

impl CriticSample {
    /// Compile a captured critic vector, checking it really is one.
    ///
    /// # Why this checks rather than trusts
    ///
    /// The corpus records both actor and critic vectors and they are both `Vec<(String, f64)>`.
    /// Nothing in that type stops a caller wiring the wrong field, and the result would be a value
    /// head trained on option-derived features — the thing §4.1 forbids and F-M09-027-2 made
    /// unrepresentable at the inference API. This restores the same guarantee on the training path,
    /// where the vectors arrive as plain data out of a file.
    ///
    /// # Errors
    /// [`SampleError`] if any name is outside `critic-state:`, or the vector is empty.
    pub fn new(
        row: FactionRow,
        facts: &[(String, f64)],
        vocabulary: &ti4_policy::vocabulary::Vocabulary,
        target: f64,
    ) -> Result<Self, SampleError> {
        if facts.is_empty() {
            return Err(SampleError::Empty);
        }
        let mut columns = Vec::with_capacity(facts.len());
        let mut values = Vec::with_capacity(facts.len());
        for (name, value) in facts {
            if ti4_policy::vocabulary::family_of(name) != ti4_policy::critic::CRITIC_FAMILY {
                return Err(SampleError::NotCritic { name: name.clone() });
            }
            columns.push(i64::try_from(vocabulary.column_of(name)).unwrap_or(0));
            #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
            values.push(*value as f32);
        }
        Ok(Self {
            row,
            critic: SparseOption { columns, values },
            target,
        })
    }
}

/// What one epoch of warm-up did.
#[derive(Debug, Clone)]
pub struct Epoch {
    /// Which epoch, from 1.
    pub number: usize,
    /// Mean squared error on the training split.
    pub train_mse: f64,
    /// Mean squared error on the validation split.
    pub validation_mse: f64,
    /// Validation explained variance — the quantity §6.2's threshold is stated in.
    pub explained_variance: f64,
}

/// What a completed warm-up produced.
#[derive(Debug, Clone)]
pub struct WarmUp {
    /// Every epoch that ran.
    pub epochs: Vec<Epoch>,
    /// The epoch selected, if any reached the threshold.
    pub selected: Option<usize>,
    /// Whether policy logits were bit-identical before and after.
    ///
    /// Not a tolerance. The design claim is that the policy reads none of the parameters the
    /// warm-up moves, and that claim predicts identical bits — so anything else is a defect in the
    /// freezing, not rounding.
    pub logits_unchanged: bool,
    /// How far the critic rows and value head actually moved, so a warm-up that trained nothing is
    /// distinguishable from one that trained and did not help.
    pub parameter_movement: f64,
}

/// Which input rows belong to the critic's namespace.
///
/// Derived from the vocabulary rather than assumed contiguous: the critic family was appended to
/// the OOV registry in M09-027b and its ordinary columns are interleaved with every other family's
/// in `FeatureKey` order, so "the critic rows" is a set and not a range.
#[must_use]
pub fn critic_rows(vocabulary: &ti4_policy::vocabulary::Vocabulary) -> Vec<i64> {
    let mut rows: BTreeSet<i64> = BTreeSet::new();
    for (column, slot) in vocabulary.slots().iter().enumerate() {
        if ti4_policy::vocabulary::family_of(&slot.name) == ti4_policy::critic::CRITIC_FAMILY {
            rows.insert(i64::try_from(column).unwrap_or(0));
        }
    }
    rows.into_iter().collect()
}

/// Explained variance: `1 − Var(y − ŷ) / Var(y)`.
///
/// Reported instead of raw MSE because MSE is not comparable across corpora — a critic predicting a
/// constant scores well when the returns barely vary. Explained variance is zero for the
/// predict-the-mean baseline and negative for anything worse, which is the comparison §6.2's
/// threshold means.
#[must_use]
pub fn explained_variance(targets: &[f64], predictions: &[f64]) -> f64 {
    if targets.len() != predictions.len() || targets.is_empty() {
        return f64::NAN;
    }
    #[expect(clippy::cast_precision_loss, reason = "sample counts are exact in f64")]
    let count = targets.len() as f64;
    let mean = targets.iter().sum::<f64>() / count;
    let total: f64 = targets.iter().map(|y| (y - mean).powi(2)).sum();
    if total <= 0.0 {
        // Every return identical. Nothing to explain, and reporting 1.0 here would let a constant
        // critic clear the threshold on a degenerate split.
        return f64::NAN;
    }
    let residual: f64 = targets
        .iter()
        .zip(predictions)
        .map(|(y, p)| (y - p).powi(2))
        .sum();
    1.0 - residual / total
}

/// Every policy logit for a fixed probe set, as raw bits.
///
/// Bits rather than values so the comparison is exact and cannot be quietly widened later.
#[must_use]
pub fn logit_fingerprint(actor: &Actor, probes: &[Sample]) -> Vec<u32> {
    let mut bits = Vec::new();
    tch::no_grad(|| {
        for probe in probes {
            let Some(head) = crate::heads().get(probe.head) else {
                continue;
            };
            let Ok(logits) = actor.logits(&probe.options, head, probe.row) else {
                continue;
            };
            if let Ok(values) = ti4_tensor::to_vec(&logits) {
                bits.extend(values.iter().map(|value| value.to_bits()));
            }
        }
    });
    bits
}

/// `V(s)` for one sample, with a graph.
fn value_of(actor: &Actor, sample: &CriticSample) -> Option<Tensor> {
    let input = crate::CriticInput::from_sparse(sample.critic.clone());
    actor.value_tensor(&input, sample.row).ok()
}

/// Predictions for a set of samples, without a graph.
#[must_use]
pub fn predict(actor: &Actor, samples: &[CriticSample]) -> Vec<f64> {
    tch::no_grad(|| {
        samples
            .iter()
            .map(|sample| value_of(actor, sample).map_or(f64::NAN, |value| value.double_value(&[])))
            .collect()
    })
}

/// Adam confined to a set of rows of one table, plus two whole tensors.
///
/// # Why the confinement is structural and not a mask applied afterwards
///
/// The obvious implementation gives `W1` a gradient, zeroes the policy rows of that gradient, and
/// steps. It does not work, and the way it fails is quiet: Adam's weight decay adds `wd × param`
/// to the gradient, so **every policy row would still decay** even though its gradient was zeroed.
/// The policy logits would drift by a little each step and the bit-identical assertion would fail —
/// or worse, if someone had written that assertion with a tolerance, it would pass while the
/// distilled imitation slowly bled away.
///
/// So the update is only ever computed for the critic rows: their gradient is gathered, the moments
/// are `[rows, width]` rather than `[capacity, width]`, and the result is added back with
/// `index_add_`. No policy row is an operand of any expression here.
struct RowAdam {
    settings: Settings,
    /// The rows of `W1` this may touch.
    index: Tensor,
    /// Moments for the critic rows, the value readout and the value bias, in that order.
    m: Vec<Tensor>,
    v: Vec<Tensor>,
    t: i64,
}

impl RowAdam {
    fn new(settings: Settings, rows: &[i64], width: i64) -> Self {
        let count = i64::try_from(rows.len()).unwrap_or(0);
        let shapes = [vec![count, width], vec![width], vec![1]];
        Self {
            settings,
            index: Tensor::from_slice(rows),
            m: shapes
                .iter()
                .map(|shape| {
                    Tensor::zeros(
                        shape.as_slice(),
                        (ti4_tensor::Kind::Float, ti4_tensor::Device::Cpu),
                    )
                })
                .collect(),
            v: shapes
                .iter()
                .map(|shape| {
                    Tensor::zeros(
                        shape.as_slice(),
                        (ti4_tensor::Kind::Float, ti4_tensor::Device::Cpu),
                    )
                })
                .collect(),
            t: 0,
        }
    }

    /// The gradients currently on the three trainables, as `[critic rows, w_value, b_value]`.
    fn gradients(&self, actor: &Actor) -> Option<Vec<Tensor>> {
        let table = actor.input().grad();
        let value = actor.value_readout().grad();
        let bias = actor.b_value().grad();
        if !table.defined() || !value.defined() || !bias.defined() {
            return None;
        }
        Some(vec![
            table.index_select(0, &self.index),
            value.shallow_clone(),
            bias.shallow_clone(),
        ])
    }

    fn step(&mut self, actor: &mut Actor) -> bool {
        let Some(grads) = self.gradients(actor) else {
            return false;
        };
        // The parameters the gradients belong to, gathered the same way.
        let params = vec![
            actor.input().detach().index_select(0, &self.index),
            actor.value_readout().detach().shallow_clone(),
            actor.b_value().detach().shallow_clone(),
        ];

        let total: f64 = grads
            .iter()
            .map(|g| g.square().sum(ti4_tensor::Kind::Float).double_value(&[]))
            .sum();
        let norm = total.sqrt();
        let clip = if norm > self.settings.clip && norm > 0.0 {
            self.settings.clip / norm
        } else {
            1.0
        };

        self.t += 1;
        let step = i32::try_from(self.t).unwrap_or(i32::MAX);
        let bias1 = 1.0 - self.settings.beta1.powi(step);
        let bias2 = 1.0 - self.settings.beta2.powi(step);

        let mut updates = Vec::with_capacity(3);
        tch::no_grad(|| {
            for (index, (grad, param)) in grads.iter().zip(&params).enumerate() {
                let g = grad * clip + param * self.settings.weight_decay;
                self.m[index] =
                    &self.m[index] * self.settings.beta1 + &g * (1.0 - self.settings.beta1);
                self.v[index] =
                    &self.v[index] * self.settings.beta2 + g.square() * (1.0 - self.settings.beta2);
                let m_hat = &self.m[index] / bias1;
                let v_hat = &self.v[index] / bias2;
                updates
                    .push(m_hat * self.settings.learning_rate / (v_hat.sqrt() + self.settings.eps));
            }
            // `index_add_` with a negated update touches exactly the named rows.
            let _ = actor
                .input_mut()
                .index_add_(0, &self.index, &(-&updates[0]));
            let _ = actor.value_readout_mut().subtract_(&updates[1]);
            let _ = actor.b_value_mut().subtract_(&updates[2]);
        });

        actor.input_mut().zero_grad();
        actor.value_readout_mut().zero_grad();
        actor.b_value_mut().zero_grad();
        true
    }
}

/// Open exactly the three trainables for gradients, and detach everything the policy reads.
///
/// # Where the guarantee actually lives
///
/// Not here. Detaching the policy tensors stops gradient accumulating into them, which saves
/// memory and makes the intent legible — but it is **not** what keeps the logits identical.
/// [`RowAdam`] only ever applies an update to the critic rows, the value readout and the value
/// bias, so a policy tensor left attached by mistake would still never move.
///
/// This was measured rather than assumed: unfreezing `W2` here and rerunning the end-to-end test
/// changes nothing at all, because no update is written to it either way. The falsification that
/// does fail the test is letting an update reach the shared trunk. Worth stating plainly, because
/// a reader who believed the freezing were the mechanism would "simplify" `RowAdam` into a masked
/// dense Adam and reintroduce the drift — weight decay alone would then move every policy row.
fn open_critic_only(actor: &mut Actor) {
    macro_rules! open {
        ($accessor:ident) => {{
            let opened = actor.$accessor().detach().copy().set_requires_grad(true);
            *actor.$accessor() = opened;
        }};
    }
    open!(input_mut);
    open!(value_readout_mut);
    open!(b_value_mut);
    // Everything the policy reads is detached, so the value loss has nowhere to flow into it.
    // §6.2 names these explicitly because a warm-up that moved them would improve every critic
    // metric while destroying the imitation distillation just bought.
    macro_rules! freeze {
        ($accessor:ident) => {{
            let frozen = actor.$accessor().detach().copy();
            *actor.$accessor() = frozen;
        }};
    }
    freeze!(b1_mut);
    freeze!(hidden_mut);
    freeze!(b2_mut);
    freeze!(shared_readout_mut);
    freeze!(b_shared_mut);
    freeze!(delta_mut);
    freeze!(b_delta_mut);
    freeze!(embedding_mut);
}

/// Run the shared critic warm-up.
///
/// `probes` is a fixed set of policy decisions whose logits are fingerprinted before and after; it
/// should come from the same corpus, and a few hundred is plenty.
///
/// # Panics
/// If the actor's width does not fit an `i64`, which the type system already bounds.
pub fn warm_up(
    actor: &mut Actor,
    vocabulary: &ti4_policy::vocabulary::Vocabulary,
    train_samples: &[CriticSample],
    validation_samples: &[CriticSample],
    probes: &[Sample],
    settings: Settings,
    mut progress: impl FnMut(&Epoch),
) -> WarmUp {
    let before = logit_fingerprint(actor, probes);
    let rows = critic_rows(vocabulary);
    open_critic_only(actor);
    let mut adam = RowAdam::new(settings, &rows, actor.width());
    let start = tch::no_grad(|| {
        (
            actor.input().detach().index_select(0, &adam.index),
            actor.value_readout().detach().copy(),
        )
    });

    let mut epochs: Vec<Epoch> = Vec::new();
    let mut selected: Option<usize> = None;
    let targets: Vec<f64> = validation_samples.iter().map(|s| s.target).collect();

    for number in 1..=settings.max_epochs {
        let mut squared = 0.0f64;
        let mut seen = 0usize;
        for batch in train_samples.chunks(settings.batch) {
            for micro in batch.chunks(settings.micro_batch) {
                let mut loss: Option<Tensor> = None;
                for sample in micro {
                    let Some(value) = value_of(actor, sample) else {
                        continue;
                    };
                    let error = value - sample.target;
                    squared += tch::no_grad(|| error.square().double_value(&[0]));
                    seen += 1;
                    let term = error.square();
                    loss = Some(loss.map_or_else(|| term.shallow_clone(), |sum| sum + &term));
                }
                if let Some(loss) = loss {
                    #[expect(clippy::cast_precision_loss, reason = "micro-batch sizes are small")]
                    let mean = loss / micro.len() as f64;
                    mean.backward();
                }
            }
            adam.step(actor);
        }

        let predictions = predict(actor, validation_samples);
        #[expect(clippy::cast_precision_loss, reason = "sample counts are exact in f64")]
        let validation_mse = targets
            .iter()
            .zip(&predictions)
            .map(|(y, p)| (y - p).powi(2))
            .sum::<f64>()
            / targets.len().max(1) as f64;
        #[expect(clippy::cast_precision_loss, reason = "sample counts are exact in f64")]
        let train_mse = if seen == 0 {
            f64::NAN
        } else {
            squared / seen as f64
        };

        let epoch = Epoch {
            number,
            train_mse,
            validation_mse,
            explained_variance: explained_variance(&targets, &predictions),
        };
        progress(&epoch);
        // The **earliest** epoch clearing the threshold, per §6.2 — not the best one. The warm-up
        // exists to give PPO a usable starting critic, not to squeeze the corpus.
        if selected.is_none() && epoch.explained_variance >= settings.threshold {
            selected = Some(number);
        }
        epochs.push(epoch);
        if selected.is_some() {
            break;
        }
    }

    let movement = tch::no_grad(|| {
        let now_rows = actor.input().detach().index_select(0, &adam.index);
        let rows_moved = (now_rows - &start.0)
            .square()
            .sum(ti4_tensor::Kind::Float)
            .double_value(&[]);
        let head_moved = (actor.value_readout().detach() - &start.1)
            .square()
            .sum(ti4_tensor::Kind::Float)
            .double_value(&[]);
        (rows_moved + head_moved).sqrt()
    });

    WarmUp {
        epochs,
        selected,
        logits_unchanged: logit_fingerprint(actor, probes) == before,
        parameter_movement: movement,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Width;

    #[test]
    fn explained_variance_is_zero_for_the_predict_the_mean_baseline() {
        // The property that makes the threshold meaningful: a critic that has learned nothing
        // except the average return must score 0, not something that looks like progress.
        let targets = vec![1.0, 2.0, 3.0, 4.0];
        let mean = vec![2.5; 4];
        assert!(explained_variance(&targets, &mean).abs() < 1e-12);

        let perfect = explained_variance(&targets, &targets);
        assert!((perfect - 1.0).abs() < 1e-12);

        // Worse than the mean is negative, not clamped.
        let bad = explained_variance(&targets, &[4.0, 3.0, 2.0, 1.0]);
        assert!(bad < 0.0, "a worse-than-mean critic scored {bad}");
    }

    #[test]
    fn a_degenerate_split_reports_nan_rather_than_a_perfect_score() {
        // Every return identical: a constant critic has residual zero and would otherwise score
        // 1.0, clearing the threshold having learned nothing at all.
        let targets = vec![2.0, 2.0, 2.0];
        assert!(explained_variance(&targets, &[2.0, 2.0, 2.0]).is_nan());
    }

    #[test]
    fn the_critic_rows_are_exactly_the_critic_namespace_and_are_not_contiguous() {
        let vocabulary = ti4_policy::vocabulary::Vocabulary::build([
            "option:a",
            "critic-state:round",
            "option:b",
            "critic-state:victory_points",
            "kind:move",
        ])
        .expect("builds");
        let rows = critic_rows(&vocabulary);
        assert_eq!(rows.len(), 2, "found {rows:?}");
        for row in &rows {
            let name = &vocabulary.slots()[usize::try_from(*row).expect("fits")].name;
            assert!(
                name.starts_with("critic-state:"),
                "{name} is not a critic row"
            );
        }
        // Non-vacuity for the "set, not a range" claim: if the two happened to be adjacent this
        // test would still pass, so assert the vocabulary really did interleave them.
        let policy_rows: Vec<i64> = (0..i64::try_from(vocabulary.slot_count()).expect("fits"))
            .filter(|row| !rows.contains(row))
            .collect();
        assert!(
            policy_rows
                .iter()
                .any(|policy| rows.iter().any(|critic| policy > critic))
                && policy_rows
                    .iter()
                    .any(|policy| rows.iter().any(|critic| policy < critic)),
            "the critic rows are not interleaved with policy rows, so this fixture proves nothing"
        );
    }

    /// A vocabulary with both policy and critic names, so the two row sets are genuinely distinct.
    fn mixed_vocabulary() -> ti4_policy::vocabulary::Vocabulary {
        let mut names: Vec<String> = (0..32).map(|n| format!("option:name{n}")).collect();
        names.extend((0..16).map(|n| format!("critic-state:fact{n}")));
        ti4_policy::vocabulary::Vocabulary::build(names).expect("builds")
    }

    fn critic_samples(
        vocabulary: &ti4_policy::vocabulary::Vocabulary,
        count: usize,
    ) -> Vec<CriticSample> {
        (0..count)
            .map(|index| {
                // A target that genuinely depends on the features, so there is something to learn.
                let a = f64::from(u32::try_from(index % 7).unwrap_or(0));
                let b = f64::from(u32::try_from(index % 3).unwrap_or(0));
                CriticSample::new(
                    FactionRow::of("sol").expect("roster"),
                    &[
                        (format!("critic-state:fact{}", index % 5), a),
                        (format!("critic-state:fact{}", 5 + index % 4), b),
                    ],
                    vocabulary,
                    2.0 * a - b,
                )
                .expect("critic names")
            })
            .collect()
    }

    #[test]
    fn a_warm_up_moves_the_critic_and_leaves_every_policy_logit_bit_identical() {
        // The package's central claim, end to end. Both halves are load-bearing: without the
        // movement check a warm-up that trained nothing would satisfy "logits unchanged" perfectly.
        let vocabulary = mixed_vocabulary();
        let capacity = i64::try_from(vocabulary.capacity()).expect("fits");
        let mut actor = Actor::zeros(Width::W128, capacity);
        // Non-zero everywhere the policy reads, so a drift would actually show up.
        *actor.input_mut() = actor.input().f_add_scalar(0.05).expect("add");
        *actor.hidden_mut() = actor.hidden().f_add_scalar(0.03).expect("add");
        *actor.shared_readout_mut() = actor.shared_readout().f_add_scalar(0.2).expect("add");
        *actor.b1_mut() = actor.b1().f_add_scalar(0.01).expect("add");

        let probes: Vec<Sample> = (0..8)
            .map(|index| Sample {
                row: FactionRow::of("sol").expect("roster"),
                head: index % crate::heads().len(),
                options: vec![
                    SparseOption {
                        columns: vec![1, 2],
                        values: vec![1.0, 0.5],
                    },
                    SparseOption {
                        columns: vec![3, 4],
                        values: vec![0.25, 1.0],
                    },
                ],
                teacher: vec![0.5, 0.5],
            })
            .collect();
        let before = logit_fingerprint(&actor, &probes);
        assert!(
            before.iter().any(|bits| *bits != 0),
            "the probes produce only zero logits, so a drift could not be detected"
        );

        let train = critic_samples(&vocabulary, 256);
        let validation = critic_samples(&vocabulary, 64);
        let settings = Settings {
            max_epochs: 3,
            batch: 64,
            micro_batch: 32,
            // Unreachable on purpose, so the loop runs every epoch rather than stopping early and
            // testing less than it looks like it does.
            threshold: 2.0,
            ..Settings::default()
        };

        let result = warm_up(
            &mut actor,
            &vocabulary,
            &train,
            &validation,
            &probes,
            settings,
            |_| {},
        );

        assert!(
            result.parameter_movement > 0.0,
            "the warm-up trained nothing, so 'logits unchanged' would hold vacuously"
        );
        assert!(result.logits_unchanged, "the warm-up moved a policy logit");
        assert_eq!(
            logit_fingerprint(&actor, &probes),
            before,
            "the policy logits are not bit-identical after the warm-up"
        );
        // And it should have learned something about a target that really depends on the features.
        let first = result.epochs.first().expect("an epoch ran").validation_mse;
        let last = result.epochs.last().expect("an epoch ran").validation_mse;
        assert!(
            last < first,
            "validation MSE did not improve: {first} then {last}"
        );
    }

    #[test]
    fn a_policy_vector_cannot_be_offered_as_a_critic_sample() {
        // The training-path half of F-M09-027-2's guarantee. Both vectors are `Vec<(String, f64)>`
        // in the corpus, so only a check stops the wrong field being wired in.
        let vocabulary = mixed_vocabulary();
        let error = CriticSample::new(
            FactionRow::of("sol").expect("roster"),
            &[("option:name3".to_owned(), 1.0)],
            &vocabulary,
            0.0,
        )
        .expect_err("a policy name must be refused");
        assert!(matches!(error, SampleError::NotCritic { .. }), "{error}");

        // And a mixed vector is refused too, not silently filtered down to its critic half.
        let mixed = CriticSample::new(
            FactionRow::of("sol").expect("roster"),
            &[
                ("critic-state:fact1".to_owned(), 1.0),
                ("option:name3".to_owned(), 1.0),
            ],
            &vocabulary,
            0.0,
        );
        assert!(mixed.is_err(), "a mixed vector was accepted");
    }

    #[test]
    fn a_logit_fingerprint_notices_a_policy_change_and_ignores_an_unrelated_one() {
        let capacity = 4_096;
        let mut actor = Actor::zeros(Width::W128, capacity);
        // Every layer non-zero. With `hidden` left at zero the trunk output is identically zero,
        // so no readout change can move a logit and both assertions below pass having checked
        // nothing — which is exactly what the first version of this test did.
        *actor.input_mut() = actor.input().f_add_scalar(0.1).expect("add");
        *actor.hidden_mut() = actor.hidden().f_add_scalar(0.05).expect("add");
        *actor.shared_readout_mut() = actor.shared_readout().f_add_scalar(0.2).expect("add");
        let probe = vec![Sample {
            row: FactionRow::of("sol").expect("roster"),
            head: 0,
            options: vec![
                SparseOption {
                    columns: vec![1, 2],
                    values: vec![1.0, 0.5],
                },
                SparseOption {
                    columns: vec![3],
                    values: vec![1.0],
                },
            ],
            teacher: vec![0.5, 0.5],
        }];

        let before = logit_fingerprint(&actor, &probe);
        assert!(!before.is_empty(), "the probe produced no logits");
        // And they must not all be zero, or a "changed" readout could still leave them at zero.
        assert!(
            before.iter().any(|bits| *bits != 0),
            "every probe logit is zero, so this fixture cannot detect a change"
        );

        // Moving the value head must not move a policy logit — that is the whole guarantee.
        *actor.value_readout_mut() = actor.value_readout().f_add_scalar(1.0).expect("add");
        assert_eq!(
            logit_fingerprint(&actor, &probe),
            before,
            "the value head changed a policy logit"
        );

        // And the fingerprint must actually be sensitive, or the equality above proves nothing.
        *actor.shared_readout_mut() = actor.shared_readout().f_add_scalar(1.0).expect("add");
        assert_ne!(
            logit_fingerprint(&actor, &probe),
            before,
            "the fingerprint did not notice a changed readout"
        );
    }
}
