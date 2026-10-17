//! Public-playtest hardening: environment flags, per-IP rate limits, caps and the shared
//! password gate. Everything is off unless `TI4_PUBLIC=1` (or the specific env var) is set, so
//! local development behaves exactly as before.

pub mod access;
pub mod ratelimit;

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::Json;
use axum::body::Body;
use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use serde::Deserialize;

use crate::session::GameRegistry;
use access::{AccessGate, COOKIE_MAX_AGE_SECS, COOKIE_NAME, cookie_value};
use ratelimit::{LimitKind, RateLimiter, RateLimits};

/// Static settings, usually read from the environment.
#[derive(Debug, Clone, Default)]
pub struct PublicConfig {
    /// `TI4_PUBLIC=1`: dev routes off, safe defaults for everything below.
    pub public: bool,
    /// `TI4_WEB_DIR`: directory with the built web client.
    pub web_dir: Option<PathBuf>,
    /// `TI4_ACCESS_PASSWORD`: when set, `/api` and `/ws` need the access cookie.
    pub access_password: Option<String>,
    /// `TI4_COOKIE_SECRET`: signing secret; random per start when unset.
    pub cookie_secret: Option<String>,
    /// `TI4_COOKIE_SECURE`: force the Secure attribute on or off.
    pub cookie_secure: Option<bool>,
    /// `TI4_PUBLIC_ORIGIN`: the only allowed CORS origin; none means same-origin only.
    pub public_origin: Option<String>,
    /// `TI4_TRUST_PROXY=1`: read the client IP from `CF-Connecting-IP` / `X-Forwarded-For`.
    pub trust_proxy: bool,
    pub max_games: Option<usize>,
    pub max_lobbies_per_ip: Option<usize>,
    pub max_data_bytes: Option<u64>,
    /// Feedback reports and the data-dir size check live under this directory.
    pub data_dir: Option<PathBuf>,
    pub rate_limits: Option<RateLimits>,
    pub ws_idle_timeout: Option<Duration>,
    pub build_commit: Option<String>,
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

fn env_flag(name: &str) -> bool {
    matches!(env_nonempty(name).as_deref(), Some("1" | "true" | "yes"))
}

impl PublicConfig {
    /// Reads the `TI4_*` environment. `data_dir` is the game store directory.
    #[must_use]
    pub fn from_env(data_dir: &Path) -> Self {
        let public = env_flag("TI4_PUBLIC");
        let parse_usize = |name: &str, default: Option<usize>| {
            env_nonempty(name).and_then(|v| v.parse().ok()).or(default)
        };
        Self {
            public,
            web_dir: env_nonempty("TI4_WEB_DIR").map(PathBuf::from),
            access_password: env_nonempty("TI4_ACCESS_PASSWORD"),
            cookie_secret: env_nonempty("TI4_COOKIE_SECRET"),
            cookie_secure: env_nonempty("TI4_COOKIE_SECURE").map(|v| v == "1" || v == "true"),
            public_origin: env_nonempty("TI4_PUBLIC_ORIGIN"),
            trust_proxy: env_flag("TI4_TRUST_PROXY"),
            max_games: parse_usize("TI4_MAX_GAMES", public.then_some(200)),
            max_lobbies_per_ip: parse_usize("TI4_MAX_LOBBIES_PER_IP", public.then_some(4)),
            max_data_bytes: env_nonempty("TI4_MAX_DATA_BYTES")
                .and_then(|v| v.parse().ok())
                .or(public.then_some(2 * 1024 * 1024 * 1024)),
            data_dir: Some(data_dir.to_path_buf()),
            rate_limits: public.then(RateLimits::from_env),
            ws_idle_timeout: parse_usize("TI4_WS_IDLE_SECS", public.then_some(300))
                .map(|s| Duration::from_secs(s as u64)),
            build_commit: env_nonempty("TI4_BUILD_COMMIT"),
        }
    }
}

/// Runtime state derived from [`PublicConfig`].
pub struct PublicState {
    pub config: PublicConfig,
    gate: Option<AccessGate>,
    limiter: Option<RateLimiter>,
    created: Mutex<HashMap<IpAddr, Vec<(String, u64)>>>,
}

impl Default for PublicState {
    fn default() -> Self {
        Self::new(PublicConfig::default())
    }
}

impl PublicState {
    #[must_use]
    pub fn new(config: PublicConfig) -> Self {
        let secret: Vec<u8> = match &config.cookie_secret {
            Some(s) => s.as_bytes().to_vec(),
            None => rand::random::<[u8; 32]>().to_vec(),
        };
        let gate = config
            .access_password
            .clone()
            .map(|pw| AccessGate::new(pw, &secret));
        let limiter = config.rate_limits.clone().map(RateLimiter::new);
        Self {
            config,
            gate,
            limiter,
            created: Mutex::new(HashMap::new()),
        }
    }

    #[must_use]
    pub fn is_public(&self) -> bool {
        self.config.public
    }

    /// Build id reported by `/health` and attached to feedback.
    #[must_use]
    pub fn build_commit(&self) -> String {
        self.config
            .build_commit
            .clone()
            .or_else(|| option_env!("TI4_BUILD_COMMIT").map(str::to_owned))
            .unwrap_or_else(|| "dev".to_owned())
    }

    fn active(&self) -> bool {
        self.gate.is_some()
            || self.limiter.is_some()
            || self.config.max_games.is_some()
            || self.config.max_lobbies_per_ip.is_some()
            || self.config.max_data_bytes.is_some()
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn json_error(status: StatusCode, code: &str, message: &str) -> Response {
    (
        status,
        Json(serde_json::json!({ "error": code, "message": message })),
    )
        .into_response()
}

/// Client IP: the socket peer, or the proxy-supplied address when `trust_proxy` is on and the
/// peer is loopback (Caddy runs on 127.0.0.1).
fn client_ip(config: &PublicConfig, headers: &HeaderMap, peer: Option<IpAddr>) -> IpAddr {
    let peer_ip = peer.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    if config.trust_proxy && peer_ip.is_loopback() {
        let from_header = headers
            .get("cf-connecting-ip")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<IpAddr>().ok())
            .or_else(|| {
                // The rightmost entry is the one our own proxy appended.
                headers
                    .get("x-forwarded-for")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.rsplit(',').next())
                    .and_then(|v| v.trim().parse::<IpAddr>().ok())
            });
        if let Some(ip) = from_header {
            return ip;
        }
    }
    peer_ip
}

fn classify(method: &Method, path: &str) -> Option<LimitKind> {
    if path.starts_with("/ws/") {
        return Some(LimitKind::WsConnect);
    }
    if !path.starts_with("/api/") {
        return None;
    }
    let post = method == Method::POST;
    if path == "/api/access" {
        return post.then_some(LimitKind::Password);
    }
    if path == "/api/feedback" {
        return post.then_some(LimitKind::Feedback);
    }
    if path == "/api/games" && post {
        return Some(LimitKind::GameCreate);
    }
    if path.ends_with("/lobby/join") || path.ends_with("/lobby/bots") {
        return Some(LimitKind::LobbyJoin);
    }
    if path.ends_with("/batches") {
        return Some(LimitKind::Batch);
    }
    if path.ends_with("/replay")
        || path.ends_with("/turn-redo")
        || path.contains("/history")
        || path.ends_with("/map-preview")
        || path.ends_with("/preview")
    {
        return Some(LimitKind::Heavy);
    }
    Some(LimitKind::General)
}

fn dir_size(root: &Path, stop_above: u64) -> u64 {
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total += meta.len();
                if total > stop_above {
                    return total;
                }
            }
        }
    }
    total
}

fn has_valid_cookie(gate: &AccessGate, headers: &HeaderMap) -> bool {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .filter_map(cookie_value)
        .any(|v| gate.verify(v, now_unix()))
}

/// Middleware: access gate, per-IP rate limits and creation caps.
pub async fn guard(
    State(registry): State<Arc<GameRegistry>>,
    mut req: Request,
    next: Next,
) -> Response {
    let public = registry.public();
    if !public.active() {
        return next.run(req).await;
    }
    let path = req.uri().path().to_owned();
    let method = req.method().clone();
    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|c| c.0.ip());
    let ip = client_ip(&public.config, req.headers(), peer);
    let is_access = path == "/api/access";
    let guarded_path = path.starts_with("/api/") || path.starts_with("/ws/");

    if let Some(gate) = &public.gate
        && guarded_path
        && !is_access
        && !has_valid_cookie(gate, req.headers())
    {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "access_required",
            "enter the playtest password",
        );
    }

    if let (Some(limiter), Some(kind)) = (&public.limiter, classify(&method, &path))
        && let Err(wait) = limiter.check(kind, ip)
    {
        let mut response = json_error(
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limited",
            "too many requests, slow down",
        );
        if let Ok(v) = HeaderValue::from_str(&wait.as_secs().max(1).to_string()) {
            response.headers_mut().insert(header::RETRY_AFTER, v);
        }
        return response;
    }

    let creating = method == Method::POST && path == "/api/games";
    if creating {
        if let Some(refusal) = public.check_create_caps(&registry, ip).await {
            return refusal;
        }
        req.extensions_mut().insert(ip);
        let response = next.run(req).await;
        return public.record_created(response, ip).await;
    }
    next.run(req).await
}

impl PublicState {
    async fn check_create_caps(
        &self,
        registry: &Arc<GameRegistry>,
        ip: IpAddr,
    ) -> Option<Response> {
        let games = registry.list_games();
        if let Some(max) = self.config.max_games
            && games.len() >= max
        {
            return Some(json_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "server_full",
                "the playtest server is full, try again later",
            ));
        }
        if let Some(max) = self.config.max_lobbies_per_ip {
            let open: std::collections::HashSet<&str> = games
                .iter()
                .filter(|g| !g.is_finished)
                .map(|g| g.game_id.as_str())
                .collect();
            let mut created = self.created.lock().expect("created lock");
            let mine = created.entry(ip).or_default();
            mine.retain(|(id, _)| open.contains(id.as_str()));
            if mine.len() >= max {
                let ids: Vec<String> = mine.iter().map(|(id, _)| id.clone()).collect();
                let created_at: HashMap<&str, u64> =
                    mine.iter().map(|(id, t)| (id.as_str(), *t)).collect();
                let listed: Vec<serde_json::Value> = registry
                    .open_game_infos(&ids)
                    .into_iter()
                    .map(|g| {
                        serde_json::json!({
                            "game_id": g.game_id,
                            "created_at": created_at.get(g.game_id.as_str()),
                            "phase": g.phase,
                            "players": g.players,
                        })
                    })
                    .collect();
                return Some(
                    (
                        StatusCode::TOO_MANY_REQUESTS,
                        Json(serde_json::json!({
                            "error": "too_many_games",
                            "message": "you already have several unfinished games on this server",
                            "games": listed,
                        })),
                    )
                        .into_response(),
                );
            }
        }
        if let (Some(max), Some(dir)) = (self.config.max_data_bytes, self.config.data_dir.clone()) {
            let size = tokio::task::spawn_blocking(move || dir_size(&dir, max))
                .await
                .unwrap_or(0);
            if size >= max {
                return Some(json_error(
                    StatusCode::INSUFFICIENT_STORAGE,
                    "disk_full",
                    "the playtest server is out of storage",
                ));
            }
        }
        None
    }

    async fn record_created(&self, response: Response, ip: IpAddr) -> Response {
        if self.config.max_lobbies_per_ip.is_none() || !response.status().is_success() {
            return response;
        }
        let (parts, body) = response.into_parts();
        let Ok(bytes) = axum::body::to_bytes(body, 256 * 1024).await else {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "response lost",
            );
        };
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&bytes)
            && let Some(id) = v.get("game_id").and_then(|g| g.as_str())
        {
            self.created
                .lock()
                .expect("created lock")
                .entry(ip)
                .or_default()
                .push((id.to_owned(), now_unix()));
        }
        Response::from_parts(parts, Body::from(bytes))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccessRequest {
    pub password: String,
}

/// `POST /api/access`: exchange the shared password for the access cookie.
pub async fn post_access(
    State(registry): State<Arc<GameRegistry>>,
    headers: HeaderMap,
    Json(request): Json<AccessRequest>,
) -> Response {
    let public = registry.public();
    let Some(gate) = &public.gate else {
        return Json(serde_json::json!({ "ok": true, "required": false })).into_response();
    };
    if !gate.password_matches(&request.password) {
        return json_error(
            StatusCode::UNAUTHORIZED,
            "invalid_password",
            "wrong password",
        );
    }
    let forwarded_https = public.config.trust_proxy
        && headers
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("https"));
    let secure = public.config.cookie_secure.unwrap_or_else(|| {
        public
            .config
            .public_origin
            .as_deref()
            .is_some_and(|o| o.starts_with("https://"))
    }) || forwarded_https;
    let cookie = format!(
        "{COOKIE_NAME}={}; Path=/; Max-Age={COOKIE_MAX_AGE_SECS}; HttpOnly; SameSite=Lax{}",
        gate.issue(now_unix()),
        if secure { "; Secure" } else { "" }
    );
    let mut response = Json(serde_json::json!({ "ok": true, "required": true })).into_response();
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        response.headers_mut().insert(header::SET_COOKIE, v);
    }
    response
}

/// `GET /api/access`: tells the client whether a password is required and held (no secrets).
pub async fn get_access(State(registry): State<Arc<GameRegistry>>, headers: HeaderMap) -> Response {
    let public = registry.public();
    let granted = public
        .gate
        .as_ref()
        .is_none_or(|gate| has_valid_cookie(gate, &headers));
    Json(serde_json::json!({ "required": public.gate.is_some(), "granted": granted }))
        .into_response()
}
