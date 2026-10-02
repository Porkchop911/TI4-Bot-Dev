//! Lobby-admitted WebSocket bot client backed by an external advisor service.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use rand::Rng;
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use serde::Deserialize;
use thiserror::Error;
use ti4_model::id::PlayerId;
use ti4_model::state::GameState;
use ti4_server::map::GalaxyLayout;
use ti4_server::protocol::server::Choice;
use ti4_server::protocol::status::ViewerRole;
use ti4_server::protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage, parse_server_message};
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const DEFAULT_TEMPERATURE: f64 = 0.25;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);
const DEFAULT_MAX_RECONNECTS: u32 = 3;
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// Configuration for one independently admitted bot process.
#[derive(Debug, Clone)]
pub struct BotConfig {
    pub server: String,
    pub game_id: String,
    pub advisor: String,
    pub nickname: String,
    pub temperature: f64,
    pub sample_seed: Option<u64>,
    pub timeout: Duration,
    pub max_reconnects: u32,
    pub player_session: Option<String>,
}

impl BotConfig {
    /// Builds a configuration using the documented deterministic defaults.
    #[must_use]
    pub fn new(server: String, game_id: String, advisor: String, nickname: String) -> Self {
        Self {
            server,
            game_id,
            advisor,
            nickname,
            temperature: DEFAULT_TEMPERATURE,
            sample_seed: None,
            timeout: DEFAULT_TIMEOUT,
            max_reconnects: DEFAULT_MAX_RECONNECTS,
            player_session: None,
        }
    }

    /// Attaches a pre-authenticated player session credential.
    #[must_use]
    pub fn with_player_session(mut self, session: impl Into<String>) -> Self {
        self.player_session = Some(session.into());
        self
    }

    fn validate(&self) -> Result<(), BotError> {
        ti4_server::storage::validate_nickname(&self.nickname).map_err(|_| {
            BotError::Configuration(
                "invalid nickname (1–64 UTF-8 bytes, trimmed, no control or format characters)"
                    .to_owned(),
            )
        })?;
        if self.server.is_empty() || !self.server.starts_with("ws://") {
            return Err(BotError::Configuration(
                "server must be a ws:// URL".to_owned(),
            ));
        }
        if self.game_id.is_empty()
            || self.game_id.len() > 64
            || !self
                .game_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        {
            return Err(BotError::Configuration("invalid game ID".to_owned()));
        }
        if self.advisor.is_empty() || !self.advisor.starts_with("http://") {
            return Err(BotError::Configuration(
                "advisor must be an http:// URL".to_owned(),
            ));
        }
        if !self.temperature.is_finite() || self.temperature <= 0.0 {
            return Err(BotError::Configuration(
                "temperature must be finite and positive".to_owned(),
            ));
        }
        if self.timeout.is_zero() {
            return Err(BotError::Configuration(
                "timeout must be positive".to_owned(),
            ));
        }
        Ok(())
    }

    fn websocket_url(&self) -> String {
        format!(
            "{}/ws/games/{}",
            self.server.trim_end_matches('/'),
            self.game_id
        )
    }

    fn advisor_url(&self) -> String {
        format!("{}/evaluate", self.advisor.trim_end_matches('/'))
    }

    fn lobby_url(&self, suffix: &str) -> String {
        format!(
            "{}/api/games/{}/lobby{suffix}",
            self.server
                .trim_end_matches('/')
                .replacen("ws://", "http://", 1),
            self.game_id
        )
    }
}

/// A terminal bot-agent failure. Errors always leave the server state untouched.
#[derive(Debug, Error)]
pub enum BotError {
    #[error("invalid configuration: {0}")]
    Configuration(String),
    #[error("websocket connection or I/O failed: {0}")]
    WebSocket(String),
    #[error("advisor request failed: {0}")]
    Advisor(String),
    #[error("server protocol failed: {0}")]
    Protocol(String),
    #[error("server rejected the bot protocol: {0}")]
    Server(String),
    #[error("lobby request failed: {0}")]
    Lobby(String),
    #[error("reconnect attempts exhausted")]
    ReconnectExhausted,
}

#[derive(Debug, Deserialize)]
struct AdvisorResponse {
    options: Vec<AdvisorOption>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdvisorOption {
    option_id: String,
    probability: f64,
    logit: f64,
}

#[derive(Deserialize)]
struct LobbyView {
    phase: String,
    slots: Vec<LobbySlot>,
}

#[derive(Deserialize)]
struct LobbySlot {
    position: usize,
    occupant: Option<PlayerId>,
    nickname: Option<String>,
    can_take_over: bool,
}

#[derive(Deserialize)]
struct JoinReply {
    player_session: Option<String>,
    player: JoinPlayer,
    lobby: LobbyView,
}

#[derive(Deserialize)]
struct JoinPlayer {
    id: PlayerId,
}

// Deliberately no Debug: a join reply carries a private bearer credential.
struct Admission {
    player: PlayerId,
    session: String,
    phase: String,
}

async fn request_lobby(
    response: Result<reqwest::Response, reqwest::Error>,
) -> Result<reqwest::Response, BotError> {
    response
        .map_err(|error| BotError::Lobby(error.to_string()))?
        .error_for_status()
        .map_err(|error| BotError::Lobby(error.to_string()))
}

async fn admit(config: &BotConfig, client: &reqwest::Client) -> Result<Admission, BotError> {
    admit_with_selection(config, client, None).await
}

async fn admit_with_selection(
    config: &BotConfig,
    client: &reqwest::Client,
    selection: Option<PlayerId>,
) -> Result<Admission, BotError> {
    if let Some(session) = &config.player_session {
        let reply: JoinReply = request_lobby(
            client
                .post(config.lobby_url("/join"))
                .header("x-ti4-player-session", session)
                .json(&serde_json::json!({"kind": "new"}))
                .send()
                .await,
        )
        .await?
        .json()
        .await
        .map_err(|e| BotError::Lobby(e.to_string()))?;
        return Ok(Admission {
            player: reply.player.id,
            session: session.clone(),
            phase: reply.lobby.phase,
        });
    }

    let lobby: LobbyView = request_lobby(client.get(config.lobby_url("")).send().await)
        .await?
        .json()
        .await
        .map_err(|e| BotError::Lobby(e.to_string()))?;
    let payload = if lobby.phase == "lobby"
        && lobby.slots.iter().any(|slot| slot.occupant.is_none())
    {
        serde_json::json!({"kind": "new", "nickname": config.nickname})
    } else {
        let eligible: Vec<_> = lobby
            .slots
            .iter()
            .filter(|slot| slot.can_take_over)
            .filter_map(|slot| {
                slot.occupant
                    .as_ref()
                    .map(|id| (slot.position, slot.nickname.clone(), id.clone()))
            })
            .collect();
        if eligible.is_empty() {
            return Err(BotError::Lobby(
                "no open position or eligible disconnected player".to_owned(),
            ));
        }
        let selection = if let Some(selected) = selection {
            if !eligible.iter().any(|(_, _, id)| id == &selected) {
                return Err(BotError::Lobby(
                    "selected player is not eligible".to_owned(),
                ));
            }
            selected
        } else {
            tokio::task::spawn_blocking(move || choose_takeover(&eligible))
                .await
                .map_err(|e| BotError::Lobby(e.to_string()))??
        };
        serde_json::json!({"kind": "takeover", "player_id": selection, "nickname": config.nickname})
    };
    let reply: JoinReply = request_lobby(
        client
            .post(config.lobby_url("/join"))
            .json(&payload)
            .send()
            .await,
    )
    .await?
    .json()
    .await
    .map_err(|e| BotError::Lobby(e.to_string()))?;
    if reply.lobby.phase != "lobby" && reply.lobby.phase != "running" {
        return Err(BotError::Protocol("unknown lobby phase".to_owned()));
    }
    let session = reply
        .player_session
        .ok_or_else(|| BotError::Protocol("join did not issue a session".to_owned()))?;
    Ok(Admission {
        player: reply.player.id,
        session,
        phase: reply.lobby.phase,
    })
}

fn choose_takeover(eligible: &[(usize, Option<String>, PlayerId)]) -> Result<PlayerId, BotError> {
    eprintln!("Eligible disconnected players (select a number to take over):");
    for (index, (position, nickname, _)) in eligible.iter().enumerate() {
        eprintln!(
            "{}: {} (position {position})",
            index + 1,
            nickname.as_deref().unwrap_or("Player")
        );
    }
    let mut input = String::new();
    std::io::stdin()
        .read_line(&mut input)
        .map_err(|e| BotError::Lobby(e.to_string()))?;
    select_takeover(eligible, &input)
}

fn select_takeover(
    eligible: &[(usize, Option<String>, PlayerId)],
    input: &str,
) -> Result<PlayerId, BotError> {
    let index = input
        .trim()
        .parse::<usize>()
        .map_err(|_| BotError::Lobby("invalid takeover selection".to_owned()))?;
    eligible
        .get(
            index
                .checked_sub(1)
                .ok_or_else(|| BotError::Lobby("invalid takeover selection".to_owned()))?,
        )
        .map(|(_, _, id)| id.clone())
        .ok_or_else(|| BotError::Lobby("invalid takeover selection".to_owned()))
}

async fn wait_for_start(
    config: &BotConfig,
    client: &reqwest::Client,
    admission: &Admission,
) -> Result<(), BotError> {
    wait_for_start_with_interval(config, client, admission, HEARTBEAT_INTERVAL).await
}

async fn wait_for_start_with_interval(
    config: &BotConfig,
    client: &reqwest::Client,
    admission: &Admission,
    heartbeat_interval: Duration,
) -> Result<(), BotError> {
    if admission.phase == "running" {
        return Ok(());
    }
    let ready = request_lobby(
        client
            .post(config.lobby_url("/ready"))
            .header("x-ti4-player-session", &admission.session)
            .json(&serde_json::json!({"ready": true}))
            .send()
            .await,
    )
    .await?;
    let mut lobby: LobbyView = ready
        .json()
        .await
        .map_err(|e| BotError::Lobby(e.to_string()))?;
    while lobby.phase == "lobby" {
        tokio::time::sleep(heartbeat_interval).await;
        lobby = request_lobby(
            client
                .post(config.lobby_url("/heartbeat"))
                .header("x-ti4-player-session", &admission.session)
                .send()
                .await,
        )
        .await?
        .json()
        .await
        .map_err(|e| BotError::Lobby(e.to_string()))?;
    }
    if lobby.phase != "running" {
        return Err(BotError::Protocol("unknown lobby phase".to_owned()));
    }
    Ok(())
}

/// Runs until the game ends or a bounded transport/protocol failure occurs.
///
/// # Errors
///
/// Returns an error for invalid configuration, unreachable peers, malformed protocol data,
/// invalid advisor output, or exhausted reconnect attempts.
pub async fn run(config: BotConfig) -> Result<(), BotError> {
    config.validate()?;
    let client = reqwest::Client::builder()
        .timeout(config.timeout)
        .build()
        .map_err(|error| {
            BotError::Configuration(format!("cannot build advisor client: {error}"))
        })?;
    let mut sampler = config.sample_seed.map(ChaCha8Rng::seed_from_u64);
    let admission = admit(&config, &client).await?;
    wait_for_start(&config, &client, &admission).await?;
    run_existing_session(&config, &client, &admission, &mut sampler).await
}

async fn run_existing_session(
    config: &BotConfig,
    client: &reqwest::Client,
    admission: &Admission,
    sampler: &mut Option<ChaCha8Rng>,
) -> Result<(), BotError> {
    for attempt in 0..=config.max_reconnects {
        match run_connection(config, client, admission, sampler.as_mut()).await {
            Ok(ConnectionEnd::GameOver) => return Ok(()),
            Ok(ConnectionEnd::Disconnected) | Err(BotError::WebSocket(_))
                if attempt < config.max_reconnects =>
            {
                tokio::time::sleep(reconnect_delay(attempt)).await;
            }
            Ok(ConnectionEnd::Disconnected) | Err(BotError::WebSocket(_)) => {
                return Err(BotError::ReconnectExhausted);
            }
            Err(error) => return Err(error),
        }
    }
    Err(BotError::ReconnectExhausted)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ConnectionEnd {
    GameOver,
    Disconnected,
}

async fn run_connection(
    config: &BotConfig,
    client: &reqwest::Client,
    admission: &Admission,
    mut sampler: Option<&mut ChaCha8Rng>,
) -> Result<ConnectionEnd, BotError> {
    let (mut stream, _) = within(config.timeout, connect_async(config.websocket_url()))
        .await
        .map_err(BotError::WebSocket)?
        .map_err(|error| BotError::WebSocket(error.to_string()))?;
    send_message(
        &mut stream,
        &ClientMessage::Subscribe {
            protocol_version: PROTOCOL_VERSION,
            game_id: config.game_id.clone(),
            player_session: Some(admission.session.clone()),
        },
        config.timeout,
    )
    .await?;

    let mut last_submission: Option<(String, u64)> = None;
    let mut authenticated = false;
    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.tick().await;
    let mut sequence = 0;
    loop {
        let incoming = tokio::select! {
            incoming = stream.next() => incoming,
            _ = heartbeat.tick() => {
                sequence += 1;
                send_message(&mut stream, &ClientMessage::Ping { protocol_version: PROTOCOL_VERSION, sequence }, config.timeout).await?;
                continue;
            }
        };
        let Some(message) = incoming else {
            return Ok(ConnectionEnd::Disconnected);
        };
        let message = message.map_err(|error| BotError::WebSocket(error.to_string()))?;
        let Message::Text(text) = message else {
            if message.is_close() {
                return Ok(ConnectionEnd::Disconnected);
            }
            continue;
        };
        let message =
            parse_server_message(&text).map_err(|error| BotError::Protocol(error.to_string()))?;
        if let Some(game_id) = message.game_id()
            && game_id != config.game_id
        {
            return Err(BotError::Protocol(
                "server message game_id differs from subscription".to_owned(),
            ));
        }
        match message {
            ServerMessage::GameOver(_) => return Ok(ConnectionEnd::GameOver),
            ServerMessage::Error(error) => return Err(BotError::Server(error.message)),
            ServerMessage::Pong(pong) if pong.sequence > sequence => {
                return Err(BotError::Protocol("unexpected pong sequence".to_owned()));
            }
            // A rejected nonce/version remains remembered, so only a new pending message may act.
            ServerMessage::ActionRejected(_) if last_submission.is_none() => {
                return Err(BotError::Protocol(
                    "received action rejection without a bot submission".to_owned(),
                ));
            }
            ServerMessage::InitialSnapshot(snapshot) => {
                require_viewer(&snapshot.viewer, &admission.player)?;
                authenticated = true;
                if let Some(pending) = snapshot.pending_choice {
                    submit_advice(
                        config,
                        &admission.player,
                        client,
                        &mut stream,
                        &snapshot.state,
                        &snapshot.galaxy_layout,
                        &pending.choice,
                        &pending.nonce,
                        snapshot.game_version,
                        sampler.as_deref_mut(),
                        &mut last_submission,
                    )
                    .await?;
                }
            }
            ServerMessage::StateUpdate(update) => {
                require_viewer(&update.viewer, &admission.player)?;
                authenticated = true;
                if let Some(pending) = update.pending_choice {
                    submit_advice(
                        config,
                        &admission.player,
                        client,
                        &mut stream,
                        &update.state,
                        &update.galaxy_layout,
                        &pending.choice,
                        &pending.nonce,
                        update.game_version,
                        sampler.as_deref_mut(),
                        &mut last_submission,
                    )
                    .await?;
                }
            }
            ServerMessage::PendingChoice(pending) => {
                if !authenticated {
                    return Err(BotError::Protocol(
                        "pending choice before authenticated snapshot".to_owned(),
                    ));
                }
                submit_advice(
                    config,
                    &admission.player,
                    client,
                    &mut stream,
                    &pending.state,
                    &pending.galaxy_layout,
                    &pending.choice,
                    &pending.nonce,
                    pending.game_version,
                    sampler.as_deref_mut(),
                    &mut last_submission,
                )
                .await?;
            }
            _ => {}
        }
    }
}

fn require_viewer(viewer: &ViewerRole, player: &PlayerId) -> Result<(), BotError> {
    if viewer != &ViewerRole::Player(player.clone()) {
        return Err(BotError::Protocol(
            "server authenticated a different player".to_owned(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn submit_advice<S>(
    config: &BotConfig,
    player: &PlayerId,
    client: &reqwest::Client,
    stream: &mut S,
    state: &GameState,
    galaxy_layout: &GalaxyLayout,
    choice: &Choice,
    nonce: &str,
    game_version: u64,
    sampler: Option<&mut ChaCha8Rng>,
    last_submission: &mut Option<(String, u64)>,
) -> Result<(), BotError>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    if &choice.player != player {
        return Err(BotError::Protocol(
            "pending choice belongs to another player".to_owned(),
        ));
    }
    let key = (nonce.to_owned(), game_version);
    if last_submission.as_ref() == Some(&key) {
        return Ok(());
    }
    let offered = offered_ids(choice)?;
    let response = client
        .post(config.advisor_url())
        .json(&serde_json::json!({
            "state": state,
            "galaxy_layout": galaxy_layout,
            "player": player,
            "choice": choice,
            "temperature": config.temperature,
        }))
        .send()
        .await
        .map_err(|error| BotError::Advisor(error.to_string()))?
        .error_for_status()
        .map_err(|error| BotError::Advisor(error.to_string()))?
        .json::<AdvisorResponse>()
        .await
        .map_err(|error| BotError::Advisor(error.to_string()))?;
    let probabilities = validate_advice(&offered, response)?;
    let option_id = select_option(&offered, &probabilities, sampler)?;
    send_message(
        stream,
        &ClientMessage::SubmitChoice {
            protocol_version: PROTOCOL_VERSION,
            game_id: config.game_id.clone(),
            nonce: nonce.to_owned(),
            expected_version: game_version,
            option_id,
        },
        config.timeout,
    )
    .await?;
    *last_submission = Some(key);
    Ok(())
}

fn offered_ids(choice: &Choice) -> Result<Vec<String>, BotError> {
    if choice.options.is_empty() {
        return Err(BotError::Protocol(
            "pending choice has no options".to_owned(),
        ));
    }
    let ids: Vec<_> = choice
        .options
        .iter()
        .map(|option| option.id.clone())
        .collect();
    if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
        return Err(BotError::Protocol(
            "pending choice has duplicate option IDs".to_owned(),
        ));
    }
    Ok(ids)
}

fn validate_advice(
    offered: &[String],
    response: AdvisorResponse,
) -> Result<BTreeMap<String, f64>, BotError> {
    if response.options.len() != offered.len() {
        return Err(BotError::Advisor(
            "advisor response does not cover every offered option".to_owned(),
        ));
    }
    let offered: BTreeSet<_> = offered.iter().map(String::as_str).collect();
    let mut probabilities = BTreeMap::new();
    for option in response.options {
        if !offered.contains(option.option_id.as_str())
            || !option.probability.is_finite()
            || option.probability < 0.0
            || !option.logit.is_finite()
            || probabilities
                .insert(option.option_id, option.probability)
                .is_some()
        {
            return Err(BotError::Advisor(
                "advisor response is malformed".to_owned(),
            ));
        }
    }
    if probabilities.values().sum::<f64>() <= 0.0 {
        return Err(BotError::Advisor(
            "advisor response has zero probability mass".to_owned(),
        ));
    }
    Ok(probabilities)
}

fn select_option(
    offered: &[String],
    probabilities: &BTreeMap<String, f64>,
    sampler: Option<&mut ChaCha8Rng>,
) -> Result<String, BotError> {
    if let Some(rng) = sampler {
        let total = probabilities.values().sum::<f64>();
        let mut target = rng.random_range(0.0..total);
        for option_id in offered {
            target -= probabilities[option_id];
            if target <= 0.0 {
                return Ok(option_id.clone());
            }
        }
        return offered
            .last()
            .cloned()
            .ok_or_else(|| BotError::Advisor("advisor response has no options".to_owned()));
    }
    let first = offered
        .first()
        .ok_or_else(|| BotError::Advisor("advisor response has no options".to_owned()))?;
    let selected = offered.iter().skip(1).fold(first, |selected, candidate| {
        if probabilities[candidate] > probabilities[selected] {
            candidate
        } else {
            selected
        }
    });
    Ok(selected.clone())
}

async fn send_message<S>(
    stream: &mut S,
    message: &ClientMessage,
    timeout: Duration,
) -> Result<(), BotError>
where
    S: SinkExt<Message> + Unpin,
    S::Error: std::fmt::Display,
{
    let text = serde_json::to_string(message)
        .map_err(|error| BotError::Protocol(format!("cannot serialize client message: {error}")))?;
    within(timeout, stream.send(Message::Text(text.into())))
        .await
        .map_err(BotError::WebSocket)?
        .map_err(|error| BotError::WebSocket(error.to_string()))
}

async fn within<T, F>(timeout: Duration, future: F) -> Result<T, String>
where
    F: std::future::Future<Output = T>,
{
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|_| "operation timed out".to_owned())
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_millis(100 * (1_u64 << attempt.min(5)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Json, Router, routing::post};
    use ti4_server::fixtures::{sample_actor_snapshot, sample_terminal_game_over};

    #[test]
    fn never_act_for_a_different_authenticated_player() {
        assert!(require_viewer(&ViewerRole::Spectator, &PlayerId::new("player_one")).is_err());
        assert!(
            require_viewer(
                &ViewerRole::Player(PlayerId::new("player_two")),
                &PlayerId::new("player_one")
            )
            .is_err()
        );
    }

    #[test]
    fn takeover_requires_a_valid_explicit_selection() {
        let choices = [
            (1, Some("Same name".to_owned()), PlayerId::new("player_one")),
            (2, Some("Same name".to_owned()), PlayerId::new("player_two")),
        ];
        assert_eq!(
            select_takeover(&choices, "2\n").unwrap(),
            PlayerId::new("player_two")
        );
        for input in ["", "0", "3", "player_one"] {
            assert!(select_takeover(&choices, input).is_err());
        }
    }

    #[test]
    fn bot_nickname_matches_server_validation_before_network_access() {
        let mut config = BotConfig::new(
            "ws://localhost:8080".to_owned(),
            "game_1".to_owned(),
            "http://localhost:8081".to_owned(),
            "🪐".repeat(16),
        );
        assert!(config.validate().is_ok());
        for invalid in [
            "",
            " bad",
            "bad ",
            "🪐".repeat(17).as_str(),
            "bad\u{202e}",
            "bad\n",
        ] {
            config.nickname = invalid.to_owned();
            assert!(matches!(config.validate(), Err(BotError::Configuration(_))));
        }
    }

    #[tokio::test]
    async fn real_lobby_join_ready_and_heartbeat_survive_the_presence_grace() {
        use std::sync::Arc;
        const GRACE: Duration = Duration::from_millis(500);
        const TEST_HEARTBEAT: Duration = Duration::from_millis(100);
        let registry =
            Arc::new(ti4_server::session::GameRegistry::new().with_presence_grace(GRACE));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(
            axum::serve(listener, ti4_server::create_app(registry.clone())).into_future(),
        );
        let result = tokio::time::timeout(Duration::from_secs(10), async {
            let client = reqwest::Client::new();
            let created: serde_json::Value = client
                .post(format!("http://{address}/api/games"))
                .json(&serde_json::json!({"player_count": 2, "seed": 42, "nickname": "Host"}))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            let game = created["game_id"].as_str().unwrap();
            let host_session = created["player_session"].as_str().unwrap();
            let mut config = BotConfig::new(
                format!("ws://{address}"),
                game.to_owned(),
                "http://127.0.0.1:1".to_owned(),
                "Bot α".to_owned(),
            );
            config.timeout = Duration::from_secs(2);
            let admitted = admit(&config, &client).await.unwrap();
            let roster: serde_json::Value = client
                .get(config.lobby_url(""))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(roster["slots"][1]["nickname"], "Bot α");
            assert_eq!(roster["slots"][1]["occupant"], admitted.player.as_str());
            let reconnect: serde_json::Value = client
                .post(config.lobby_url("/join"))
                .header("x-ti4-player-session", &admitted.session)
                .json(&serde_json::json!({"kind": "new"}))
                .send()
                .await
                .unwrap()
                .error_for_status()
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(reconnect["player"]["id"], admitted.player.as_str());
            assert_eq!(reconnect["lobby"]["slots"][1]["nickname"], "Bot α");
            assert_ne!(
                admitted.player.as_str(),
                created["player"]["id"].as_str().unwrap()
            );
            let bot = tokio::spawn({
                let config = config.clone();
                let admission = Admission {
                    player: admitted.player.clone(),
                    session: admitted.session.clone(),
                    phase: admitted.phase.clone(),
                };
                async move {
                    wait_for_start_with_interval(
                        &config,
                        &reqwest::Client::new(),
                        &admission,
                        TEST_HEARTBEAT,
                    )
                    .await
                }
            });
            for _ in 0..4 {
                tokio::time::sleep(Duration::from_millis(200)).await;
                let view: serde_json::Value = client
                    .get(config.lobby_url(""))
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                let seat = view["slots"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|slot| slot["occupant"] == admitted.player.as_str())
                    .unwrap();
                assert_eq!(seat["ready"], true);
                assert_eq!(seat["can_take_over"], false);
            }
            request_lobby(
                client
                    .post(config.lobby_url("/ready"))
                    .header("x-ti4-player-session", host_session)
                    .json(&serde_json::json!({"ready": true}))
                    .send()
                    .await,
            )
            .await
            .unwrap();
            request_lobby(
                client
                    .post(config.lobby_url("/start"))
                    .header("x-ti4-player-session", host_session)
                    .send()
                    .await,
            )
            .await
            .unwrap();
            tokio::time::timeout(Duration::from_secs(5), bot)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            let (mut socket, _) = connect_async(config.websocket_url()).await.unwrap();
            send_message(
                &mut socket,
                &ClientMessage::Subscribe {
                    protocol_version: PROTOCOL_VERSION,
                    game_id: game.to_owned(),
                    player_session: Some(admitted.session.clone()),
                },
                Duration::from_secs(2),
            )
            .await
            .unwrap();
            let text = socket.next().await.unwrap().unwrap().into_text().unwrap();
            let ServerMessage::InitialSnapshot(snapshot) = parse_server_message(&text).unwrap()
            else {
                panic!("expected snapshot");
            };
            require_viewer(&snapshot.viewer, &admitted.player).unwrap();
            drop(socket);
            tokio::time::sleep(GRACE + Duration::from_millis(100)).await;
            config.nickname = "Replacement bot".to_owned();
            let replacement = admit_with_selection(&config, &client, Some(admitted.player.clone()))
                .await
                .unwrap();
            assert_eq!(replacement.player, admitted.player);
            assert_eq!(replacement.phase, "running");
            assert_ne!(replacement.session, admitted.session);
            let renamed: serde_json::Value = client
                .get(config.lobby_url(""))
                .send()
                .await
                .unwrap()
                .json()
                .await
                .unwrap();
            assert_eq!(renamed["slots"][1]["nickname"], "Replacement bot");
            assert_eq!(renamed["slots"][1]["occupant"], admitted.player.as_str());
            let revoked = client
                .post(config.lobby_url("/heartbeat"))
                .header("x-ti4-player-session", admitted.session)
                .send()
                .await
                .unwrap();
            assert_eq!(revoked.status(), reqwest::StatusCode::FORBIDDEN);
            let current = client
                .post(config.lobby_url("/heartbeat"))
                .header("x-ti4-player-session", replacement.session)
                .send()
                .await
                .unwrap();
            assert!(current.status().is_success());
        })
        .await;
        server.abort();
        result.expect("lobby heartbeat test timed out after 10 seconds");
    }

    fn advice(options: &[(&str, f64, f64)]) -> AdvisorResponse {
        AdvisorResponse {
            options: options
                .iter()
                .map(|(option_id, probability, logit)| AdvisorOption {
                    option_id: (*option_id).to_owned(),
                    probability: *probability,
                    logit: *logit,
                })
                .collect(),
        }
    }

    #[test]
    fn argmax_selects_the_highest_advisor_probability() {
        let offered = vec!["first".to_owned(), "second".to_owned()];
        let probabilities = validate_advice(
            &offered,
            advice(&[("first", 0.25, 2.0), ("second", 0.75, 1.0)]),
        )
        .expect("valid advice");
        assert_eq!(
            select_option(&offered, &probabilities, None).unwrap(),
            "second"
        );
    }

    #[test]
    fn argmax_breaks_equal_probabilities_by_original_option_order() {
        let offered = vec!["first".to_owned(), "second".to_owned()];
        let probabilities = validate_advice(
            &offered,
            advice(&[("first", 0.5, 2.0), ("second", 0.5, 1.0)]),
        )
        .expect("valid advice");
        assert_eq!(
            select_option(&offered, &probabilities, None).unwrap(),
            "first"
        );
    }

    #[test]
    fn seeded_sampling_is_reproducible() {
        let offered = vec!["first".to_owned(), "second".to_owned()];
        let probabilities = validate_advice(
            &offered,
            advice(&[("first", 0.25, 0.0), ("second", 0.75, 1.0)]),
        )
        .expect("valid advice");
        let mut left = ChaCha8Rng::seed_from_u64(42);
        let mut right = ChaCha8Rng::seed_from_u64(42);
        let left: Vec<_> = (0..10)
            .map(|_| select_option(&offered, &probabilities, Some(&mut left)).unwrap())
            .collect();
        let right: Vec<_> = (0..10)
            .map(|_| select_option(&offered, &probabilities, Some(&mut right)).unwrap())
            .collect();
        assert_eq!(left, right);
    }

    #[test]
    fn malformed_advice_is_rejected_without_a_submission() {
        let offered = vec!["first".to_owned(), "second".to_owned()];
        assert!(
            validate_advice(
                &offered,
                advice(&[("first", 0.5, 0.0), ("unknown", 0.5, 0.0)])
            )
            .is_err()
        );
        assert!(
            validate_advice(
                &offered,
                advice(&[("first", 0.5, 0.0), ("first", 0.5, 0.0)])
            )
            .is_err()
        );
        assert!(
            validate_advice(
                &offered,
                advice(&[("first", f64::NAN, 0.0), ("second", 1.0, 0.0)])
            )
            .is_err()
        );
        assert!(
            validate_advice(
                &offered,
                advice(&[("first", 0.0, 0.0), ("second", 0.0, 0.0)])
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn initial_snapshot_submission_uses_its_legal_option_nonce_and_version() {
        let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind advisor");
        let advisor_address = advisor_listener.local_addr().expect("advisor address");
        let advisor = Router::new().route(
            "/evaluate",
            post(|| async {
                Json(serde_json::json!({
                    "options": [
                        { "option_id": "opt_carrier", "probability": 0.2, "logit": 1.0 },
                        { "option_id": "opt_infantry", "probability": 0.8, "logit": 2.0 },
                        { "option_id": "decline", "probability": 0.0, "logit": -1.0 }
                    ]
                }))
            }),
        );
        let advisor_task = tokio::spawn(async move {
            axum::serve(advisor_listener, advisor)
                .await
                .expect("serve advisor");
        });

        let websocket_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind websocket");
        let websocket_address = websocket_listener.local_addr().expect("websocket address");
        let websocket_task = tokio::spawn(async move {
            let (stream, _) = websocket_listener.accept().await.expect("accept bot");
            let mut socket = tokio_tungstenite::accept_async(stream)
                .await
                .expect("upgrade websocket");
            let subscribe = socket
                .next()
                .await
                .expect("subscribe message")
                .expect("subscribe transport");
            let subscribe: ClientMessage =
                serde_json::from_str(subscribe.to_text().expect("text")).expect("parse subscribe");
            assert!(matches!(
                subscribe,
                ClientMessage::Subscribe { ref game_id, ref player_session, .. }
                    if game_id == "game_12345" && player_session.as_deref() == Some("private-session")
            ));

            let snapshot =
                serde_json::to_string(&sample_actor_snapshot()).expect("serialize snapshot");
            socket
                .send(Message::Text(snapshot.into()))
                .await
                .expect("send snapshot");
            let submission = socket
                .next()
                .await
                .expect("submission message")
                .expect("submission transport");
            let submission: ClientMessage =
                serde_json::from_str(submission.to_text().expect("text"))
                    .expect("parse submission");
            assert!(matches!(
                submission,
                ClientMessage::SubmitChoice {
                    ref nonce,
                    expected_version: 42,
                    ref option_id,
                    ..
                } if nonce == "nonce_xyz789" && option_id == "opt_infantry"
            ));
            let ping = tokio::time::timeout(Duration::from_secs(12), socket.next())
                .await
                .expect("application heartbeat deadline")
                .expect("ping message")
                .expect("ping transport");
            let ping: ClientMessage =
                serde_json::from_str(ping.to_text().expect("text")).expect("parse ping");
            assert!(matches!(
                ping,
                ClientMessage::Ping {
                    protocol_version: PROTOCOL_VERSION,
                    sequence: 1
                }
            ));
            let pong = serde_json::to_string(&ServerMessage::Pong(
                ti4_server::protocol::server::PongMsg {
                    protocol_version: PROTOCOL_VERSION,
                    sequence: 1,
                },
            ))
            .unwrap();
            socket.send(Message::Text(pong.into())).await.unwrap();
            let terminal =
                serde_json::to_string(&sample_terminal_game_over()).expect("serialize terminal");
            socket
                .send(Message::Text(terminal.into()))
                .await
                .expect("send terminal");
        });

        let mut config = BotConfig::new(
            format!("ws://{websocket_address}"),
            "game_12345".to_owned(),
            format!("http://{advisor_address}"),
            "Bot".to_owned(),
        );
        config.timeout = Duration::from_secs(2);
        config.max_reconnects = 0;
        let admission = Admission {
            player: PlayerId::new("seat_a"),
            session: "private-session".to_owned(),
            phase: "running".to_owned(),
        };
        let client = reqwest::Client::new();
        tokio::time::timeout(
            Duration::from_secs(16),
            run_existing_session(&config, &client, &admission, &mut None),
        )
        .await
        .expect("bot run should not hang")
        .expect("bot should finish at game over");
        websocket_task.await.expect("websocket task");
        advisor_task.abort();
    }

    #[tokio::test]
    async fn reconnect_uses_a_new_initial_snapshot_without_resending_a_cached_choice() {
        let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind advisor");
        let advisor_address = advisor_listener.local_addr().expect("advisor address");
        let advisor = Router::new().route(
            "/evaluate",
            post(|| async {
                Json(serde_json::json!({
                    "options": [
                        { "option_id": "opt_carrier", "probability": 1.0, "logit": 1.0 },
                        { "option_id": "opt_infantry", "probability": 0.0, "logit": 0.0 },
                        { "option_id": "decline", "probability": 0.0, "logit": -1.0 }
                    ]
                }))
            }),
        );
        let advisor_task = tokio::spawn(async move {
            axum::serve(advisor_listener, advisor)
                .await
                .expect("serve advisor");
        });

        let websocket_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind websocket");
        let websocket_address = websocket_listener.local_addr().expect("websocket address");
        let websocket_task = tokio::spawn(async move {
            let (first_stream, _) = websocket_listener.accept().await.expect("first accept");
            let mut first = tokio_tungstenite::accept_async(first_stream)
                .await
                .expect("first upgrade");
            first
                .next()
                .await
                .expect("first subscribe")
                .expect("first transport");
            drop(first);

            let (second_stream, _) = websocket_listener.accept().await.expect("second accept");
            let mut second = tokio_tungstenite::accept_async(second_stream)
                .await
                .expect("second upgrade");
            second
                .next()
                .await
                .expect("second subscribe")
                .expect("second transport");
            let snapshot =
                serde_json::to_string(&sample_actor_snapshot()).expect("serialize snapshot");
            second
                .send(Message::Text(snapshot.into()))
                .await
                .expect("send snapshot");
            let submission = second
                .next()
                .await
                .expect("submission message")
                .expect("submission transport");
            let submission: ClientMessage =
                serde_json::from_str(submission.to_text().expect("text"))
                    .expect("parse submission");
            assert!(matches!(
                submission,
                ClientMessage::SubmitChoice { ref option_id, .. } if option_id == "opt_carrier"
            ));
            let terminal =
                serde_json::to_string(&sample_terminal_game_over()).expect("serialize terminal");
            second
                .send(Message::Text(terminal.into()))
                .await
                .expect("send terminal");
        });

        let mut config = BotConfig::new(
            format!("ws://{websocket_address}"),
            "game_12345".to_owned(),
            format!("http://{advisor_address}"),
            "Bot".to_owned(),
        );
        config.timeout = Duration::from_secs(2);
        config.max_reconnects = 1;
        let admission = Admission {
            player: PlayerId::new("seat_a"),
            session: "private-session".to_owned(),
            phase: "running".to_owned(),
        };
        let client = reqwest::Client::new();
        tokio::time::timeout(
            Duration::from_secs(5),
            run_existing_session(&config, &client, &admission, &mut None),
        )
        .await
        .expect("bot run should not hang")
        .expect("bot should reconnect and finish");
        websocket_task.await.expect("websocket task");
        advisor_task.abort();
    }

    #[tokio::test]
    async fn bots_play_against_each_other_to_game_over_with_mock_advisor() {
        let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind advisor");
        let advisor_address = advisor_listener.local_addr().expect("advisor address");
        let advisor = Router::new().route(
            "/evaluate",
            post(|Json(req): Json<serde_json::Value>| async move {
                let empty = Vec::new();
                let options_slice = req["choice"]["options"]
                    .as_array()
                    .or_else(|| req["options"].as_array())
                    .unwrap_or(&empty);
                let options = options_slice
                    .iter()
                    .map(|opt| {
                        let id = opt["id"]
                            .as_str()
                            .or_else(|| opt["option_id"].as_str())
                            .unwrap_or("unknown");
                        serde_json::json!({
                            "option_id": id,
                            "probability": 1.0,
                            "logit": 0.0,
                        })
                    })
                    .collect::<Vec<_>>();
                Json(serde_json::json!({ "options": options }))
            }),
        );
        let advisor_task = tokio::spawn(async move {
            axum::serve(advisor_listener, advisor)
                .await
                .expect("serve advisor");
        });

        let registry = std::sync::Arc::new(ti4_server::session::GameRegistry::new());
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind server");
        let server_address = server_listener.local_addr().expect("server address");
        let server_registry = registry.clone();
        let server_task = tokio::spawn(async move {
            axum::serve(server_listener, ti4_server::create_app(server_registry))
                .await
                .expect("serve server");
        });

        let client = reqwest::Client::new();
        let launched = client
            .post(format!("http://{server_address}/api/dev/scenarios/launch"))
            .json(&serde_json::json!({ "scenario_id": "endgame", "seed": 4_242 }))
            .send()
            .await
            .expect("launch request")
            .error_for_status()
            .expect("launch status")
            .json::<serde_json::Value>()
            .await
            .expect("launch json");

        let game_id = launched["game_id"].as_str().expect("game_id").to_owned();
        let test_seats = launched["test_seats"].as_object().expect("test_seats");
        let mut tokens: Vec<String> = test_seats
            .values()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        assert_eq!(tokens.len(), 3, "expected 3 seat tokens");
        let p1_token = tokens.remove(0);
        let p2_token = tokens.remove(0);
        let p3_token = tokens.remove(0);

        let mut b1 = BotConfig::new(
            format!("ws://{server_address}"),
            game_id.clone(),
            format!("http://{advisor_address}"),
            "Bot Sol".to_owned(),
        )
        .with_player_session(p1_token);
        b1.timeout = Duration::from_secs(5);
        b1.max_reconnects = 1;

        let mut b2 = BotConfig::new(
            format!("ws://{server_address}"),
            game_id.clone(),
            format!("http://{advisor_address}"),
            "Bot Hacan".to_owned(),
        )
        .with_player_session(p2_token);
        b2.timeout = Duration::from_secs(5);
        b2.max_reconnects = 1;

        let mut b3 = BotConfig::new(
            format!("ws://{server_address}"),
            game_id.clone(),
            format!("http://{advisor_address}"),
            "Bot Letnev".to_owned(),
        )
        .with_player_session(p3_token);
        b3.timeout = Duration::from_secs(5);
        b3.max_reconnects = 1;

        let b1_task = tokio::spawn(run(b1));
        let b2_task = tokio::spawn(run(b2));
        let b3_task = tokio::spawn(run(b3));

        let run_result = tokio::time::timeout(Duration::from_secs(20), async {
            tokio::try_join!(b1_task, b2_task, b3_task)
        })
        .await;

        advisor_task.abort();
        server_task.abort();

        let (r1, r2, r3) = run_result
            .expect("game should complete within timeout")
            .expect("bots joined successfully");
        r1.expect("bot 1 finished with Ok(()) on GameOver");
        r2.expect("bot 2 finished with Ok(()) on GameOver");
        r3.expect("bot 3 finished with Ok(()) on GameOver");

        let session = registry.get_game(&game_id).expect("session exists");
        assert!(session.is_finished(), "session marked is_finished");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    #[ignore = "on-demand full game simulation from round 1 to GameOver"]
    async fn bots_play_full_game_start_to_finish_on_demand_with_mock_advisor() {
        let advisor_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind advisor");
        let advisor_address = advisor_listener.local_addr().expect("advisor address");
        let advisor = axum::Router::new().route(
            "/evaluate",
            post(|Json(req): Json<serde_json::Value>| async move {
                let empty = Vec::new();
                let options_slice = req["choice"]["options"]
                    .as_array()
                    .or_else(|| req["options"].as_array())
                    .unwrap_or(&empty);
                let options = options_slice
                    .iter()
                    .map(|opt| {
                        let id = opt["id"]
                            .as_str()
                            .or_else(|| opt["option_id"].as_str())
                            .unwrap_or("unknown");
                        serde_json::json!({
                            "option_id": id,
                            "probability": 1.0,
                            "logit": 0.0,
                        })
                    })
                    .collect::<Vec<_>>();
                Json(serde_json::json!({ "options": options }))
            }),
        );
        let advisor_task = tokio::spawn(async move {
            axum::serve(advisor_listener, advisor)
                .await
                .expect("serve advisor");
        });

        let registry = std::sync::Arc::new(ti4_server::session::GameRegistry::new());
        let server_listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind server");
        let server_address = server_listener.local_addr().expect("server address");
        let server_registry = registry.clone();
        let server_task = tokio::spawn(async move {
            axum::serve(server_listener, ti4_server::create_app(server_registry))
                .await
                .expect("serve server");
        });

        let client = reqwest::Client::new();
        let launched = client
            .post(format!("http://{server_address}/api/dev/scenarios/launch"))
            .json(&serde_json::json!({ "scenario_id": "full_game", "seed": 42 }))
            .send()
            .await
            .expect("launch request")
            .error_for_status()
            .expect("launch status")
            .json::<serde_json::Value>()
            .await
            .expect("launch json");

        let game_id = launched["game_id"].as_str().expect("game_id").to_owned();
        let test_seats = launched["test_seats"].as_object().expect("test_seats");
        let mut tokens: Vec<String> = test_seats
            .values()
            .filter_map(|v| v.as_str().map(str::to_owned))
            .collect();
        assert_eq!(tokens.len(), 3, "expected 3 seat tokens");
        let p1_token = tokens.remove(0);
        let p2_token = tokens.remove(0);
        let p3_token = tokens.remove(0);

        let mut b1 = BotConfig::new(
            format!("ws://{server_address}"),
            game_id.clone(),
            format!("http://{advisor_address}"),
            "Bot Sol".to_owned(),
        )
        .with_player_session(p1_token);
        b1.timeout = Duration::from_secs(5);
        b1.max_reconnects = 1;

        let mut b2 = BotConfig::new(
            format!("ws://{server_address}"),
            game_id.clone(),
            format!("http://{advisor_address}"),
            "Bot Hacan".to_owned(),
        )
        .with_player_session(p2_token);
        b2.timeout = Duration::from_secs(5);
        b2.max_reconnects = 1;

        let mut b3 = BotConfig::new(
            format!("ws://{server_address}"),
            game_id.clone(),
            format!("http://{advisor_address}"),
            "Bot Letnev".to_owned(),
        )
        .with_player_session(p3_token);
        b3.timeout = Duration::from_secs(5);
        b3.max_reconnects = 1;

        let b1_task = tokio::spawn(run(b1));
        let b2_task = tokio::spawn(run(b2));
        let b3_task = tokio::spawn(run(b3));

        let run_result = tokio::time::timeout(Duration::from_secs(60), async {
            tokio::try_join!(b1_task, b2_task, b3_task)
        })
        .await;

        advisor_task.abort();
        server_task.abort();

        let (r1, r2, r3) = run_result
            .expect("game should complete within timeout")
            .expect("bots joined successfully");
        r1.expect("bot 1 finished with Ok(()) on GameOver");
        r2.expect("bot 2 finished with Ok(()) on GameOver");
        r3.expect("bot 3 finished with Ok(()) on GameOver");

        let session = registry.get_game(&game_id).expect("session exists");
        assert!(session.is_finished(), "session marked is_finished");
        assert!(
            session.decision_log().len() > 10,
            "full game should record many decisions: {}",
            session.decision_log().len()
        );
    }
}
