//! F-M09-026-10/-11: the fail-closed boundary is a property of the API, not of caller discipline.
//!
//! These run in their own process, so `configure_deterministic` is genuinely first.

use ti4_mlp::bot::MlpBot;
use ti4_mlp::{Actor, FactionRow, Width};
use ti4_policy::vocabulary::Vocabulary;

fn bot() -> MlpBot {
    ti4_tensor::configure_deterministic(20_260_821).expect("configured");
    let vocabulary = Vocabulary::build(["option:a", "option:b"]).expect("builds");
    let capacity = i64::try_from(vocabulary.capacity()).expect("fits");
    MlpBot::new(
        Actor::zeros(Width::W128, capacity),
        vocabulary,
        FactionRow::of("sol").expect("in the roster"),
        1,
    )
}

#[test]
fn a_bot_cannot_be_seated_without_receiving_its_inference_status() {
    // The API test. `MlpBot` does not implement `Decider`, so this does not compile:
    //
    //     let d: Box<dyn Decider> = Box::new(MlpBot::new(..));
    //
    // and `seat` is the only route to a boxed decider. Asserted here by construction: the tuple
    // `seat` returns cannot be destructured without binding the status.
    let (decider, status) = bot().seat();
    drop(decider);
    // A bot that answered nothing is a failure, not a success — so even discarding the decider
    // cannot yield a clean result.
    let error = status
        .into_result()
        .expect_err("a campaign where the model answered nothing is not a success");
    assert_eq!(error.decisions, 0);
    assert_eq!(error.fallbacks, 0);
}

#[test]
fn the_position_free_decider_path_refuses_instead_of_guessing() {
    let (mut decider, status) = bot().seat();
    let choice = ti4_engine::choice::Choice::new(
        ti4_model::id::PlayerId::new("a"),
        "position required",
        vec![ti4_engine::choice::ChoiceOption::labelled("x", "kind", "x")],
    );
    let error = decider
        .choose(&choice)
        .expect_err("an MLP without an observation must not guess");
    assert!(matches!(
        error,
        ti4_engine::choice::IllegalChoice::DeciderFailed { .. }
    ));
    assert_eq!(
        status
            .into_result()
            .expect_err("the refusal is recorded")
            .fallbacks,
        1
    );
}

#[test]
fn an_inference_failure_is_a_typed_decider_error_and_a_failed_status() {
    // A temperature the actor must refuse makes the decision itself fail. The status is retained
    // for reporting, but it is no longer what makes the campaign fail closed.
    let seated = bot().at_temperature(0.0);
    let (mut decider, status) = seated.seat();

    let content = ti4_content::ContentStore::embedded();
    let mut state = ti4_engine::fixtures::game(&["a"]);
    state
        .player_mut(&ti4_model::id::PlayerId::new("a"))
        .unwrap()
        .faction = ti4_model::id::FactionId::new("sol");
    let choice = ti4_engine::choice::Choice::new(
        ti4_model::id::PlayerId::new("a"),
        "decide",
        vec![ti4_engine::choice::ChoiceOption::labelled("x", "kind", "x")],
    );
    let error = ti4_engine::choice::ask_private(
        &choice,
        &state,
        content,
        ti4_model::content_types::POK,
        None,
        decider.as_mut(),
    )
    .expect_err("inference failure must propagate through the decider boundary");
    assert!(
        matches!(
            error,
            ti4_engine::choice::IllegalChoice::DeciderFailed { ref reason, .. }
                if reason.contains("temperature")
        ),
        "wrong error: {error}"
    );

    let error = status
        .into_result()
        .expect_err("a run containing a fallback is not a success");
    assert_eq!(error.fallbacks, 1, "the inference failure was not counted");
}
