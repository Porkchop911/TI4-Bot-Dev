//! The actor on `ti4-infer`: the rollouts of training without libtorch in the forward pass
//! (`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`, section D2).
//!
//! [`InferModel::from_actor`] copies the tensors of an [`Actor`] into [`ti4_infer::Parts`] in
//! memory (no bundle on disk), packs them ([`ti4_infer::format::write`]) and loads the packed bytes
//! ([`ti4_infer::format::read`]). That is the same code path as a `.ti4m` file of the page, so the
//! rollouts play the bits that the hosts of the game would play with the same weights (on
//! [`ti4_infer::Backend::Avx2Fma`] within `1e-4 · max(1, |logit|)` of them).
//!
//! A model of training also carries the value readout of a shared critic
//! ([`ti4_infer::Model::with_value`]), so a rollout records `V(s)` without libtorch. A value head
//! is never in a `.ti4m`: the file is packed from [`ti4_infer::Parts`], which has none. An actor
//! with a separate critic gets no value head, and [`InferModel::value`] refuses.
//!
//! On [`ti4_infer::Backend::Avx2Vnni`] the actor is packed with int8 weights
//! ([`ti4_infer::Quant::Int8`], section H of the plan): the dense layers on AVX-VNNI, the bag on
//! int8 rows; the residual stream, the readout and the value readout stay f32. Its logits are not
//! those of the f32 model (the bound is in the plan), and PPO recomputes the probabilities on
//! libtorch as for the other backends.
//!
//! The model is `Send + Sync`: one `Arc<InferModel>` serves every rollout worker, and the next
//! update builds a fresh one from the updated actor.

use std::cell::RefCell;
use std::sync::Arc;

use ti4_policy::battle::BattlePredictor;
use ti4_policy::mlp::{Columns, HeadLayout, MlpError, ModelInfo, Scorer};

use crate::{Actor, ActorError, CriticInput, FactionRow};

/// The bound of the logits of a model on `backend` against the actor's on libtorch, relative to
/// the largest: `1e-4` for the f32 backends (each logit within `1e-4 · max(1, |logit|)`, one
/// rounding per term on `Avx2Fma`), and [`INT8_LOGIT_BOUND`] for `Avx2Vnni`. The tolerance of a
/// recorded log-probability is twice it times the largest logit, over the temperature.
#[must_use]
pub const fn logit_bound(backend: ti4_infer::Backend) -> f64 {
    if backend.is_int8() {
        INT8_LOGIT_BOUND
    } else {
        1e-4
    }
}

/// The bound of the logits of an int8 model (section H of the plan) on [`int8_gap`]: the logits
/// of a decision, each less the mean of the decision, within this times `max(1, the largest
/// |logit|)` of libtorch's. The softmax does not see a shift that every option shares, and an
/// int8 layer puts the error of what the options share into such a shift.
pub const INT8_LOGIT_BOUND: f64 = 0.2;

/// The bound of an int8 value against libtorch's, relative to `max(1, |value|)` (section H).
pub const INT8_VALUE_BOUND: f64 = 0.1;

/// The gap of the logits `int8` of a decision against libtorch's `tch` that
/// [`INT8_LOGIT_BOUND`] bounds: the largest `|(int8_i − mean) − (tch_i − mean)|`, over
/// `max(1, max_j |tch_j|)`.
#[must_use]
pub fn int8_gap(tch: &[f32], int8: &[f32]) -> f64 {
    let mean = |logits: &[f32]| {
        logits.iter().map(|&l| f64::from(l)).sum::<f64>() / logits.len().max(1) as f64
    };
    let (a, b) = (mean(tch), mean(int8));
    let scale = tch
        .iter()
        .fold(1.0f64, |scale, &l| scale.max(f64::from(l.abs())));
    tch.iter()
        .zip(int8)
        .map(|(&t, &q)| ((f64::from(q) - b) - (f64::from(t) - a)).abs())
        .fold(0.0, f64::max)
        / scale
}

/// The name of a model built from an actor in memory.
const NAME: &str = "training";

thread_local! {
    /// The working memory of the forward passes of this thread, reused across decisions.
    static SCRATCH: RefCell<ti4_infer::Scratch> = RefCell::new(ti4_infer::Scratch::new());
}

/// A [`ti4_infer::Model`] of the weights of an actor, as the model of the decision pipeline.
#[derive(Debug)]
pub struct InferModel {
    model: ti4_infer::Model,
    layout: HeadLayout,
    battle: Option<Arc<BattlePredictor>>,
}

fn not_usable(detail: impl std::fmt::Display) -> ActorError {
    ActorError::NotUsable {
        what: "the ti4-infer model",
        detail: detail.to_string(),
    }
}

/// The values of a tensor of the actor, on the CPU.
fn floats(tensor: &ti4_tensor::Tensor) -> Result<Vec<f32>, ActorError> {
    Ok(ti4_tensor::to_vec(
        &tensor.detach().to_device(ti4_tensor::Device::Cpu),
    )?)
}

/// The weights of `actor` as [`ti4_infer::Parts`], with `slots` (the vocabulary JSON of its
/// bundle) kept beside them. The source names no manifest (all zeros): the weights are not a
/// bundle on disk.
///
/// # Errors
/// [`ActorError::NotUsable`] for an actor that no bundle schema describes (a battle predictor on
/// the legacy heads, or one that does not serialize), and a tensor that does not convert.
pub fn parts_of(actor: &Actor, slots: &str) -> Result<ti4_infer::Parts, ActorError> {
    let (schema, projection_abi) = crate::bundle::schema_of(actor).map_err(not_usable)?;
    let battle = actor
        .battle_predictor()
        .map(|predictor| predictor.to_json().map_err(not_usable))
        .transpose()?;
    let blocks = actor
        .residual_tensors()
        .chunks_exact(4)
        .map(|block| {
            Ok(ti4_infer::BlockParts {
                w_in: floats(block[0].1)?,
                b_in: floats(block[1].1)?,
                w_out: floats(block[2].1)?,
                b_out: floats(block[3].1)?,
            })
        })
        .collect::<Result<Vec<_>, ActorError>>()?;
    let size = |value: i64| usize::try_from(value).map_err(not_usable);
    Ok(ti4_infer::Parts {
        width: size(actor.width())?,
        capacity: size(actor.capacity())?,
        heads: actor
            .head_names()
            .iter()
            .map(|head| (*head).to_owned())
            .collect(),
        w1: floats(actor.input())?,
        embedding: floats(actor.embedding())?,
        b1: floats(actor.b1())?,
        w2: floats(actor.hidden())?,
        b2: floats(actor.b2())?,
        blocks,
        w_shared: floats(actor.shared_readout())?,
        b_shared: floats(actor.b_shared())?,
        delta: floats(actor.delta())?,
        b_delta: floats(actor.b_delta())?,
        slots: slots.to_owned(),
        battle,
        source: ti4_infer::Source {
            manifest_sha256: "0".repeat(64),
            schema,
            projection_abi,
        },
    })
}

impl InferModel {
    /// The weights of `actor` on `backend` ([`ti4_infer::Backend::fastest`] for the rollouts of
    /// training), with the value readout when the actor's critic shares its trunk. `slots` is the
    /// vocabulary JSON of its bundle. On [`ti4_infer::Backend::Avx2Vnni`] the weights are packed
    /// in int8 ([`ti4_infer::Quant::Int8`]), on every other backend in f32.
    ///
    /// # Errors
    /// As [`parts_of`], and [`ActorError::NotUsable`] when the weights do not pack (a weight that is
    /// not finite) or the backend is not available on this CPU.
    pub fn from_actor(
        actor: &Actor,
        slots: &str,
        backend: ti4_infer::Backend,
    ) -> Result<Self, ActorError> {
        let parts = parts_of(actor, slots)?;
        let meta = ti4_infer::Meta {
            name: NAME.to_owned(),
            label: "Training".to_owned(),
            temperature: ti4_infer::format::DEFAULT_TEMPERATURE,
        };
        let quant = if backend.is_int8() {
            ti4_infer::Quant::Int8
        } else {
            ti4_infer::Quant::F32
        };
        let bytes = ti4_infer::format::write_as(&meta, &parts, quant).map_err(not_usable)?;
        drop(parts);
        let mut model = ti4_infer::format::read(&bytes)
            .and_then(|model| model.with_backend(backend))
            .map_err(not_usable)?;
        if actor.separate_critic().is_none() {
            let w = floats(actor.value_readout())?;
            let b = floats(actor.b_value())?
                .first()
                .copied()
                .ok_or_else(|| not_usable("the value bias is empty"))?;
            model = model.with_value(w, b).map_err(not_usable)?;
        }
        Ok(Self {
            model,
            layout: actor.head_layout(),
            battle: actor.battle_predictor().cloned(),
        })
    }

    /// The model of `ti4-infer`.
    #[must_use]
    pub const fn model(&self) -> &ti4_infer::Model {
        &self.model
    }

    /// `V(s)` of one position: [`Actor::value`] on `ti4-infer`, for an actor whose critic shares
    /// its trunk.
    ///
    /// # Errors
    /// [`ActorError::NotUsable`] for a model without the value head (a separate critic), a malformed
    /// input, or a value that is not finite.
    pub fn value(&self, critic: &CriticInput, row: FactionRow) -> Result<f64, ActorError> {
        let sparse = critic.sparse();
        let columns = sparse
            .columns
            .iter()
            .zip(&sparse.values)
            .map(|(column, value)| Ok((u32::try_from(*column).map_err(not_usable)?, *value)))
            .collect::<Result<Vec<_>, ActorError>>()?;
        if columns.is_empty() {
            return Err(ActorError::EmptyLegalSet);
        }
        let value = SCRATCH
            .with_borrow_mut(|scratch| self.model.value_with(scratch, row.index(), &columns))
            .map_err(not_usable)?;
        if !value.is_finite() {
            return Err(ActorError::NotUsable {
                what: "the value estimate",
                detail: format!("{value}"),
            });
        }
        Ok(f64::from(value))
    }
}

impl ModelInfo for InferModel {
    fn layout(&self) -> HeadLayout {
        self.layout
    }

    fn battle_predictor(&self) -> Option<&BattlePredictor> {
        self.battle.as_deref()
    }

    fn temperature(&self) -> f64 {
        f64::from(self.model.temperature())
    }
}

impl Scorer for InferModel {
    fn logits(
        &self,
        faction: usize,
        head: usize,
        options: &[Columns],
    ) -> Result<Vec<f32>, MlpError> {
        let mut out = Vec::with_capacity(options.len());
        SCRATCH
            .with_borrow_mut(|scratch| {
                self.model
                    .forward_with(scratch, faction, head, options, &mut out)
            })
            .map_err(|error| MlpError::Model(error.to_string()))?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SparseOption, Width};
    use ti4_tensor::{Kind, Tensor};

    /// An actor of the diplomacy heads with two residual blocks, every weight random (a fresh
    /// block's `W_out` is zero, and a zero delta would hide a wrong faction row).
    fn random_actor() -> Actor {
        tch::manual_seed(20_261_015);
        let mut actor = Actor::zeros_diplomacy(Width::W128, 2_048);
        actor.add_residual_blocks(2, 7);
        let mut tensors: Vec<Tensor> = [
            &actor.input,
            &actor.embedding,
            &actor.b1,
            &actor.hidden,
            &actor.b2,
            &actor.w_shared,
            &actor.b_shared,
            &actor.delta,
            &actor.b_delta,
            &actor.w_value,
            &actor.b_value,
        ]
        .into_iter()
        .map(Tensor::shallow_clone)
        .collect();
        for block in &actor.blocks {
            tensors.extend(block.tensors().into_iter().map(Tensor::shallow_clone));
        }
        tch::no_grad(|| {
            for mut tensor in tensors {
                let noise = Tensor::randn(tensor.size(), (Kind::Float, tensor.device())) * 0.1;
                let _ = tensor.copy_(&noise);
            }
        });
        actor
    }

    /// Options of 1 to 60 columns, some named twice, from a small LCG.
    fn options(seed: u64, count: usize, capacity: u32) -> Vec<Columns> {
        let mut state = seed;
        let mut next = move || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            state >> 33
        };
        (0..count)
            .map(|_| {
                let columns = 1 + usize::try_from(next() % 60).unwrap_or(0);
                (0..columns)
                    .map(|_| {
                        let column = u32::try_from(next() % u64::from(capacity)).unwrap_or(0);
                        #[expect(clippy::cast_precision_loss, reason = "a small fixture value")]
                        let value = (next() % 1_000) as f32 / 250.0 - 2.0;
                        (column, value)
                    })
                    .collect()
            })
            .collect()
    }

    fn close(tch: f32, infer: f32) -> bool {
        f64::from((tch - infer).abs()) <= 1e-4 * f64::from(tch.abs()).max(1.0)
    }

    #[test]
    fn a_model_built_from_an_actor_gives_its_logits_and_values() {
        let actor = random_actor();
        for backend in [ti4_infer::Backend::Scalar, ti4_infer::Backend::fastest()] {
            let model = InferModel::from_actor(&actor, "{}", backend).expect("packs");
            assert_eq!(model.model().backend(), backend);
            assert_eq!(model.model().residual_blocks(), 2);
            assert!(model.model().has_value());
            for (seed, faction) in [(1, 0), (2, 7), (3, 27), (4, 32)] {
                let options = options(seed, 9, 2_048);
                for head in 0..actor.head_names().len() {
                    let want = Scorer::logits(&actor, faction, head, &options).expect("tch");
                    let got = Scorer::logits(&model, faction, head, &options).expect("infer");
                    assert_eq!(want.len(), got.len());
                    for (a, b) in want.iter().zip(&got) {
                        assert!(close(*a, *b), "{backend:?} head {head}: tch {a}, infer {b}");
                    }
                }
                let row = FactionRow::at(faction).expect("a roster row");
                for option in &options {
                    let critic = CriticInput::from_sparse(SparseOption::from_columns(option));
                    let want = actor.value(&critic, row).expect("tch value");
                    let got = model.value(&critic, row).expect("infer value");
                    #[expect(clippy::cast_possible_truncation, reason = "f32 values")]
                    let (want, got) = (want as f32, got as f32);
                    assert!(
                        close(want, got),
                        "{backend:?} value: tch {want}, infer {got}"
                    );
                }
            }
        }
    }

    /// The int8 model of the actor (on `Avx2Vnni`, where the CPU has it): every head's logits
    /// within [`INT8_LOGIT_BOUND`] on [`int8_gap`], and the value within [`INT8_VALUE_BOUND`].
    #[test]
    fn an_int8_model_built_from_an_actor_is_within_its_bound() {
        if !ti4_infer::Backend::Avx2Vnni.is_available() {
            eprintln!("skipped: this CPU has no avx2-vnni");
            return;
        }
        let actor = random_actor();
        let model = InferModel::from_actor(&actor, "{}", ti4_infer::Backend::Avx2Vnni)
            .expect("packs in int8");
        assert_eq!(model.model().quant(), ti4_infer::Quant::Int8);
        assert!(model.model().has_value());
        for (seed, faction) in [(1, 0), (2, 7), (3, 27)] {
            let options = options(seed, 9, 2_048);
            for head in 0..actor.head_names().len() {
                let want = Scorer::logits(&actor, faction, head, &options).expect("tch");
                let got = Scorer::logits(&model, faction, head, &options).expect("int8");
                let gap = int8_gap(&want, &got);
                assert!(
                    gap <= INT8_LOGIT_BOUND,
                    "head {head}: tch {want:?}, int8 {got:?}"
                );
            }
            let row = FactionRow::at(faction).expect("a roster row");
            for option in &options {
                let critic = CriticInput::from_sparse(SparseOption::from_columns(option));
                #[expect(clippy::cast_possible_truncation, reason = "f32 values")]
                let (want, got) = (
                    actor.value(&critic, row).expect("tch value") as f32,
                    model.value(&critic, row).expect("int8 value") as f32,
                );
                assert!(
                    f64::from((want - got).abs())
                        <= INT8_VALUE_BOUND * f64::from(want.abs()).max(1.0),
                    "value: tch {want}, int8 {got}"
                );
            }
        }
    }

    #[test]
    fn a_separate_critic_gives_no_value_head() {
        let mut actor = random_actor();
        let zeros = |shape: &[i64]| Tensor::zeros(shape, (Kind::Float, ti4_tensor::Device::Cpu));
        actor.set_separate_critic(Some(crate::SeparateCritic::new(
            zeros(&[2_048, 128]),
            zeros(&[128]),
            zeros(&[128, 128]),
            zeros(&[128]),
            zeros(&[128]),
            zeros(&[1]),
        )));
        let model =
            InferModel::from_actor(&actor, "{}", ti4_infer::Backend::Scalar).expect("packs");
        assert!(!model.model().has_value());
        let critic = CriticInput::from_sparse(SparseOption::from_columns(&[(3, 1.0)]));
        assert!(
            model
                .value(&critic, FactionRow::at(0).expect("a row"))
                .is_err()
        );
    }
}
