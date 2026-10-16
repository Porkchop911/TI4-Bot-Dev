//! The two script messages: a draft that is run on a copy of the game (`RunDraft`), and a
//! script that is sent to the live game (`ApplyDraft`). The scripts are those of the tests of
//! `ti4_host::planning::draft`, played against a session.

use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ti4_content::ContentStore;
use ti4_engine::choice::Choice;
use ti4_host::planning::audit::PlanScope;
use ti4_host::planning::draft::{DraftOutcome, DraftStep, DraftStop};
use ti4_host::planning::steps::PlanStep;
use ti4_host::protocol::PROTOCOL_VERSION;
use ti4_host::protocol::client::ClientMessage;
use ti4_host::protocol::server::ServerMessage;
use ti4_host::protocol::status::{RejectionReason, ViewerRole};
use ti4_host::view::tactical::TacticalFacts;
use ti4_model::content_types::POK;
use ti4_model::id::{PlayerId, SystemId};
use ti4_server::session::{GameRegistry, GameSession, MockClient, SeatController, SessionConfig};

const DEADLINE: Duration = Duration::from_secs(60);

/// A seeded game of six on the recommended map: seat A is played here, the others at random.
pub(crate) fn config(game_id: &str, seed: u64) -> (SessionConfig, PlayerId) {
    let content = ContentStore::embedded();
    let ids: Vec<PlayerId> = ["a", "b", "c", "d", "e", "f"]
        .iter()
        .map(|name| PlayerId::new(*name))
        .collect();
    let loader = ti4_host::maps::TemplateLoader::load().expect("the templates load");
    let template =
        ti4_host::maps::default_template_for(content, &loader, 6, POK).expect("a template for six");
    let (state, galaxy) =
        ti4_host::map::create_game_with_template(content, &ids, seed, Some(&template))
            .expect("a game");
    let tiles = ti4_host::map::build_board_tiles(content, &galaxy);
    let mut config = SessionConfig::new(game_id, state)
        .with_seed(seed)
        .with_player_ids(ids.clone())
        .with_galaxy(galaxy, tiles);
    for (index, seat) in ids.iter().enumerate() {
        let controller = if index == 0 {
            SeatController::Human
        } else {
            SeatController::RANDOM
        };
        config = config.with_seat(seat.clone(), controller);
    }
    (config, ids[0].clone())
}

/// The choice that the game waits in, once it is one of `seat`.
pub(crate) fn asked(session: &GameSession, seat: &PlayerId) -> (Choice, String, u64) {
    let deadline = Instant::now() + DEADLINE;
    loop {
        assert!(session.error().is_none(), "{:?}", session.error());
        assert!(Instant::now() < deadline, "the game does not ask the seat");
        // A choice that already has its answer (of a script, or one on its way) is not open.
        if session.history_ready()
            && let Some((asked, nonce, version)) = session.current_pending_decision()
            && asked == *seat
            && let Some(pending) = session
                .get_snapshot(&ViewerRole::Player(seat.clone()))
                .pending_choice
            && pending.nonce == nonce
        {
            return (pending.choice, nonce, version);
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(crate) fn subtype(choice: &Choice) -> &str {
    choice
        .context
        .as_ref()
        .map_or("", |context| context.subtype.as_str())
}

/// The answer of a seat that only wants the game to go on: decline, else the first option.
pub(crate) fn passive(choice: &Choice) -> String {
    let decline = choice.options.iter().find(|option| option.is_decline());
    decline.unwrap_or(&choice.options[0]).id.clone()
}

/// Plays `seat` passively until the game asks it for the action of its turn.
pub(crate) fn to_the_turn_menu(session: &GameSession, seat: &PlayerId) -> (Choice, String, u64) {
    loop {
        let (choice, nonce, version) = asked(session, seat);
        if subtype(&choice) == "action_menu" {
            return (choice, nonce, version);
        }
        session
            .submit_choice(seat, &nonce, version, &passive(&choice))
            .expect("the answer is taken");
    }
}

fn choose(id: &str) -> DraftStep {
    DraftStep::choose(id)
}

fn draft(session: &GameSession, seat: &PlayerId, script: &[DraftStep]) -> DraftOutcome {
    session.run_draft(seat, &PlanScope::Tactical, script).1
}

/// The script of a whole tactical action with one ship moved, and the system and the ship.
pub(crate) fn movement_script(
    session: &GameSession,
    seat: &PlayerId,
) -> (Vec<DraftStep>, String, String) {
    let mut script = vec![choose("tactical")];
    let activation = draft(session, seat, &script);
    assert_eq!(activation.stop, DraftStop::Open);
    let Some(TacticalFacts::Activation(facts)) = &activation.update.tactical else {
        panic!("no activation facts");
    };
    let target = facts
        .systems
        .iter()
        .find(|reach| reach.ships > 0)
        .expect("a system in range")
        .system
        .clone();
    script.push(choose(&target));
    let movement = draft(session, seat, &script);
    assert_eq!(movement.stop, DraftStop::Open);
    assert_eq!(movement.movement.as_ref(), Some(&movement.update));
    let Some(TacticalFacts::Movement(facts)) = &movement.update.tactical else {
        panic!("no movement facts");
    };
    let ship = facts
        .ships
        .iter()
        .find(|ship| ship.r#move.is_some())
        .expect("a ship that can move");
    let plan = ship.r#move.as_ref().unwrap();
    script.extend([
        DraftStep::Step(PlanStep::Move {
            origin: ship.origin.clone(),
            unit: ship.unit.clone(),
            damaged: ship.damaged,
            gravity_drive: plan.gravity_drive,
            ionian: plan.ionian,
        }),
        DraftStep::Step(PlanStep::DoneLoading),
        DraftStep::Step(PlanStep::DoneMoving),
    ]);
    (script, target, ship.unit.clone())
}

/// What the seat is told of its script, once it is told.
fn applied(client: &MockClient) -> ti4_host::protocol::server::DraftAppliedMsg {
    let deadline = Instant::now() + DEADLINE;
    loop {
        assert!(
            Instant::now() < deadline,
            "the seat is not told of its script"
        );
        match client.try_recv() {
            Ok(ServerMessage::DraftApplied(message)) => return message,
            Ok(_) => {}
            Err(_) => std::thread::sleep(Duration::from_millis(2)),
        }
    }
}

pub(crate) fn ships_at(session: &GameSession, system: &str, seat: &PlayerId, unit: &str) -> usize {
    let state = session.current_state();
    state
        .system_state(&SystemId::new(system))
        .units
        .iter()
        .filter(|ship| ship.owner == *seat && ship.type_id.as_str() == unit)
        .count()
}

#[test]
fn a_draft_is_run_on_a_copy_with_the_facts_of_a_live_update() {
    let (config, seat) = config("drafts_run", 3);
    let session = GameSession::start(config);
    let (_, nonce, version) = to_the_turn_menu(&session, &seat);
    let decisions = session.decision_log().len();
    let state = session.current_state();

    let (base, menu) = session.run_draft(&seat, &PlanScope::Tactical, &[]);
    assert_eq!(base, version);
    assert_eq!(menu.stop, DraftStop::Open);
    let offered = &menu.update.pending_choice.as_ref().unwrap().choice;
    assert_eq!(subtype(offered), "action_menu");
    assert_eq!(
        offered.options.len(),
        1,
        "only the tactical action is shown"
    );
    assert!(menu.update.exploration.is_some() && menu.update.score.is_some());

    let (script, target, unit) = movement_script(&session, &seat);
    let before = ships_at(&session, &target, &seat, &unit);
    let moved = draft(&session, &seat, &script);
    assert_eq!(moved.stop, DraftStop::Complete);
    assert_eq!(moved.consumed, script.len());
    let arrived = &moved.update.view.board.systems[&SystemId::new(target.as_str())].units;
    assert_eq!(
        arrived
            .iter()
            .filter(|ship| ship.owner == seat && ship.unit_type.as_str() == unit)
            .count(),
        before + 1
    );

    let refused = draft(
        &session,
        &seat,
        &[choose("tactical"), choose("no such system")],
    );
    assert!(matches!(refused.stop, DraftStop::Refused { step: 1, .. }));

    // Nothing of a draft is in the game: the same choice waits, and it can be answered.
    assert_eq!(session.decision_log().len(), decisions);
    assert_eq!(session.current_state(), state);
    assert_eq!(
        session.current_pending_decision(),
        Some((seat.clone(), nonce, version))
    );
    // A seat that is not of the game gets a failed run, not a copy.
    let stranger = draft(&session, &PlayerId::new("nobody"), &[]);
    assert!(matches!(stranger.stop, DraftStop::Failed { .. }));
    session.stop();
}

#[test]
fn a_script_that_is_sent_plays_the_action_in_the_live_game() {
    let (config, seat) = config("drafts_apply", 3);
    let session = Arc::new(GameSession::start(config));
    let (_, nonce, version) = to_the_turn_menu(&session, &seat);
    let (script, target, unit) = movement_script(&session, &seat);
    let before = ships_at(&session, &target, &seat, &unit);
    let decisions = session.decision_log().len();
    let client = MockClient::connect(session.clone(), ViewerRole::Player(seat.clone()));

    assert_eq!(
        session.apply_draft(&seat, "another nonce", version, &script),
        Err(RejectionReason::StaleNonce)
    );
    assert_eq!(
        session.apply_draft(&seat, &nonce, version, &script),
        Ok(None)
    );
    let told = applied(&client);
    assert_eq!(told.nonce, nonce);
    assert_eq!(told.stopped, None, "the game played the whole script");

    // The game goes on to the next choice of the seat with the action in it.
    let (next, ..) = asked(&session, &seat);
    assert_ne!(subtype(&next), "movement_step");
    assert_eq!(ships_at(&session, &target, &seat, &unit), before + 1);
    let log = session.decision_log();
    let own: Vec<&str> = log[decisions..]
        .iter()
        .filter(|record| record.player == seat)
        .map(|record| record.chosen.as_str())
        .collect();
    assert_eq!(own[..2], ["tactical", target.as_str()]);
    assert!(own.contains(&"done_moving"), "{own:?}");
    session.stop();
}

#[test]
fn a_script_that_the_game_would_refuse_changes_nothing() {
    let (config, seat) = config("drafts_refuse", 3);
    let session = Arc::new(GameSession::start(config));
    let (_, nonce, version) = to_the_turn_menu(&session, &seat);
    let (script, target, _) = movement_script(&session, &seat);
    let decisions = session.decision_log().len();

    // The third entry is refused: the two before it are not sent either.
    let mut wrong = script[..2].to_vec();
    wrong.push(DraftStep::Step(PlanStep::Move {
        origin: "nowhere".to_owned(),
        unit: "carrier".to_owned(),
        damaged: false,
        gravity_drive: false,
        ionian: false,
    }));
    wrong.push(DraftStep::Step(PlanStep::DoneMoving));
    let stopped = session
        .apply_draft(&seat, &nonce, version, &wrong)
        .expect("the request is of the open choice")
        .expect("the script is refused");
    assert_eq!(stopped.applied, 0);
    assert!(!stopped.interrupted);
    assert_eq!(stopped.remaining, wrong);

    // An answer that the game does not ask for after the action is refused too.
    let mut long = script.clone();
    long.push(choose("one more"));
    let stopped = session
        .apply_draft(&seat, &nonce, version, &long)
        .unwrap()
        .expect("the script is refused");
    assert_eq!((stopped.applied, stopped.interrupted), (0, false));

    // And so is a script whose first entry is not of the open choice.
    let stopped = session
        .apply_draft(&seat, &nonce, version, &[choose(&target)])
        .unwrap()
        .expect("the script is refused");
    assert_eq!(stopped.applied, 0);

    assert_eq!(session.decision_log().len(), decisions);
    assert_eq!(
        session.current_pending_decision(),
        Some((seat.clone(), nonce.clone(), version))
    );
    // The choice is open again: a part of the script is taken, and the game waits at the
    // movement for the seat.
    assert_eq!(
        session.apply_draft(&seat, &nonce, version, &script[..2]),
        Ok(None)
    );
    let (next, ..) = asked(&session, &seat);
    assert_eq!(subtype(&next), "movement_step");
    // What stages the movement is sent to the seat that is asked, and to nobody else.
    let shown = |viewer: &ViewerRole| session.get_snapshot(viewer);
    let own = shown(&ViewerRole::Player(seat.clone()));
    assert!(matches!(own.tactical, Some(TacticalFacts::Movement(_))));
    println!(
        "the facts of a movement of six seats: {} bytes",
        serde_json::to_string(&own.tactical).unwrap().len()
    );
    for other in [
        ViewerRole::Player(PlayerId::new("b")),
        ViewerRole::Spectator,
    ] {
        let snapshot = shown(&other);
        assert_eq!(snapshot.tactical, None, "{other:?}");
        assert_eq!(snapshot.pending_choice, None, "{other:?}");
        assert!(snapshot.exploration.is_some(), "{other:?}");
    }
    session.stop();
}

pub(crate) async fn next_message<S>(
    stream: &mut S,
    wanted: impl Fn(&ServerMessage) -> bool,
) -> ServerMessage
where
    S: StreamExt<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    let found = async {
        while let Some(Ok(message)) = stream.next().await {
            if let Ok(text) = message.to_text()
                && let Ok(parsed) = serde_json::from_str::<ServerMessage>(text)
                && wanted(&parsed)
            {
                return parsed;
            }
        }
        panic!("the stream ended without the message");
    };
    tokio::time::timeout(DEADLINE, found)
        .await
        .expect("the message arrives")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_script_messages_are_answered_over_the_websocket() {
    let registry = Arc::new(GameRegistry::new());
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
    let address = listener.local_addr().expect("an address");
    let app = ti4_server::create_app(registry.clone());
    tokio::spawn(async move { axum::serve(listener, app).await.expect("the server runs") });

    // A game with random seats, as `POST /api/games` with `fill_seats` starts it.
    let game_id = "game_scripts".to_owned();
    let (_, seat, credential) = registry
        .create_player_game_with_random_seats(
            game_id.clone(),
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
    let session = registry.get_game(&game_id).expect("the game runs");
    let (_, nonce, version) = {
        let session = session.clone();
        let seat = seat.clone();
        tokio::task::spawn_blocking(move || to_the_turn_menu(&session, &seat))
            .await
            .unwrap()
    };

    let (mut socket, _) =
        tokio_tungstenite::connect_async(format!("ws://{address}/ws/games/{game_id}"))
            .await
            .expect("a websocket");
    let send =
        |message: ClientMessage| Message::Text(serde_json::to_string(&message).unwrap().into());
    socket
        .send(send(ClientMessage::Subscribe {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            player_session: Some(credential.as_str().to_owned()),
            staged: false,
        }))
        .await
        .unwrap();
    let snapshot = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::InitialSnapshot(_))
    })
    .await;
    let ServerMessage::InitialSnapshot(snapshot) = snapshot else {
        unreachable!()
    };
    assert!(
        snapshot.exploration.is_some(),
        "a game with its map has the facts"
    );
    assert!(snapshot.action_cards.is_some() && snapshot.score.is_some());

    socket
        .send(send(ClientMessage::RunDraft {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            request_id: "draft-1".to_owned(),
            script: vec![choose("tactical")],
            scope: PlanScope::Tactical,
        }))
        .await
        .unwrap();
    let outcome = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::DraftOutcome(_))
    })
    .await;
    let ServerMessage::DraftOutcome(outcome) = outcome else {
        unreachable!()
    };
    assert_eq!(outcome.request_id, "draft-1");
    assert_eq!(outcome.base_version, version);
    assert_eq!(outcome.outcome.stop, DraftStop::Open);
    assert!(matches!(
        outcome.outcome.update.tactical,
        Some(TacticalFacts::Activation(_))
    ));

    // A refused script is answered to the connection that sent it.
    let apply = |script: Vec<DraftStep>| {
        send(ClientMessage::ApplyDraft {
            protocol_version: PROTOCOL_VERSION,
            game_id: game_id.clone(),
            nonce: nonce.clone(),
            expected_version: version,
            script,
        })
    };
    socket
        .send(apply(vec![choose("tactical"), choose("no such system")]))
        .await
        .unwrap();
    let told = |message: &ServerMessage| matches!(message, ServerMessage::DraftApplied(_));
    let ServerMessage::DraftApplied(refused) = next_message(&mut socket, told).await else {
        unreachable!()
    };
    assert_eq!(refused.nonce, nonce);
    assert_eq!(refused.stopped.expect("the script is refused").applied, 0);

    // A script that the game takes is told to the seat when it ends, and the game goes on.
    socket.send(apply(vec![choose("tactical")])).await.unwrap();
    let ServerMessage::DraftApplied(played) = next_message(&mut socket, told).await else {
        unreachable!()
    };
    assert_eq!(played.stopped, None);
    let update = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::StateUpdate(update) if update.pending_choice.is_some())
    })
    .await;
    let ServerMessage::StateUpdate(update) = update else {
        unreachable!()
    };
    assert!(matches!(
        update.tactical,
        Some(TacticalFacts::Activation(_))
    ));

    // A request with the nonce of the choice before is told as any stale answer is.
    socket.send(apply(vec![choose("tactical")])).await.unwrap();
    let rejected = next_message(&mut socket, |message| {
        matches!(message, ServerMessage::ActionRejected(_))
    })
    .await;
    let ServerMessage::ActionRejected(rejected) = rejected else {
        unreachable!()
    };
    assert_eq!(rejected.reason, RejectionReason::StaleNonce);
}
