//! Rule seats: seats the hand-written rule bot answers at a reviewer or replayer table.
//!
//! A table with rule seats must play on, must record which seats they were, must differ from the
//! all-policy table, and must be reproducible from its own inputs, because a replayer branch is
//! rebuilt by replaying those inputs.

use std::path::PathBuf;

use ti4_review::{LiveReview, ProfileTable, SimulationConfig};

fn workspace_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir
            .join("examples/reviewer/checkpoint-473312/slots.json")
            .is_file()
        {
            return dir;
        }
        assert!(dir.pop(), "no workspace root carries the reviewer inputs");
    }
}

fn config(rule_seats: Vec<usize>) -> SimulationConfig {
    let root = workspace_root();
    SimulationConfig {
        checkpoint: root.join("examples/reviewer/checkpoint-473312/slots.json"),
        map_pool: root.join("examples/reviewer/full_np8_12_holdout.json"),
        seed: 4_242,
        rotation: 1,
        table: ProfileTable::Learner,
        temperature: 0.01,
        diplomacy: false,
        lineup: None,
        rule_seats,
    }
}

/// Every answer of the first two rounds, as `seat: chosen`.
fn two_rounds(rule_seats: Vec<usize>) -> (Vec<usize>, Vec<String>) {
    let mut review = LiveReview::start(&config(rule_seats)).expect("the example table starts");
    for round in 0..2 {
        let report = review.advance_to_next_round();
        let finished = review.session.frames.last().expect("frames").finished;
        assert!(
            report.reached_target || finished,
            "round {} did not finish ({} steps)",
            round + 1,
            report.steps
        );
    }
    let answers = review
        .session
        .frames
        .iter()
        .flat_map(|frame| frame.decisions.iter())
        .map(|decision| format!("{}: {:?}", decision.player, decision.chosen))
        .collect();
    (review.session.manifest.rule_seats.clone(), answers)
}

#[test]
fn a_three_against_three_table_plays_records_its_seats_and_repeats() {
    let (recorded, mixed) = two_rounds(vec![0, 2, 4]);
    assert_eq!(recorded, vec![0, 2, 4]);
    let (_, again) = two_rounds(vec![0, 2, 4]);
    assert_eq!(mixed, again, "the same inputs must replay the same table");
    let (none, policy_only) = two_rounds(Vec::new());
    assert!(none.is_empty());
    assert_ne!(mixed, policy_only, "the rule bot never answered");
}

#[test]
fn a_seat_past_the_table_is_refused() {
    assert!(LiveReview::start(&config(vec![6])).is_err());
}
