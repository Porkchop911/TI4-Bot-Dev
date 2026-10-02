//! Opt-in full BYOA integration test requiring the pinned local libtorch runtime.

#![cfg(feature = "real-e2e")]

use std::sync::Arc;
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use ti4_bot_agent::{BotConfig, run};
use ti4_server::create_app;
use ti4_server::protocol::{ClientMessage, ServerMessage, parse_server_message};
use ti4_server::session::GameRegistry;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const TARGET_DECISIONS: usize = 12;
const TEST_TIMEOUT: Duration = Duration::from_secs(75);

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires LIBTORCH, LD_LIBRARY_PATH, and the pinned checkpoint runtime"]
async fn real_server_advisor_and_two_bots_advance_a_bounded_game_prefix() {
    ti4_tensor::configure_deterministic(20_260_821).expect("configure tensor backend");
    let advisor =
        ti4_advisor::Advisor::load(&checkpoint_path()).expect("load pinned advisor checkpoint");
    let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind advisor");
    let advisor_address = advisor_listener.local_addr().expect("advisor address");
    let advisor_task = tokio::spawn(async move {
        axum::serve(advisor_listener, advisor.router())
            .await
            .expect("serve advisor");
    });

    let registry = Arc::new(GameRegistry::new());
    let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind server");
    let server_address = server_listener.local_addr().expect("server address");
    let server_registry = registry.clone();
    let server_task = tokio::spawn(async move {
        axum::serve(server_listener, create_app(server_registry))
            .await
            .expect("serve server");
    });

    let result = tokio::time::timeout(
        TEST_TIMEOUT,
        exercise_game_prefix(
            format!("http://{server_address}"),
            format!("ws://{server_address}"),
            format!("http://{advisor_address}"),
            registry,
        ),
    )
    .await;
    advisor_task.abort();
    server_task.abort();
    result
        .expect("bounded E2E test deadline")
        .expect("real BYOA game prefix");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires LIBTORCH, LD_LIBRARY_PATH, and the pinned checkpoint runtime"]
async fn real_server_and_bots_play_to_game_over() {
    ti4_tensor::configure_deterministic(20_260_821).expect("configure tensor backend");
    let advisor =
        ti4_advisor::Advisor::load(&checkpoint_path()).expect("load pinned advisor checkpoint");
    let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind advisor");
    let advisor_address = advisor_listener.local_addr().expect("advisor address");
    let advisor_task = tokio::spawn(async move {
        axum::serve(advisor_listener, advisor.router())
            .await
            .expect("serve advisor");
    });

    let registry = Arc::new(GameRegistry::new());
    let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind server");
    let server_address = server_listener.local_addr().expect("server address");
    let server_registry = registry.clone();
    let server_task = tokio::spawn(async move {
        axum::serve(server_listener, create_app(server_registry))
            .await
            .expect("serve server");
    });

    let result = tokio::time::timeout(
        TEST_TIMEOUT,
        exercise_game_to_end(
            format!("http://{server_address}"),
            format!("ws://{server_address}"),
            format!("http://{advisor_address}"),
            registry,
            "endgame",
        ),
    )
    .await;
    advisor_task.abort();
    server_task.abort();
    result
        .expect("game-over E2E test deadline")
        .expect("real BYOA game over");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "on-demand start-to-finish game with real neural net checkpoint"]
async fn real_server_and_bots_play_full_game_start_to_finish() {
    ti4_tensor::configure_deterministic(20_260_821).expect("configure tensor backend");
    let advisor =
        ti4_advisor::Advisor::load(&checkpoint_path()).expect("load pinned advisor checkpoint");
    let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind advisor");
    let advisor_address = advisor_listener.local_addr().expect("advisor address");
    let advisor_task = tokio::spawn(async move {
        axum::serve(advisor_listener, advisor.router())
            .await
            .expect("serve advisor");
    });

    let registry = Arc::new(GameRegistry::new());
    let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind server");
    let server_address = server_listener.local_addr().expect("server address");
    let server_registry = registry.clone();
    let server_task = tokio::spawn(async move {
        axum::serve(server_listener, create_app(server_registry))
            .await
            .expect("serve server");
    });

    let result = tokio::time::timeout(
        Duration::from_secs(120),
        exercise_game_to_end(
            format!("http://{server_address}"),
            format!("ws://{server_address}"),
            format!("http://{advisor_address}"),
            registry,
            "full_game",
        ),
    )
    .await;
    advisor_task.abort();
    server_task.abort();
    result
        .expect("start-to-finish E2E test deadline")
        .expect("real BYOA game from start to finish");
}

fn checkpoint_path() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("examples/reviewer/checkpoint-473312")
}

async fn exercise_game_prefix(
    http_base: String,
    websocket_base: String,
    advisor_base: String,
    registry: Arc<GameRegistry>,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let created = client
        .post(format!("{http_base}/api/games"))
        .json(&serde_json::json!({ "player_count": 3, "seed": 4_242, "nickname": "Host" }))
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|error| error.to_string())?;
    let game_id = required_string(&created, "game_id")?;
    let p1_token = required_string(&created, "player_session")?;
    client
        .post(format!("{http_base}/api/games/{game_id}/lobby/ready"))
        .header("x-ti4-player-session", &p1_token)
        .json(&serde_json::json!({ "ready": true }))
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let mut p2 = BotConfig::new(
        websocket_base.clone(),
        game_id.clone(),
        advisor_base.clone(),
        "Bot Two".to_owned(),
    );
    p2.timeout = Duration::from_secs(5);
    p2.max_reconnects = 1;
    let mut p3 = BotConfig::new(
        websocket_base.clone(),
        game_id.clone(),
        advisor_base,
        "Bot Three".to_owned(),
    );
    p3.timeout = Duration::from_secs(5);
    p3.max_reconnects = 1;
    let mut p2_task = tokio::spawn(run(p2));
    // Wait until the first agent has admitted before launching the second.
    while registry
        .player_lobby_status(&game_id, None)
        .map_err(|e| e.message())?
        .0
        .slots
        .iter()
        .filter(|s| s.occupant.is_some())
        .count()
        < 2
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut p3_task = tokio::spawn(run(p3));
    while !registry
        .player_lobby_status(&game_id, None)
        .map_err(|e| e.message())?
        .0
        .slots
        .iter()
        .all(|s| s.occupant.is_some() && s.ready)
    {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    client
        .post(format!("{http_base}/api/games/{game_id}/lobby/start"))
        .header("x-ti4-player-session", &p1_token)
        .send()
        .await
        .map_err(|error| error.to_string())?
        .error_for_status()
        .map_err(|error| error.to_string())?;
    let players: Vec<_> = registry
        .player_lobby_status(&game_id, None)
        .map_err(|e| e.message())?
        .0
        .slots
        .iter()
        .filter_map(|s| s.occupant.clone())
        .collect();
    let scripted_task = tokio::spawn(scripted_first_option(
        websocket_base,
        game_id.clone(),
        p1_token,
    ));

    let session = registry
        .get_game(&game_id)
        .ok_or_else(|| "server did not start the requested game".to_owned())?;
    let outcome = 'decisions: loop {
        if p2_task.is_finished() {
            break 'decisions Err(format!(
                "p2 bot exited before the decision target: {:?}",
                (&mut p2_task).await
            ));
        }
        if p3_task.is_finished() {
            break 'decisions Err(format!(
                "p3 bot exited before the decision target: {:?}",
                (&mut p3_task).await
            ));
        }
        let decisions = session.decision_log();
        if decisions.len() >= TARGET_DECISIONS {
            if session.error().is_some() {
                break 'decisions Err(format!("server session failed: {:?}", session.error()));
            }
            for seat in players.iter().skip(1) {
                if !decisions.iter().any(|decision| &decision.player == seat) {
                    break 'decisions Err(format!(
                        "bot seat {seat} did not make a recorded decision"
                    ));
                }
            }
            break 'decisions Ok(());
        }
        if session.error().is_some() {
            break 'decisions Err(format!("server session failed: {:?}", session.error()));
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let idle_result = if outcome.is_ok() {
        // Both real agents remain subscribed through multiple application-ping intervals.
        // Without their pings the server would offer both identities for takeover.
        tokio::time::sleep(Duration::from_secs(31)).await;
        let lobby = registry
            .player_lobby_status(&game_id, None)
            .map(|status| status.0);
        let lobby = lobby.map_err(|error| error.message())?;
        let mut result = Ok(());
        for bot in players.iter().skip(1) {
            match lobby
                .slots
                .iter()
                .find(|slot| slot.occupant.as_ref() == Some(bot))
            {
                Some(slot) if slot.connected && !slot.can_take_over => {}
                _ => result = Err(format!("bot {bot} lost authenticated presence while idle")),
            }
        }
        result
    } else {
        Ok(())
    };
    p2_task.abort();
    p3_task.abort();
    scripted_task.abort();
    outcome.and(idle_result)
}

async fn exercise_game_to_end(
    http_base: String,
    websocket_base: String,
    advisor_base: String,
    registry: Arc<GameRegistry>,
    scenario_id: &str,
) -> Result<(), String> {
    let client = reqwest::Client::new();
    let launched = client
        .post(format!("{http_base}/api/dev/scenarios/launch"))
        .json(&serde_json::json!({ "scenario_id": scenario_id, "seed": 4_242 }))
        .send()
        .await
        .map_err(|e| e.to_string())?
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| e.to_string())?;

    let game_id = required_string(&launched, "game_id")?;
    let test_seats = launched
        .get("test_seats")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "launch response omitted test_seats".to_owned())?;

    let mut tokens: Vec<String> = test_seats
        .values()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    if tokens.len() < 3 {
        return Err("expected at least 3 tokens".to_owned());
    }
    let p1_token = tokens.remove(0);
    let p2_token = tokens.remove(0);
    let p3_token = tokens.remove(0);

    let mut b1 = BotConfig::new(
        websocket_base.clone(),
        game_id.clone(),
        advisor_base.clone(),
        "Bot Sol".to_owned(),
    )
    .with_player_session(p1_token);
    b1.timeout = Duration::from_secs(10);
    b1.max_reconnects = 1;

    let mut b2 = BotConfig::new(
        websocket_base.clone(),
        game_id.clone(),
        advisor_base.clone(),
        "Bot Hacan".to_owned(),
    )
    .with_player_session(p2_token);
    b2.timeout = Duration::from_secs(10);
    b2.max_reconnects = 1;

    let mut b3 = BotConfig::new(
        websocket_base.clone(),
        game_id.clone(),
        advisor_base,
        "Bot Letnev".to_owned(),
    )
    .with_player_session(p3_token);
    b3.timeout = Duration::from_secs(10);
    b3.max_reconnects = 1;

    let b1_task = tokio::spawn(run(b1));
    let b2_task = tokio::spawn(run(b2));
    let b3_task = tokio::spawn(run(b3));

    let (r1, r2, r3) =
        tokio::try_join!(b1_task, b2_task, b3_task).map_err(|e| format!("bot join error: {e}"))?;
    r1.map_err(|e| format!("bot 1 failed: {e:?}"))?;
    r2.map_err(|e| format!("bot 2 failed: {e:?}"))?;
    r3.map_err(|e| format!("bot 3 failed: {e:?}"))?;

    let session = registry
        .get_game(&game_id)
        .ok_or_else(|| "server session not found".to_owned())?;
    if !session.is_finished() {
        return Err("game session did not mark is_finished after bots finished".to_owned());
    }

    Ok(())
}

async fn scripted_first_option(
    websocket_base: String,
    game_id: String,
    token: String,
) -> Result<(), String> {
    let (mut socket, _) = connect_async(format!("{websocket_base}/ws/games/{game_id}"))
        .await
        .map_err(|error| error.to_string())?;
    send_client(
        &mut socket,
        ClientMessage::Subscribe {
            protocol_version: ti4_server::protocol::PROTOCOL_VERSION,
            game_id: game_id.clone(),
            player_session: Some(token),
        },
    )
    .await?;
    while let Some(message) = socket.next().await {
        let message = message.map_err(|error| error.to_string())?;
        let Message::Text(text) = message else {
            continue;
        };
        let message = parse_server_message(&text).map_err(|error| error.to_string())?;
        match message {
            ServerMessage::InitialSnapshot(snapshot) => {
                if let Some(pending) = snapshot.pending_choice {
                    submit_first(
                        &mut socket,
                        &game_id,
                        pending.nonce,
                        snapshot.game_version,
                        pending.choice.options,
                    )
                    .await?;
                }
            }
            ServerMessage::StateUpdate(update) => {
                if let Some(pending) = update.pending_choice {
                    submit_first(
                        &mut socket,
                        &game_id,
                        pending.nonce,
                        update.game_version,
                        pending.choice.options,
                    )
                    .await?;
                }
            }
            ServerMessage::PendingChoice(pending) => {
                submit_first(
                    &mut socket,
                    &game_id,
                    pending.nonce,
                    pending.game_version,
                    pending.choice.options,
                )
                .await?;
            }
            ServerMessage::GameOver(_) => return Ok(()),
            ServerMessage::Error(error) => return Err(error.message),
            _ => {}
        }
    }
    Err("scripted seat disconnected".to_owned())
}

fn required_string(value: &serde_json::Value, key: &str) -> Result<String, String> {
    value[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("response omitted {key}"))
}

async fn submit_first<S>(
    socket: &mut S,
    game_id: &str,
    nonce: String,
    game_version: u64,
    options: Vec<ti4_server::protocol::server::ChoiceOption>,
) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let option_id = options
        .first()
        .ok_or_else(|| "scripted seat received no options".to_owned())?
        .id
        .clone();
    send_client(
        socket,
        ClientMessage::SubmitChoice {
            protocol_version: ti4_server::protocol::PROTOCOL_VERSION,
            game_id: game_id.to_owned(),
            nonce,
            expected_version: game_version,
            option_id,
        },
    )
    .await
}

async fn send_client<S>(socket: &mut S, message: ClientMessage) -> Result<(), String>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let text = serde_json::to_string(&message).map_err(|error| error.to_string())?;
    socket
        .send(Message::Text(text.into()))
        .await
        .map_err(|error| error.to_string())
}
