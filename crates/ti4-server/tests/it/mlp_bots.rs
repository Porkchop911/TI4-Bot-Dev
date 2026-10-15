//! MLP seats on the server (`ti4_host::bots`): a game recovered from its decisions goes on as the
//! game that was not interrupted, also when it is cut in the middle of an activation of an MLP
//! seat, between the fleet decision and the moves that its plan answers.

use std::time::Duration;

use ti4_engine::choice::DecisionRecord;
use ti4_host::bots::BotKind;
use ti4_server::session::{GameSession, SeatController, SessionConfig};

use crate::random_bots::{config_with, drive};

/// The seats of the tests: two MLP seats and a rule bot.
fn seats() -> Vec<SeatController> {
    let mlp = SeatController::Bot(BotKind::Mlp(ti4_host::bots::test_model()));
    vec![mlp.clone(), SeatController::SMART, mlp]
}

fn config(id: &str) -> SessionConfig {
    config_with(id, 5, "te", &seats())
}

/// The first `count` decisions of the game of `config`.
fn play(config: SessionConfig, count: usize) -> Vec<DecisionRecord> {
    let session = GameSession::start(config);
    let out = drive(
        &session,
        count,
        Duration::from_secs(60),
        Duration::from_secs(600),
    );
    session.stop();
    assert!(out.error.is_none(), "{:?}", out.error);
    assert!(out.decisions.len() >= count, "the game stopped early");
    out.decisions[..count].to_vec()
}

#[test]
fn slow_a_game_cut_in_an_activation_of_an_mlp_seat_goes_on_as_before() {
    const DECISIONS: usize = 450;
    let reference = play(config("mlp_ref"), DECISIONS);
    // The seats of the models are p1 and p3. A cut before a move of theirs falls between the
    // activation (and its fleet decision) and the moves that its plan answers.
    let moves = |record: &DecisionRecord| {
        record
            .context
            .as_ref()
            .is_some_and(|context| context.subtype == "movement_step")
    };
    // The first move of each activation of theirs.
    let mut cuts: Vec<usize> = (1..reference.len())
        .filter(|&index| {
            let (before, record) = (&reference[index - 1], &reference[index]);
            matches!(record.player.as_str(), "p1" | "p3")
                && moves(record)
                && !(moves(before) && before.player == record.player)
        })
        .collect();
    assert!(!cuts.is_empty(), "the MLP seats move ships");
    cuts.push(DECISIONS / 2);
    for cut in cuts {
        let recovered = play(
            config(&format!("mlp_cut_{cut}"))
                .with_prior_history(reference[..cut].to_vec(), Vec::new()),
            DECISIONS,
        );
        for (number, (again, before)) in recovered.iter().zip(&reference).enumerate() {
            assert_eq!(
                again, before,
                "decision {number} of the game recovered after {cut} decisions (left)"
            );
        }
    }
}

/// A seat of a record of an earlier build reads as the kind it was; an MLP seat needs its model.
#[test]
fn a_seat_controller_is_read_from_the_records_of_every_build() {
    let model = ti4_host::bots::test_model();
    let read = |text: &str| serde_json::from_str::<SeatController>(text);
    assert_eq!(read("\"BotSmart\"").unwrap(), SeatController::SMART);
    assert_eq!(read("\"BotRandom\"").unwrap(), SeatController::RANDOM);
    assert_eq!(read(r#"{"Bot":"smart"}"#).unwrap(), SeatController::SMART);
    let mlp = SeatController::Bot(BotKind::Mlp(model));
    assert_eq!(read(r#"{"Bot":"mlp:test"}"#).unwrap(), mlp);
    assert_eq!(
        serde_json::to_string(&mlp).unwrap(),
        r#"{"Bot":"mlp:test"}"#
    );
    let error = read(r#"{"Bot":"mlp:nobody"}"#).unwrap_err().to_string();
    assert!(error.contains("'nobody'"), "{error}");
}
