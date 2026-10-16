//! A server and the calls of a table over HTTP, for the tests of its settings and its start.
#![allow(dead_code)]

use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::{Value, json};
use ti4_server::create_app;
use ti4_server::session::GameRegistry;

pub struct Server {
    addr: std::net::SocketAddr,
    pub registry: Arc<GameRegistry>,
    pub client: reqwest::Client,
    handle: tokio::task::JoinHandle<()>,
}

impl Drop for Server {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

pub async fn serve(registry: Arc<GameRegistry>) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = create_app(registry.clone());
    let handle = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    Server {
        addr,
        registry,
        client: reqwest::Client::new(),
        handle,
    }
}

/// A table and the credential of its host.
pub struct Made {
    pub game: String,
    pub host: String,
    pub host_id: String,
    pub lobby: Value,
}

impl Server {
    pub fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.addr)
    }

    /// Makes a table. `extra` is merged into the body of `POST /api/games`.
    pub async fn table(&self, players: usize, extra: Value) -> Made {
        let mut body = json!({"player_count": players, "seed": 42, "nickname": "Ada"});
        for (key, value) in extra.as_object().expect("an object") {
            body[key] = value.clone();
        }
        let response = self
            .client
            .post(self.url("/api/games"))
            .json(&body)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let created: Value = response.json().await.unwrap();
        Made {
            game: created["game_id"].as_str().unwrap().to_owned(),
            host: created["player_session"].as_str().unwrap().to_owned(),
            host_id: created["player"]["id"].as_str().unwrap().to_owned(),
            lobby: created["lobby"].clone(),
        }
    }

    pub async fn post(
        &self,
        game: &str,
        route: &str,
        token: Option<&str>,
        body: Value,
    ) -> (StatusCode, Value) {
        let mut request = self
            .client
            .post(self.url(&format!("/api/games/{game}/lobby/{route}")))
            .json(&body);
        if let Some(token) = token {
            request = request.header("x-ti4-player-session", token);
        }
        let response = request.send().await.unwrap();
        let status = response.status();
        let text = response.text().await.unwrap();
        let value = serde_json::from_str(&text).unwrap_or(Value::String(text));
        (status, value)
    }

    /// A call that the table takes: the lobby view that it answers.
    pub async fn ok(&self, game: &str, route: &str, token: &str, body: Value) -> Value {
        let (status, view) = self.post(game, route, Some(token), body).await;
        assert_eq!(status, StatusCode::OK, "{route}: {view}");
        view
    }

    pub async fn get(&self, path: &str) -> Value {
        self.client
            .get(self.url(path))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap()
    }

    pub async fn view(&self, game: &str) -> Value {
        self.get(&format!("/api/games/{game}/lobby")).await
    }

    /// A person joins: (player id, credential).
    pub async fn join(&self, game: &str, nickname: &str) -> (String, String) {
        let (status, joined) = self
            .post(
                game,
                "join",
                None,
                json!({"kind": "new", "nickname": nickname}),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{joined}");
        (
            joined["player"]["id"].as_str().unwrap().to_owned(),
            joined["player_session"].as_str().unwrap().to_owned(),
        )
    }
}

pub fn open_seats(view: &Value) -> usize {
    view["slots"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|slot| slot["occupant"].is_null())
        .count()
}

/// The nicknames of the seats, in their order; an open seat is "".
pub fn names(view: &Value) -> Vec<String> {
    view["slots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|slot| slot["nickname"].as_str().unwrap_or("").to_owned())
        .collect()
}
