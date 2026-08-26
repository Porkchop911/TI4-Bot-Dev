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
pub mod bundle;
pub mod critic_warmup;
pub mod distill;
pub mod ppo;

use thiserror::Error;
use ti4_policy::vocabulary::Vocabulary;
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

    /// Alias for [`Self::dim`], for call sites that read better as a count of units.
    #[must_use]
    pub const fn units(self) -> i64 {
        self.dim()
    }

    /// The width a stored dimension names, or `None` if it is not one this build supports.
    ///
    /// Only 256 and 128 exist, and a bundle claiming any other width is refused rather than
    /// accommodated — §4.2 fixes the two, and a third would not match any reviewed measurement.
    #[must_use]
    pub const fn of(dim: i64) -> Option<Self> {
        match dim {
            256 => Some(Self::W256),
            128 => Some(Self::W128),
            _ => None,
        }
    }
}

/// The dimension of the identity embedding (§4.2's 16 × 33 budget).
pub const EMBED_DIM: i64 = 16;

/// The **pinned** roster of selectable faction identities, in the order their rows are allocated.
///
/// This is the model's conditioning key, and it is a faction identity — not a physical table seat.
/// An earlier version took a raw `seat: usize` and the smoke passed the player index, so across
/// rotations one faction was conditioned on a different residual and embedding row every game
/// (F-M09-026-2). Thirty-three rows, including the three Keleres separately, because §3 sizes them
/// on 33 for exactly that reason.
///
/// Frozen like the OOV registry: a trained row is addressed by index, so reordering this list
/// silently repoints every faction's residual and embedding. `the_roster_is_the_corpus_selectable_
/// seats` fails when the corpus and this list disagree.
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

/// A validated row in [`FACTION_ROSTER`].
///
/// Constructing one is the only way to condition the model, so a physical seat index cannot be
/// passed by mistake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FactionRow(usize);

impl FactionRow {
    /// Resolve a faction alias to its pinned row.
    ///
    /// # Errors
    /// [`ActorError::UnknownFaction`] for an alias the roster does not carry.
    pub fn of(alias: &str) -> Result<Self, ActorError> {
        FACTION_ROSTER
            .iter()
            .position(|known| *known == alias)
            .map(Self)
            .ok_or_else(|| ActorError::UnknownFaction(alias.to_owned()))
    }

    /// The row index.
    #[must_use]
    pub const fn index(self) -> usize {
        self.0
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
    /// A faction alias the pinned roster does not carry.
    #[error("faction {0:?} is not in the pinned 33-identity roster")]
    UnknownFaction(String),
    /// A legal set with no options. Not a position: a decision always has something to choose.
    #[error("the legal set is empty")]
    EmptyLegalSet,
    /// A logit, probability or normaliser that is not finite, or a distribution that does not sum
    /// to one. Never returned as a plausible-looking distribution.
    #[error("{what} is not usable: {detail}")]
    NotUsable { what: &'static str, detail: String },
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

/// The only thing [`Actor::value`] accepts.
///
/// # Why this is not just `SparseOption`
///
/// F-M09-027-2: the value head used to take the public `SparseOption`, the same type the policy
/// path builds from options. So a caller could hand it an option's vector — legal-set-derived
/// columns and all — and the documented claim that the value function "has no way to see the legal
/// set" was false at the actual inference API. It was true of the *extractor* and I wrote it about
/// the *model*, which is one step further than the construction supported.
///
/// The field is private and the only constructor takes a [`ti4_policy::critic::CriticVector`],
/// which in turn comes only from `critic_vector`, which takes only an engine-bound
/// `SeatObservation` and no `Choice`. That makes the whole chain from capability to value typed:
///
/// ```text
/// SeatObservation -> CriticVector -> CriticInput -> Actor::value
/// ```
///
/// A policy option cannot enter anywhere along it. The positive control first — every line of the
/// setup below is valid on its own, so the refusal that follows is about the argument type and not
/// about a typo in the fixture:
///
/// ```
/// # use ti4_mlp::{Actor, FactionRow, SparseOption, Width};
/// let actor = Actor::zeros(Width::W128, 4_096);
/// let option = SparseOption { columns: vec![1], values: vec![1.0] };
/// let row = FactionRow::of("sol").unwrap();
/// let _ = (&actor, &option, row);
/// ```
///
/// And the refusal itself — `E0308`, mismatched types, pinned so this cannot pass on an unrelated
/// compile error:
///
/// ```compile_fail,E0308
/// # use ti4_mlp::{Actor, FactionRow, SparseOption, Width};
/// let actor = Actor::zeros(Width::W128, 4_096);
/// let option = SparseOption { columns: vec![1], values: vec![1.0] };
/// // A policy option is not a critic input, and must not compile as one.
/// let _ = actor.value(&option, FactionRow::of("sol").unwrap());
/// ```
#[derive(Debug, Clone)]
pub struct CriticInput {
    sparse: SparseOption,
}

impl CriticInput {
    /// Resolve a critic vector's names against the vocabulary.
    ///
    /// A name with no column of its own is **not dropped**: it routes to its family's
    /// out-of-vocabulary column, or the global one, so an unknown fact stays distinguishable from
    /// an absent one.
    #[must_use]
    pub fn new(
        vector: &ti4_policy::critic::CriticVector,
        vocabulary: &ti4_policy::vocabulary::Vocabulary,
    ) -> Self {
        let facts = vector.facts();
        let mut columns = Vec::with_capacity(facts.len());
        let mut values = Vec::with_capacity(facts.len());
        for (key, value) in facts {
            columns.push(i64::try_from(vocabulary.column_of_key(*key)).unwrap_or(0));
            #[expect(clippy::cast_possible_truncation, reason = "features are f32-scale")]
            values.push(*value as f32);
        }
        Self {
            sparse: SparseOption { columns, values },
        }
    }

    /// Wrap an already-verified critic vector.
    ///
    /// `pub(crate)` on purpose. Making this public would hand back exactly the escape hatch
    /// F-M09-027-2 closed: a caller could wrap a policy option's `SparseOption` and feed the value
    /// head option-derived columns. Training code inside this crate reaches it through
    /// [`crate::critic_warmup::CriticSample`], which checks that every name belongs to the critic
    /// namespace before it gets here.
    pub(crate) const fn from_sparse(sparse: SparseOption) -> Self {
        Self { sparse }
    }

    /// How many distinct columns this input actually occupies.
    ///
    /// Exposed because it is the difference between a critic and a rank-1 sum: when every
    /// `critic-state:*` name falls to the same out-of-vocabulary column, `V` is one weighted row no
    /// matter how rich the position is. Tests assert on it (M09-027b).
    #[must_use]
    pub fn distinct_columns(&self) -> usize {
        let mut seen: Vec<i64> = self.sparse.columns.clone();
        seen.sort_unstable();
        seen.dedup();
        seen.len()
    }

    /// The canonical form the frozen PPO batch puts this into, once, before training reads it.
    pub(crate) const fn sparse_mut(&mut self) -> &mut SparseOption {
        &mut self.sparse
    }

    pub(crate) const fn sparse(&self) -> &SparseOption {
        &self.sparse
    }
}

/// The fixed fallback critic: an independent two-layer, width-128 trunk and scalar readout.
#[derive(Debug)]
pub struct SeparateCritic {
    input: Tensor,
    b1: Tensor,
    hidden: Tensor,
    b2: Tensor,
    readout: Tensor,
    bias: Tensor,
}

impl SeparateCritic {
    /// Build a separate critic from already initialised tensors.
    #[must_use]
    pub fn new(
        input: Tensor,
        b1: Tensor,
        hidden: Tensor,
        b2: Tensor,
        readout: Tensor,
        bias: Tensor,
    ) -> Self {
        Self {
            input,
            b1,
            hidden,
            b2,
            readout,
            bias,
        }
    }

    /// Named tensors in stable bundle order.
    pub fn tensors(&self) -> [(&'static str, &Tensor); 6] {
        [
            ("critic_W1", &self.input),
            ("critic_b1", &self.b1),
            ("critic_W2", &self.hidden),
            ("critic_b2", &self.b2),
            ("critic_readout", &self.readout),
            ("critic_bias", &self.bias),
        ]
    }

    /// Shallow parameter handles for an optimizer; mutations update this critic's tensors.
    #[must_use]
    pub fn parameters(&self) -> Vec<Tensor> {
        self.tensors()
            .iter()
            .map(|(_, tensor)| (*tensor).shallow_clone())
            .collect()
    }

    pub(crate) fn open_for_training(&mut self) {
        self.input = self.input.detach().copy().set_requires_grad(true);
        self.b1 = self.b1.detach().copy().set_requires_grad(true);
        self.hidden = self.hidden.detach().copy().set_requires_grad(true);
        self.b2 = self.b2.detach().copy().set_requires_grad(true);
        self.readout = self.readout.detach().copy().set_requires_grad(true);
        self.bias = self.bias.detach().copy().set_requires_grad(true);
    }

    fn move_to(&mut self, device: ti4_tensor::Device) {
        for tensor in [
            &mut self.input,
            &mut self.b1,
            &mut self.hidden,
            &mut self.b2,
            &mut self.readout,
            &mut self.bias,
        ] {
            *tensor = tensor.to_device(device);
        }
    }

    fn value_tensor(&self, critic: &CriticInput) -> Result<Tensor, ActorError> {
        let batch = [(
            critic.sparse.columns.as_slice(),
            critic.sparse.values.as_slice(),
        )];
        self.value_batch(&batch)
    }

    /// `V(s)` for a whole batch of critic positions: `[n]`.
    ///
    /// The separate critic has no faction identity, so a batch is simply a wider gather.
    fn value_batch(&self, batch: &[(&[i64], &[f32])]) -> Result<Tensor, ActorError> {
        let x = ti4_tensor::gather_reduce_batch(&self.input, batch)?;
        let first = (x + &self.b1).relu();
        let second = (first.matmul(&self.hidden.tr()) + &self.b2).relu();
        Ok(second.matmul(&self.readout) + &self.bias)
    }

    fn copied(&self) -> Self {
        Self {
            input: self.input.detach().copy(),
            b1: self.b1.detach().copy(),
            hidden: self.hidden.detach().copy(),
            b2: self.b2.detach().copy(),
            readout: self.readout.detach().copy(),
            bias: self.bias.detach().copy(),
        }
    }
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
    /// `[33, EMBED_DIM]`, zero. §4.2's identity embedding.
    embedding: Tensor,
    /// `[width]`, zero. §4.2's value readout.
    w_value: Tensor,
    /// Scalar value bias.
    b_value: Tensor,
    /// Present only when the fixed shared-critic fallback selected the separate trunk.
    separate_critic: Option<SeparateCritic>,
}

impl Actor {
    /// A detached inference copy with independent tensor storage.
    ///
    /// This exists so a multi-seat evaluation can give each consuming bot its own actor without
    /// sharing mutable tensor storage between deciders.
    #[must_use]
    pub fn inference_copy(&self) -> Self {
        Self {
            width: self.width,
            capacity: self.capacity,
            input: self.input.detach().copy(),
            b1: self.b1.detach().copy(),
            hidden: self.hidden.detach().copy(),
            b2: self.b2.detach().copy(),
            w_shared: self.w_shared.detach().copy(),
            b_shared: self.b_shared.detach().copy(),
            delta: self.delta.detach().copy(),
            b_delta: self.b_delta.detach().copy(),
            embedding: self.embedding.detach().copy(),
            w_value: self.w_value.detach().copy(),
            b_value: self.b_value.detach().copy(),
            separate_critic: self.separate_critic.as_ref().map(SeparateCritic::copied),
        }
    }

    /// Install or clear the separately trained fallback critic.
    pub fn set_separate_critic(&mut self, critic: Option<SeparateCritic>) {
        self.separate_critic = critic;
    }

    /// The separate fallback critic, when selected.
    #[must_use]
    pub const fn separate_critic(&self) -> Option<&SeparateCritic> {
        self.separate_critic.as_ref()
    }

    /// Mutable access for the bounded separate-critic warm-up and PPO optimizer.
    pub const fn separate_critic_mut(&mut self) -> Option<&mut SeparateCritic> {
        self.separate_critic.as_mut()
    }

    pub(crate) fn open_main_for_training(&mut self, include_value: bool) {
        macro_rules! open {
            ($field:ident) => {
                self.$field = self.$field.detach().copy().set_requires_grad(true);
            };
        }
        open!(input);
        open!(b1);
        open!(hidden);
        open!(b2);
        open!(w_shared);
        open!(b_shared);
        open!(delta);
        open!(b_delta);
        open!(embedding);
        if include_value {
            open!(w_value);
            open!(b_value);
        }
    }

    pub(crate) fn main_parameters(&self, include_value: bool) -> Vec<Tensor> {
        let mut parameters = vec![
            self.input.shallow_clone(),
            self.b1.shallow_clone(),
            self.hidden.shallow_clone(),
            self.b2.shallow_clone(),
            self.w_shared.shallow_clone(),
            self.b_shared.shallow_clone(),
            self.delta.shallow_clone(),
            self.b_delta.shallow_clone(),
            self.embedding.shallow_clone(),
        ];
        if include_value {
            parameters.push(self.w_value.shallow_clone());
            parameters.push(self.b_value.shallow_clone());
        }
        parameters
    }

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
    /// The faction dimension is **not** a parameter. `FactionRow` can name any of the 33 roster
    /// rows, so an actor built with fewer would pass the typed API and then panic inside
    /// `embedding.get` — the type would be guaranteeing a shape the constructor did not build
    /// (F-M09-026-7). It is always `FACTION_ROSTER.len()`.
    #[must_use]
    pub fn zeros(width: Width, capacity: i64) -> Self {
        let w = width.dim();
        let heads = i64::try_from(heads().len()).expect("fourteen heads");
        let factions_dim = i64::try_from(FACTION_ROSTER.len()).expect("thirty-three seats");
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
            embedding: Tensor::zeros([factions_dim, EMBED_DIM], opts),
            w_value: Tensor::zeros([w], opts),
            b_value: Tensor::zeros([1], opts),
            separate_critic: None,
        }
    }

    /// The trunk width.
    #[must_use]
    pub const fn width(&self) -> i64 {
        self.width
    }

    /// Faction rows allocated. Always [`FACTION_ROSTER`]'s length.
    #[must_use]
    pub fn faction_rows(&self) -> i64 {
        self.delta.size()[0]
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

    /// Move every tensor to `device`.
    ///
    /// # Why this is an optimiser-only facility
    ///
    /// MLP plan §7.1 permits exactly one device switch: *"after CPU rollouts produce a fixed batch,
    /// the model and Adam state may move to CUDA for forward/backward/update and return to CPU
    /// before the next decision."* Distillation is the cleanest case of that — there are no
    /// rollouts at all, only optimisation over a corpus captured on CPU beforehand — so nothing
    /// that selects an action is involved.
    ///
    /// A bundle is always written from CPU (§4.4), so a checkpoint produced on CUDA still loads on a
    /// machine without one.
    #[must_use]
    pub fn to_device(mut self, device: ti4_tensor::Device) -> Self {
        for tensor in [
            &mut self.input,
            &mut self.b1,
            &mut self.hidden,
            &mut self.b2,
            &mut self.w_shared,
            &mut self.b_shared,
            &mut self.delta,
            &mut self.b_delta,
            &mut self.embedding,
            &mut self.w_value,
            &mut self.b_value,
        ] {
            *tensor = tensor.to_device(device);
        }
        if let Some(critic) = &mut self.separate_critic {
            critic.move_to(device);
        }
        self
    }

    /// Which device this actor's parameters live on.
    #[must_use]
    pub fn device(&self) -> ti4_tensor::Device {
        self.input.device()
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

    /// Mutable access to the value readout.
    pub const fn value_readout_mut(&mut self) -> &mut Tensor {
        &mut self.w_value
    }

    /// Mutable access to the identity embedding.
    pub const fn embedding_mut(&mut self) -> &mut Tensor {
        &mut self.embedding
    }

    /// The identity embedding.
    pub const fn embedding(&self) -> &Tensor {
        &self.embedding
    }

    /// The hidden layer.
    pub const fn hidden(&self) -> &Tensor {
        &self.hidden
    }

    /// The first-layer bias.
    pub const fn b1(&self) -> &Tensor {
        &self.b1
    }

    /// Mutable access to the first-layer bias.
    pub const fn b1_mut(&mut self) -> &mut Tensor {
        &mut self.b1
    }

    /// The hidden-layer bias.
    pub const fn b2(&self) -> &Tensor {
        &self.b2
    }

    /// Mutable access to the hidden-layer bias.
    pub const fn b2_mut(&mut self) -> &mut Tensor {
        &mut self.b2
    }

    /// The shared readout.
    pub const fn shared_readout(&self) -> &Tensor {
        &self.w_shared
    }

    /// The shared readout bias.
    pub const fn b_shared(&self) -> &Tensor {
        &self.b_shared
    }

    /// Mutable access to the shared readout bias.
    pub const fn b_shared_mut(&mut self) -> &mut Tensor {
        &mut self.b_shared
    }

    /// The per-faction residual.
    pub const fn delta(&self) -> &Tensor {
        &self.delta
    }

    /// Mutable access to the per-faction residual.
    pub const fn delta_mut(&mut self) -> &mut Tensor {
        &mut self.delta
    }

    /// The per-faction residual bias.
    pub const fn b_delta(&self) -> &Tensor {
        &self.b_delta
    }

    /// Mutable access to the per-faction residual bias.
    pub const fn b_delta_mut(&mut self) -> &mut Tensor {
        &mut self.b_delta
    }

    /// The value readout.
    pub const fn value_readout(&self) -> &Tensor {
        &self.w_value
    }

    /// The value bias.
    pub const fn b_value(&self) -> &Tensor {
        &self.b_value
    }

    /// Mutable access to the value bias.
    pub const fn b_value_mut(&mut self) -> &mut Tensor {
        &mut self.b_value
    }

    /// The selected identity, zero-padded from [`EMBED_DIM`] to the trunk width.
    ///
    /// §4.2 budgets exactly 528 embedding parameters and fixes the `[V_cap, width]` and
    /// `[width, width]` shapes, so the embedding joins the input by **addition into the first-layer
    /// preactivation**, before `b1` and the `ReLU` — the architecture direction the review gave for
    /// O-M09-026-1. Concatenation or a projection would change those shapes and the budget, and
    /// would need its own ruling.
    ///
    /// # Panics
    /// If a validated roster index does not fit an `i64`; there are thirty-three.
    pub fn identity_row(&self, row: FactionRow) -> Tensor {
        let index = i64::try_from(row.index()).expect("roster fits");
        let selected = self.embedding.get(index);
        let padding = self.width - EMBED_DIM;
        if padding <= 0 {
            selected
        } else {
            // The pad follows the embedding's device: a CPU zero concatenated onto a CUDA row is
            // a hard error in libtorch, not a silent copy.
            let device = selected.device();
            Tensor::cat(
                &[selected, Tensor::zeros([padding], (Kind::Float, device))],
                0,
            )
        }
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
    pub fn trunk(&self, options: &[SparseOption], row: FactionRow) -> Result<Tensor, ActorError> {
        if options.is_empty() {
            return Err(ActorError::EmptyLegalSet);
        }
        // One gather for the whole decision, per MLP plan §4.3. The per-option loop this replaced
        // dispatched three libtorch ops and allocated three vectors *per option*, which M09-029
        // measured as the dominant cost of a decision — the tell was that halving the trunk width
        // barely changed the total.
        let batch: Vec<(&[i64], &[f32])> = options
            .iter()
            .map(|option| (option.columns.as_slice(), option.values.as_slice()))
            .collect();
        let x = ti4_tensor::gather_reduce_batch(&self.input, &batch)?;
        // The identity joins here: zero-padded to the trunk width and added to the first-layer
        // preactivation, before `b1` and the ReLU.
        let first = (x + self.identity_row(row) + &self.b1).relu();
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
        row: FactionRow,
    ) -> Result<Tensor, ActorError> {
        let head_index = Self::head_index(head)?;
        let z = self.trunk(options, row)?;
        let seat_i = i64::try_from(row.index()).expect("roster fits");
        let head_i = i64::try_from(head_index).expect("head fits");
        // w_effective[f,h] = w_shared[h] + delta[f,h] — the decomposition §3 fixes.
        let w = self.w_shared.get(head_i) + self.delta.get(seat_i).get(head_i);
        let b = self.b_shared.get(head_i) + self.b_delta.get(seat_i).get(head_i);
        Ok(z.matmul(&w) + b)
    }

    /// Logits for options belonging to **different** decisions, factions and heads, in one pass.
    ///
    /// # Why this exists
    ///
    /// [`Self::logits`] takes one head and one faction, so a training minibatch has to be split into
    /// `(faction, head)` groups — up to 6 × 14 of them — and each group issues its own kernels. On a
    /// CPU that merely wastes dispatches; on a GPU it is fatal, because every group is a handful of
    /// tiny launches and launch overhead swamps the arithmetic. Measured: a CUDA epoch structured
    /// that way took 343.9 s against the CPU's 135.9 s.
    ///
    /// Here the whole minibatch is one gather, one trunk, and one row-wise dot product. The
    /// per-option readout weight `w_shared[h] + delta[f, h]` is gathered by index rather than
    /// selected in a loop, which is what removes the grouping.
    ///
    /// `heads` and `rows` are per **option**, not per decision — the caller repeats a decision's
    /// head and faction across its options — so this function needs to know nothing about where one
    /// decision ends and the next begins.
    ///
    /// # Errors
    /// [`ActorError::EmptyLegalSet`] for no options, [`ActorError::NotUsable`] if the three slices
    /// disagree in length, and anything the gather raises.
    pub fn logits_mixed(
        &self,
        options: &[SparseOption],
        heads: &[i64],
        rows: &[i64],
    ) -> Result<Tensor, ActorError> {
        if options.is_empty() {
            return Err(ActorError::EmptyLegalSet);
        }
        if heads.len() != options.len() || rows.len() != options.len() {
            return Err(ActorError::NotUsable {
                what: "per-option head/row indices",
                detail: format!(
                    "{} options, {} heads, {} rows",
                    options.len(),
                    heads.len(),
                    rows.len()
                ),
            });
        }
        let batch: Vec<(&[i64], &[f32])> = options
            .iter()
            .map(|option| (option.columns.as_slice(), option.values.as_slice()))
            .collect();
        self.logits_mixed_parts(&batch, heads, rows)
    }

    /// [`Self::logits_mixed`] over borrowed sparse parts.
    ///
    /// The public entry point builds its `(columns, values)` pairs from a `&[SparseOption]`. A PPO
    /// minibatch already owns its options inside the frozen batch and would have to clone thousands
    /// of vectors per step to call it, so it assembles the pairs itself and enters here.
    ///
    /// # Errors
    /// [`ActorError::EmptyLegalSet`] for an empty batch, [`ActorError::NotUsable`] for a length
    /// disagreement, plus anything the gather raises.
    pub(crate) fn logits_mixed_parts(
        &self,
        batch: &[(&[i64], &[f32])],
        heads: &[i64],
        rows: &[i64],
    ) -> Result<Tensor, ActorError> {
        if batch.is_empty() {
            return Err(ActorError::EmptyLegalSet);
        }
        if heads.len() != batch.len() || rows.len() != batch.len() {
            return Err(ActorError::NotUsable {
                what: "per-option head/row indices",
                detail: format!(
                    "{} options, {} heads, {} rows",
                    batch.len(),
                    heads.len(),
                    rows.len()
                ),
            });
        }
        let device = self.input.device();
        let head_index = Tensor::from_slice(heads).to_device(device);
        let row_index = Tensor::from_slice(rows).to_device(device);
        let z = self.trunk_mixed(batch, &row_index)?;

        // w_effective[option] = w_shared[head] + delta[faction, head], gathered rather than looped.
        // `delta` is [factions, heads, width]; flattening to [factions*heads, width] turns the pair
        // into one index.
        let heads_count = i64::try_from(crate::heads().len()).unwrap_or(0);
        let pair_index = &row_index * heads_count + &head_index;
        let delta_flat = self.delta.view([-1, self.width]);
        let w =
            self.w_shared.index_select(0, &head_index) + delta_flat.index_select(0, &pair_index);
        let b = self.b_shared.index_select(0, &head_index)
            + self.b_delta.view([-1]).index_select(0, &pair_index);

        // A row-wise dot product, which is what `z.matmul(w)` degenerates to when every row has its
        // own weight vector.
        Ok((z * w).sum_dim_intlist([1i64].as_slice(), false, Kind::Float) + b)
    }

    /// The two-layer trunk over a batch whose rows may each belong to a different faction.
    ///
    /// [`Self::trunk`] is the same computation with one faction for the whole batch. Both add the
    /// identity embedding, zero-padded to the trunk width, to the first-layer preactivation before
    /// `b1` and the `ReLU`.
    fn trunk_mixed(
        &self,
        batch: &[(&[i64], &[f32])],
        row_index: &Tensor,
    ) -> Result<Tensor, ActorError> {
        let device = self.input.device();
        let x = ti4_tensor::gather_reduce_batch(&self.input, batch)?;
        let identity = self.embedding.index_select(0, row_index);
        let padding = self.width - EMBED_DIM;
        let identity = if padding > 0 {
            let pad = Tensor::zeros([identity.size()[0], padding], (Kind::Float, device));
            Tensor::cat(&[identity, pad], 1)
        } else {
            identity
        };
        let first = (x + identity + &self.b1).relu();
        Ok((first.matmul(&self.hidden.tr()) + &self.b2).relu())
    }

    /// `V(s)` for a batch of positions, each with its own faction row: `[n]`.
    ///
    /// [`Self::value_tensor`] is this for one position. A PPO minibatch evaluates thousands of
    /// critic positions per optimizer step, and doing that one position at a time is what made the
    /// update launch-bound rather than compute-bound.
    ///
    /// # Errors
    /// [`ActorError::EmptyLegalSet`] for an empty batch, [`ActorError::NotUsable`] for a length
    /// disagreement, plus anything the gather raises.
    pub fn value_batch(
        &self,
        critics: &[&CriticInput],
        rows: &[i64],
    ) -> Result<Tensor, ActorError> {
        if critics.is_empty() {
            return Err(ActorError::EmptyLegalSet);
        }
        if rows.len() != critics.len() {
            return Err(ActorError::NotUsable {
                what: "per-position faction rows",
                detail: format!("{} positions, {} rows", critics.len(), rows.len()),
            });
        }
        let batch: Vec<(&[i64], &[f32])> = critics
            .iter()
            .map(|critic| {
                (
                    critic.sparse.columns.as_slice(),
                    critic.sparse.values.as_slice(),
                )
            })
            .collect();
        if let Some(separate) = &self.separate_critic {
            return separate.value_batch(&batch);
        }
        let row_index = Tensor::from_slice(rows).to_device(self.input.device());
        let z = self.trunk_mixed(&batch, &row_index)?;
        Ok(z.matmul(&self.w_value) + &self.b_value)
    }

    /// `V(s)` for one position, from the canonical critic vector.
    ///
    /// §4.2: computed **once per decision** by a separate trunk pass over a disjoint namespace,
    /// never from anything derived from the option set. The argument is a [`CriticInput`], which
    /// has no public constructor other than one taking a `CriticVector` — so an option's vector
    /// cannot be passed here, and the two invariance properties follow from the type rather than
    /// from care at the call site.
    ///
    /// # Errors
    /// [`ActorError::NotUsable`] if the resulting value is not finite, plus anything the gather
    /// raises.
    pub fn value(&self, critic: &CriticInput, row: FactionRow) -> Result<f64, ActorError> {
        let value = self.value_tensor(critic, row)?.double_value(&[0]);
        if !value.is_finite() {
            return Err(ActorError::NotUsable {
                what: "the value estimate",
                detail: format!("{value}"),
            });
        }
        Ok(value)
    }

    /// `V(s)` as a tensor, so a critic loss can be differentiated through it.
    ///
    /// [`Self::value`] reads a number out of this and checks it is finite; a warm-up needs the
    /// graph instead. Kept as one implementation so the number the critic optimises and the number
    /// inference reports cannot drift apart.
    ///
    /// # Errors
    /// Anything the gather raises. Unlike [`Self::value`] this does **not** check finiteness — the
    /// caller is differentiating and will see a non-finite loss.
    pub fn value_tensor(
        &self,
        critic: &CriticInput,
        row: FactionRow,
    ) -> Result<Tensor, ActorError> {
        if let Some(separate) = &self.separate_critic {
            return separate.value_tensor(critic);
        }
        // The same trunk, one row. `trunk` refuses an empty batch, and a position with no critic
        // facts at all is a malformed extraction rather than a legal state.
        let z = self.trunk(std::slice::from_ref(&critic.sparse), row)?;
        Ok(z.matmul(&self.w_value) + &self.b_value)
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
        row: FactionRow,
        temperature: f64,
    ) -> Result<Vec<f64>, ActorError> {
        if options.is_empty() {
            return Err(ActorError::EmptyLegalSet);
        }
        if !temperature.is_finite() || temperature <= 0.0 {
            return Err(ActorError::Temperature(temperature));
        }
        let scores = self.logits(options, head, row)? / temperature;
        let probabilities = stable_softmax(&scores)?;
        if probabilities.len() != options.len() {
            return Err(ActorError::NotUsable {
                what: "distribution length",
                detail: format!("{} for {} options", probabilities.len(), options.len()),
            });
        }
        Ok(probabilities)
    }

    /// The rows that must never move, derived from the vocabulary rather than supplied.
    ///
    /// Two sets: every row from `slot_count` to `capacity` (the append headroom M09-024a
    /// allocates), and the reserved columns M09-024b1 classified inactive — the three unbounded
    /// crosses and the two legacy-only channels, five in all, which the projection suppresses
    /// before lookup so nothing can ever route to them.
    ///
    /// Derived, because an earlier version took caller-supplied indices and its test passed
    /// `[1,2,3,4,5]` — not the actual reserved columns — so the gate could pass without checking a
    /// single real row (F-M09-026-5).
    ///
    /// # Errors
    /// [`ActorError::NotUsable`] if the vocabulary does not fit this actor's table.
    ///
    /// # Panics
    /// If a validated slot count or column index does not fit an `i64`.
    pub fn inactive_rows(&self, vocabulary: &Vocabulary) -> Result<Vec<i64>, ActorError> {
        let slot_count = i64::try_from(vocabulary.slot_count()).expect("slots fit");
        let capacity = i64::try_from(vocabulary.capacity()).expect("capacity fits");
        if capacity != self.capacity {
            return Err(ActorError::NotUsable {
                what: "vocabulary capacity",
                detail: format!("{capacity} against a table of {}", self.capacity),
            });
        }
        if slot_count < 0 || slot_count > capacity {
            return Err(ActorError::NotUsable {
                what: "slot count",
                detail: format!("{slot_count} against capacity {capacity}"),
            });
        }
        let mut rows: Vec<i64> = (slot_count..capacity).collect();
        for family in ti4_policy::vocabulary::dead_reserved_families() {
            let column = vocabulary.column_of(&ti4_policy::vocabulary::oov_name(family));
            let column = i64::try_from(column).expect("column fits");
            if column >= capacity {
                return Err(ActorError::NotUsable {
                    what: "a reserved column",
                    detail: format!("{family} at {column}, capacity {capacity}"),
                });
            }
            rows.push(column);
        }
        rows.sort_unstable();
        rows.dedup();
        Ok(rows)
    }

    /// Whether every row that must never move is exactly zero.
    ///
    /// # Errors
    /// Propagates [`Self::inactive_rows`].
    pub fn inactive_rows_are_zero(&self, vocabulary: &Vocabulary) -> Result<bool, ActorError> {
        let rows = self.inactive_rows(vocabulary)?;
        Ok(rows
            .iter()
            .all(|row| self.input.get(*row).abs().max().double_value(&[]) == 0.0))
    }

    /// A `[capacity]` mask that is 1 where a row may train and 0 where it may not.
    ///
    /// The optimizer boundary M09-024a's headroom and M09-024b1's dead rows both depend on:
    /// multiplying a gradient by this cannot move a free or inactive row, which is stronger than
    /// asserting they are still zero afterwards.
    ///
    /// # Errors
    /// Propagates [`Self::inactive_rows`].
    pub fn trainable_mask(&self, vocabulary: &Vocabulary) -> Result<Tensor, ActorError> {
        let mask = Tensor::ones([self.capacity], (Kind::Float, Device::Cpu));
        for row in self.inactive_rows(vocabulary)? {
            let _ = mask.get(row).fill_(0.0);
        }
        Ok(mask)
    }
}

/// Softmax with the maximum subtracted first, validated end to end.
///
/// Subtracting the max is what stops `exp` overflowing `f32` on a logit around 90 — without it the
/// result is `NaN` for every option rather than a wrong one for some. But overflow is not the only
/// way this goes wrong: a `NaN` logit, an all-`-inf` set, or a normaliser that underflows to zero
/// each produce a non-finite or meaningless distribution, and an earlier version returned those as
/// success (F-M09-026-4). Every stage is checked, because a model refusal that becomes a
/// legal-looking action is worse than a model refusal.
///
/// # Errors
/// [`ActorError::NotUsable`] for non-finite logits, a non-finite or non-positive normaliser, or a
/// distribution that is not a distribution.
pub fn stable_softmax(scores: &Tensor) -> Result<Vec<f64>, ActorError> {
    let raw = ti4_tensor::to_vec(scores).map_err(|error| ActorError::NotUsable {
        what: "logits",
        detail: error.to_string(),
    })?;
    if raw.is_empty() {
        return Err(ActorError::EmptyLegalSet);
    }
    if let Some(bad) = raw.iter().copied().find(|value| !value.is_finite()) {
        return Err(ActorError::NotUsable {
            what: "a logit",
            detail: format!("{bad}"),
        });
    }

    let highest = raw.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let weights: Vec<f64> = raw
        .iter()
        .map(|value| f64::from(*value - highest).exp())
        .collect();
    let total: f64 = weights.iter().sum();
    if !total.is_finite() || total <= 0.0 {
        return Err(ActorError::NotUsable {
            what: "the softmax normaliser",
            detail: format!("{total}"),
        });
    }
    let probabilities: Vec<f64> = weights.into_iter().map(|weight| weight / total).collect();
    if let Some(bad) = probabilities
        .iter()
        .copied()
        .find(|value| !value.is_finite() || *value < 0.0)
    {
        return Err(ActorError::NotUsable {
            what: "a probability",
            detail: format!("{bad}"),
        });
    }
    let sum: f64 = probabilities.iter().sum();
    if (sum - 1.0).abs() > 1e-9 {
        return Err(ActorError::NotUsable {
            what: "the distribution total",
            detail: format!("{sum}"),
        });
    }
    Ok(probabilities)
}

#[cfg(test)]
mod mixed_tests {
    use super::*;

    fn option(seed: i64) -> SparseOption {
        SparseOption {
            columns: vec![seed % 97 + 1, (seed * 7) % 97 + 1],
            values: vec![
                f32::from(u8::try_from(seed % 3).unwrap_or(0)).mul_add(0.25, 1.0),
                0.5,
            ],
        }
    }

    #[test]
    fn the_mixed_path_agrees_with_the_single_faction_path() {
        // `logits_mixed` exists only to be faster. If it computed something different the training
        // numbers would change silently, so it is pinned against the path it replaces.
        let mut actor = Actor::zeros(Width::W128, 4_096);
        *actor.input_mut() = actor.input().f_add_scalar(0.05).expect("add");
        *actor.hidden_mut() = actor.hidden().f_add_scalar(0.03).expect("add");
        *actor.shared_readout_mut() = actor.shared_readout().f_add_scalar(0.2).expect("add");
        *actor.b1_mut() = actor.b1().f_add_scalar(0.01).expect("add");
        *actor.b_shared_mut() = actor.b_shared().f_add_scalar(0.02).expect("add");
        // Non-zero residual and embedding, or the faction index would not matter and the test
        // would pass for a version that ignored it.
        *actor.delta_mut() = actor.delta().f_add_scalar(0.004).expect("add");
        *actor.embedding_mut() = actor.embedding().f_add_scalar(0.03).expect("add");
        *actor.b_delta_mut() = actor.b_delta().f_add_scalar(0.007).expect("add");

        // Three decisions, deliberately different factions and heads.
        let cases = [("sol", 0usize, 3usize), ("letnev", 5, 2), ("xxcha", 11, 4)];
        let mut flat: Vec<SparseOption> = Vec::new();
        let mut heads_idx: Vec<i64> = Vec::new();
        let mut rows_idx: Vec<i64> = Vec::new();
        let mut expected: Vec<f32> = Vec::new();
        let mut seed = 0i64;

        for (faction, head, count) in cases {
            let row = FactionRow::of(faction).expect("roster");
            let options: Vec<SparseOption> = (0..count)
                .map(|_| {
                    seed += 1;
                    option(seed)
                })
                .collect();
            let head_name = crate::heads()[head];
            let single = actor.logits(&options, head_name, row).expect("logits");
            expected.extend(ti4_tensor::to_vec(&single).expect("vec"));
            for opt in &options {
                flat.push(opt.clone());
                heads_idx.push(i64::try_from(head).unwrap());
                rows_idx.push(i64::try_from(row.index()).unwrap());
            }
        }

        let mixed = actor
            .logits_mixed(&flat, &heads_idx, &rows_idx)
            .expect("mixed logits");
        let got = ti4_tensor::to_vec(&mixed).expect("vec");
        assert_eq!(got.len(), expected.len());
        // Non-vacuity: constant logits would match trivially.
        assert!(
            expected.windows(2).any(|w| (w[0] - w[1]).abs() > 1e-4),
            "every expected logit is the same, so agreement proves nothing"
        );
        for (index, (a, b)) in got.iter().zip(&expected).enumerate() {
            assert!(
                (a - b).abs() < 2e-4,
                "logit {index}: mixed {a} against single {b}"
            );
        }
    }

    #[test]
    fn ragged_index_slices_are_refused() {
        let actor = Actor::zeros(Width::W128, 4_096);
        let options = vec![option(1), option(2)];
        assert!(actor.logits_mixed(&options, &[0], &[0, 0]).is_err());
        assert!(actor.logits_mixed(&[], &[], &[]).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: i64 = 20_260_821;
    const CAPACITY: i64 = 512;

    fn row(alias: &str) -> FactionRow {
        FactionRow::of(alias).expect("in the roster")
    }

    /// Reproducible pseudo-random weights that do **not** touch libtorch's RNG.
    ///
    /// `Tensor::rand` draws from a process-global generator, and cargo runs a binary's tests in
    /// parallel threads of one process — so two fixtures built from the same seed in different
    /// tests are not the same fixture, and a comparison between them fails for reasons that have
    /// nothing to do with the model. A pure function of `(index, salt)` has no such coupling.
    fn patterned(rows: i64, cols: i64, salt: u64) -> Tensor {
        let mut values = Vec::with_capacity(usize::try_from(rows * cols).expect("fixture fits"));
        for index in 0..(rows * cols) {
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
        let mut actor = Actor::zeros(width, CAPACITY);
        let w = width.dim();
        *actor.input_mut() = patterned(CAPACITY, w, 1);
        *actor.hidden_mut() = patterned(w, w, 2);
        *actor.shared_readout_mut() = patterned(i64::try_from(heads().len()).expect("small"), w, 3);
        actor
    }

    /// Agreement to f32 precision, relative to the magnitude being compared.
    ///
    /// A fixed absolute tolerance is wrong for a dense-versus-sparse comparison: the two paths sum
    /// the same terms in different groupings, so their disagreement scales with the values.
    fn close(a: f32, b: f32) -> bool {
        let scale = a.abs().max(b.abs()).max(1.0);
        (a - b).abs() <= 1e-5 * scale
    }

    /// The same forward pass, computed densely: materialise `[n, V_cap]` and use a real matmul.
    fn dense_trunk(actor: &Actor, options: &[SparseOption], identity: FactionRow) -> Tensor {
        let n = i64::try_from(options.len()).expect("small batch");
        let dense = Tensor::zeros([n, actor.capacity()], (Kind::Float, Device::Cpu));
        for (index, option) in options.iter().enumerate() {
            for (column, value) in option.columns.iter().zip(option.values.iter()) {
                let mut cell = dense.get(i64::try_from(index).expect("small")).get(*column);
                let _ = cell.g_add_(&Tensor::from(*value));
            }
        }
        let x = dense.matmul(actor.input());
        let first = (x + actor.identity_row(identity) + &actor.b1).relu();
        (first.matmul(&actor.hidden.tr()) + &actor.b2).relu()
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
        // Schema 5 heads fold to `other` rather than failing at a call site.
        assert_eq!(Actor::resolve_head("scoring"), "other");
        assert_eq!(Actor::resolve_head("movement"), "movement");
    }

    #[test]
    fn the_roster_is_the_corpus_selectable_seats() {
        // F-M09-026-2. The conditioning key is a faction identity and the rows are addressed by
        // index, so this list is frozen and must match the corpus. A drift here silently repoints
        // every trained residual and embedding.
        let content = ti4_content::ContentStore::embedded();
        let selectable: Vec<String> =
            ti4_content::factions::catalogue(content, ti4_model::content_types::DEFAULT)
                .into_iter()
                .filter(|(_, faction)| ti4_policy::features::is_selectable_seat(faction))
                .map(|(alias, _)| alias.to_owned())
                .collect();
        let roster: Vec<String> = FACTION_ROSTER.iter().map(|a| (*a).to_owned()).collect();
        assert_eq!(roster, selectable, "the roster and the corpus disagree");
        assert_eq!(FACTION_ROSTER.len(), 33);

        // No duplicates, and the three Keleres are three rows.
        let unique: std::collections::BTreeSet<&str> = FACTION_ROSTER.iter().copied().collect();
        assert_eq!(unique.len(), 33, "the roster has a duplicate");
        for keleres in ["keleresa", "keleresm", "keleresx"] {
            assert!(unique.contains(keleres), "{keleres} is missing");
        }
        assert_ne!(row("keleresa"), row("keleresm"));
        assert_ne!(row("keleresm"), row("keleresx"));
    }

    #[test]
    fn every_roster_row_is_safe_because_the_actor_is_always_roster_sized() {
        // F-M09-026-7. `FactionRow` can name any of 33 rows, so an actor with fewer would pass the
        // typed API and panic inside the tensor. The dimension is no longer a caller's to choose —
        // and every row is exercised rather than assumed.
        let actor = Actor::zeros(Width::W128, CAPACITY);
        assert_eq!(actor.faction_rows(), 33);
        assert_eq!(actor.embedding().size(), vec![33, 16]);
        for alias in FACTION_ROSTER {
            let identity = row(alias);
            let selected = ti4_tensor::to_vec_or_panic(&actor.identity_row(identity));
            assert_eq!(selected.len(), 128, "{alias}: wrong padded width");
            let logits = actor
                .logits(&[option(&[1], &[1.0])], "turn", identity)
                .unwrap_or_else(|error| panic!("{alias} is a valid row but failed: {error}"));
            assert_eq!(logits.size(), vec![1], "{alias}: wrong logit shape");
        }
    }

    #[test]
    fn an_unknown_identity_is_refused_and_a_seat_index_cannot_be_passed() {
        assert!(matches!(
            FactionRow::of("neutral"),
            Err(ActorError::UnknownFaction(_))
        ));
        assert!(matches!(
            FactionRow::of("seat0"),
            Err(ActorError::UnknownFaction(_))
        ));
        // `neutral` is a corpus record but not a selectable seat, which is why it is not a row.
        assert_eq!(FACTION_ROSTER.iter().position(|a| *a == "neutral"), None);
    }

    #[test]
    fn one_faction_keeps_one_row_across_physical_seats() {
        // The rotation property the finding named: a faction's conditioning must not depend on
        // where it happens to sit. `FactionRow::of` is a pure function of the alias, so six
        // rotations of the same six factions give the same six rows.
        let table: Vec<&str> = vec!["sol", "letnev", "xxcha", "hacan", "jolnar", "l1z1x"];
        let baseline: Vec<usize> = table.iter().map(|a| row(a).index()).collect();
        for rotation in 1..table.len() {
            let rotated: Vec<usize> = (0..table.len())
                .map(|seat| row(table[(seat + rotation) % table.len()]).index())
                .collect();
            for (seat, index) in rotated.iter().enumerate() {
                let faction = table[(seat + rotation) % table.len()];
                assert_eq!(
                    *index,
                    row(faction).index(),
                    "{faction} changed row at physical seat {seat}"
                );
            }
            assert_ne!(rotated, baseline, "the fixture must actually rotate");
        }
    }

    #[test]
    fn the_sparse_trunk_matches_a_dense_reference() {
        for width in [Width::W256, Width::W128] {
            let actor = actor(width);
            let options = vec![
                option(&[3, 17, 200], &[1.0, 0.5, -0.25]),
                option(&[7, 7, 7], &[0.5, 0.25, 0.125]),
                option(&[0, 511], &[-1.5, 2.75]),
                option(&[499], &[0.0]),
                option(&[42, 41, 40], &[0.3, -0.3, 0.9]),
            ];
            let identity = row("sol");
            let sparse =
                ti4_tensor::to_vec_or_panic(&actor.trunk(&options, identity).expect("sparse"));
            let dense = ti4_tensor::to_vec_or_panic(&dense_trunk(&actor, &options, identity));
            assert_eq!(sparse.len(), dense.len());
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
    fn the_identity_embedding_influences_the_trunk_at_both_widths() {
        // F-M09-026-1. A zero embedding is the correct start, so influence is proved by setting a
        // row and showing it moves that faction's trunk and no other's.
        for width in [Width::W256, Width::W128] {
            let mut actor = actor(width);
            let options = vec![option(&[3, 17], &[1.0, 0.5])];
            let sol = row("sol");
            let hacan = row("hacan");
            let before_sol = ti4_tensor::to_vec_or_panic(&actor.trunk(&options, sol).expect("t"));
            let before_hacan =
                ti4_tensor::to_vec_or_panic(&actor.trunk(&options, hacan).expect("t"));
            assert_eq!(
                before_sol, before_hacan,
                "a zero embedding must not separate seats"
            );

            let sol_row = i64::try_from(sol.index()).expect("fits");
            let _ = actor.embedding_mut().get(sol_row).fill_(0.75);

            let after_sol = ti4_tensor::to_vec_or_panic(&actor.trunk(&options, sol).expect("t"));
            let after_hacan =
                ti4_tensor::to_vec_or_panic(&actor.trunk(&options, hacan).expect("t"));
            assert_ne!(
                before_sol, after_sol,
                "{width:?}: the embedding had no influence"
            );
            assert_eq!(
                before_hacan, after_hacan,
                "{width:?}: it leaked to another identity"
            );
        }
    }

    #[test]
    fn an_untrained_identity_selects_a_zero_row() {
        // §3: a faction absent from training uses the shared readout, a zero residual and a zero
        // embedding. Every row starts zero, and the padded selection is all zeros.
        let actor = Actor::zeros(Width::W256, CAPACITY);
        for alias in ["bastion", "crimson", "ralnel"] {
            let selected = ti4_tensor::to_vec_or_panic(&actor.identity_row(row(alias)));
            assert_eq!(selected.len(), 256, "the selection is padded to the width");
            assert!(selected.iter().all(|v| *v == 0.0), "{alias} is not zero");
        }
    }

    #[test]
    fn the_padded_identity_occupies_only_the_first_sixteen_slots() {
        let mut actor = actor(Width::W256);
        let sol = row("sol");
        let sol_row = i64::try_from(sol.index()).expect("fits");
        let _ = actor.embedding_mut().get(sol_row).fill_(1.0);
        let padded = ti4_tensor::to_vec_or_panic(&actor.identity_row(sol));
        assert_eq!(padded.len(), 256);
        assert!(
            padded[..16].iter().all(|v| (*v - 1.0).abs() < f32::EPSILON),
            "the embedding is not at the front"
        );
        assert!(
            padded[16..].iter().all(|v| *v == 0.0),
            "the padding is not zero"
        );
        assert_eq!(EMBED_DIM, 16);
        // The budget §4.2 fixes: 33 x 16.
        assert_eq!(actor.embedding().size(), vec![33, 16]);
    }

    #[test]
    fn input_row_gradients_match_the_dense_reference() {
        ti4_tensor::configure_deterministic(SEED).expect("configured");
        let options = vec![
            option(&[3, 17], &[1.0, -0.5]),
            option(&[7, 7], &[0.25, 0.75]),
        ];
        let identity = row("sol");

        let grad_of = |dense_path: bool| -> Vec<f32> {
            let mut actor = Actor::zeros(Width::W128, CAPACITY);
            *actor.input_mut() = patterned(CAPACITY, 128, 1);
            *actor.hidden_mut() = patterned(128, 128, 2);
            let _ = actor.input.set_requires_grad(true);
            let z = if dense_path {
                dense_trunk(&actor, &options, identity)
            } else {
                actor.trunk(&options, identity).expect("sparse")
            };
            z.sum(Kind::Float).backward();
            ti4_tensor::to_vec_or_panic(&actor.input.grad())
        };

        let sparse = grad_of(false);
        let dense = grad_of(true);
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
    fn the_embedding_receives_gradient_only_on_the_selected_row() {
        ti4_tensor::configure_deterministic(SEED).expect("configured");
        let mut actor = Actor::zeros(Width::W128, CAPACITY);
        *actor.input_mut() = patterned(CAPACITY, 128, 1);
        *actor.hidden_mut() = patterned(128, 128, 2);
        let _ = actor.embedding.set_requires_grad(true);

        let sol = row("sol");
        actor
            .trunk(&[option(&[3, 17], &[1.0, 0.5])], sol)
            .expect("trunk")
            .sum(Kind::Float)
            .backward();
        let grad = actor.embedding.grad();

        let sol_row = i64::try_from(sol.index()).expect("fits");
        assert!(
            grad.get(sol_row).abs().max().double_value(&[]) > 0.0,
            "the selected identity got no gradient"
        );
        for other in [row("hacan"), row("bastion"), row("yssaril")] {
            let index = i64::try_from(other.index()).expect("fits");
            assert!(
                grad.get(index).abs().max().double_value(&[]) == 0.0,
                "an unselected identity received gradient"
            );
        }
    }

    #[test]
    fn only_the_named_rows_receive_gradient() {
        ti4_tensor::configure_deterministic(SEED).expect("configured");
        let mut actor = Actor::zeros(Width::W128, CAPACITY);
        *actor.input_mut() = patterned(CAPACITY, 128, 1);
        *actor.hidden_mut() = patterned(128, 128, 2);
        let _ = actor.input.set_requires_grad(true);

        actor
            .trunk(&[option(&[5, 9], &[1.0, 1.0])], row("sol"))
            .expect("sparse")
            .sum(Kind::Float)
            .backward();
        let grad = actor.input.grad();

        for named in [5_i64, 9] {
            assert!(
                grad.get(named).abs().max().double_value(&[]) > 0.0,
                "row {named} was named and got no gradient"
            );
        }
        for untouched in [0_i64, 6, 100, 511] {
            assert!(
                grad.get(untouched).abs().max().double_value(&[]) == 0.0,
                "row {untouched} was never named and received gradient"
            );
        }
    }

    #[test]
    fn a_zero_faction_residual_leaves_the_shared_readout_alone() {
        let actor = actor(Width::W128);
        let options = vec![option(&[3, 17], &[1.0, 0.5]), option(&[7], &[0.25])];
        let first = ti4_tensor::to_vec_or_panic(
            &actor.logits(&options, "movement", row("sol")).expect("l"),
        );
        assert!(first.iter().any(|value| *value != 0.0), "vacuous fixture");
        for alias in ["hacan", "keleresx", "yssaril"] {
            let other = ti4_tensor::to_vec_or_panic(
                &actor.logits(&options, "movement", row(alias)).expect("l"),
            );
            assert_eq!(
                first, other,
                "{alias} differs with a zero residual and embedding"
            );
        }
    }

    #[test]
    fn a_non_zero_residual_moves_only_its_own_seat_and_head() {
        let mut actor = actor(Width::W128);
        let probe = [option(&[3], &[1.0])];
        let read = |actor: &Actor, head: &str, alias: &str| -> Vec<f32> {
            ti4_tensor::to_vec_or_panic(&actor.logits(&probe, head, row(alias)).expect("logits"))
        };

        let own_before = read(&actor, "movement", "sol");
        let neighbour_before = read(&actor, "movement", "hacan");
        let other_head_before = read(&actor, "cargo", "sol");
        assert!(own_before.iter().any(|v| *v != 0.0), "vacuous fixture");

        let head = i64::try_from(Actor::head_index("movement").expect("known")).expect("small");
        let sol = i64::try_from(row("sol").index()).expect("fits");
        let _ = actor.residual_mut().get(sol).get(head).fill_(0.05);

        assert_ne!(
            own_before,
            read(&actor, "movement", "sol"),
            "missed its own seat"
        );
        assert_eq!(
            neighbour_before,
            read(&actor, "movement", "hacan"),
            "leaked into another seat"
        );
        assert_eq!(
            other_head_before,
            read(&actor, "cargo", "sol"),
            "leaked into another head"
        );
    }

    #[test]
    fn the_softmax_survives_logits_that_would_overflow() {
        let scores = Tensor::from_slice(&[90.0f32, 89.0, 0.0, -90.0]);
        let probabilities = stable_softmax(&scores).expect("finite logits");
        assert_eq!(probabilities.len(), 4);
        assert!(
            probabilities.iter().all(|p| p.is_finite()),
            "{probabilities:?}"
        );
        let total: f64 = probabilities.iter().sum();
        assert!((total - 1.0).abs() < 1e-9, "probabilities sum to {total}");
        assert!(probabilities[0] > probabilities[1], "ordering was lost");
    }

    #[test]
    fn a_softmax_that_cannot_produce_a_distribution_refuses_to() {
        // F-M09-026-4. Overflow is not the only failure. Each of these once returned a non-finite
        // "distribution" as success, and the sampler then fell through to the last option — a model
        // refusal arriving as a legal-looking action.
        for bad in [
            vec![f32::NAN, 1.0],
            vec![f32::INFINITY, 0.0],
            vec![f32::NEG_INFINITY, f32::NEG_INFINITY],
            vec![1.0, f32::NAN],
        ] {
            let scores = Tensor::from_slice(&bad);
            assert!(
                matches!(stable_softmax(&scores), Err(ActorError::NotUsable { .. })),
                "{bad:?} was accepted"
            );
        }
        // An empty set is not a position.
        assert!(matches!(
            stable_softmax(&Tensor::from_slice(&[] as &[f32])),
            Err(ActorError::EmptyLegalSet)
        ));
        // And a finite set still works, so the guard is not rejecting everything.
        assert!(stable_softmax(&Tensor::from_slice(&[1.0f32, 2.0])).is_ok());
    }

    #[test]
    fn probabilities_are_a_distribution_over_the_legal_set() {
        let actor = actor(Width::W256);
        let options: Vec<SparseOption> = (0..8)
            .map(|i| option(&[i * 7 + 1, i * 3 + 2], &[1.0, 0.5]))
            .collect();
        let p = actor
            .probabilities(&options, "production", row("jolnar"), 1.0)
            .expect("probabilities");
        assert_eq!(p.len(), 8, "one probability per legal option");
        let total: f64 = p.iter().sum();
        assert!((total - 1.0).abs() < 1e-9, "sum {total}");
        assert!(p.iter().all(|value| *value >= 0.0 && value.is_finite()));
    }

    #[test]
    fn an_empty_legal_set_and_a_bad_temperature_are_refused() {
        let actor = actor(Width::W128);
        assert!(matches!(
            actor.probabilities(&[], "turn", row("sol"), 1.0),
            Err(ActorError::EmptyLegalSet)
        ));
        assert!(matches!(
            actor.trunk(&[], row("sol")),
            Err(ActorError::EmptyLegalSet)
        ));
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(
                matches!(
                    actor.probabilities(&[option(&[1], &[1.0])], "turn", row("sol"), bad),
                    Err(ActorError::Temperature(_))
                ),
                "temperature {bad} was accepted"
            );
        }
    }

    #[test]
    fn a_full_option_batch_goes_through_in_one_pass() {
        let actor = actor(Width::W256);
        let identity = row("l1z1x");
        let options: Vec<SparseOption> = (0..64)
            .map(|i| option(&[(i * 5) % CAPACITY, (i * 11) % CAPACITY], &[0.75, -0.25]))
            .collect();
        let logits = actor
            .logits(&options, "activation", identity)
            .expect("logits");
        assert_eq!(logits.size(), vec![64]);
        assert!(
            ti4_tensor::to_vec_or_panic(&logits)
                .iter()
                .all(|value| value.is_finite())
        );
        let dense = ti4_tensor::to_vec_or_panic(&dense_trunk(&actor, &options, identity));
        let sparse = ti4_tensor::to_vec_or_panic(&actor.trunk(&options, identity).expect("sparse"));
        for (a, b) in sparse.iter().zip(dense.iter()) {
            assert!(close(*a, *b), "sparse {a} against dense {b}");
        }
    }

    /// A vocabulary shaped like the real one: 40 reserved columns, then ordinary names.
    fn vocabulary(slots: usize) -> Vocabulary {
        let names: Vec<String> = (0..slots).map(|n| format!("option:name{n}")).collect();
        Vocabulary::build(names).expect("builds")
    }

    #[test]
    fn the_inactive_rows_are_the_real_ones_and_are_zero() {
        // F-M09-026-5. The rows are derived from the vocabulary, not supplied: an earlier version
        // took caller indices and its test passed `[1,2,3,4,5]`, which are not the reserved family
        // columns, so the gate could pass without checking a single real row.
        let built = vocabulary(200);
        let capacity = i64::try_from(built.capacity()).expect("fits");
        let actor = Actor::zeros(Width::W128, capacity);

        let rows = actor.inactive_rows(&built).expect("derives");
        // Five reserved columns plus every free row above `slot_count`.
        let free = built.capacity() - built.slot_count();
        assert_eq!(rows.len(), free + 5, "wrong inactive-row count");
        for family in ti4_policy::vocabulary::dead_reserved_families() {
            let column = i64::try_from(built.column_of(&ti4_policy::vocabulary::oov_name(family)))
                .expect("fits");
            assert!(
                rows.contains(&column),
                "{family}'s reserved column is missing"
            );
        }
        assert!(actor.inactive_rows_are_zero(&built).expect("checks"));

        // A dirty reserved row is detected — the real one, not an arbitrary index.
        let mut dirty = Actor::zeros(Width::W128, capacity);
        let dead =
            i64::try_from(built.column_of(&ti4_policy::vocabulary::oov_name("state-option")))
                .expect("fits");
        let _ = dirty.input_mut().get(dead).fill_(0.1);
        assert!(!dirty.inactive_rows_are_zero(&built).expect("checks"));

        // And a dirty free row.
        let mut free_dirty = Actor::zeros(Width::W128, capacity);
        let _ = free_dirty.input_mut().get(capacity - 1).fill_(0.1);
        assert!(!free_dirty.inactive_rows_are_zero(&built).expect("checks"));
    }

    #[test]
    fn a_mismatched_vocabulary_is_refused_rather_than_checked_against_the_wrong_table() {
        let built = vocabulary(200);
        let actor = Actor::zeros(Width::W128, 4_096 * 4);
        assert!(matches!(
            actor.inactive_rows(&built),
            Err(ActorError::NotUsable { .. })
        ));
    }

    #[test]
    fn the_trainable_mask_pins_every_inactive_row_against_an_optimizer_step() {
        // Stronger than asserting the rows are still zero afterwards: multiplying a gradient by
        // this mask cannot move them at all.
        let built = vocabulary(200);
        let capacity = i64::try_from(built.capacity()).expect("fits");
        let mut actor = Actor::zeros(Width::W128, capacity);
        let mask = actor.trainable_mask(&built).expect("mask");
        assert_eq!(mask.size(), vec![capacity]);

        let inactive = actor.inactive_rows(&built).expect("rows");
        for row in &inactive {
            assert!(
                mask.get(*row).double_value(&[]) == 0.0,
                "row {row} is trainable"
            );
        }
        assert!(
            mask.sum(Kind::Float).double_value(&[]) > 0.0,
            "the mask blocks everything: it would be vacuous"
        );

        // Simulate a weight-decay/optimizer step over every row and confirm the mask holds.
        let dense_gradient = Tensor::ones([capacity, 128], (Kind::Float, Device::Cpu));
        let masked = dense_gradient * mask.unsqueeze(1);
        let _ = actor.input_mut().g_sub_(&(masked * 0.1));
        assert!(
            actor.inactive_rows_are_zero(&built).expect("checks"),
            "an inactive row moved under a masked step"
        );
    }
}
