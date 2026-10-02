//! Helpers to generate protocol golden fixtures for tests and documentation.

use serde_json::json;
use std::collections::BTreeMap;
use ti4_engine::choice::{Choice, ChoiceOption};
use ti4_engine::decision_context::{
    ConstraintKind, DecisionContext, DecisionSource, OutstandingConstraint,
};
use ti4_model::id::{
    ActionCardId, FactionId, ObjectiveId, PlayerId, SecretObjectiveId, StrategyCardId, SystemId,
    UnitTypeId,
};
use ti4_model::state::{GameState, Phase, SystemState};
use ti4_model::units::Unit;

use crate::projection::project_initial_snapshot;
use crate::protocol::PROTOCOL_VERSION;
use crate::protocol::server::{ActionRejectedMsg, GameOverMsg, ServerMessage};
use crate::protocol::status::{RejectionReason, ViewerRole};

/// Creates a standard sample game state for fixture generation and testing.
///
/// # Panics
///
/// Panics if the created player seats cannot be found in the fresh game state.
#[must_use]
pub fn create_sample_game() -> GameState {
    let seat_a = PlayerId::new("seat_a");
    let seat_b = PlayerId::new("seat_b");
    let seat_c = PlayerId::new("seat_c");
    let players = [seat_a.clone(), seat_b.clone(), seat_c.clone()];

    let mut state = GameState::new(&players, &[], BTreeMap::new(), None, 1);
    state.round = 2;
    state.phase = Phase::Action;
    state.active = Some(seat_a.clone());
    state.speaker = seat_a.clone();

    // Configure Player A
    {
        let p_a = state.player_mut(&seat_a).unwrap();
        p_a.faction = FactionId::new("sol");
        p_a.victory_points = 3;
        p_a.trade_goods = 4;
        p_a.commodities = 2;
        p_a.tactic_tokens = 3;
        p_a.fleet_tokens = 4;
        p_a.strategic_tokens = 2;
        p_a.strategy_cards = vec![StrategyCardId::new("leadership")];
        p_a.action_cards = vec![
            ActionCardId::new("direct_hit"),
            ActionCardId::new("morale_boost"),
        ];
        p_a.secret_objectives = vec![SecretObjectiveId::new("destroy_their_greatest_ship")];
    }

    // Configure Player B
    {
        let p_b = state.player_mut(&seat_b).unwrap();
        p_b.faction = FactionId::new("hacan");
        p_b.victory_points = 2;
        p_b.trade_goods = 8;
        p_b.commodities = 6;
        p_b.tactic_tokens = 2;
        p_b.fleet_tokens = 3;
        p_b.strategic_tokens = 3;
        p_b.strategy_cards = vec![StrategyCardId::new("trade")];
        p_b.action_cards = vec![ActionCardId::new("flank_speed")];
        p_b.secret_objectives = vec![SecretObjectiveId::new("brave_the_void")];
    }

    // Configure Player C
    {
        let p_c = state.player_mut(&seat_c).unwrap();
        p_c.faction = FactionId::new("letnev");
        p_c.victory_points = 1;
        p_c.trade_goods = 1;
        p_c.strategy_cards = vec![StrategyCardId::new("warfare")];
        p_c.action_cards = vec![ActionCardId::new("shields_holding")];
        p_c.secret_objectives = vec![SecretObjectiveId::new("unveil_flagship")];
    }

    // Add board systems and units
    let sys_18 = SystemId::new("18");
    let mut s18 = SystemState::default();
    s18.units.extend_from_slice(&[
        Unit::new(UnitTypeId::new("carrier"), seat_a.clone()),
        Unit::new(UnitTypeId::new("fighter"), seat_a.clone()),
    ]);
    s18.command_tokens.insert(seat_a.clone());
    state.board.insert(sys_18, s18);

    state.revealed_objectives = vec![
        ObjectiveId::new("corner_the_market"),
        ObjectiveId::new("develop_weaponry"),
    ];

    state
}

/// Creates a sample pending choice with decision context and an outstanding constraint.
#[must_use]
pub fn create_sample_pending_choice() -> Choice {
    let seat_a = PlayerId::new("seat_a");
    let options = vec![
        ChoiceOption::labelled("opt_carrier", "produce_unit", "Produce Carrier (Cost: 3)")
            .with("cost", json!(3))
            .with("unit", json!("carrier")),
        ChoiceOption::labelled(
            "opt_infantry",
            "produce_unit",
            "Produce 2 Infantry (Cost: 1)",
        )
        .with("cost", json!(1))
        .with("unit", json!("infantry")),
        ChoiceOption::decline(),
    ];

    let context = DecisionContext::new(
        seat_a.clone(),
        DecisionSource::Rule("68".to_owned()),
        "produce_units",
        Phase::Action,
        2,
    )
    .optional(true)
    .owing(OutstandingConstraint::new(ConstraintKind::Resources, 6, 2));

    Choice {
        player: seat_a,
        prompt: "Choose units to produce in Mecatol Rex".to_owned(),
        options,
        context: Some(context),
    }
}

/// Generates the sample actor snapshot message.
#[must_use]
pub fn sample_actor_snapshot() -> ServerMessage {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let msg = project_initial_snapshot(
        "game_12345",
        42,
        &game,
        &ViewerRole::Player(PlayerId::new("seat_a")),
        Some((&choice, "nonce_xyz789")),
    );
    ServerMessage::InitialSnapshot(msg)
}

/// Generates the sample opponent snapshot message.
#[must_use]
pub fn sample_opponent_snapshot() -> ServerMessage {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let msg = project_initial_snapshot(
        "game_12345",
        42,
        &game,
        &ViewerRole::Player(PlayerId::new("seat_b")),
        Some((&choice, "nonce_xyz789")),
    );
    ServerMessage::InitialSnapshot(msg)
}

/// Generates the sample spectator snapshot message.
#[must_use]
pub fn sample_spectator_snapshot() -> ServerMessage {
    let game = create_sample_game();
    let choice = create_sample_pending_choice();
    let msg = project_initial_snapshot(
        "game_12345",
        42,
        &game,
        &ViewerRole::Spectator,
        Some((&choice, "nonce_xyz789")),
    );
    ServerMessage::InitialSnapshot(msg)
}

/// Generates a sample rejected action response due to stale version.
#[must_use]
pub fn sample_stale_submission_rejected() -> ServerMessage {
    ServerMessage::ActionRejected(ActionRejectedMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_12345".to_owned(),
        game_version: 42,
        reason: RejectionReason::StaleVersion {
            expected: 40,
            current: 42,
        },
    })
}

/// Generates a sample terminal game over message.
#[must_use]
pub fn sample_terminal_game_over() -> ServerMessage {
    let mut scores = BTreeMap::new();
    scores.insert(PlayerId::new("seat_a"), 10);
    scores.insert(PlayerId::new("seat_b"), 8);
    scores.insert(PlayerId::new("seat_c"), 7);

    ServerMessage::GameOver(GameOverMsg {
        protocol_version: PROTOCOL_VERSION,
        game_id: "game_12345".to_owned(),
        game_version: 156,
        winner: Some(PlayerId::new("seat_a")),
        final_scores: scores,
    })
}
