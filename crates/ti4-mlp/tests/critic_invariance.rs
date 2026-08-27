//! MLP plan §4.2's three properties for the value path (M09-027).
//!
//! The first two say `V` does not depend on the legal set. The third exists because a model that
//! ignores option features entirely satisfies both — so invariance alone would let that through,
//! and the policy is checked for the opposite property on the same shuffle.
//!
//! Every model output here is reached through `ask_private` -> `choose_seeing`, the engine's own
//! ask path. F-M09-027-2: the first version of this file never used a production boundary — it
//! built a `Choice`, discarded it with `let _ = choice`, and then called the extractor three times
//! with *identical arguments*, asserting the three results agreed. That is `f(x) == f(x)`: a
//! determinism check wearing an invariance test's name, which no implementation could fail.

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, IllegalChoice, Observed, SeatObservation};
use ti4_mlp::{Actor, CriticInput, FactionRow, SparseOption, Width};
use ti4_model::content_types::POK;
use ti4_model::id::{FactionId, PlayerId};
use ti4_policy::critic::{CriticFeatures, critic_vector};
use ti4_policy::vocabulary::Vocabulary;

const CAPACITY: i64 = 4_096;

/// Deterministic weights that never touch libtorch's global RNG.
fn patterned(rows: i64, cols: i64, salt: u64) -> ti4_tensor::Tensor {
    let mut values = Vec::with_capacity(usize::try_from(rows * cols).expect("fits"));
    for index in 0..(rows * cols) {
        let mut state = u64::try_from(index)
            .expect("non-negative")
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(salt.wrapping_mul(1_442_695_040_888_963_407));
        state ^= state >> 33;
        state = state.wrapping_mul(0xff51_afd7_ed55_8ccd);
        state ^= state >> 29;
        #[allow(clippy::cast_precision_loss)]
        let unit = ((state >> 48) as f32) / f32::from(u16::MAX);
        values.push(unit - 0.5);
    }
    ti4_tensor::Tensor::from_slice(&values).view([rows, cols])
}

fn actor() -> Actor {
    ti4_tensor::configure_deterministic(20_260_821).expect("configured");
    let mut actor = Actor::zeros(Width::W128, CAPACITY);
    *actor.input_mut() = patterned(CAPACITY, 128, 1);
    *actor.hidden_mut() = patterned(128, 128, 2);
    *actor.shared_readout_mut() = patterned(14, 128, 3);
    *actor.value_readout_mut() = patterned(1, 128, 4).view([128]);
    actor
}

fn sparse(vector: &ti4_policy::features::FeatureVector, vocab: &Vocabulary) -> SparseOption {
    let mut columns = Vec::new();
    let mut values = Vec::new();
    for (key, value) in vector {
        let name = ti4_policy::intern::name_of(*key);
        columns.push(i64::try_from(vocab.column_of(&name)).expect("fits"));
        #[allow(clippy::cast_possible_truncation)]
        values.push(*value as f32);
    }
    SparseOption { columns, values }
}

fn position() -> (ti4_model::state::GameState, PlayerId) {
    let player = PlayerId::new("a");
    let mut state = ti4_engine::fixtures::game(&["a", "b", "c"]);
    state.round = 3;
    {
        let seat = state.player_mut(&player).unwrap();
        seat.faction = FactionId::new("sol");
        seat.trade_goods = 5;
        seat.victory_points = 1;
    }
    state
        .player_mut(&PlayerId::new("b"))
        .unwrap()
        .victory_points = 3;
    (state, player)
}

fn options(ids: &[&str]) -> Vec<ChoiceOption> {
    ids.iter()
        .map(|id| ChoiceOption::labelled(*id, "production", *id))
        .collect()
}

const BASE: [&str; 3] = [
    "produce|fighter@18",
    "produce|scout@19",
    "produce|carrier@20",
];
const SHUFFLED: [&str; 3] = [
    "produce|carrier@20",
    "produce|fighter@18",
    "produce|scout@19",
];
const SHORTER: [&str; 2] = ["produce|fighter@18", "produce|scout@19"];
/// `SHUFFLED[i]` is `BASE[ORDER[i]]`.
const ORDER: [usize; 3] = [2, 0, 1];

/// One decision, answered the way a seated bot answers one.
///
/// Both model outputs are produced inside `choose_seeing`, which is the only place a
/// [`SeatObservation`] exists: `SeatObservation::bind` is `pub(crate)` to `ti4-engine`, so this
/// test cannot mint the capability, and the critic cannot be called without it.
struct Decision {
    actor: Actor,
    vocabulary: Vocabulary,
    row: FactionRow,
    value: f64,
    critic_columns: usize,
    critic_names: Vec<String>,
    probabilities: Vec<f64>,
    option_columns: Vec<Vec<i64>>,
}

impl ti4_engine::choice::Decider for Decision {
    fn choose(&mut self, choice: &Choice) -> Result<ChoiceOption, IllegalChoice> {
        Ok(choice.options[0].clone())
    }

    fn choose_seeing(
        &mut self,
        choice: &Choice,
        seen: &SeatObservation<'_>,
    ) -> Result<ChoiceOption, IllegalChoice> {
        // The critic half: the capability goes in, and the choice in scope stays out — there is no
        // parameter to pass it through.
        let vector = critic_vector(seen, CriticFeatures::full());
        self.critic_names = ti4_policy::features::names_of(vector.facts());
        let critic = CriticInput::new(&vector, &self.vocabulary);
        self.critic_columns = critic.distinct_columns();
        self.value = self.actor.value(&critic, self.row).expect("a value");

        // The policy half: the same decision, option-conditioned.
        let scored: Vec<SparseOption> = ti4_policy::projection::mlp_choice_features(
            seen.observed(),
            choice,
            seen.bound_seat(),
            &[],
            ti4_policy::progress::Baseline::default(),
        )
        .iter()
        .map(|vector| sparse(vector, &self.vocabulary))
        .collect();
        self.option_columns = scored
            .iter()
            .map(|option| {
                let mut columns = option.columns.clone();
                columns.sort_unstable();
                columns.dedup();
                columns
            })
            .collect();
        self.probabilities = self
            .actor
            .probabilities(&scored, "production", self.row, 1.0)
            .expect("probabilities");

        Ok(choice.options[0].clone())
    }
}

/// Put one legal set through the engine and report what the model saw.
fn run(
    state: &ti4_model::state::GameState,
    player: &PlayerId,
    ids: &[&str],
    vocabulary: &Vocabulary,
) -> Decision {
    let mut decision = Decision {
        actor: actor(),
        vocabulary: vocabulary.clone(),
        row: FactionRow::of("sol").expect("roster"),
        value: 0.0,
        critic_columns: 0,
        critic_names: Vec::new(),
        probabilities: Vec::new(),
        option_columns: Vec::new(),
    };
    let choice = Choice::new(player.clone(), "produce a unit", options(ids));
    ti4_engine::choice::ask_private(
        &choice,
        state,
        ContentStore::embedded(),
        POK,
        None,
        &mut decision,
    )
    .expect("the decision is answered");
    decision
}

/// A vocabulary built from the names this position actually emits, critic and policy alike.
///
/// A synthetic one does not work, and the way it fails is the point: every real name falls to its
/// family's OOV column, so all options collapse onto the same columns and the policy comes out
/// uniform. An earlier version of this file did that, and its non-vacuity guard passed on f32
/// accumulation noise — a distribution that "differed" by 2e-6 while the two were bit-identical.
fn vocabulary_for_position(state: &ti4_model::state::GameState, player: &PlayerId) -> Vocabulary {
    // A first pass through the engine to collect the critic's names; the bootstrap vocabulary only
    // has to let the run complete, not to resolve anything.
    let bootstrap = Vocabulary::build(["option:bootstrap"]).expect("builds");
    let mut names = run(state, player, &BASE, &bootstrap).critic_names;

    let seen = Observed::new(state, ContentStore::embedded(), POK, None);
    let choice = Choice::new(player.clone(), "produce a unit", options(&BASE));
    for vector in ti4_policy::projection::mlp_choice_features(
        &seen,
        &choice,
        player,
        &[],
        ti4_policy::progress::Baseline::default(),
    ) {
        names.extend(ti4_policy::features::names_of(&vector));
    }
    Vocabulary::build(names).expect("builds")
}

#[test]
fn the_critic_vector_and_value_are_invariant_to_legal_set_order_and_contents() {
    let (state, player) = position();
    let vocabulary = vocabulary_for_position(&state, &player);

    let base = run(&state, &player, &BASE, &vocabulary);
    let shuffled = run(&state, &player, &SHUFFLED, &vocabulary);
    let shorter = run(&state, &player, &SHORTER, &vocabulary);

    // Non-vacuity: the runs must really have been different decisions, or "V did not move" is a
    // statement about the same decision repeated — which is what the earlier version asserted.
    assert_ne!(
        base.option_columns, shuffled.option_columns,
        "the shuffled run presented the same option vectors, so the order was never varied"
    );
    assert_eq!(
        shorter.option_columns.len(),
        2,
        "the shorter run did not drop an option"
    );
    assert!(base.value.abs() > 0.0, "V = 0, so invariance is vacuous");

    for (label, other) in [("shuffled", &shuffled), ("shorter", &shorter)] {
        assert_eq!(
            other.critic_names, base.critic_names,
            "the critic vector moved with the {label} legal set"
        );
        assert!(
            (other.value - base.value).abs() < f64::EPSILON,
            "V moved with the {label} legal set: {} against {}",
            other.value,
            base.value
        );
    }
}

/// The agreement two orderings of the same options can actually reach.
///
/// The trunk is f32, so reordering the options re-associates the sums behind each logit and behind
/// the softmax normaliser. A few tens of ulps of f32 (eps 1.19e-7) is the floor; measured here it is
/// ~5e-7 on probabilities near 0.5.
///
/// This is not a tuned number. A genuine position dependence does not miss by ppm — when the
/// fixture accidentally made the options indistinguishable the two distributions differed by the
/// whole mass (0.049 against 0.498). The gap between that and 1e-5 is five orders of magnitude, so
/// the bound separates f32 re-association from the defect it is here to catch.
const REASSOCIATION: f64 = 1e-5;

#[test]
fn the_policy_is_not_accidentally_invariant_to_the_same_shuffle() {
    let (state, player) = position();
    let vocabulary = vocabulary_for_position(&state, &player);

    let base = run(&state, &player, &BASE, &vocabulary);
    let shuffled = run(&state, &player, &SHUFFLED, &vocabulary);

    // Non-vacuity, checked on the *inputs* rather than the outputs: the options must occupy
    // different columns. Checking only that probabilities differ numerically is what let the
    // earlier version pass on float noise.
    assert_ne!(
        base.option_columns[0], base.option_columns[1],
        "the options are indistinguishable to the model"
    );
    assert_ne!(
        base.option_columns[1], base.option_columns[2],
        "the options are indistinguishable to the model"
    );
    let spread = base.probabilities.iter().copied().fold(f64::MIN, f64::max)
        - base.probabilities.iter().copied().fold(f64::MAX, f64::min);
    assert!(
        spread > 1e-3,
        "the policy is uniform, so the shuffle proves nothing: {spread}"
    );

    for (shuffled_index, original_index) in ORDER.iter().enumerate() {
        assert!(
            (shuffled.probabilities[shuffled_index] - base.probabilities[*original_index]).abs()
                < REASSOCIATION,
            "probability {original_index} did not follow the shuffle"
        );
    }

    // The same statement in one number, which a permutation bug cannot satisfy by accident.
    let entropy = |p: &[f64]| -> f64 {
        -p.iter()
            .filter(|value| **value > 0.0)
            .map(|value| value * value.ln())
            .sum::<f64>()
    };
    assert!(
        (entropy(&base.probabilities) - entropy(&shuffled.probabilities)).abs() < REASSOCIATION,
        "the shuffle changed the distribution's entropy"
    );
}

#[test]
fn the_critic_input_reports_how_many_columns_it_actually_occupies() {
    // The measurement F-M09-027-3 turns into an acceptance criterion. Against a vocabulary built
    // from the critic's own names every fact has a column of its own; against one that has never
    // seen the family, the whole vector sums onto the global OOV column and `V` is a rank-1
    // projection of the position no matter how rich the position is.
    let (state, player) = position();

    let known = vocabulary_for_position(&state, &player);
    let rich = run(&state, &player, &BASE, &known);
    assert!(
        rich.critic_columns > 1,
        "the critic collapsed onto {} column(s) against its own vocabulary",
        rich.critic_columns
    );

    let stranger = Vocabulary::build(["option:unrelated"]).expect("builds");
    let collapsed = run(&state, &player, &BASE, &stranger);
    assert_eq!(
        collapsed.critic_columns, 1,
        "a vocabulary that has never seen `critic-state` should collapse every fact onto one column"
    );
    assert!(
        rich.critic_columns > collapsed.critic_columns,
        "the two vocabularies are indistinguishable, so this test measures nothing"
    );
}
