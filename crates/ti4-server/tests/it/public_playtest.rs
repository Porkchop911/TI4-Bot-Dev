//! Public-playtest mode: dev routes, CORS, access gate, rate limits, caps, feedback, SPA serving.

use std::path::PathBuf;
use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;

use ti4_server::create_app;
use ti4_server::public::PublicConfig;
use ti4_server::public::ratelimit::{RateLimits, RateSpec};
use ti4_server::session::GameRegistry;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ti4_public_{tag}_{:016x}", rand::random::<u64>()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn public_config(data_dir: &std::path::Path) -> PublicConfig {
    PublicConfig {
        public: true,
        data_dir: Some(data_dir.to_path_buf()),
        build_commit: Some("testcommit".into()),
        ..PublicConfig::default()
    }
}

async fn spawn(config: PublicConfig) -> String {
    let registry = Arc::new(GameRegistry::new().with_public(config));
    let app = create_app(registry);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(
            listener,
            app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
        )
        .await
        .unwrap()
    });
    format!("127.0.0.1:{}", addr.port())
}

fn create_body() -> Value {
    json!({ "player_count": 2, "seed": 1, "nickname": "Host" })
}

#[tokio::test]
async fn dev_routes_exist_locally_and_are_absent_in_public_mode() {
    let local = spawn(PublicConfig::default()).await;
    let c = reqwest::Client::new();
    assert_eq!(
        c.get(format!("http://{local}/api/dev/scenarios"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let dir = temp_dir("dev");
    let public = spawn(public_config(&dir)).await;
    for path in ["/api/dev/scenarios"] {
        let r = c
            .get(format!("http://{public}{path}"))
            .send()
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::NOT_FOUND);
    }
    let r = c
        .post(format!("http://{public}/api/dev/scenarios/launch"))
        .json(&json!({ "scenario_id": "x", "seed": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    let health: Value = c
        .get(format!("http://{public}/health"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(health["commit"], "testcommit");
    assert_eq!(health["public"], true);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn cors_is_open_locally_same_origin_only_in_public_and_pinned_with_an_origin() {
    let c = reqwest::Client::new();
    let acao = |r: &reqwest::Response| {
        r.headers()
            .get("access-control-allow-origin")
            .map(|v| v.to_str().unwrap().to_owned())
    };
    let local = spawn(PublicConfig::default()).await;
    let r = c
        .get(format!("http://{local}/health"))
        .header("origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(acao(&r).as_deref(), Some("*"));

    let dir = temp_dir("cors");
    let same = spawn(public_config(&dir)).await;
    let r = c
        .get(format!("http://{same}/health"))
        .header("origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    assert_eq!(acao(&r), None);

    let mut cfg = public_config(&dir);
    cfg.public_origin = Some("https://play.example.org/".into());
    let pinned = spawn(cfg).await;
    let ok = c
        .get(format!("http://{pinned}/health"))
        .header("origin", "https://play.example.org")
        .send()
        .await
        .unwrap();
    assert_eq!(acao(&ok).as_deref(), Some("https://play.example.org"));
    let bad = c
        .get(format!("http://{pinned}/health"))
        .header("origin", "https://evil.example")
        .send()
        .await
        .unwrap();
    // The pinned origin is the only one ever echoed; a browser on another origin is refused.
    assert_ne!(acao(&bad).as_deref(), Some("https://evil.example"));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn password_gate_blocks_api_and_ws_until_the_cookie_is_held() {
    let dir = temp_dir("gate");
    let mut cfg = public_config(&dir);
    cfg.access_password = Some("hunter2".into());
    cfg.cookie_secret = Some("test-secret".into());
    cfg.cookie_secure = Some(true);
    let base = spawn(cfg).await;
    let c = reqwest::Client::new();

    // Open: health, access probe.
    assert_eq!(
        c.get(format!("http://{base}/health"))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::OK
    );
    let probe: Value = c
        .get(format!("http://{base}/api/access"))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(probe, json!({ "required": true, "granted": false }));

    // Closed: everything else under /api and /ws.
    let r = c
        .get(format!("http://{base}/api/games"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    let body: Value = r.json().await.unwrap();
    assert_eq!(body["error"], "access_required");
    let r = c
        .post(format!("http://{base}/api/games"))
        .json(&create_body())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    let r = c
        .get(format!("http://{base}/api/catalog"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    let r = c
        .post(format!("http://{base}/api/feedback"))
        .json(&json!({ "category": "bug", "text": "x" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

    // Wrong password.
    let r = c
        .post(format!("http://{base}/api/access"))
        .json(&json!({ "password": "nope" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert!(r.headers().get("set-cookie").is_none());

    // Right password sets a hardened cookie.
    let r = c
        .post(format!("http://{base}/api/access"))
        .json(&json!({ "password": "hunter2" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let set_cookie = r
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    for needle in [
        "HttpOnly",
        "SameSite=Lax",
        "Secure",
        "Max-Age=2592000",
        "Path=/",
    ] {
        assert!(set_cookie.contains(needle), "{set_cookie} lacks {needle}");
    }
    let cookie = set_cookie.split(';').next().unwrap().to_owned();

    // With the cookie the API works.
    let r = c
        .post(format!("http://{base}/api/games"))
        .header("cookie", &cookie)
        .json(&create_body())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let created: Value = r.json().await.unwrap();
    let game_id = created["game_id"].as_str().unwrap().to_owned();
    // A forged cookie does not.
    let r = c
        .get(format!("http://{base}/api/games"))
        .header("cookie", "ti4_access=9999999999.00")
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

    // Start the game with two humans so the socket has something to join.
    let token = created["player_session"].as_str().unwrap();
    let r = c
        .post(format!("http://{base}/api/games/{game_id}/lobby/join"))
        .header("cookie", &cookie)
        .json(&json!({ "kind": "new", "nickname": "Guest" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let guest = r.json::<Value>().await.unwrap()["player_session"]
        .as_str()
        .unwrap()
        .to_owned();
    for (path, who, body) in [
        (
            "lobby/nickname",
            guest.as_str(),
            json!({ "nickname": "Guest 2" }),
        ),
        ("lobby/start", token, json!({})),
    ] {
        let r = c
            .post(format!("http://{base}/api/games/{game_id}/{path}"))
            .header("cookie", &cookie)
            .header("x-ti4-player-session", who)
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(
            r.status(),
            StatusCode::OK,
            "{path}: {}",
            r.text().await.unwrap()
        );
    }
    // WebSocket: refused without the cookie, accepted with it.
    let url = format!("ws://{base}/ws/games/{game_id}");
    let err = tokio_tungstenite::connect_async(&url).await.unwrap_err();
    assert!(
        matches!(
            err,
            tokio_tungstenite::tungstenite::Error::Http(ref resp) if resp.status() == 401
        ),
        "{err:?}"
    );
    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("cookie", cookie.parse().unwrap());
    let (_socket, response) = tokio_tungstenite::connect_async(request).await.unwrap();
    assert_eq!(response.status(), 101);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn password_attempts_are_rate_limited() {
    let dir = temp_dir("pwrate");
    let mut cfg = public_config(&dir);
    cfg.access_password = Some("pw".into());
    let mut limits = RateLimits::default();
    limits.password = RateSpec::new(3.0, 0.001);
    cfg.rate_limits = Some(limits);
    let base = spawn(cfg).await;
    let c = reqwest::Client::new();
    let mut statuses = Vec::new();
    for _ in 0..5 {
        statuses.push(
            c.post(format!("http://{base}/api/access"))
                .json(&json!({ "password": "bad" }))
                .send()
                .await
                .unwrap()
                .status(),
        );
    }
    assert_eq!(statuses[..3], [StatusCode::UNAUTHORIZED; 3]);
    assert_eq!(statuses[3..], [StatusCode::TOO_MANY_REQUESTS; 2]);
    // Even the right password is refused while limited.
    let r = c
        .post(format!("http://{base}/api/access"))
        .json(&json!({ "password": "pw" }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::TOO_MANY_REQUESTS);
    assert!(r.headers().get("retry-after").is_some());
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn rate_limits_are_per_ip_and_trust_the_proxy_header_only_when_asked() {
    let dir = temp_dir("rate");
    let limits = |trust: bool| {
        let mut cfg = public_config(&dir);
        let mut l = RateLimits::default();
        l.game_create = RateSpec::new(2.0, 0.0001);
        cfg.rate_limits = Some(l);
        cfg.max_lobbies_per_ip = None;
        cfg.trust_proxy = trust;
        cfg
    };
    let c = reqwest::Client::new();
    let create = |base: String, ip: &'static str| {
        let c = c.clone();
        async move {
            c.post(format!("http://{base}/api/games"))
                .header("x-forwarded-for", ip)
                .json(&create_body())
                .send()
                .await
                .unwrap()
                .status()
        }
    };
    // Not trusting the proxy: spoofed X-Forwarded-For does not buy a fresh bucket.
    let base = spawn(limits(false)).await;
    assert_eq!(create(base.clone(), "1.1.1.1").await, StatusCode::OK);
    assert_eq!(create(base.clone(), "2.2.2.2").await, StatusCode::OK);
    assert_eq!(
        create(base.clone(), "3.3.3.3").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    // Trusting it (peer is loopback): each forwarded address has its own bucket.
    let base = spawn(limits(true)).await;
    assert_eq!(create(base.clone(), "1.1.1.1").await, StatusCode::OK);
    assert_eq!(create(base.clone(), "1.1.1.1").await, StatusCode::OK);
    assert_eq!(
        create(base.clone(), "1.1.1.1").await,
        StatusCode::TOO_MANY_REQUESTS
    );
    assert_eq!(create(base.clone(), "9.9.9.9").await, StatusCode::OK);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn game_lobby_and_disk_caps_refuse_creation() {
    let c = reqwest::Client::new();
    let dir = temp_dir("caps");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    cfg.max_games = Some(2);
    cfg.max_lobbies_per_ip = Some(1);
    let base = spawn(cfg).await;
    let post = || {
        c.post(format!("http://{base}/api/games"))
            .json(&create_body())
            .send()
    };
    assert_eq!(post().await.unwrap().status(), StatusCode::OK);
    let second = post().await.unwrap();
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        second.json::<Value>().await.unwrap()["error"],
        "too_many_games"
    );

    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    cfg.max_games = Some(1);
    cfg.max_lobbies_per_ip = None;
    let base = spawn(cfg).await;
    let post = || {
        c.post(format!("http://{base}/api/games"))
            .json(&create_body())
            .send()
    };
    assert_eq!(post().await.unwrap().status(), StatusCode::OK);
    let full = post().await.unwrap();
    assert_eq!(full.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(full.json::<Value>().await.unwrap()["error"], "server_full");

    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    cfg.max_games = None;
    cfg.max_lobbies_per_ip = None;
    cfg.max_data_bytes = Some(10);
    std::fs::write(dir.join("filler.bin"), [0u8; 64]).unwrap();
    let base = spawn(cfg).await;
    let r = c
        .post(format!("http://{base}/api/games"))
        .json(&create_body())
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::INSUFFICIENT_STORAGE);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn public_mode_refuses_presets_and_keeps_the_small_body_cap() {
    let c = reqwest::Client::new();
    let dir = temp_dir("bots");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    let base = spawn(cfg).await;
    let created: Value = c
        .post(format!("http://{base}/api/games"))
        .json(&create_body())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let id = created["game_id"].as_str().unwrap();
    let token = created["player_session"].as_str().unwrap();
    // The in-process rule bot works.
    let r = c
        .post(format!("http://{base}/api/games/{id}/lobby/bots"))
        .header("x-ti4-player-session", token)
        .json(&json!({ "kind": "smart", "count": 1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let r = c
        .post(format!("http://{base}/api/games"))
        .json(&json!({ "player_count": 2, "nickname": "A", "start_preset": ti4_host::preset::COMBAT }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let big = json!({ "player_count": 2, "nickname": "x".repeat(9000) });
    let r = c
        .post(format!("http://{base}/api/games"))
        .json(&big)
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn feedback_is_validated_stored_as_a_file_and_has_its_own_size_limit() {
    let c = reqwest::Client::new();
    let dir = temp_dir("fb");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = Some({
        let mut l = RateLimits::default();
        l.feedback = RateSpec::new(30.0, 0.0001);
        l
    });
    let base = spawn(cfg).await;
    let url = format!("http://{base}/api/feedback");
    let send = |v: Value| c.post(&url).json(&v).send();

    let ok = send(json!({ "category": "bug", "text": "The map is blank", "context": { "build_commit": "abc", "game_id": "g1" } })).await.unwrap();
    assert_eq!(ok.status(), StatusCode::OK);
    let id = ok.json::<Value>().await.unwrap()["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let reports = ti4_server::feedback::list_recent(&dir, 10);
    assert_eq!(reports.len(), 1);
    let (path, record) = &reports[0];
    assert_eq!(record.id, id);
    assert_eq!(record.text, "The map is blank");
    assert_eq!(record.server_commit, "testcommit");
    assert!(path.starts_with(dir.join("feedback")));
    assert!(path.to_string_lossy().ends_with(&format!("{id}.json")));
    assert!(ti4_server::feedback::render(record, true).contains("The map is blank"));

    // Without game info the context is simply absent.
    let r = send(json!({ "category": "idea", "text": "More bots" }))
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);

    for bad in [
        json!({ "category": "nonsense", "text": "x" }),
        json!({ "category": "bug", "text": "   " }),
        json!({ "category": "bug", "text": "x".repeat(4001) }),
        json!({ "category": "bug", "text": "x", "context": [1, 2] }),
        json!({ "category": "bug", "text": "x", "extra": 1 }),
    ] {
        let r = send(bad.clone()).await.unwrap();
        assert!(
            r.status() == StatusCode::BAD_REQUEST || r.status() == StatusCode::UNPROCESSABLE_ENTITY,
            "{bad} -> {}",
            r.status()
        );
    }
    assert_eq!(ti4_server::feedback::list_recent(&dir, 10).len(), 2);
    let _ = std::fs::remove_dir_all(dir);

    // Body limit: far above 8 KiB passes, above 3 MiB is refused; separate dir and bucket.
    let dir = temp_dir("fb2");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    let base = spawn(cfg).await;
    let url = format!("http://{base}/api/feedback");
    let ctx = json!({ "events": "e".repeat(100_000) });
    let r = c
        .post(&url)
        .json(&json!({ "category": "other", "text": "big", "context": ctx }))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let huge = json!({ "category": "other", "text": "big", "context": { "events": "e".repeat(4_000_000) } });
    let r = c.post(&url).json(&huge).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::PAYLOAD_TOO_LARGE);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn feedback_rate_limit_triggers_with_429() {
    let c = reqwest::Client::new();
    let dir = temp_dir("fbrate");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = Some({
        let mut l = RateLimits::default();
        l.feedback = RateSpec::new(2.0, 0.0001);
        l
    });
    let base = spawn(cfg).await;
    let mut seen = Vec::new();
    for _ in 0..3 {
        seen.push(
            c.post(format!("http://{base}/api/feedback"))
                .json(&json!({ "category": "bug", "text": "x" }))
                .send()
                .await
                .unwrap()
                .status(),
        );
    }
    assert_eq!(
        seen,
        [
            StatusCode::OK,
            StatusCode::OK,
            StatusCode::TOO_MANY_REQUESTS
        ]
    );
    assert_eq!(ti4_server::feedback::list_recent(&dir, 10).len(), 2);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn offline_feedback_is_stored_once_per_report_id_and_extractable() {
    let c = reqwest::Client::new();
    let dir = temp_dir("fboffline");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    let base = spawn(cfg).await;
    let url = format!("http://{base}/api/feedback");
    let ids: Vec<String> = (0..2000).map(|i| format!("option_{i}")).collect();
    let times: Vec<String> = (0..20_000).map(|_| "10:00:00.000Z".to_owned()).collect();
    let decisions: Vec<String> = (0..55).map(|i| format!("option_{i}")).collect();
    let local = json!({
        "seed": 9, "answers": ids,
        "engine": { "build": "b1", "content_digest": "cd" }, "wasm_sha256": "aa",
        "save": { "format": 2, "seed": 9, "game": { "setup": { "seed": 9, "players": 3 } },
            "decisions": decisions, "answers": ids, "redo": [], "never": [],
            "event_times": times, "event_versions": [], "generation": 0, "version": 3,
            "decisions_hash": "0123456789abcdef" }
    });
    let body = json!({ "category": "bug", "text": "offline problem", "report_id": "client-rep-0001",
        "context": { "sender": { "nickname": "Ada" }, "local": local } });
    let first: Value = c
        .post(&url)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(first["duplicate"], false);
    // The client did not see the answer and sends again.
    let second: Value = c
        .post(&url)
        .json(&body)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(second["duplicate"], true);
    assert_eq!(second["id"], first["id"]);
    let reports = ti4_server::feedback::list_recent(&dir, 10);
    assert_eq!(reports.len(), 1);
    let blob = ti4_server::feedback::save_blob(&reports[0].1).unwrap();
    assert_eq!(blob["decisions"].as_array().unwrap().len(), 55);
    assert_eq!(blob["answers"].as_array().unwrap().len(), 2000);
    assert!(
        ti4_server::feedback::render(&reports[0].1, false).contains("source: offline | seed 9")
    );
    // A malformed local is refused.
    let mut bad = body.clone();
    bad["report_id"] = json!("client-rep-0002");
    bad["context"]["local"]["save"]["seed"] = json!(10);
    let r = c.post(&url).json(&bad).send().await.unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(ti4_server::feedback::list_recent(&dir, 10).len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stored_feedback_never_contains_seat_tokens() {
    let c = reqwest::Client::new();
    let dir = temp_dir("fbtoken");
    let mut cfg = public_config(&dir);
    cfg.rate_limits = None;
    let base = spawn(cfg).await;
    let created: Value = c
        .post(format!("http://{base}/api/games"))
        .json(&create_body())
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    let token = created["player_session"].as_str().unwrap().to_owned();
    // A client sends its viewer-projected context; the server adds only the build commit.
    c.post(format!("http://{base}/api/feedback"))
        .header("x-ti4-player-session", &token)
        .json(&json!({ "category": "bug", "text": "t", "context": { "game_id": created["game_id"] } }))
        .send()
        .await
        .unwrap();
    let reports = ti4_server::feedback::list_recent(&dir, 5);
    let file = std::fs::read_to_string(&reports[0].0).unwrap();
    assert!(!file.contains(&token));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn web_dir_is_served_with_spa_fallback_and_cache_headers() {
    let c = reqwest::Client::new();
    let dir = temp_dir("web");
    let web = dir.join("dist");
    std::fs::create_dir_all(web.join("assets")).unwrap();
    std::fs::write(web.join("index.html"), "<html>INDEX</html>").unwrap();
    std::fs::write(web.join("assets/app-abc123.js"), "console.log(1)").unwrap();
    let mut cfg = public_config(&dir);
    cfg.web_dir = Some(web);
    cfg.access_password = Some("pw".into());
    cfg.rate_limits = None;
    let base = spawn(cfg).await;

    let root = c.get(format!("http://{base}/")).send().await.unwrap();
    assert_eq!(root.status(), StatusCode::OK);
    assert_eq!(root.headers()["cache-control"], "no-cache");
    assert!(root.text().await.unwrap().contains("INDEX"));

    // Static files are reachable without the password.
    let route = c
        .get(format!("http://{base}/game/some-id/lobby"))
        .send()
        .await
        .unwrap();
    assert_eq!(route.status(), StatusCode::OK);
    assert!(route.text().await.unwrap().contains("INDEX"));

    let asset = c
        .get(format!("http://{base}/assets/app-abc123.js"))
        .send()
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers()["cache-control"],
        "public, max-age=31536000, immutable"
    );

    // API paths never fall back to index.html.
    let c2 = reqwest::Client::new();
    let api = c2
        .get(format!("http://{base}/api/unknown"))
        .send()
        .await
        .unwrap();
    assert_eq!(api.status(), StatusCode::UNAUTHORIZED);
    assert!(!api.text().await.unwrap().contains("INDEX"));
    let _ = std::fs::remove_dir_all(dir);
}

#[tokio::test]
async fn api_404_is_not_the_spa_page_once_authorised() {
    let c = reqwest::Client::new();
    let dir = temp_dir("web404");
    let web = dir.join("dist");
    std::fs::create_dir_all(&web).unwrap();
    std::fs::write(web.join("index.html"), "<html>INDEX</html>").unwrap();
    let mut cfg = public_config(&dir);
    cfg.web_dir = Some(web);
    cfg.rate_limits = None;
    let base = spawn(cfg).await;
    let r = c
        .get(format!("http://{base}/api/unknown"))
        .send()
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::NOT_FOUND);
    assert!(!r.text().await.unwrap().contains("INDEX"));
    let _ = std::fs::remove_dir_all(dir);
}
