//! A table seated from a separate-models run (`ppo_update --separate-models`): picking one faction's
//! checkpoint seats every faction on its own model at the same update count.
//!
//! Needs a real run, so it is ignored by default:
//!
//! ```text
//! TI4_SEPARATE_CHECKPOINT=<out>/checkpoints/sol/checkpoint-N cargo test -p ti4-review --test separate_models -- --ignored
//! ```

use std::path::PathBuf;

use ti4_review::{LiveReview, ProfileTable, SEPARATE_MODELS_FORMAT, SimulationConfig};

#[test]
#[ignore = "needs TI4_SEPARATE_CHECKPOINT pointing into a separate-models run"]
fn a_separate_models_checkpoint_seats_each_faction_on_its_own_model() {
    let checkpoint = PathBuf::from(
        std::env::var("TI4_SEPARATE_CHECKPOINT").expect("TI4_SEPARATE_CHECKPOINT is set"),
    );
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    while !root.join("examples/reviewer/full_np8_12_holdout.json").is_file() {
        assert!(root.pop(), "no workspace root carries the reviewer inputs");
    }
    let config = SimulationConfig {
        checkpoint,
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.5,
        diplomacy: true,
        lineup: None,
        rule_seats: Vec::new(),
    };
    let mut review = LiveReview::start(&config).expect("the separate models start a review");
    let policy = review.session.manifest.policy.clone();
    assert_eq!(policy.format, SEPARATE_MODELS_FORMAT);
    assert_eq!(policy.profiles.len(), 6, "{:?}", policy.profiles);
    println!("{:#?}", policy.profiles);
    review.advance(ti4_review::AdvanceUnit::Step, 200);
    assert!(
        review.session.frames.iter().any(|frame| !frame.decisions.is_empty()),
        "the table made decisions"
    );
}
