//! Models and decisions made from a seed, with the same bits on every target.
//!
//! The tests, the bench and the check of the wasm build need a model without a training bundle.
//! The weights come from SplitMix64 and exact float arithmetic, so a model of one seed is the same
//! file (the same [`Model::digest`]) on x86-64 and in wasm, and [`check`] gives the logits that
//! the native test writes to `fixtures/infer/synthetic_logits.json` and the wasm build must match.

use serde_json::{Value, json};

use crate::format::{self, BlockParts, Meta, Parts, Source};
use crate::{EMBED_DIM, FACTION_ROSTER, LEGACY_HEADS, Model, Scratch, SparseOption};

/// SplitMix64: a small generator whose output is defined bit for bit.
#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    /// The generator of `seed`.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next 64 bits.
    pub const fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `0..bound` (`bound > 0`), by remainder.
    pub fn below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound as u64) as usize
    }

    /// A float in `[0, 1)`: 24 random bits times 2⁻²⁴, which is exact.
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 * (1.0 / 16_777_216.0)
    }

    /// A float in `[-bound, bound)`.
    pub fn uniform(&mut self, bound: f32) -> f32 {
        (self.unit() * 2.0 - 1.0) * bound
    }

    fn floats(&mut self, count: usize, bound: f32) -> Vec<f32> {
        (0..count).map(|_| self.uniform(bound)).collect()
    }
}

/// The name, label and temperature of a synthetic model.
#[must_use]
pub fn meta() -> Meta {
    Meta {
        name: "synthetic".to_owned(),
        label: "Synthetic".to_owned(),
        temperature: 0.25,
    }
}

/// A model of the legacy heads with weights of `seed`, of the residual schema 8 (any number of
/// blocks is packed).
#[must_use]
pub fn parts(seed: u64, width: usize, capacity: usize, blocks: usize) -> Parts {
    let mut rng = Rng::new(seed);
    let heads = LEGACY_HEADS.len();
    let factions = FACTION_ROSTER.len();
    // 1/√width for the widths of a model, which are powers of two times 8; any width works.
    let scale = 1.0 / (width as f32).sqrt();
    Parts {
        width,
        capacity,
        heads: LEGACY_HEADS.iter().map(|head| (*head).to_owned()).collect(),
        w1: rng.floats(capacity * width, 0.5),
        embedding: rng.floats(factions * EMBED_DIM, 0.5),
        b1: rng.floats(width, 0.2),
        w2: rng.floats(width * width, 2.0 * scale),
        b2: rng.floats(width, 0.2),
        blocks: (0..blocks)
            .map(|_| BlockParts {
                w_in: rng.floats(width * width, 2.0 * scale),
                b_in: rng.floats(width, 0.2),
                w_out: rng.floats(width * width, scale),
                b_out: rng.floats(width, 0.2),
            })
            .collect(),
        w_shared: rng.floats(heads * width, scale),
        b_shared: rng.floats(heads, 0.5),
        delta: rng.floats(factions * heads * width, 0.2 * scale),
        b_delta: rng.floats(factions * heads, 0.1),
        slots: r#"{"slots":[],"capacity":0}"#.to_owned(),
        battle: None,
        source: Source {
            manifest_sha256: "ab".repeat(32),
            schema: 8,
            projection_abi: 2,
        },
    }
}

/// The options of a decision from `seed`, over a table of `capacity > 17` rows. Of every five, one
/// is empty, one has sorted columns, and three have 1 to `columns` random columns with up to four
/// of them named twice, so the bag sorts and folds them.
#[must_use]
pub fn options(seed: u64, count: usize, capacity: usize, columns: usize) -> Vec<SparseOption> {
    let mut rng = Rng::new(seed);
    let last = i64::try_from(capacity - 1).expect("a capacity fits i64");
    (0..count)
        .map(|index| match index % 5 {
            1 => SparseOption::default(),
            3 => SparseOption {
                columns: vec![0, 3, 17, last],
                values: vec![1.0, 0.5, -2.0, 1.0],
            },
            _ => {
                let len = 1 + rng.below(columns.max(1));
                let mut picked: Vec<i64> = (0..len)
                    .map(|_| i64::try_from(rng.below(capacity)).expect("a row fits i64"))
                    .collect();
                picked.extend_from_within(..len.min(4));
                let values = (0..picked.len())
                    .map(|_| (rng.below(7) as f32 - 3.0) * 0.5 + rng.uniform(0.1))
                    .collect();
                SparseOption {
                    columns: picked,
                    values,
                }
            }
        })
        .collect()
}

fn field(value: &Value, name: &str) -> Result<u64, String> {
    value[name]
        .as_u64()
        .ok_or_else(|| format!("the check request needs the number `{name}`"))
}

fn size(value: &Value, name: &str) -> Result<usize, String> {
    usize::try_from(field(value, name)?).map_err(|_| format!("`{name}` is too large"))
}

/// The logits of a synthetic model on some decisions, from a request in JSON:
///
/// ```json
/// {"seed": 11, "width": 128, "capacity": 500, "blocks": 2,
///  "cases": [{"seed": 1, "options": 9, "columns": 40, "faction": 7, "head": 4}]}
/// ```
///
/// The model is [`parts`] of `seed`, written and read again as a file; the options of a case are
/// [`options`] of its `seed`. The answer is
/// `{"backend": "sse" | "simd128" | "scalar", "digest": <the file's sha>, "logits": [[bits]]}`,
/// each logit as the `u32` of its bits, one list per case.
///
/// # Errors
/// A request that is not of this shape, a model the writer refuses, or a refused forward, as text.
pub fn check(request: &str) -> Result<String, String> {
    let request: Value =
        serde_json::from_str(request).map_err(|error| format!("the check request: {error}"))?;
    let (width, capacity) = (size(&request, "width")?, size(&request, "capacity")?);
    if capacity <= 17 {
        return Err("the check request needs a capacity above 17".to_owned());
    }
    let parts = parts(
        field(&request, "seed")?,
        width,
        capacity,
        size(&request, "blocks")?,
    );
    let bytes = format::write(&meta(), &parts).map_err(|error| error.to_string())?;
    let model = Model::from_bytes(&bytes).map_err(|error| error.to_string())?;
    let cases = request["cases"]
        .as_array()
        .ok_or("the check request needs the list `cases`")?;
    let mut scratch = Scratch::new();
    let mut out = Vec::new();
    let mut logits = Vec::with_capacity(cases.len());
    for case in cases {
        let options = options(
            field(case, "seed")?,
            size(case, "options")?,
            capacity,
            size(case, "columns")?,
        );
        model
            .forward_with(
                &mut scratch,
                size(case, "faction")?,
                size(case, "head")?,
                &options,
                &mut out,
            )
            .map_err(|error| error.to_string())?;
        logits.push(out.iter().map(|logit| logit.to_bits()).collect::<Vec<_>>());
    }
    Ok(json!({
        "backend": crate::backend(),
        "digest": model.digest(),
        "logits": logits,
    })
    .to_string())
}
