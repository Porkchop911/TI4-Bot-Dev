//! Planning through real authenticated WebSockets, with the live worker blocked.

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ti4_content::{ContentStore, galaxy::all_systems};
use ti4_engine::fixtures::{game, hub_with_centre, put};
use ti4_model::{
    content_types::POK,
    id::{PlayerId, SystemId},
    state::Phase,
};
use ti4_server::planning::runner::{AttemptIdentity, PlanningEnvelope, PlanningUpdate, StopReason};
use ti4_server::protocol::{
    PROTOCOL_VERSION,
    client::ClientMessage,
    server::{PlanningRejection, ServerMessage},
    status::ViewerRole,
};
use ti4_server::session::{
    GameRegistry, GameSession, SeatController, SessionConfig, registry::HistoryAction,
};
use ti4_server::storage::{
    LobbySlotId, PLAYER_RECORD_VERSION, PersistedLobbyPhase, PlayerLobbyMember, PlayerLobbyRecord,
    PlayerLobbySlot, PlayerSession,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
const DEADLINE: Duration = Duration::from_secs(5);
const A: &str = "player_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const B: &str = "player_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

struct Fixture {
    registry: Arc<GameRegistry>,
    session: Arc<GameSession>,
    address: String,
    origin_a: SystemId,
    origin_b: SystemId,
    target: SystemId,
    centre: SystemId,
    server: tokio::task::JoinHandle<()>,
}

impl Fixture {
    async fn new(cargo: bool) -> Self {
        let empty = all_systems(ContentStore::embedded(), POK)
            .iter()
            .find(|(_, system)| {
                system.planets().is_empty() && !system.is_anomaly() && !system.is_hyperlane()
            })
            .unwrap()
            .0
            .to_string();
        let hub = hub_with_centre(&empty);
        let origin_b = SystemId::new(&hub.outer[0]);
        let origin_a = SystemId::new(&hub.outer[1]);
        let centre = SystemId::new(&hub.centre);
        let target = if cargo {
            centre.clone()
        } else {
            SystemId::new(hub.across(origin_b.as_str()))
        };
        let mut state = game(&[A, B]);
        state.phase = Phase::Action;
        state.active = Some(PlayerId::new(A));
        for (player, origin, ship) in [
            (A, &origin_a, if cargo { "carrier" } else { "destroyer" }),
            (B, &origin_b, if cargo { "carrier" } else { "cruiser" }),
        ] {
            put(&mut state, origin, ship, &PlayerId::new(player), 1);
            if cargo {
                put(&mut state, origin, "fighter", &PlayerId::new(player), 1);
            }
        }
        let mut config = SessionConfig::new("planning_transport", state)
            .with_galaxy(hub.galaxy, vec![])
            .with_player_ids(vec![PlayerId::new(A), PlayerId::new(B)])
            .with_seat(PlayerId::new(A), SeatController::Human)
            .with_seat(PlayerId::new(B), SeatController::Human);
        let players = [A, B]
            .into_iter()
            .map(|id| {
                let player = PlayerId::new(id);
                let credential = PlayerSession::generate();
                config
                    .seat_tokens
                    .insert(player.clone(), credential.as_str().into());
                (
                    player,
                    PlayerLobbyMember {
                        legacy_ready: None,
                        bot: None,
                        faction: None,
                        session: credential,
                        nickname: if id == A { "A" } else { "B" }.into(),
                    },
                )
            })
            .collect();
        let lobby = PlayerLobbyRecord {
            schema_version: PLAYER_RECORD_VERSION,
            game_id: config.game_id.clone(),
            phase: PersistedLobbyPhase::Running,
            host_player_id: PlayerId::new(A),
            slots: [A, B]
                .into_iter()
                .enumerate()
                .map(|(i, id)| PlayerLobbySlot {
                    slot_id: LobbySlotId(format!("slot_{}", i + 1)),
                    occupant: Some(PlayerId::new(id)),
                })
                .collect(),
            players,
            seed: 42,
            seat_order: None,
            bot_delay_ms: None,
            start_from: None,
            lobby_version: 1,
            map_template: None,
            start_preset: None,
            map_revision: 0,
            strategy_card_set: None,
            speaker_order: None,
        };
        let registry =
            Arc::new(GameRegistry::new().with_presence_grace(Duration::from_millis(200)));
        let session = registry.launch_dev_scenario(config, lobby).unwrap();
        session.wait_replayed().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let app = ti4_server::create_app(registry.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            registry,
            session,
            address,
            origin_a,
            origin_b,
            target,
            centre,
            server,
        }
    }

    fn token(&self, player: &str) -> String {
        self.session.seat_tokens()[&PlayerId::new(player)].clone()
    }

    async fn socket(&self, player: Option<&str>) -> Socket {
        let (mut socket, _) = tokio_tungstenite::connect_async(format!(
            "ws://{}/ws/games/{}",
            self.address,
            self.session.id()
        ))
        .await
        .unwrap();
        send(
            &mut socket,
            ClientMessage::Subscribe {
                protocol_version: PROTOCOL_VERSION,
                game_id: self.session.id().into(),
                player_session: player.map(|player| self.token(player)),
                staged: false,
            },
        )
        .await;
        assert!(matches!(
            receive(&mut socket).await,
            ServerMessage::InitialSnapshot(_)
        ));
        socket
    }

    async fn live(&self, option: &str) {
        let session = self.registry.get_game(self.session.id()).unwrap();
        let snapshot = tokio::time::timeout(DEADLINE, async {
            loop {
                let snapshot = session.get_snapshot(&ViewerRole::Player(PlayerId::new(A)));
                if snapshot.pending_choice.is_some() {
                    break snapshot;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("live offer deadline");
        let pending = snapshot.pending_choice.unwrap();
        send_live(
            &self.registry,
            &session,
            &self.token(A),
            &pending.nonce,
            snapshot.game_version,
            option,
        )
        .await;
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        if let Some(session) = self.registry.get_game(self.session.id()) {
            session.stop();
        }
        self.server.abort();
    }
}

async fn send_live(
    registry: &Arc<GameRegistry>,
    session: &Arc<GameSession>,
    token: &str,
    nonce: &str,
    version: u64,
    option: &str,
) {
    let registry = registry.clone();
    let session = session.clone();
    let token = token.to_owned();
    let nonce = nonce.to_owned();
    let option = option.to_owned();
    tokio::task::spawn_blocking(move || {
        registry.submit_player_choice(
            session.id(),
            &token,
            &PlayerId::new(A),
            &session,
            &nonce,
            version,
            &option,
        )
    })
    .await
    .unwrap()
    .unwrap();
}

async fn send(socket: &mut Socket, message: ClientMessage) {
    socket
        .send(Message::Text(
            serde_json::to_string(&message).unwrap().into(),
        ))
        .await
        .unwrap();
}

async fn receive(socket: &mut Socket) -> ServerMessage {
    tokio::time::timeout(DEADLINE, async {
        loop {
            let message = socket.next().await.expect("socket ended").unwrap();
            if let Message::Text(text) = message {
                return serde_json::from_str(&text).unwrap();
            }
        }
    })
    .await
    .expect("message deadline")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn phase_retirement_notifies_the_owner_without_a_replacement_preview() {
    let fixture = Fixture::new(false).await;
    let mut b = fixture.socket(Some(B)).await;
    let old = start(&mut b).await;
    let owner = PlayerId::new(B);
    for seat in [A, B] {
        let player = PlayerId::new(seat);
        let snapshot = tokio::time::timeout(DEADLINE, async {
            loop {
                let snapshot = fixture
                    .session
                    .get_snapshot(&ViewerRole::Player(player.clone()));
                if snapshot.pending_choice.is_some() {
                    break snapshot;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
        let pending = snapshot.pending_choice.unwrap();
        assert!(pending.choice.option("pass").is_some());
        fixture
            .session
            .submit_choice(&player, &pending.nonce, snapshot.game_version, "pass")
            .unwrap();
    }
    loop {
        if let ServerMessage::PlanningStatus(status) = receive(&mut b).await {
            if !status.available {
                assert!(!status.can_start);
                assert!(status.has_draft);
                assert!(status.identity.is_none());
                break;
            }
        }
    }
    assert!(fixture.session.plans().contains_key(&owner));
    assert!(
        fixture
            .session
            .submit_planning_choice(&owner, old.identity, "tactical")
            .is_err()
    );
}

async fn result(
    socket: &mut Socket,
    identity: Option<AttemptIdentity>,
    rejection: Option<PlanningRejection>,
) {
    loop {
        if let ServerMessage::PlanningResult(message) = receive(socket).await {
            assert_eq!(message.identity, identity);
            assert_eq!(message.rejection, rejection);
            return;
        }
    }
}

async fn offer(socket: &mut Socket) -> PlanningEnvelope {
    loop {
        if let ServerMessage::PlanningUpdate(message) = receive(socket).await {
            if message.envelope.awaiting_answer {
                assert!(matches!(
                    message.envelope.update,
                    PlanningUpdate::SafeOffer(_)
                ));
                assert_eq!(message.envelope.progress.remaining, 0);
                return message.envelope;
            }
        }
    }
}

async fn answer(socket: &mut Socket, identity: AttemptIdentity, option: &str) {
    send(
        socket,
        ClientMessage::SubmitPlanningChoice {
            protocol_version: PROTOCOL_VERSION,
            game_id: "planning_transport".into(),
            identity,
            option_id: option.into(),
            request_id: None,
        },
    )
    .await;
    result(socket, Some(identity), None).await;
}

async fn start(socket: &mut Socket) -> PlanningEnvelope {
    send(
        socket,
        ClientMessage::StartPlanning {
            protocol_version: PROTOCOL_VERSION,
            game_id: "planning_transport".into(),
        },
    )
    .await;
    result(socket, None, None).await;
    offer(socket).await
}

async fn draft(fixture: &Fixture, socket: &mut Socket, cargo: bool) -> PlanningEnvelope {
    let mut current = start(socket).await;
    let mut answers = vec![
        "tactical".to_owned(),
        fixture.target.to_string(),
        format!("move|{}|0", fixture.origin_b),
    ];
    if cargo {
        answers.push("load|0".into());
    }
    for option in answers {
        answer(socket, current.identity, &option).await;
        current = offer(socket).await;
    }
    current
}

async fn assert_private(socket: &mut Socket) {
    send(
        socket,
        ClientMessage::Ping {
            protocol_version: PROTOCOL_VERSION,
            sequence: 99,
        },
    )
    .await;
    loop {
        match receive(socket).await {
            ServerMessage::PlanningUpdate(_) | ServerMessage::PlanningResult(_) => {
                panic!("another viewer received private planning output")
            }
            ServerMessage::Pong(message) if message.sequence == 99 => return,
            _ => {}
        }
    }
}

#[tokio::test]
async fn movement_editor_is_seat_private_and_reconnect_retains_the_original_script() {
    let fixture = Fixture::new(true).await;
    let mut a = fixture.socket(Some(A)).await;
    let mut spectator = fixture.socket(None).await;
    let mut b = fixture.socket(Some(B)).await;
    let original = draft(&fixture, &mut b, true).await;
    let edit = ClientMessage::EditPlanningMovement {
        protocol_version: PROTOCOL_VERSION,
        game_id: fixture.session.id().into(),
        identity: original.identity,
    };
    send(&mut spectator, edit.clone()).await;
    result(
        &mut spectator,
        Some(original.identity),
        Some(PlanningRejection::Unauthorized),
    )
    .await;
    assert_eq!(
        fixture.registry.edit_player_planning_movement(
            fixture.session.id(),
            "invalid",
            &PlayerId::new(B),
            &fixture.session,
            original.identity,
        ),
        Err(PlanningRejection::Unauthorized)
    );
    send(&mut b, edit.clone()).await;
    let mut acknowledged = false;
    let mut editing = None;
    while !acknowledged || editing.is_none() {
        match receive(&mut b).await {
            ServerMessage::PlanningResult(message) => {
                assert_eq!(message.identity, Some(original.identity));
                assert_eq!(message.rejection, None);
                acknowledged = true;
            }
            ServerMessage::PlanningUpdate(message) if message.envelope.awaiting_answer => {
                assert!(message.envelope.editing_movement);
                editing = Some(message.envelope);
            }
            _ => {}
        }
    }
    let editing = editing.unwrap();
    assert_eq!(editing.recorded_decisions, original.recorded_decisions);
    assert_eq!(editing.progress.replayed, 2);
    assert_private(&mut a).await;
    assert_private(&mut spectator).await;
    b.close(None).await.unwrap();
    b = fixture.socket(Some(B)).await;
    assert_eq!(offer(&mut b).await, editing);
    send(&mut b, edit).await;
    result(
        &mut b,
        Some(original.identity),
        Some(PlanningRejection::Retired),
    )
    .await;
    assert_eq!(
        fixture.session.plans()[&PlayerId::new(B)].recorded_decisions,
        original.recorded_decisions
    );
}

#[tokio::test]
async fn private_draft_refresh_reconnect_and_history() {
    let fixture = Fixture::new(true).await;
    let mut a = fixture.socket(Some(A)).await;
    let mut spectator = fixture.socket(None).await;
    let mut b = fixture.socket(Some(B)).await;
    let live_before = fixture
        .session
        .get_snapshot(&ViewerRole::Player(PlayerId::new(A)));
    let old = draft(&fixture, &mut b, true).await;
    let retained = fixture.session.plans()[&PlayerId::new(B)]
        .recorded_decisions
        .clone();
    assert_eq!(retained.len(), 4);
    assert_eq!(
        fixture
            .session
            .get_snapshot(&ViewerRole::Player(PlayerId::new(A))),
        live_before
    );
    assert_private(&mut a).await;
    assert_private(&mut spectator).await;

    // Reconnect receives the current offer, without starting another runner.
    b.close(None).await.unwrap();
    b = fixture.socket(Some(B)).await;
    assert_eq!(offer(&mut b).await, old);

    for option in [
        "tactical".to_owned(),
        fixture.centre.to_string(),
        format!("move|{}|0", fixture.origin_a),
        "load|0".into(),
    ] {
        fixture.live(&option).await;
    }
    let refreshed = loop {
        let current = offer(&mut b).await;
        let PlanningUpdate::SafeOffer(publication) = &current.update else {
            unreachable!()
        };
        if publication.position.board.systems[&fixture.centre]
            .units
            .iter()
            .any(|unit| unit.owner == PlayerId::new(A))
        {
            break current;
        }
    };
    assert!(refreshed.identity.checkpoint_id > old.identity.checkpoint_id);
    assert_eq!(refreshed.progress.replayed, retained.len());
    send(
        &mut b,
        ClientMessage::SubmitPlanningChoice {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
            identity: old.identity,
            option_id: "done_moving".into(),
            request_id: None,
        },
    )
    .await;
    result(&mut b, Some(old.identity), Some(PlanningRejection::Retired)).await;

    let board_before_history = fixture.session.current_state().board.clone();
    for action in [HistoryAction::Undo, HistoryAction::Redo] {
        let undo = matches!(action, HistoryAction::Undo);
        let session = fixture.registry.get_game(fixture.session.id()).unwrap();
        fixture
            .registry
            .change_history(
                session.id(),
                &fixture.token(A),
                session.game_version(),
                action,
            )
            .unwrap();
        let replacement = fixture.registry.get_game(session.id()).unwrap();
        replacement.wait_replayed().unwrap();
        if undo {
            assert_ne!(replacement.current_state().board, board_before_history);
        } else {
            assert_eq!(replacement.current_state().board, board_before_history);
        }
        b = fixture.socket(Some(B)).await;
        let current = offer(&mut b).await;
        assert!(current.identity.checkpoint_id > refreshed.identity.checkpoint_id);
        assert_eq!(current.progress.replayed, retained.len());
        assert_eq!(
            replacement.plans()[&PlayerId::new(B)].recorded_decisions,
            retained
        );
        assert_eq!(
            fixture.registry.submit_player_planning(
                session.id(),
                &fixture.token(B),
                &PlayerId::new(B),
                &session,
                None
            ),
            Err(PlanningRejection::Unavailable)
        );
        let mut a = fixture.socket(Some(A)).await;
        let mut spectator = fixture.socket(None).await;
        assert_private(&mut a).await;
        assert_private(&mut spectator).await;
    }
}

#[tokio::test]
async fn blockade_reports_replay_mismatch_and_keeps_script() {
    let fixture = Fixture::new(false).await;
    let mut b = fixture.socket(Some(B)).await;
    let _ = draft(&fixture, &mut b, false).await;
    let retained = fixture.session.plans()[&PlayerId::new(B)]
        .recorded_decisions
        .clone();
    for option in [
        "tactical".to_owned(),
        fixture.centre.to_string(),
        format!("move|{}|0", fixture.origin_a),
    ] {
        fixture.live(&option).await;
    }
    loop {
        if let ServerMessage::PlanningUpdate(message) = receive(&mut b).await {
            if let PlanningUpdate::Stopped { reason, .. } = message.envelope.update {
                assert_eq!(reason, StopReason::ReplayMismatch);
                assert!(!message.envelope.awaiting_answer);
                break;
            }
        }
    }
    assert_eq!(
        fixture.session.plans()[&PlayerId::new(B)].recorded_decisions,
        retained
    );
}

#[tokio::test]
async fn rejects_unauthorized_wrong_game_unknown_and_duplicate_answers() {
    let fixture = Fixture::new(true).await;
    let mut spectator = fixture.socket(None).await;
    send(
        &mut spectator,
        ClientMessage::StartPlanning {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
        },
    )
    .await;
    result(&mut spectator, None, Some(PlanningRejection::Unauthorized)).await;
    let (mut anonymous, _) = tokio_tungstenite::connect_async(format!(
        "ws://{}/ws/games/{}",
        fixture.address,
        fixture.session.id()
    ))
    .await
    .unwrap();
    send(
        &mut anonymous,
        ClientMessage::StartPlanning {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
        },
    )
    .await;
    result(&mut anonymous, None, Some(PlanningRejection::Unauthorized)).await;
    let mut a = fixture.socket(Some(A)).await;
    send(
        &mut a,
        ClientMessage::StartPlanning {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
        },
    )
    .await;
    result(&mut a, None, Some(PlanningRejection::ActivePlayer)).await;
    let mut b = fixture.socket(Some(B)).await;
    send(
        &mut b,
        ClientMessage::StartPlanning {
            protocol_version: PROTOCOL_VERSION,
            game_id: "wrong".into(),
        },
    )
    .await;
    result(&mut b, None, Some(PlanningRejection::WrongGame)).await;
    let current = start(&mut b).await;
    send(
        &mut b,
        ClientMessage::SubmitPlanningChoice {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
            identity: current.identity,
            option_id: "not offered".into(),
            request_id: None,
        },
    )
    .await;
    result(
        &mut b,
        Some(current.identity),
        Some(PlanningRejection::UnknownOption),
    )
    .await;
    assert!(
        fixture.session.plans()[&PlayerId::new(B)]
            .recorded_decisions
            .is_empty()
    );
    answer(&mut b, current.identity, "tactical").await;
    let _ = offer(&mut b).await;
    send(
        &mut b,
        ClientMessage::SubmitPlanningChoice {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
            identity: current.identity,
            option_id: "tactical".into(),
            request_id: None,
        },
    )
    .await;
    result(
        &mut b,
        Some(current.identity),
        Some(PlanningRejection::Retired),
    )
    .await;
    assert_eq!(
        fixture.session.plans()[&PlayerId::new(B)]
            .recorded_decisions
            .len(),
        1
    );
    assert_eq!(
        fixture.registry.submit_player_planning(
            fixture.session.id(),
            "invalid",
            &PlayerId::new(B),
            &fixture.session,
            None
        ),
        Err(PlanningRejection::Unauthorized)
    );
}

#[tokio::test]
async fn revoked_credential_cannot_answer_or_receive_a_planning_offer() {
    let fixture = Fixture::new(true).await;
    let mut b = fixture.socket(Some(B)).await;
    let current = start(&mut b).await;
    let old_token = fixture.token(B);
    // Let presence expire while keeping the old WebSocket open.
    tokio::time::sleep(Duration::from_millis(250)).await;
    let (_, credential) = fixture
        .registry
        .take_over_player(fixture.session.id(), &PlayerId::new(B), "Replacement")
        .unwrap();
    assert_eq!(
        fixture.registry.submit_player_planning(
            fixture.session.id(),
            &old_token,
            &PlayerId::new(B),
            &fixture.session,
            Some((current.identity, "tactical")),
        ),
        Err(PlanningRejection::Unauthorized)
    );
    assert!(
        fixture.session.plans()[&PlayerId::new(B)]
            .recorded_decisions
            .is_empty()
    );
    let closed = tokio::time::timeout(DEADLINE, b.next()).await.unwrap();
    assert!(!matches!(closed, Some(Ok(Message::Text(_)))));

    let (mut replacement, _) = tokio_tungstenite::connect_async(format!(
        "ws://{}/ws/games/{}",
        fixture.address,
        fixture.session.id()
    ))
    .await
    .unwrap();
    send(
        &mut replacement,
        ClientMessage::Subscribe {
            protocol_version: PROTOCOL_VERSION,
            game_id: fixture.session.id().into(),
            player_session: Some(credential.as_str().into()),
            staged: false,
        },
    )
    .await;
    assert!(matches!(
        receive(&mut replacement).await,
        ServerMessage::InitialSnapshot(_)
    ));
    assert_eq!(offer(&mut replacement).await, current);
    answer(&mut replacement, current.identity, "tactical").await;
    assert!(offer(&mut replacement).await.identity.plan_revision > current.identity.plan_revision);
}
