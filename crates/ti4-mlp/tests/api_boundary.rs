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
fn the_status_reports_fallbacks_rather_than_a_clean_result() {
    // A temperature the actor must refuse turns every decision into a counted fallback.
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
    let answered = ti4_engine::choice::ask_private(
        &choice,
        &state,
        content,
        ti4_model::content_types::POK,
        None,
        decider.as_mut(),
    )
    .expect("the game still receives a legal answer");
    assert_eq!(answered.id, "x", "a fallback must still be legal");

    let error = status
        .into_result()
        .expect_err("a run containing a fallback is not a success");
    assert_eq!(error.fallbacks, 1, "the fallback was not counted");
}
