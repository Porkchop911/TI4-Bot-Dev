//! The undo of a seat (`ClientMessage::Undo`): a player takes back its own decisions, back to
//! where something hidden was shown, something was rolled or another seat decided.

use std::collections::BTreeSet;
use std::sync::Arc;

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_host::planning::draft::DraftStep;
use ti4_host::planning::steps::PlanStep;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::{RejectionReason, UndoBarrier, ViewerRole};
use ti4_model::id::PlayerId;
use ti4_server::session::{GameRegistry, GameSession};

use crate::draft_scripts::{
    asked, config, movement_script, next_message, passive, ships_at, subtype, to_the_turn_menu,
};

/// A game of six on the server: the first seat is played here, the others at random.
fn game(game_id: &str) -> (Arc<GameRegistry>, PlayerId, String) {
    let registry = Arc::new(GameRegistry::new());
    let (_, seat, credential) = registry
        .create_player_game_with_random_seats(
            game_id.to_owned(),
            6,
            3,
            "Host",
            None,
            None,
            ti4_host::setup::card_set::DEFAULT,
            ti4_host::setup::SpeakerOrder::Seat,
            None,
        )
        .expect("a started game");
    (registry, seat, credential.as_str().to_owned())
}

fn can_undo(session: &GameSession, seat: &PlayerId) -> bool {
    session
        .get_snapshot(&ViewerRole::Player(seat.clone()))
        .history
        .can_undo
}

fn steps(script: &[DraftStep]) -> Vec<PlanStep> {
    script
        .iter()
        .filter_map(|entry| match entry {
            DraftStep::Step(step) => Some(step.clone()),
            DraftStep::Chosen(_) => None,
        })
        .collect()
}

/// A game in which the seat moved one ship, and what the test needs of it.
struct Moved {
    game_id: String,
    registry: Arc<GameRegistry>,
    seat: PlayerId,
    credential: String,
    session: Arc<GameSession>,
    at_the_menu: Vec<ti4_engine::choice::DecisionRecord>,
    script: Vec<DraftStep>,
    target: String,
    unit: String,
    before: usize,
    version: u64,
}

/// Plays the seat to its turn and sends the movement of one ship. In the game of the seed of
/// [`game`] no other seat answers the movement (as Foresight of the Naalu could, whose ships are
/// in the system): the seat may take back all of it. The seats that the server plays decide by
/// their place, so the seed tells the game.
fn moved_alone(game_id: &str) -> Moved {
    let (registry, seat, credential) = game(game_id);
    let session = registry.get_game(game_id).expect("the game runs");
    let (_, nonce, version) = to_the_turn_menu(&session, &seat);
    let at_the_menu = session.decision_log();
    let (script, target, unit) = movement_script(&session, &seat);
    let before = ships_at(&session, &target, &seat, &unit);

    // The seat sends its movement: the game asks what comes after it.
    assert_eq!(
        session.apply_draft(&seat, &nonce, version, &script),
        Ok(None)
    );
    let (_, _, version) = asked(&session, &seat);
    assert_eq!(ships_at(&session, &target, &seat, &unit), before + 1);
    assert!(
        session.decision_log()[at_the_menu.len()..]
            .iter()
            .all(|record| record.player == seat),
        "another seat answers the movement: the test needs a seed where none does"
    );
    Moved {
        game_id: game_id.to_owned(),
        registry,
        seat,
        credential,
        session,
        at_the_menu,
        script,
        target,
        unit,
        before,
        version,
    }
}

#[test]
fn a_seat_takes_back_its_action_and_not_the_decision_of_another_seat() {
    let Moved {
        game_id,
        registry,
        seat,
        credential,
        session,
        at_the_menu,
        script,
        target,
        unit,
        before,
        version,
    } = moved_alone("undo_action");
    let game_id = game_id.as_str();
    assert!(can_undo(&session, &seat));
    // No other viewer may undo, and a version that is not the game's is refused.
    assert!(!can_undo(&session, &PlayerId::new("player_2")));
    assert!(
        !session
            .get_snapshot(&ViewerRole::Spectator)
            .history
            .can_undo
    );
    assert_eq!(
        registry.undo_player(game_id, &credential, &seat, &session, version + 1),
        Err(RejectionReason::StaleVersion {
            expected: version + 1,
            current: version
        })
    );
    assert!(matches!(
        registry.undo_player(game_id, "not the credential", &seat, &session, version),
        Err(RejectionReason::UnauthorizedSeat { .. })
    ));

    // One undo takes the movement back whole: the game asks for the first ship again.
    registry
        .undo_player(game_id, &credential, &seat, &session, version)
        .expect("the undo is taken");
    let session = registry.get_game(game_id).expect("the game runs");
    let (choice, _, version) = asked(&session, &seat);
    assert_eq!(subtype(&choice), "movement_step");
    assert_eq!(ships_at(&session, &target, &seat, &unit), before);
    assert_eq!(session.decision_log().len(), at_the_menu.len() + 2);
    let undone = session.undone_for(&seat);
    let sent = steps(&script);
    assert_eq!(undone.first(), sent.first());
    assert_eq!(undone.last(), Some(&PlanStep::DoneMoving));
    assert_eq!(
        session.undone_for(&seat),
        undone,
        "every connection of the seat is told the steps"
    );

    // Then the activation, then the choice of the action: one answer each.
    assert!(can_undo(&session, &seat));
    registry
        .undo_player(game_id, &credential, &seat, &session, version)
        .expect("the undo is taken");
    let session = registry.get_game(game_id).expect("the game runs");
    let (_, _, version) = asked(&session, &seat);
    assert_eq!(session.decision_log().len(), at_the_menu.len() + 1);
    assert!(session.undone_for(&seat).is_empty());
    registry
        .undo_player(game_id, &credential, &seat, &session, version)
        .expect("the undo is taken");
    let session = registry.get_game(game_id).expect("the game runs");
    let (choice, nonce, mut version) = asked(&session, &seat);
    assert_eq!(subtype(&choice), "action_menu");
    assert_eq!(session.decision_log(), at_the_menu);

    // Before the turn of the seat another seat decided, or the seat learned something: the
    // undo ends at a barrier, and the game is as it is.
    let mut session = session;
    while can_undo(&session, &seat) {
        registry
            .undo_player(game_id, &credential, &seat, &session, version)
            .expect("the undo is taken");
        session = registry.get_game(game_id).expect("the game runs");
        version = asked(&session, &seat).2;
    }
    let log = session.decision_log();
    assert!(
        matches!(
            registry.undo_player(game_id, &credential, &seat, &session, version),
            Err(RejectionReason::UndoBarrier { .. })
        ),
        "{:?}",
        log.last()
    );
    assert_eq!(session.decision_log(), log);

    // The game goes on from where the undo left it: the movement is sent again.
    let (_, nonce, version) = if log == at_the_menu {
        (choice, nonce, version)
    } else {
        to_the_turn_menu(&session, &seat)
    };
    assert_eq!(
        session.apply_draft(&seat, &nonce, version, &script),
        Ok(None)
    );
    asked(&session, &seat);
    assert_eq!(ships_at(&session, &target, &seat, &unit), before + 1);
    session.stop();
}

/// Plays `seat` passively for `own` of its decisions. At each choice of the seat it notes why
/// the seat may not undo, when it may not.
fn barriers(session: &GameSession, seat: &PlayerId, own: usize) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for _ in 0..own {
        let (choice, nonce, version) = asked(session, seat);
        let last_is_own = session
            .decision_log()
            .last()
            .is_some_and(|record| record.player == *seat);
        match session.undo_plan(seat, version) {
            Ok(_) => {
                assert!(last_is_own, "a seat takes back its own decision only");
                assert!(can_undo(session, seat));
            }
            Err(RejectionReason::UndoBarrier { barrier }) => {
                assert!(!can_undo(session, seat));
                if barrier == UndoBarrier::OtherSeat {
                    assert!(!last_is_own);
                } else {
                    assert!(last_is_own, "{barrier:?} is of the decision of the seat");
                }
                found.insert(format!("{barrier:?}"));
            }
            Err(other) => assert_eq!(other, RejectionReason::NoPendingChoice),
        }
        session
            .submit_choice(seat, &nonce, version, &passive(&choice))
            .expect("the answer is taken");
    }
    found
}

#[test]
fn slow_a_card_that_is_drawn_ends_the_undo_and_a_recovered_game_knows_it() {
    let (straight, seat) = config("undo_barriers", 11);
    let session = GameSession::start(straight);
    // A seat that only passes rolls nothing after a decision of its own: the die is in the
    // tests of `session/undo.rs`. It draws a card in the status phase, which no latch tells.
    let found = barriers(&session, &seat, 120);
    assert!(found.contains("OtherSeat"), "{found:?}");
    assert!(found.contains("Revealed"), "{found:?}");

    // The game that is played again from its log knows the same decisions as its seat's.
    let (_, _, version) = asked(&session, &seat);
    let plan = session.undo_plan(&seat, version);
    let (log, events) = (session.decision_log(), session.event_log());
    session.stop();
    let (again, _) = config("undo_barriers_again", 11);
    let recovered = GameSession::start_recovered(again, log, events);
    recovered.wait_replayed().expect("the replay ends");
    let (_, _, version) = asked(&recovered, &seat);
    // A recovered session has no entries of its own: its first look at the game finds decisions
    // no seat of the session answered, so it is cut as `OtherSeat`. When the live game stands
    // right after an own decision of the seat without any barrier, the plans differ by that.
    let again_plan = recovered.undo_plan(&seat, version);
    if plan.is_ok() {
        assert_eq!(
            again_plan,
            Err(RejectionReason::UndoBarrier {
                barrier: UndoBarrier::OtherSeat
            })
        );
    } else {
        assert_eq!(again_plan, plan);
    }
    recovered.stop();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_undo_over_the_websocket_ends_the_connection_and_the_next_snapshot_has_the_steps() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    let game_id = "undo_socket".to_owned();
    let (registry, seat, credential) = game(&game_id);
    let app = ti4_server::create_app(registry.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.expect("the server runs") });
    let session = registry.get_game(&game_id).expect("the game runs");
    let (script, nonce, version) = {
        let (session, seat) = (session.clone(), seat.clone());
        tokio::task::spawn_blocking(move || {
            let (_, nonce, version) = to_the_turn_menu(&session, &seat);
            (movement_script(&session, &seat).0, nonce, version)
        })
        .await
        .unwrap()
    };

    let send =
        |message: ClientMessage| Message::Text(serde_json::to_string(&message).unwrap().into());
    let subscribe = || {
        send(ClientMessage::Subscribe {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            player_session: Some(credential.clone()),
            staged: false,
        })
    };
    let undo = |expected_version: u64| {
        send(ClientMessage::Undo {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            expected_version,
        })
    };
    let url = format!("ws://{address}/ws/games/{game_id}");
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    socket.send(subscribe()).await.unwrap();
    next_message(&mut socket, |message| {
        matches!(message, ServerMessage::InitialSnapshot(_))
    })
    .await;
    socket
        .send(send(ClientMessage::ApplyDraft {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            nonce,
            expected_version: version,
            script: script.clone(),
        }))
        .await
        .unwrap();
    let update = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::StateUpdate(update)
            if update.pending_choice.is_some() && update.history.can_undo)
    })
    .await;
    let ServerMessage::StateUpdate(update) = update else {
        unreachable!()
    };

    // An undo of a game that went on is refused, and the connection stays.
    socket.send(undo(version)).await.unwrap();
    let rejected = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::ActionRejected(_))
    })
    .await;
    let ServerMessage::ActionRejected(rejected) = rejected else {
        unreachable!()
    };
    assert!(matches!(
        rejected.reason,
        RejectionReason::StaleVersion { .. }
    ));

    // The undo replaces the game: this connection is closed as replaced.
    socket.send(undo(update.game_version)).await.unwrap();
    let closed = tokio::time::timeout(std::time::Duration::from_secs(60), async {
        loop {
            match socket.next().await {
                Some(Ok(Message::Close(frame))) => return frame.map(|frame| u16::from(frame.code)),
                Some(Ok(_)) => {}
                other => panic!("the connection ends without a close: {other:?}"),
            }
        }
    })
    .await
    .expect("the connection is closed");
    assert_eq!(closed, Some(4001));

    // The page subscribes again: the movement is open, and every tab of the seat is told its
    // steps.
    let (mut socket, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    socket.send(subscribe()).await.unwrap();
    let snapshot = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::InitialSnapshot(_))
    })
    .await;
    let ServerMessage::InitialSnapshot(snapshot) = snapshot else {
        unreachable!()
    };
    assert!(snapshot.game_version > update.game_version);
    assert_eq!(
        subtype(&snapshot.pending_choice.expect("a choice").choice),
        "movement_step"
    );
    assert!(snapshot.history.can_undo, "the activation can go back too");
    assert_eq!(snapshot.undone.first(), steps(&script).first());
    assert_eq!(snapshot.undone.last(), Some(&PlanStep::DoneMoving));

    let (mut second, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    second.send(subscribe()).await.unwrap();
    let again = next_message(&mut second, |message| {
        matches!(message, ServerMessage::InitialSnapshot(_))
    })
    .await;
    let ServerMessage::InitialSnapshot(again) = again else {
        unreachable!()
    };
    assert_eq!(again.undone, snapshot.undone);
}
