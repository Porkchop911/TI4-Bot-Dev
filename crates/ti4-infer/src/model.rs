//! [`Model`]: the sections of one `.ti4m` file and the forward pass over them.
//!
//! # The order of every sum
//!
//! - The bag: each element starts at zero and adds `v · W1[column]` over the option's columns,
//!   strictly increasing after duplicates are folded ([`crate::total_order_key`]); then the identity
//!   row is added, then `b1`, then the ReLU.
//! - A dense layer: each output starts at zero and adds `x[k] · W[out][k]` for `k = 0, 1, …`; then
//!   `+ b` and the ReLU, or for the outer layer of a residual block `(z + sum) + b_out`.
//! - The readout: four partial sums, lane `l` over the columns `4i + l` in order, combined as
//!   `(l0 + l1) + (l2 + l3)`, then `+ b`.
//!
//! The ReLU is `x > 0 ? x : 0`, and no multiply is fused into an add, on every [`Backend`] but
//! [`Backend::Avx2Fma`].
//!
//! # The backend
//!
//! A model is loaded on [`Backend::best`] (the CPU is asked once, then), and
//! [`Model::with_backend`] picks another. A forward pass matches the backend once and runs the
//! forward monomorphized for it.

use std::ops::Range;

use crate::format::Source;
use crate::kernel::{self, Epilogue};
use crate::simd::{self, AlignedF32, Backend, Lanes, Run};
use crate::{EMBED_DIM, FACTION_ROSTER, InferError};

/// The ranges of one residual block in the buffer of a model.
#[derive(Debug, Clone)]
pub(crate) struct BlockRanges {
    pub(crate) w_in: Range<usize>,
    pub(crate) b_in: Range<usize>,
    pub(crate) w_out: Range<usize>,
    pub(crate) b_out: Range<usize>,
}

/// A loaded model. Read-only after load, so one `Arc<Model>` serves every seat of every game.
#[derive(Debug)]
pub struct Model {
    pub(crate) name: String,
    pub(crate) label: String,
    pub(crate) temperature: f32,
    pub(crate) width: usize,
    pub(crate) capacity: usize,
    pub(crate) heads: Vec<String>,
    pub(crate) source: Source,
    pub(crate) digest: String,
    /// Every f32 section, one copy of the file.
    pub(crate) buffer: AlignedF32,
    /// `[capacity][width]`.
    pub(crate) w1: Range<usize>,
    /// `[33][16]`.
    pub(crate) embedding: Range<usize>,
    pub(crate) b1: Range<usize>,
    pub(crate) b2: Range<usize>,
    /// Packed panels of `W2 [width][width]`.
    pub(crate) w2: Range<usize>,
    pub(crate) blocks: Vec<BlockRanges>,
    /// `[33][heads][width]`.
    pub(crate) readout: Range<usize>,
    /// `[33][heads]`.
    pub(crate) readout_b: Range<usize>,
    pub(crate) slots: String,
    pub(crate) battle: Option<String>,
    /// Available on this CPU: checked by [`Model::with_backend`].
    pub(crate) backend: Backend,
    /// The value head of training ([`Model::with_value`]): never read from or written to a file.
    pub(crate) value: Option<ValueHead>,
}

/// The value readout of a shared critic: `V = z · w + b` on the trunk of the policy.
#[derive(Debug)]
pub(crate) struct ValueHead {
    /// `[width]`.
    w: Vec<f32>,
    b: f32,
}

/// Which readout a forward pass ends with.
#[derive(Debug, Clone, Copy)]
enum Target {
    /// A policy head row.
    Head(usize),
    /// The value head of training.
    Value,
}

/// The working memory of a forward pass. Reuse one per thread to avoid allocating per decision.
#[derive(Debug, Default)]
pub struct Scratch {
    entries: Vec<(usize, f32)>,
    sort: Vec<u64>,
    identity: Vec<f32>,
    z: Vec<f32>,
    h: Vec<f32>,
    t: Vec<f32>,
    packed: Vec<f32>,
}

impl Scratch {
    /// Empty scratch; it grows to the largest decision it has seen.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Model {
    /// Reads a `.ti4m` file: [`crate::format::read`].
    ///
    /// # Errors
    /// As [`crate::format::read`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, InferError> {
        crate::format::read(bytes)
    }

    /// The model on `backend` in place of [`Backend::best`], on which it was loaded:
    /// [`Backend::Avx2Fma`] where no other host must reproduce its logits, or [`Backend::Scalar`]
    /// for a comparison.
    ///
    /// # Errors
    /// [`InferError::Backend`] when this target or this CPU does not have it
    /// ([`Backend::is_available`]).
    pub fn with_backend(mut self, backend: Backend) -> Result<Self, InferError> {
        if !backend.is_available() {
            return Err(InferError::Backend(backend));
        }
        self.backend = backend;
        Ok(self)
    }

    /// The model with the value readout of a shared critic, `V(s) = trunk(s) · w + b`, for the
    /// rollouts of training (`ti4-mlp`), which record a value at every decision. A model file has
    /// no value head: [`crate::format::write`] packs [`crate::Parts`], which has none, and
    /// [`crate::format::read`] gives a model without one.
    ///
    /// # Errors
    /// [`InferError::Parts`] when `w` is not `[width]` or a weight is not finite.
    pub fn with_value(mut self, w: Vec<f32>, b: f32) -> Result<Self, InferError> {
        if w.len() != self.width {
            return Err(InferError::Parts(format!(
                "the value readout has {} floats, the width is {}",
                w.len(),
                self.width
            )));
        }
        if let Some(value) = w.iter().chain([&b]).find(|value| !value.is_finite()) {
            return Err(InferError::Parts(format!(
                "the value readout holds {value}"
            )));
        }
        self.value = Some(ValueHead { w, b });
        Ok(self)
    }

    /// Whether the model has the value head of training ([`Self::with_value`]).
    #[must_use]
    pub const fn has_value(&self) -> bool {
        self.value.is_some()
    }

    /// `V(s)` of one position of the faction row `faction`: its critic columns through the trunk,
    /// then the value readout ([`Self::with_value`]), with the order of every sum of a logit.
    ///
    /// # Errors
    /// [`InferError::NoValue`] for a model without the value head, [`InferError::Faction`], or a
    /// malformed column list.
    pub fn value_with(
        &self,
        scratch: &mut Scratch,
        faction: usize,
        columns: &[(u32, f32)],
    ) -> Result<f32, InferError> {
        let mut out = Vec::with_capacity(1);
        let position = [columns.to_vec()];
        self.run(
            self.backend,
            scratch,
            faction,
            Target::Value,
            &position,
            &mut out,
        )?;
        Ok(out[0])
    }

    /// The backend its forward pass runs on.
    #[must_use]
    pub const fn backend(&self) -> Backend {
        self.backend
    }

    /// The name of the model, the `<name>` of the bot kind `mlp:<name>`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The label shown in the lobby.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The softmax temperature a bot plays this model at.
    #[must_use]
    pub const fn temperature(&self) -> f32 {
        self.temperature
    }

    /// The trunk width.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }

    /// The rows of the input table.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// The readout heads, in the order of their rows.
    #[must_use]
    pub fn heads(&self) -> &[String] {
        &self.heads
    }

    /// The row of a head.
    #[must_use]
    pub fn head_index(&self, name: &str) -> Option<usize> {
        self.heads.iter().position(|head| head == name)
    }

    /// The faction roster, in the order of its rows: [`FACTION_ROSTER`].
    #[must_use]
    pub const fn factions(&self) -> &'static [&'static str] {
        &FACTION_ROSTER
    }

    /// The row of a faction.
    #[must_use]
    pub fn faction_index(&self, alias: &str) -> Option<usize> {
        FACTION_ROSTER.iter().position(|known| *known == alias)
    }

    /// How many residual blocks follow the two-layer trunk.
    #[must_use]
    pub fn residual_blocks(&self) -> usize {
        self.blocks.len()
    }

    /// The bundle the model was packed from.
    #[must_use]
    pub const fn source(&self) -> &Source {
        &self.source
    }

    /// The hexadecimal SHA-256 trailer of the file: the digest of every byte before it.
    #[must_use]
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// The vocabulary of the bundle (`slots.json`), verbatim.
    #[must_use]
    pub fn slots_json(&self) -> &str {
        &self.slots
    }

    /// The battle predictor of the bundle (`battle_predictor.json`, schemas 11 and 12), verbatim.
    #[must_use]
    pub fn battle_json(&self) -> Option<&str> {
        self.battle.as_deref()
    }

    fn f(&self, range: &Range<usize>) -> &[f32] {
        &self.buffer.as_slice()[range.clone()]
    }

    /// One logit per option, for a decision of the faction row `faction` on the head row `head`.
    ///
    /// Each option is its active `(column, value)` pairs, as the decision pipeline of
    /// `ti4_policy::mlp` makes them (`Columns`). A column may be named more than once; its values
    /// are summed in a fixed order before the bag. The type is concrete, not generic, so that the
    /// forward pass is compiled in this crate with its own optimization.
    ///
    /// # Errors
    /// [`InferError::EmptyLegalSet`], [`InferError::Faction`], [`InferError::Head`], or a malformed
    /// option ([`InferError::OutOfRange`], [`InferError::NotFinite`]).
    pub fn forward(
        &self,
        faction: usize,
        head: usize,
        options: &[Vec<(u32, f32)>],
    ) -> Result<Vec<f32>, InferError> {
        let mut out = Vec::with_capacity(options.len());
        self.forward_with(&mut Scratch::new(), faction, head, options, &mut out)?;
        Ok(out)
    }

    /// [`Self::forward`] into `out` (cleared first), with reused scratch.
    ///
    /// # Errors
    /// As [`Self::forward`].
    pub fn forward_with(
        &self,
        scratch: &mut Scratch,
        faction: usize,
        head: usize,
        options: &[Vec<(u32, f32)>],
        out: &mut Vec<f32>,
    ) -> Result<(), InferError> {
        self.forward_on(self.backend, scratch, faction, head, options, out)
    }

    /// [`Self::forward_with`] on `backend`, which must be available.
    pub(crate) fn forward_on(
        &self,
        backend: Backend,
        scratch: &mut Scratch,
        faction: usize,
        head: usize,
        options: &[Vec<(u32, f32)>],
        out: &mut Vec<f32>,
    ) -> Result<(), InferError> {
        self.run(backend, scratch, faction, Target::Head(head), options, out)
    }

    fn run(
        &self,
        backend: Backend,
        scratch: &mut Scratch,
        faction: usize,
        target: Target,
        options: &[Vec<(u32, f32)>],
        out: &mut Vec<f32>,
    ) -> Result<(), InferError> {
        simd::run(
            backend,
            Forward {
                model: self,
                scratch,
                faction,
                target,
                options,
                out,
            },
        )
    }

    /// The forward pass on the lanes `L`. Inlined, with the kernels, into the function of
    /// [`simd::run`] that calls it, so that it is compiled with the target features of `L`.
    #[inline(always)]
    fn forward_lanes<L: Lanes>(
        &self,
        scratch: &mut Scratch,
        faction: usize,
        target: Target,
        options: &[Vec<(u32, f32)>],
        out: &mut Vec<f32>,
    ) -> Result<(), InferError> {
        out.clear();
        if options.is_empty() {
            return Err(InferError::EmptyLegalSet);
        }
        if faction >= FACTION_ROSTER.len() {
            return Err(InferError::Faction(faction));
        }
        match target {
            Target::Head(head) if head >= self.heads.len() => return Err(InferError::Head(head)),
            Target::Value if self.value.is_none() => return Err(InferError::NoValue),
            _ => {}
        }
        let width = self.width;
        let rows = options.len();
        let Scratch {
            entries,
            sort,
            identity,
            z,
            h,
            t,
            packed,
        } = scratch;

        identity.clear();
        identity.resize(width, 0.0);
        identity[..EMBED_DIM]
            .copy_from_slice(&self.f(&self.embedding)[faction * EMBED_DIM..][..EMBED_DIM]);

        z.resize(rows * width, 0.0);
        let (w1, b1) = (self.f(&self.w1), self.f(&self.b1));
        for (option, row) in options.iter().zip(z.chunks_exact_mut(width)) {
            kernel::canonical_bag(option, self.capacity, entries, sort)?;
            kernel::bag::<L>(w1, width, entries, identity, b1, row);
        }

        h.resize(rows * width, 0.0);
        let b2 = self.f(&self.b2);
        kernel::gemm::<L>(
            z,
            rows,
            width,
            self.f(&self.w2),
            width,
            Epilogue::BiasRelu(b2),
            packed,
            h,
        );
        std::mem::swap(z, h);

        t.resize(rows * width, 0.0);
        for block in &self.blocks {
            let b_in = self.f(&block.b_in);
            kernel::gemm::<L>(
                z,
                rows,
                width,
                self.f(&block.w_in),
                width,
                Epilogue::BiasRelu(b_in),
                packed,
                h,
            );
            let bias = self.f(&block.b_out);
            let epilogue = Epilogue::Residual { z, bias };
            kernel::gemm::<L>(
                h,
                rows,
                width,
                self.f(&block.w_out),
                width,
                epilogue,
                packed,
                t,
            );
            std::mem::swap(z, t);
        }

        let (w, b) = match (target, &self.value) {
            (Target::Value, Some(value)) => (value.w.as_slice(), value.b),
            (Target::Head(head), _) => {
                let at = faction * self.heads.len() + head;
                (
                    &self.f(&self.readout)[at * width..][..width],
                    self.f(&self.readout_b)[at],
                )
            }
            (Target::Value, None) => return Err(InferError::NoValue),
        };
        kernel::readout::<L>(&z[..rows * width], width, w, b, out);
        Ok(())
    }
}

/// One forward pass, as the work of [`simd::run`].
struct Forward<'a> {
    model: &'a Model,
    scratch: &'a mut Scratch,
    faction: usize,
    target: Target,
    options: &'a [Vec<(u32, f32)>],
    out: &'a mut Vec<f32>,
}

impl Run for Forward<'_> {
    type Output = Result<(), InferError>;

    #[inline(always)]
    fn run<L: Lanes>(self) -> Self::Output {
        let Self {
            model,
            scratch,
            faction,
            target,
            options,
            out,
        } = self;
        model.forward_lanes::<L>(scratch, faction, target, options, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_model_is_shared_across_threads() {
        fn shared<T: Send + Sync>() {}
        shared::<Model>();
    }

    #[test]
    fn the_value_head_is_a_readout_on_the_trunk_of_the_policy() {
        let (width, capacity, faction, head) = (128, 300, 5, 3);
        let mut parts = crate::synthetic::parts(11, width, capacity, 2);
        // A head whose readout for `faction` is the value readout: no delta, so the packed row is
        // `w_shared[head]` itself, and the logit of one option is the value of that position.
        let heads = parts.heads.len();
        parts.delta[(faction * heads + head) * width..][..width].fill(0.0);
        parts.b_delta[faction * heads + head] = 0.0;
        let w = parts.w_shared[head * width..][..width].to_vec();
        let b = parts.b_shared[head];
        let bytes = crate::format::write(&crate::synthetic::meta(), &parts).expect("packs");
        let model = Model::from_bytes(&bytes).expect("loads");
        assert!(!model.has_value());
        let mut scratch = Scratch::new();
        let options = crate::synthetic::options(3, 7, capacity, 40);
        assert!(matches!(
            model.value_with(&mut scratch, faction, &options[1]),
            Err(InferError::NoValue)
        ));
        assert!(matches!(
            model.with_value(vec![0.0; width - 8], 0.0),
            Err(InferError::Parts(_))
        ));
        let model = Model::from_bytes(&bytes)
            .expect("loads")
            .with_value(w, b)
            .expect("a value head");
        assert!(model.has_value());
        let logits = model.forward(faction, head, &options).expect("logits");
        for (option, logit) in options.iter().zip(&logits) {
            let value = model
                .value_with(&mut scratch, faction, option)
                .expect("a value");
            assert_eq!(value.to_bits(), logit.to_bits());
        }
    }
}
