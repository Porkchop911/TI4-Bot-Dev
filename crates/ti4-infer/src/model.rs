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
//! The ReLU is `x > 0 ? x : 0`, and no multiply is fused into an add.

use std::ops::Range;

use crate::format::Source;
use crate::kernel::{self, Epilogue};
use crate::simd::{AlignedF32, Lanes, Native};
use crate::{EMBED_DIM, FACTION_ROSTER, InferError};

/// One option's active columns and their values: `ti4_mlp::SparseOption`.
///
/// A column may be named more than once; its values are summed in a fixed order before the bag.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SparseOption {
    /// Rows of the input table.
    pub columns: Vec<i64>,
    /// Their values, positionally matched.
    pub values: Vec<f32>,
}

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
}

/// The working memory of a forward pass. Reuse one per thread to avoid allocating per decision.
#[derive(Debug, Default)]
pub struct Scratch {
    entries: Vec<(usize, f32)>,
    sort: Vec<(i64, f32)>,
    identity: Vec<f32>,
    z: Vec<f32>,
    h: Vec<f32>,
    t: Vec<f32>,
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
    /// # Errors
    /// [`InferError::EmptyLegalSet`], [`InferError::Faction`], [`InferError::Head`], or a malformed
    /// option ([`InferError::Ragged`], [`InferError::OutOfRange`], [`InferError::NotFinite`]).
    pub fn forward(
        &self,
        faction: usize,
        head: usize,
        options: &[SparseOption],
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
        options: &[SparseOption],
        out: &mut Vec<f32>,
    ) -> Result<(), InferError> {
        self.forward_lanes::<Native>(scratch, faction, head, options, out)
    }

    /// The forward pass on the backend `L`.
    pub(crate) fn forward_lanes<L: Lanes>(
        &self,
        scratch: &mut Scratch,
        faction: usize,
        head: usize,
        options: &[SparseOption],
        out: &mut Vec<f32>,
    ) -> Result<(), InferError> {
        out.clear();
        if options.is_empty() {
            return Err(InferError::EmptyLegalSet);
        }
        if faction >= FACTION_ROSTER.len() {
            return Err(InferError::Faction(faction));
        }
        if head >= self.heads.len() {
            return Err(InferError::Head(head));
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
        } = scratch;

        identity.clear();
        identity.resize(width, 0.0);
        identity[..EMBED_DIM]
            .copy_from_slice(&self.f(&self.embedding)[faction * EMBED_DIM..][..EMBED_DIM]);

        z.resize(rows * width, 0.0);
        let (w1, b1) = (self.f(&self.w1), self.f(&self.b1));
        for (option, row) in options.iter().zip(z.chunks_exact_mut(width)) {
            kernel::canonical_bag(
                &option.columns,
                &option.values,
                self.capacity,
                entries,
                sort,
            )?;
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
                h,
            );
            let bias = self.f(&block.b_out);
            let epilogue = Epilogue::Residual { z, bias };
            kernel::gemm::<L>(h, rows, width, self.f(&block.w_out), width, epilogue, t);
            std::mem::swap(z, t);
        }

        let at = faction * self.heads.len() + head;
        let w = &self.f(&self.readout)[at * width..][..width];
        let b = self.f(&self.readout_b)[at];
        kernel::readout::<L>(&z[..rows * width], width, w, b, out);
        Ok(())
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
}
