//! The host's map choice: validation, who may change it, the public view, previews that equal
//! the real game, the seed staying private, and recovery of a chosen map.

use std::sync::Arc;

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use ti4_server::create_app;
use ti4_server::session::GameRegistry;
use ti4_server::storage::FileGameStore;

const SECRET_SEED: u64 = 6_755_399_441_055_744;

async fn spawn(registry: Arc<GameRegistry>) -> String {
    let app = create_app(registry);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{address}")
}

struct Lobby {
    base: String,
    client: reqwest::Client,
    id: String,
    host: String,
    created: Value,
}

async fn create(base: &str, body: Value) -> (StatusCode, Value) {
    let response = reqwest::Client::new()
        .post(format!("{base}/api/games"))
        .json(&body)
        .send()
        .await
        .expect("create");
    (
        response.status(),
        response.json().await.unwrap_or(Value::Null),
    )
}

/// The preview is the board of the started game but for the homes: a home of the preview is the
/// place of a seat, and the game has the home system of the faction that was drawn for it there.
/// A faction may bring a tile that is not on the main map (the Ghosts of Creuss).
fn assert_preview_is_the_board(preview: &Value, tiles: &Value) {
    let game = tiles.as_array().expect("the tiles of the game");
    let on_map = |tile: &&Value| tile.get("special_area").is_none();
    let at = |tile: &Value| (tile["q"].as_i64().unwrap(), tile["r"].as_i64().unwrap());
    let pictured = preview["tiles"]
        .as_array()
        .expect("the tiles of the preview");
    assert_eq!(
        pictured.iter().filter(on_map).count(),
        game.iter().filter(on_map).count(),
        "the main map has the same number of tiles"
    );
    for tile in pictured {
        if tile["system_id"].as_str().unwrap().starts_with("seat_") {
            let home = game
                .iter()
                .filter(on_map)
                .find(|other| at(other) == at(tile))
                .unwrap_or_else(|| panic!("the game has no tile at the place {tile}"));
            assert_ne!(home["system_id"], tile["system_id"]);
        } else {
            assert!(game.contains(tile), "the game has another tile for {tile}");
        }
    }
}

async fn lobby(base: &str, players: usize) -> Lobby {
    let (status, created) = create(
        base,
        json!({"player_count": players, "nickname": "Host", "seed": SECRET_SEED}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{created}");
    Lobby {
        base: base.to_owned(),
        client: reqwest::Client::new(),
        id: created["game_id"].as_str().unwrap().to_owned(),
        host: created["player_session"].as_str().unwrap().to_owned(),
        created,
    }
}

impl Lobby {
    fn url(&self, tail: &str) -> String {
        format!("{}/api/games/{}/lobby{tail}", self.base, self.id)
    }

    async fn choose(&self, token: &str, map: Value) -> (StatusCode, Value) {
        let response = self
            .client
            .post(self.url("/map"))
            .header("x-ti4-player-session", token)
            .json(&json!({ "map": map }))
            .send()
            .await
            .expect("choose");
        (
            response.status(),
            response.json().await.unwrap_or(Value::Null),
        )
    }

    async fn get(&self, tail: &str) -> Value {
        self.client
            .get(self.url(tail))
            .send()
            .await
            .expect("get")
            .json()
            .await
            .expect("json")
    }

    async fn join(&self) -> String {
        let joined: Value = self
            .client
            .post(self.url("/join"))
            .json(&json!({"kind": "new", "nickname": "Guest"}))
            .send()
            .await
            .expect("join")
            .json()
            .await
            .expect("join json");
        joined["player_session"].as_str().unwrap().to_owned()
    }

    async fn post_ok(&self, tail: &str, token: &str, body: Value) -> Value {
        let response = self
            .client
            .post(self.url(tail))
            .header("x-ti4-player-session", token)
            .json(&body)
            .send()
            .await
            .expect("post");
        assert_eq!(response.status(), StatusCode::OK, "{tail}");
        response.json().await.unwrap_or(Value::Null)
    }
}

#[tokio::test]
async fn a_lobby_nobody_chose_for_gets_the_default_template_and_a_random_choice_is_explicit() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 6).await;
    assert_eq!(made.created["lobby"]["map"]["kind"], "template");
    assert_eq!(made.created["lobby"]["map"]["alias"], "6pStandard");
    assert_eq!(made.created["lobby"]["map"]["recommended"], true);
    assert_eq!(made.created["lobby"]["map_revision"], 0);

    let (status, random) = create(
        &base,
        json!({"player_count": 6, "nickname": "Host", "map": "random"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(random["lobby"]["map"]["kind"], "random");
    let (status, _) = create(
        &base,
        json!({"player_count": 6, "nickname": "Host", "map": "random", "map_template": "6pStandard"}),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_host_can_choose_a_template_or_random_and_the_public_view_shows_it() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 6).await;
    let (status, body) = made
        .choose(
            &made.host,
            json!({"kind": "template", "alias": "6pBeMyNeighbor"}),
        )
        .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["map"]["alias"], "6pBeMyNeighbor");
    assert_eq!(body["map_revision"], 1);
    assert!(
        body["lobby_version"].as_u64().unwrap()
            > made.created["lobby"]["lobby_version"].as_u64().unwrap()
    );
    assert!(body["map"]["systems"].as_u64().unwrap() > 20);

    // Anyone without a credential sees the same public choice.
    let seen = made.get("").await;
    assert_eq!(seen["map"], body["map"]);

    let (status, body) = made.choose(&made.host, json!({"kind": "random"})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["map"]["kind"], "random");
    assert!(body["map"].get("alias").is_none());
    assert_eq!(body["map_revision"], 2);
}

#[tokio::test]
async fn invalid_choices_are_rejected_without_changing_the_lobby() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 4).await;
    let before = made.get("").await;
    for map in [
        json!({"kind": "template", "alias": "nope"}),
        // another player count
        json!({"kind": "template", "alias": "6pStandard"}),
        // a template the data cannot build
        json!({"kind": "template", "alias": "4pStaticEq"}),
    ] {
        let (status, body) = made.choose(&made.host, map.clone()).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{map} {body}");
    }
    assert_eq!(made.get("").await, before);
}

#[tokio::test]
async fn only_the_host_may_choose_and_only_before_start() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 2).await;
    let guest = made.join().await;
    let (status, _) = made.choose(&guest, json!({"kind": "random"})).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = made
        .choose("not-a-session", json!({"kind": "random"}))
        .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    made.post_ok("/start", &made.host, Value::Null).await;
    let (status, _) = made.choose(&made.host, json!({"kind": "random"})).await;
    assert_eq!(status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn the_maps_list_offers_only_what_builds_for_the_player_count() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let client = reqwest::Client::new();
    let list = |query: &str| {
        let url = format!("{base}/api/maps{query}");
        let client = client.clone();
        async move {
            client
                .get(url)
                .send()
                .await
                .unwrap()
                .json::<Vec<Value>>()
                .await
                .unwrap()
        }
    };
    let four = list("?player_count=4").await;
    assert!(!four.is_empty());
    for entry in &four {
        assert_eq!(entry["player_count"], 4);
        assert_eq!(entry["buildable"], true);
        assert!(entry["systems"].as_u64().unwrap() > 0);
    }
    assert!(four.iter().all(|e| e["alias"] != "4pStaticEq"));
    assert_eq!(four.iter().filter(|e| e["recommended"] == true).count(), 1);

    // Everything the picker offers really starts a lobby.
    for entry in &four {
        let (status, _) = create(
            &base,
            json!({"player_count": 4, "nickname": "H", "map_template": entry["alias"]}),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{entry}");
    }

    // The unfiltered list keeps the old shape and marks what does not build.
    let all = list("").await;
    let broken = all
        .iter()
        .find(|e| e["alias"] == "4pStaticEq")
        .expect("listed");
    assert_eq!(broken["buildable"], false);
    assert!(
        all.iter()
            .all(|e| e["alias"].is_string() && e["author"].is_string())
    );
    for n in 2..=8 {
        for entry in list(&format!("?player_count={n}")).await {
            assert_eq!(entry["player_count"], n);
        }
    }
}

/// The picture of the lobby's current choice is the board the started game has.
#[tokio::test]
async fn the_lobby_preview_equals_the_started_games_board() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    for map in [
        json!({"kind": "template", "alias": "4pHyperlanes"}),
        json!({"kind": "random"}),
    ] {
        let made = lobby(&base, 4).await;
        let (status, _) = made.choose(&made.host, map.clone()).await;
        assert_eq!(status, StatusCode::OK);
        let preview = made.get("/map-preview").await;
        // A seat of the preview is a place: who sits there, and with which faction, is drawn at
        // the start, and nothing of it is told before.
        let seats = preview["seats"].as_array().unwrap();
        assert_eq!(seats.len(), 4);
        for seat in seats {
            assert!(seat.get("faction").is_none(), "{seat}");
            assert!(seat.get("faction_name").is_none(), "{seat}");
            let place = seat["home_system_id"].as_str().unwrap();
            let tile = preview["tiles"]
                .as_array()
                .unwrap()
                .iter()
                .find(|tile| tile["system_id"] == place)
                .unwrap_or_else(|| panic!("no tile for {place}"));
            assert!(tile.get("planets").is_none(), "{tile}");
        }

        let mut tokens = vec![made.host.clone()];
        for _ in 0..3 {
            tokens.push(made.join().await);
        }
        // The picture does not change when people join.
        assert_eq!(made.get("/map-preview").await, preview);
        made.post_ok("/start", &made.host, Value::Null).await;
        let tiles: Value = made
            .client
            .get(format!("{}/api/games/{}/map", made.base, made.id))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_preview_is_the_board(&preview, &tiles);
        assert_eq!(made.get("/map-preview").await, preview);
    }
}

#[tokio::test]
async fn choosing_again_rerolls_the_open_slots_but_not_the_fixed_ones() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 6).await;
    let template = json!({"kind": "template", "alias": "6pStandard"});
    made.choose(&made.host, template.clone()).await;
    let a = made.get("/map-preview").await;
    let mut differs = false;
    for _ in 0..4 {
        made.choose(&made.host, template.clone()).await;
        let b = made.get("/map-preview").await;
        let ids = |p: &Value| -> Vec<String> {
            p["tiles"]
                .as_array()
                .unwrap()
                .iter()
                .map(|t| t["system_id"].as_str().unwrap().to_owned())
                .collect()
        };
        differs |= ids(&a) != ids(&b);
        // Mecatol and all six homes stay.
        assert!(ids(&b).contains(&"18".to_owned()));
        for seat in b["seats"].as_array().unwrap() {
            assert!(ids(&b).contains(&seat["home_system_id"].as_str().unwrap().to_owned()));
        }
    }
    assert!(differs, "four re-rolls never changed a tile");
}

#[tokio::test]
async fn card_previews_are_stable_per_variant_and_a_wrong_alias_is_a_404() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let client = reqwest::Client::new();
    let get = |tail: &str| {
        let url = format!("{base}/api/maps/{tail}");
        let client = client.clone();
        async move {
            let r = client.get(url).send().await.unwrap();
            (r.status(), r.json::<Value>().await.unwrap_or(Value::Null))
        }
    };
    let (s, a) = get("6pStandard/preview?variant=0").await;
    assert_eq!(s, StatusCode::OK);
    let (_, again) = get("6pStandard/preview?variant=0").await;
    assert_eq!(a, again);
    let (_, other) = get("6pStandard/preview?variant=1").await;
    assert_ne!(a["tiles"], other["tiles"]);
    assert_eq!(get("nope/preview").await.0, StatusCode::NOT_FOUND);
    assert_eq!(get("random/preview").await.0, StatusCode::BAD_REQUEST);
    let (s, random) = get("random/preview?player_count=3").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(random["seats"].as_array().unwrap().len(), 3);
    assert_eq!(get("4pStaticEq/preview").await.0, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn every_startable_player_count_has_a_working_random_board() {
    // Seats seven and eight have factions of their own (`seating::EXTRA_SEAT_FACTIONS`), so
    // every table size the lobby allows has distinct homes.
    for n in 2..=8 {
        ti4_server::maps::catalog::validate(&ti4_server::maps::MapChoice::Random, n)
            .unwrap_or_else(|e| panic!("random for {n}: {e}"));
    }
}

#[tokio::test]
async fn the_seed_is_in_no_response() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 4).await;
    let guest = made.join().await;
    let mut texts = vec![made.created.to_string()];
    let (_, chosen) = made
        .choose(
            &made.host,
            json!({"kind": "template", "alias": "4pHyperlanes"}),
        )
        .await;
    texts.push(chosen.to_string());
    texts.push(made.get("").await.to_string());
    texts.push(made.get("/map-preview").await.to_string());
    let guest_view: Value = made
        .client
        .get(made.url(""))
        .header("x-ti4-player-session", &guest)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    texts.push(guest_view.to_string());
    for tail in [
        "4pHyperlanes/preview?variant=2",
        "random/preview?player_count=4",
    ] {
        texts.push(
            reqwest::get(format!("{base}/api/maps/{tail}"))
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
        );
    }
    texts.push(
        reqwest::get(format!("{base}/api/maps?player_count=4"))
            .await
            .unwrap()
            .text()
            .await
            .unwrap(),
    );
    for text in texts {
        assert!(!text.contains("seed"), "{text}");
        assert!(!text.contains(&SECRET_SEED.to_string()), "{text}");
        assert!(!text.contains(&format!("{SECRET_SEED:x}")), "{text}");
    }
}

#[tokio::test]
async fn a_chosen_map_survives_a_restart_and_the_started_game_matches_the_preview() {
    let path = std::env::temp_dir().join(format!("ti4_map_choice_{:016x}", rand::random::<u64>()));
    let store = Arc::new(FileGameStore::new(&path).expect("store"));
    let registry = Arc::new(GameRegistry::new().with_store(store));
    let base = spawn(registry).await;
    let made = lobby(&base, 3).await;
    let alias = "3pHyperlanes";
    let (status, chosen) = made
        .choose(&made.host, json!({"kind": "template", "alias": alias}))
        .await;
    assert_eq!(status, StatusCode::OK, "{chosen}");
    let preview = made.get("/map-preview").await;
    let view = made.get("").await;

    let reopened = Arc::new(
        GameRegistry::new().with_store(Arc::new(FileGameStore::new(&path).expect("reopen"))),
    );
    reopened.recover_all_games().expect("recover");
    let base2 = spawn(reopened).await;
    let after = Lobby {
        base: base2,
        client: reqwest::Client::new(),
        id: made.id.clone(),
        host: made.host.clone(),
        created: Value::Null,
    };
    let seen = after.get("").await;
    assert_eq!(seen["map"]["alias"], alias);
    assert_eq!(seen["map_revision"], view["map_revision"]);
    assert_eq!(after.get("/map-preview").await, preview);

    let mut tokens = vec![after.host.clone()];
    for _ in 0..2 {
        tokens.push(after.join().await);
    }
    after.post_ok("/start", &after.host, Value::Null).await;
    let tiles: Value = reqwest::get(format!("{}/api/games/{}/map", after.base, after.id))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_preview_is_the_board(&preview, &tiles);
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn an_old_lobby_record_without_the_new_fields_loads() {
    let record = json!({
        "schema_version": ti4_server::storage::PLAYER_RECORD_VERSION,
        "game_id": "old_lobby",
        "phase": "lobby",
        "host_player_id": "player_1",
        "slots": [{"slot_id": "slot_1", "occupant": "player_1"}, {"slot_id": "slot_2", "occupant": null}],
        "players": {"player_1": {"ready": false, "session": "x".repeat(43), "nickname": "Host"}},
        "seed": 5,
        "lobby_version": 3
    });
    let parsed: Result<ti4_server::storage::PlayerLobbyRecord, _> = serde_json::from_value(record);
    let parsed = parsed.expect("old record parses");
    assert_eq!(parsed.map_revision, 0);
    assert_eq!(parsed.public_view().map.kind, "random");
}

#[tokio::test]
async fn a_seven_and_an_eight_player_table_have_a_random_board_and_templates() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    for n in [7, 8] {
        let made = lobby(&base, n).await;
        let (status, _) = made.choose(&made.host, json!({"kind": "random"})).await;
        assert_eq!(status, StatusCode::OK, "random for {n}");
        let list: Vec<Value> = reqwest::get(format!("{base}/api/maps?player_count={n}"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert!(!list.is_empty(), "templates for {n}");
    }
}

#[tokio::test]
async fn a_dev_start_preset_rides_along_with_the_choice_and_is_validated() {
    let base = spawn(Arc::new(GameRegistry::new())).await;
    let made = lobby(&base, 3).await;
    let post = |body: Value| {
        let made = &made;
        async move {
            let response = made
                .client
                .post(made.url("/map"))
                .header("x-ti4-player-session", &made.host)
                .json(&body)
                .send()
                .await
                .unwrap();
            (response.status(), response.text().await.unwrap())
        }
    };
    let (status, _) = post(json!({"map": {"kind": "random"}, "start_preset": "nope"})).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, body) = post(json!({"map": {"kind": "random"}, "start_preset": "combat"})).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body.contains("combat"),
        "the preset is never public: {body}"
    );
    let (status, _) = post(json!({"map": {"kind": "random"}, "start_preset": ""})).await;
    assert_eq!(status, StatusCode::OK);
    // Chosen again with the preset, the table still starts.
    let (status, _) = post(json!({"map": {"kind": "random"}, "start_preset": "combat"})).await;
    assert_eq!(status, StatusCode::OK);
    let mut tokens = vec![made.host.clone()];
    for _ in 0..2 {
        tokens.push(made.join().await);
    }
    made.post_ok("/start", &made.host, Value::Null).await;
}
