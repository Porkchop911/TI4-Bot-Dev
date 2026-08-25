//! MLP plan §4.2's three properties for the value path (M09-027).
//!
//! The first two say `V` does not depend on the legal set. The third exists because a model that
//! ignores option features entirely satisfies both — so invariance alone would let that through,
//! and the policy is checked for the opposite property on the same shuffle.

use ti4_content::ContentStore;
use ti4_engine::choice::{Choice, ChoiceOption, Observed};
use ti4_mlp::{Actor, FactionRow, SparseOption, Width};
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

/// A vocabulary built from the names this position actually emits.
///
/// A synthetic one does not work, and the way it fails is the point: every real name falls to its
/// family's OOV column, so all options collapse onto the same columns and the policy comes out
/// uniform. An earlier version of this test did that and its non-vacuity guard passed on f32
/// accumulation noise — a distribution that "differed" by 2e-6 while `p` and `q` were bit-identical.
fn vocabulary_for(names: &[String]) -> Vocabulary {
    Vocabulary::build(names).expect("builds")
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

#[test]
fn the_critic_vector_and_value_are_invariant_to_legal_set_order_and_contents() {
    let content = ContentStore::embedded();
    let (state, player) = position();
    let seen = Observed::new(&state, content, POK, None);
    let actor = actor();
    let row = FactionRow::of("sol").expect("roster");
    let vocab = vocabulary_for(&ti4_policy::features::names_of(&critic_vector(
        &seen,
        &player,
        CriticFeatures::full(),
        &[],
    )));

    // The critic never receives a choice, so these are the same call. Building the three legal sets
    // anyway is the point: the value must be identical across all of them.
    let base = options(&[
        "produce|fighter@18",
        "produce|scout@19",
        "produce|carrier@20",
    ]);
    let shuffled = options(&[
        "produce|carrier@20",
        "produce|fighter@18",
        "produce|scout@19",
    ]);
    let shorter = options(&["produce|fighter@18", "produce|scout@19"]);

    let vector = critic_vector(&seen, &player, CriticFeatures::full(), &[]);
    let critic = sparse(&vector, &vocab);
    let value = actor.value(&critic, row).expect("a value");

    for legal in [&base, &shuffled, &shorter] {
        let choice = Choice::new(player.clone(), "produce a unit", legal.clone());
        // Constructing the choice and then recomputing must change nothing.
        let again = critic_vector(&seen, &player, CriticFeatures::full(), &[]);
        assert_eq!(
            ti4_policy::features::names_of(&again),
            ti4_policy::features::names_of(&vector),
            "the critic vector moved with the legal set"
        );
        let recomputed = actor.value(&sparse(&again, &vocab), row).expect("a value");
        assert!(
            (recomputed - value).abs() < f64::EPSILON,
            "V moved with the legal set: {recomputed} against {value}"
        );
        let _ = choice;
    }

    // Non-vacuity: the value is not simply zero for every input.
    assert!(
        value.abs() > 0.0,
        "the fixture produced V = 0, so invariance is vacuous"
    );
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
    // §4.2's third test, and the reason the first two are not enough: a model ignoring option
    // features passes both of them. The same shuffle must permute `p` correspondingly and leave
    // its entropy unchanged.
    let content = ContentStore::embedded();
    let (state, player) = position();
    let seen = Observed::new(&state, content, POK, None);
    let actor = actor();
    let row = FactionRow::of("sol").expect("roster");

    let ids = [
        "produce|fighter@18",
        "produce|scout@19",
        "produce|carrier@20",
    ];
    let choice = Choice::new(player.clone(), "produce a unit", options(&ids));
    let order = [2usize, 0, 1];
    let shuffled_ids: Vec<&str> = order.iter().map(|i| ids[*i]).collect();
    let shuffled = Choice::new(player.clone(), "produce a unit", options(&shuffled_ids));

    // The vocabulary must contain the names this choice emits, or every option routes to the same
    // OOV columns and the policy is uniform by construction.
    let emitted: Vec<String> =
        ti4_policy::projection::mlp_choice_features(&seen, &choice, &player, &[])
            .iter()
            .flat_map(ti4_policy::features::names_of)
            .collect();
    let vocab = vocabulary_for(&emitted);

    let extract = |choice: &Choice| -> Vec<SparseOption> {
        ti4_policy::projection::mlp_choice_features(&seen, choice, &player, &[])
            .iter()
            .map(|vector| sparse(vector, &vocab))
            .collect()
    };

    // Non-vacuity, checked on the *inputs* rather than on the outputs: the three options must
    // occupy different columns. Checking only that probabilities differ numerically is what let the
    // earlier version pass on float noise.
    let columns: Vec<Vec<i64>> = extract(&choice)
        .iter()
        .map(|option| {
            let mut c = option.columns.clone();
            c.sort_unstable();
            c.dedup();
            c
        })
        .collect();
    assert_ne!(
        columns[0], columns[1],
        "the options are indistinguishable to the model"
    );
    assert_ne!(
        columns[1], columns[2],
        "the options are indistinguishable to the model"
    );

    let p = actor
        .probabilities(&extract(&choice), "production", row, 1.0)
        .expect("probabilities");
    let q = actor
        .probabilities(&extract(&shuffled), "production", row, 1.0)
        .expect("probabilities");

    // Non-vacuity first: the options must actually score differently, or a uniform policy would
    // satisfy the permutation check trivially — which is exactly the bug this test exists for.
    let spread =
        p.iter().copied().fold(f64::MIN, f64::max) - p.iter().copied().fold(f64::MAX, f64::min);
    assert!(
        spread > 1e-3,
        "the policy is uniform over these options, so the permutation check proves nothing"
    );

    for (shuffled_index, original_index) in order.iter().enumerate() {
        assert!(
            (q[shuffled_index] - p[*original_index]).abs() < REASSOCIATION,
            "probability {original_index} did not follow the shuffle"
        );
    }

    let entropy = |d: &[f64]| -> f64 {
        -d.iter()
            .filter(|x| **x > 0.0)
            .map(|x| x * x.ln())
            .sum::<f64>()
    };
    assert!(
        (entropy(&p) - entropy(&q)).abs() < REASSOCIATION,
        "entropy changed under a permutation"
    );
}
