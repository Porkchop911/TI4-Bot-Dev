//! The batched MLP actor (M09-026), per MLP plan §§4.2–4.3.
//!
//! # The model
//!
//! ```text
//! per legal option i, for a decision by faction f on head h:
//!   z_i = trunk( x_policy(s, o_i, f) )
//!   w_i = w_shared[h] + delta[f, h]
//!   s_i = w_i · z_i + b_shared[h] + b_delta[f, h]
//!   p   = softmax(s / temperature)
//!
//!   trunk(x) = relu(W2 · relu(W1 · x + b1) + b2)
//! ```
//!
//! `W1` is the `[V_cap, width]` input table and `W1 · x` is a **sparse gather**, not a matrix
//! product: a decision names around thirty active columns against sixteen thousand, so §4.3
//! requires an embedding-bag calculation rather than a materialised `[N, V_cap]` tensor.
//!
//! # Batched within a decision
//!
//! §4.3 is explicit that this is not optional: every option of one decision goes through the trunk
//! in a single pass, turning N small matrix-vector products into one `[N, width]` matmul. The
//! gather is still per option — that is what a sparse input is — but everything after it is batched.
//!
//! # Faction conditioning
//!
//! At the output only, in this package: `w_shared[h] + delta[f, h]`, with every faction residual
//! **zero-initialised**, so a faction absent from training uses the learned shared readout and a
//! zero residual rather than falling onto an untrained output row (§3, "The redundancy to watch").
//!
//! **The identity embedding is not implemented here, deliberately.** §4.2's parameter budget lists
//! one (16 × 33) and §3 says faction information enters "at the input (abilities + embedding)", but
//! §4.2's own formula has no embedding term and nothing in the plan says how a dim-16 vector joins
//! a width-256 input. The M09-026 row does not name it either. Inventing the wiring here would put
//! an unreviewed architectural choice under every later weight, so it is recorded as an open
//! question instead. The ability decomposition (M09-022) is what carries faction identity at the
//! input today.

pub mod bot;

use thiserror::Error;
use ti4_tensor::{Device, Kind, Tensor};

/// The trunk width. §4.2 admits exactly two, and the fallback to 128 is the only in-plan response
/// to the §7.1 throughput gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Width {
    /// The design width.
    W256,
    /// The pre-registered throughput fallback, and the only one.
    W128,
}

impl Width {
    /// The width as a dimension.
    #[must_use]
    pub const fn dim(self) -> i64 {
        match self {
            Self::W256 => 256,
            Self::W128 => 128,
        }
    }
}

/// Schema 4's fourteen decision heads, in their fixed order.
#[must_use]
pub fn heads() -> &'static [&'static str] {
    &ti4_policy::learned::STAGE1_DECISION_HEADS
}

/// Anything that stopped a forward pass.
#[derive(Debug, Error)]
pub enum ActorError {
    /// A head name the schema does not define.
    #[error("unknown head {0:?}")]
    UnknownHead(String),
    /// A faction index outside the allocated residual rows.
    #[error("faction {seat} is outside {factions} residual rows")]
    UnknownFaction { seat: usize, factions: usize },
    /// A sparse option vector was malformed.
    #[error(transparent)]
    Tensor(#[from] ti4_tensor::TensorError),
    /// Temperature must be positive: dividing logits by zero is not a softmax.
    #[error("temperature {0} is not positive")]
    Temperature(f64),
}

/// One option's active columns and their values.
///
/// Duplicates are permitted and are summed by the gather, in column order — a feature name can
/// legitimately be contributed twice, and a fixed order is what makes the sum reproducible.
#[derive(Debug, Clone, Default)]
pub struct SparseOption {
    /// Dense column indices.
    pub columns: Vec<i64>,
    /// Their values, positionally matched.
    pub values: Vec<f32>,
}

/// The actor: one shared trunk, one shared readout, and a thin per-faction residual.
#[derive(Debug)]
pub struct Actor {
    width: i64,
    capacity: i64,
    /// `W1`, the sparse input table. `[capacity, width]`.
    input: Tensor,
    b1: Tensor,
    /// `W2`, the hidden layer. `[width, width]`.
    hidden: Tensor,
    b2: Tensor,
    /// `[heads, width]`.
    w_shared: Tensor,
    /// `[heads]`.
    b_shared: Tensor,
    /// `[factions, heads, width]`, zero.
    delta: Tensor,
    /// `[factions, heads]`, zero.
    b_delta: Tensor,
}

impl Actor {
    /// A zero-initialised actor.
    ///
    /// Everything starts at zero here. §6.1 fixes the real initialisation — a pinned RNG domain and
    /// seed, with specific uniform ranges per block — and that belongs to the distillation package
    /// that consumes it, not to the architecture. What this constructor does guarantee is the part
    /// §6.1 also requires and that *is* architectural: **faction residuals and biases start at
    /// zero**, so an untrained faction contributes nothing rather than noise.
    ///
    /// # Panics
    /// If the head or faction counts do not fit an `i64`, which they cannot: there are fourteen
    /// heads and thirty-three seats.
    #[must_use]
    pub fn zeros(width: Width, capacity: i64, factions: usize) -> Self {
        let w = width.dim();
        let heads = i64::try_from(heads().len()).expect("fourteen heads");
        let factions_dim = i64::try_from(factions).expect("thirty-three seats");
        let opts = (Kind::Float, Device::Cpu);
        Self {
            width: w,
            capacity,
            input: Tensor::zeros([capacity, w], opts),
            b1: Tensor::zeros([w], opts),
            hidden: Tensor::zeros([w, w], opts),
            b2: Tensor::zeros([w], opts),
            w_shared: Tensor::zeros([heads, w], opts),
            b_shared: Tensor::zeros([heads], opts),
            delta: Tensor::zeros([factions_dim, heads, w], opts),
            b_delta: Tensor::zeros([factions_dim, heads], opts),
        }
    }

    /// The trunk width.
    #[must_use]
    pub const fn width(&self) -> i64 {
        self.width
    }

    /// Allocated input rows — `V_cap`.
    #[must_use]
    pub const fn capacity(&self) -> i64 {
        self.capacity
    }

    /// Mutable access to the input table, for an initialiser or a loader.
    pub const fn input_mut(&mut self) -> &mut Tensor {
        &mut self.input
    }

    /// The input table.
    pub const fn input(&self) -> &Tensor {
        &self.input
    }

    /// Mutable access to the hidden layer.
    pub const fn hidden_mut(&mut self) -> &mut Tensor {
        &mut self.hidden
    }

    /// Mutable access to the shared readout.
    pub const fn shared_readout_mut(&mut self) -> &mut Tensor {
        &mut self.w_shared
    }

    /// Mutable access to the per-faction residual.
    pub const fn residual_mut(&mut self) -> &mut Tensor {
        &mut self.delta
    }

    /// The schema-4 head that carries a requested head.
    ///
    /// `decision_head` names schema 5's nineteen; schema 4 carries fourteen and routes the later
    /// splits — `scoring`, `agenda`, `exploration`, `ability`, `transit` — to `other`, exactly as
    /// `Profile::resolved_head` does for the linear champions. Folding here rather than at each
    /// call site keeps one rule.
    #[must_use]
    pub fn resolve_head(requested: &str) -> &str {
        if heads().contains(&requested) {
            requested
        } else {
            "other"
        }
    }

    /// The index of a head by name.
    ///
    /// # Errors
    /// [`ActorError::UnknownHead`] if the schema does not define it.
    pub fn head_index(name: &str) -> Result<usize, ActorError> {
        heads()
            .iter()
            .position(|head| *head == name)
            .ok_or_else(|| ActorError::UnknownHead(name.to_owned()))
    }

    /// Every option of one decision through the trunk, in one pass.
    ///
    /// Returns `[n, width]`. The gather is per option because the input is sparse; the two dense
    /// stages that follow are one batched matmul each, which is the whole point of §4.3.
    ///
    /// # Errors
    /// Propagates a malformed sparse vector.
    pub fn trunk(&self, options: &[SparseOption]) -> Result<Tensor, ActorError> {
        if options.is_empty() {
            return Ok(Tensor::zeros([0, self.width], (Kind::Float, Device::Cpu)));
        }
        let mut gathered = Vec::with_capacity(options.len());
        for option in options {
            gathered.push(ti4_tensor::gather_reduce(
                &self.input,
                &option.columns,
                &option.values,
            )?);
        }
        let x = Tensor::stack(&gathered, 0);
        let first = (x + &self.b1).relu();
        let second = (first.matmul(&self.hidden.tr()) + &self.b2).relu();
        Ok(second)
    }

    /// Logits for one decision: `[n]`, one per option.
    ///
    /// # Errors
    /// [`ActorError::UnknownHead`], [`ActorError::UnknownFaction`], or a malformed sparse vector.
    ///
    /// # Panics
    /// If a validated head or seat index does not fit an `i64`. Both are bounded by the schema.
    pub fn logits(
        &self,
        options: &[SparseOption],
        head: &str,
        seat: usize,
    ) -> Result<Tensor, ActorError> {
        let head_index = Self::head_index(head)?;
        let factions = usize::try_from(self.delta.size()[0]).expect("non-negative");
        if seat >= factions {
            return Err(ActorError::UnknownFaction { seat, factions });
        }
        let z = self.trunk(options)?;
        let seat_i = i64::try_from(seat).expect("seat fits");
        let head_i = i64::try_from(head_index).expect("head fits");
        // w_effective[f,h] = w_shared[h] + delta[f,h] — the decomposition §3 fixes.
        let w = self.w_shared.get(head_i) + self.delta.get(seat_i).get(head_i);
        let b = self.b_shared.get(head_i) + self.b_delta.get(seat_i).get(head_i);
        Ok(z.matmul(&w) + b)
    }

    /// Probabilities over one decision's options.
    ///
    /// The max is subtracted before exponentiating. That is not a micro-optimisation: without it a
    /// logit around 90 overflows `f32` in `exp`, and the result is `NaN` for every option rather
    /// than a wrong one for some.
    ///
    /// # Errors
    /// [`ActorError::Temperature`] if the temperature is not positive, plus anything
    /// [`Self::logits`] raises.
    pub fn probabilities(
        &self,
        options: &[SparseOption],
        head: &str,
        seat: usize,
        temperature: f64,
    ) -> Result<Vec<f64>, ActorError> {
        if temperature.partial_cmp(&0.0) != Some(std::cmp::Ordering::Greater) {
            return Err(ActorError::Temperature(temperature));
        }
        if options.is_empty() {
            return Ok(Vec::new());
        }
        let scores = self.logits(options, head, seat)? / temperature;
        Ok(stable_softmax(&scores))
    }

    /// Whether every row from `slot_count` up, and every row named by `dead`, is exactly zero.
    ///
    /// M09-024a allocates capacity above the assigned columns and M09-024b1 retains five reserved
    /// rows that the projection can never route to. Both must stay zero and out of the optimizer;
    /// this is the assertion side of that obligation, for save/load and for tests.
    #[must_use]
    pub fn inactive_rows_are_zero(&self, slot_count: i64, dead: &[i64]) -> bool {
        let free_clean = if slot_count >= self.capacity {
            true
        } else {
            let free = self.input.narrow(0, slot_count, self.capacity - slot_count);
            free.abs().max().double_value(&[]) == 0.0
        };
        free_clean
            && dead.iter().all(|row| {
                *row < self.capacity && self.input.get(*row).abs().max().double_value(&[]) == 0.0
            })
    }
}

/// Softmax with the maximum subtracted first.
#[must_use]
pub fn stable_softmax(scores: &Tensor) -> Vec<f64> {
    let shifted = scores - scores.max();
    let weights = shifted.exp();
    let total = weights.sum(Kind::Float);
    let normalised = weights / total;
    ti4_tensor::to_vec_or_panic(&normalised)
        .into_iter()
        .map(f64::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: i64 = 20_260_821;
    const CAPACITY: i64 = 512;
    const FACTIONS: usize = 33;

    /// Reproducible pseudo-random weights that do **not** touch libtorch's RNG.
    ///
    /// `Tensor::rand` draws from a process-global generator, and cargo runs a binary's tests in
    /// parallel threads of one process — so two fixtures built from the same seed in different
    /// tests are not the same fixture, and a comparison between them fails for reasons that have
    /// nothing to do with the model. A pure function of `(row, column, salt)` has no such
    /// coupling.
    fn patterned(rows: i64, cols: i64, salt: u64) -> Tensor {
        let mut values = Vec::with_capacity(usize::try_from(rows * cols).expect("fixture fits"));
        for index in 0..(rows * cols) {
            // A small LCG, folded to [-0.5, 0.5).
            let mut state = u64::try_from(index)
                .expect("non-negative")
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(salt.wrapping_mul(1_442_695_040_888_963_407));
            state ^= state >> 33;
            state = state.wrapping_mul(0xff51_afd7_ed55_8ccd);
            state ^= state >> 29;
            #[expect(clippy::cast_precision_loss, reason = "fixture values only")]
            let unit = ((state >> 48) as f32) / f32::from(u16::MAX);
            values.push(unit - 0.5);
        }
        Tensor::from_slice(&values).view([rows, cols])
    }

    fn actor(width: Width) -> Actor {
        ti4_tensor::configure_deterministic(SEED).expect("configured");
        let mut actor = Actor::zeros(width, CAPACITY, FACTIONS);
        let w = width.dim();
        *actor.input_mut() = patterned(CAPACITY, w, 1);
        *actor.hidden_mut() = patterned(w, w, 2);
        *actor.shared_readout_mut() = patterned(i64::try_from(heads().len()).expect("small"), w, 3);
        actor
    }

    /// The same forward pass, computed densely: materialise `[n, V_cap]` and use a real matmul.
    /// §4.3 requires the sparse path to be tested against this.
    fn dense_trunk(actor: &Actor, options: &[SparseOption]) -> Tensor {
        let n = i64::try_from(options.len()).expect("small batch");
        let dense = Tensor::zeros([n, actor.capacity], (Kind::Float, Device::Cpu));
        for (row, option) in options.iter().enumerate() {
            for (column, value) in option.columns.iter().zip(option.values.iter()) {
                let mut cell = dense.get(i64::try_from(row).expect("small")).get(*column);
                let _ = cell.g_add_(&Tensor::from(*value));
            }
        }
        let x = dense.matmul(actor.input());
        let first = (x + &actor.b1).relu();
        (first.matmul(&actor.hidden.tr()) + &actor.b2).relu()
    }

    /// Agreement to f32 precision, relative to the magnitude being compared.
    ///
    /// A fixed absolute tolerance is wrong for a dense-versus-sparse comparison: the two paths sum
    /// the same terms in different groupings, so their disagreement scales with the values, not
    /// with a constant. f32 carries about seven significant digits.
    fn close(a: f32, b: f32) -> bool {
        let scale = a.abs().max(b.abs()).max(1.0);
        (a - b).abs() <= 1e-5 * scale
    }

    fn option(columns: &[i64], values: &[f32]) -> SparseOption {
        SparseOption {
            columns: columns.to_vec(),
            values: values.to_vec(),
        }
    }

    #[test]
    fn only_two_widths_exist() {
        assert_eq!(Width::W256.dim(), 256);
        assert_eq!(Width::W128.dim(), 128);
    }

    #[test]
    fn the_schema_four_head_set_is_fourteen() {
        assert_eq!(heads().len(), 14);
        assert_eq!(Actor::head_index("strategy").expect("known"), 0);
        assert_eq!(Actor::head_index("other").expect("known"), 13);
        assert!(matches!(
            Actor::head_index("scoring"),
            Err(ActorError::UnknownHead(_))
        ));
    }

    #[test]
    fn the_sparse_trunk_matches_a_dense_reference() {
        // §4.3's requirement. The awkward cases are the point: duplicated columns, negative and
        // fractional values, a column that is a free row (the OOV/unassigned case), and an option
        // with no active columns at all.
        for width in [Width::W256, Width::W128] {
            let actor = actor(width);
            let options = vec![
                option(&[3, 17, 200], &[1.0, 0.5, -0.25]),
                option(&[7, 7, 7], &[0.5, 0.25, 0.125]),
                option(&[0, 511], &[-1.5, 2.75]),
                option(&[499], &[0.0]),
                option(&[], &[]),
                option(&[42, 41, 40], &[0.3, -0.3, 0.9]),
            ];
            let sparse = ti4_tensor::to_vec_or_panic(&actor.trunk(&options).expect("sparse"));
            let dense = ti4_tensor::to_vec_or_panic(&dense_trunk(&actor, &options));
            assert_eq!(sparse.len(), dense.len());
            assert!(!sparse.is_empty());
            // Non-vacuity: a zero trunk would agree with anything.
            assert!(
                sparse.iter().any(|value| *value != 0.0),
                "the fixture produced an all-zero trunk"
            );
            for (index, (a, b)) in sparse.iter().zip(dense.iter()).enumerate() {
                assert!(
                    close(*a, *b),
                    "{width:?} element {index}: sparse {a} against dense {b}"
                );
            }
        }
    }

    #[test]
    fn input_row_gradients_match_the_dense_reference() {
        // The other half of §4.3's requirement. If the gather's backward pass disagreed with the
        // dense one, training would move the wrong rows — and no forward test would notice.
        ti4_tensor::configure_deterministic(SEED).expect("configured");
        let options = vec![
            option(&[3, 17], &[1.0, -0.5]),
            option(&[7, 7], &[0.25, 0.75]),
            option(&[], &[]),
        ];

        let grad_of = |dense_path: bool| -> Vec<f32> {
            let mut actor = Actor::zeros(Width::W128, CAPACITY, FACTIONS);
            *actor.input_mut() = patterned(CAPACITY, 128, 1);
            *actor.hidden_mut() = patterned(128, 128, 2);
            let _ = actor.input.set_requires_grad(true);
            let z = if dense_path {
                dense_trunk(&actor, &options)
            } else {
                actor.trunk(&options).expect("sparse")
            };
            z.sum(Kind::Float).backward();
            ti4_tensor::to_vec_or_panic(&actor.input.grad())
        };

        let sparse = grad_of(false);
        let dense = grad_of(true);
        assert_eq!(sparse.len(), dense.len());
        assert!(
            sparse.iter().any(|value| *value != 0.0),
            "no gradient reached the input table: the comparison would be vacuous"
        );
        for (index, (a, b)) in sparse.iter().zip(dense.iter()).enumerate() {
            assert!(
                close(*a, *b),
                "gradient element {index}: sparse {a} against dense {b}"
            );
        }
    }

    #[test]
    fn only_the_named_rows_receive_gradient() {
        // A row nothing referenced must stay untouched, or the optimizer would move columns no
        // decision used — including the free and dead rows M09-024 requires to stay zero.
        ti4_tensor::configure_deterministic(SEED).expect("configured");
        let mut actor = Actor::zeros(Width::W128, CAPACITY, FACTIONS);
        *actor.input_mut() = patterned(CAPACITY, 128, 1);
        *actor.hidden_mut() = patterned(128, 128, 2);
        let _ = actor.input.set_requires_grad(true);

        let options = vec![option(&[5, 9], &[1.0, 1.0])];
        actor
            .trunk(&options)
            .expect("sparse")
            .sum(Kind::Float)
            .backward();
        let grad = actor.input.grad();

        for row in [5_i64, 9] {
            assert!(
                grad.get(row).abs().max().double_value(&[]) > 0.0,
                "row {row} was named and got no gradient"
            );
        }
        for row in [0_i64, 6, 100, 511] {
            assert!(
                grad.get(row).abs().max().double_value(&[]) == 0.0,
                "row {row} was never named and received gradient"
            );
        }
    }

    #[test]
    fn a_zero_faction_residual_leaves_the_shared_readout_alone() {
        // §3: a faction absent from training uses the learned shared readout and a zero residual
        // rather than an untrained output row. Every seat must therefore agree at initialisation.
        let actor = actor(Width::W128);
        let options = vec![option(&[3, 17], &[1.0, 0.5]), option(&[7], &[0.25])];
        let first =
            ti4_tensor::to_vec_or_panic(&actor.logits(&options, "movement", 0).expect("logits"));
        assert!(first.iter().any(|value| *value != 0.0), "vacuous fixture");
        for seat in [1_usize, 17, FACTIONS - 1] {
            let other = ti4_tensor::to_vec_or_panic(
                &actor.logits(&options, "movement", seat).expect("logits"),
            );
            assert_eq!(first, other, "seat {seat} differs with a zero residual");
        }
    }

    #[test]
    fn a_non_zero_residual_moves_only_its_own_seat_and_head() {
        // Every baseline comes from one actor before the mutation. Comparing against a second
        // freshly built actor would compare two fixtures, not one change.
        let mut actor = actor(Width::W128);
        let probe = [option(&[3], &[1.0])];
        let read = |actor: &Actor, head: &str, seat: usize| -> Vec<f32> {
            ti4_tensor::to_vec_or_panic(&actor.logits(&probe, head, seat).expect("logits"))
        };

        let own_before = read(&actor, "movement", 4);
        let neighbour_before = read(&actor, "movement", 5);
        let other_head_before = read(&actor, "cargo", 4);
        assert!(own_before.iter().any(|v| *v != 0.0), "vacuous fixture");

        let head = i64::try_from(Actor::head_index("movement").expect("known")).expect("small");
        let _ = actor.residual_mut().get(4).get(head).fill_(0.05);

        assert_ne!(
            own_before,
            read(&actor, "movement", 4),
            "the residual missed its own seat"
        );
        assert_eq!(
            neighbour_before,
            read(&actor, "movement", 5),
            "the residual leaked into another seat"
        );
        assert_eq!(
            other_head_before,
            read(&actor, "cargo", 4),
            "the residual leaked into another head"
        );
    }

    #[test]
    fn the_softmax_survives_logits_that_would_overflow() {
        // Without subtracting the max, exp(90) overflows f32 and every probability becomes NaN.
        let scores = Tensor::from_slice(&[90.0f32, 89.0, 0.0, -90.0]);
        let probabilities = stable_softmax(&scores);
        assert_eq!(probabilities.len(), 4);
        assert!(
            probabilities.iter().all(|p| p.is_finite()),
            "{probabilities:?}"
        );
        let total: f64 = probabilities.iter().sum();
        // f32 accumulation: the sum is exact to about seven digits, not to 1e-9.
        assert!((total - 1.0).abs() < 1e-6, "probabilities sum to {total}");
        assert!(probabilities[0] > probabilities[1], "ordering was lost");
        assert!(probabilities[3] >= 0.0);
    }

    #[test]
    fn probabilities_are_a_distribution_over_the_legal_set() {
        let actor = actor(Width::W256);
        let options: Vec<SparseOption> = (0..8)
            .map(|i| option(&[i * 7 + 1, i * 3 + 2], &[1.0, 0.5]))
            .collect();
        let p = actor
            .probabilities(&options, "production", 2, 1.0)
            .expect("probabilities");
        assert_eq!(p.len(), 8);
        let total: f64 = p.iter().sum();
        assert!((total - 1.0).abs() < 1e-6, "sum {total}");
        assert!(p.iter().all(|value| *value >= 0.0 && value.is_finite()));
    }

    #[test]
    fn an_empty_legal_set_and_a_bad_temperature_are_refused_rather_than_guessed() {
        let actor = actor(Width::W128);
        assert!(
            actor
                .probabilities(&[], "turn", 0, 1.0)
                .expect("empty")
                .is_empty()
        );
        assert!(matches!(
            actor.probabilities(&[option(&[1], &[1.0])], "turn", 0, 0.0),
            Err(ActorError::Temperature(_))
        ));
        assert!(matches!(
            actor.logits(&[option(&[1], &[1.0])], "turn", FACTIONS),
            Err(ActorError::UnknownFaction { .. })
        ));
    }

    #[test]
    fn a_full_option_batch_goes_through_in_one_pass() {
        // §4.3's worst case: an activation or production decision with a large legal set. The
        // batched path must produce one logit per option and stay finite.
        let actor = actor(Width::W256);
        let options: Vec<SparseOption> = (0..64)
            .map(|i| option(&[(i * 5) % CAPACITY, (i * 11) % CAPACITY], &[0.75, -0.25]))
            .collect();
        let logits = actor.logits(&options, "activation", 9).expect("logits");
        assert_eq!(logits.size(), vec![64]);
        let values = ti4_tensor::to_vec_or_panic(&logits);
        assert!(values.iter().all(|value| value.is_finite()));
        let dense = ti4_tensor::to_vec_or_panic(&dense_trunk(&actor, &options));
        let sparse = ti4_tensor::to_vec_or_panic(&actor.trunk(&options).expect("sparse"));
        for (a, b) in sparse.iter().zip(dense.iter()) {
            assert!(close(*a, *b), "sparse {a} against dense {b}");
        }
    }

    #[test]
    fn free_and_dead_rows_start_zero_and_are_reported() {
        // The assertion side of M09-024's obligation: rows above `slot_count`, and the five
        // reserved rows the projection can never route to, must be zero.
        let actor = Actor::zeros(Width::W128, CAPACITY, FACTIONS);
        assert!(actor.inactive_rows_are_zero(100, &[1, 2, 3, 4, 5]));

        // And it detects a violation rather than always agreeing.
        let mut dirty = Actor::zeros(Width::W128, CAPACITY, FACTIONS);
        let _ = dirty.input_mut().get(200).fill_(0.1);
        assert!(
            !dirty.inactive_rows_are_zero(100, &[]),
            "a dirty free row went unreported"
        );
        let mut dead = Actor::zeros(Width::W128, CAPACITY, FACTIONS);
        let _ = dead.input_mut().get(3).fill_(0.1);
        assert!(
            !dead.inactive_rows_are_zero(100, &[3]),
            "a dirty dead row went unreported"
        );
    }
}
