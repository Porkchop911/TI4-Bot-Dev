//! The MLP policy without libtorch (`web2/plans/2026-10-15-MLP_INFERENCE_PLAN.md`).
//!
//! A trained bundle (`ti4-mlp/src/bundle.rs`) is checked and packed once into one `.ti4m` file
//! ([`bundle::read`], [`format::write`], the `ti4-pack` tool). The server and the page read only
//! that file ([`format::read`]) and run [`Model::forward`] on hand-written kernels.
//!
//! # The forward pass
//!
//! For the options of one decision by the faction `f` on the head `h`:
//!
//! ```text
//! z1 = relu(Σ_j v_j · W1[col_j] + identity(f) + b1)     the bag; identity = embedding[f] zero-padded
//! z  = relu(W2 · z1 + b2)
//! z  = z + W_out · relu(W_in · z + b_in) + b_out        for each residual block
//! s  = z · readout[f][h] + readout_b[f][h]              readout = w_shared[h] + delta[f][h]
//! ```
//!
//! This is `Actor::trunk` and `Actor::logits` of `ti4-mlp`. The order of every sum is fixed (see
//! [`model`]), so a backend gives the same bits on every target.
//!
//! # Modules
//!
//! - [`bundle`]: the reader of a training bundle directory, with its own `safetensors` parser.
//! - [`format`]: the `.ti4m` file, writer and reader.
//! - [`model`]: [`Model`] and its forward pass.
//! - `kernel`: the bag, the packed-panel matrix product and the readout, generic over the lanes.
//! - `simd`: the `Lanes` trait and its backends. The only module that may use `unsafe`.

pub mod bundle;
mod error;
pub mod format;
mod kernel;
pub mod model;
mod simd;

pub use error::InferError;
pub use format::{BlockParts, Meta, Parts, Source};
pub use kernel::total_order_key;
pub use model::{Model, Scratch, SparseOption};

/// The dimension of the faction identity embedding. Its row is zero-padded to the trunk width and
/// added to the first-layer preactivation.
pub const EMBED_DIM: usize = 16;

/// The most residual blocks a model may have (the bound of the bundle reader of `ti4-mlp`).
pub const MAX_RESIDUAL_BLOCKS: usize = 8;

/// The largest input table a model may have (`slot_capacity` of a bundle).
pub const MAX_CAPACITY: usize = 65_536;

/// The pinned roster of faction identities, in the order of their rows: `ti4_mlp::FACTION_ROSTER`.
/// A model file and a bundle carry their roster, and both readers refuse one that differs.
pub const FACTION_ROSTER: [&str; 33] = [
    "arborec",
    "argent",
    "bastion",
    "cabal",
    "crimson",
    "deepwrought",
    "empyrean",
    "firmament",
    "ghost",
    "hacan",
    "jolnar",
    "keleresa",
    "keleresm",
    "keleresx",
    "l1z1x",
    "letnev",
    "mahact",
    "mentak",
    "muaat",
    "naalu",
    "naaz",
    "nekro",
    "nomad",
    "obsidian",
    "ralnel",
    "saar",
    "sardakk",
    "sol",
    "titans",
    "winnu",
    "xxcha",
    "yin",
    "yssaril",
];

/// The readout heads of schemas 7 and 8: `ti4_policy::learned::STAGE1_DECISION_HEADS`.
pub const LEGACY_HEADS: [&str; 14] = [
    "strategy",
    "secondary",
    "turn",
    "activation",
    "movement",
    "cargo",
    "landing",
    "trade",
    "tokens",
    "production",
    "payment",
    "development",
    "combat",
    "other",
];

/// The readout heads of schemas 9 to 12: `ti4_policy::learned::DIPLOMACY_DECISION_HEADS`, the
/// legacy heads and `diplomacy` appended.
pub const DIPLOMACY_HEADS: [&str; 15] = [
    "strategy",
    "secondary",
    "turn",
    "activation",
    "movement",
    "cargo",
    "landing",
    "trade",
    "tokens",
    "production",
    "payment",
    "development",
    "combat",
    "other",
    "diplomacy",
];

/// Whether a head list is one of the two layouts a model may have.
fn is_head_layout(heads: &[String]) -> bool {
    let same = |layout: &[&str]| {
        heads.len() == layout.len() && heads.iter().zip(layout).all(|(a, b)| a == b)
    };
    same(&LEGACY_HEADS) || same(&DIPLOMACY_HEADS)
}

/// The hexadecimal SHA-256 of some bytes.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    hex(&sha2::Sha256::digest(bytes))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut text, byte| {
        let _ = write!(text, "{byte:02x}");
        text
    })
}

fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|byte| byte.is_ascii_hexdigit())
}
