//! The per-game strategy card set: creation values and default, validation, the legacy rule
//! (a record with no value is a PoK game and replays exactly as recorded), persistence across a
//! restart, and the set deciding which cards the draft offers.

use std::sync::Arc;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use ti4_content::ContentStore;
use ti4_model::id::PlayerId;
use ti4_server::create_app;
use ti4_server::session::{GameRegistry, GameSession};
use ti4_server::storage::FileGameStore;

async fn spawn(registry: Arc<GameRegistry>) -> String {
    let app = create_app(registry);
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let address = listener.local_addr().expect("addr");
    tokio::spawn(async move { axum::serve(listener, app).await.expect("serve") });
    format!("http://{address}")
}

async fn create(base: &str, body: Value) -> (StatusCode, Value) {
    let response = reqwest::Client::new()
        .post(format!("{base}/api/games"))
        .json(&body)
        .send()
        .await
        .expect("create");
    let status = response.status();
    let text = response.text().await.expect("body");
    (
        status,
        serde_json::from_str(&text).unwrap_or(Value::String(text)),
    )
}

async fn post(base: &str, id: &str, tail: &str, token: &str, body: Value) -> Value {
    let response = reqwest::Client::new()
        .post(format!("{base}/api/games/{id}/lobby{tail}"))
        .header("x-ti4-player-session", token)
        .json(&body)
        .send()
        .await
        .expect("post");
    assert_eq!(response.status(), StatusCode::OK, "{tail}");
    response.json().await.unwrap_or(Value::Null)
}

/// Creates a two-player game with `extra` fields, joins a guest and starts it.
async fn started(base: &str, extra: Value) -> (String, Value) {
    let mut body = json!({"player_count": 2, "nickname": "Host", "seed": 77});
    for (key, value) in extra.as_object().expect("object") {
        body[key] = value.clone();
    }
    let (status, created) = create(base, body).await;
    assert_eq!(status, StatusCode::OK, "{created}");
    let id = created["game_id"].as_str().unwrap().to_owned();
    let host = created["player_session"].as_str().unwrap().to_owned();
    let joined: Value = reqwest::Client::new()
        .post(format!("{base}/api/games/{id}/lobby/join"))
        .json(&json!({"kind": "new", "nickname": "Guest"}))
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert!(joined["player_session"].is_string(), "{joined}");
    post(base, &id, "/start", &host, Value::Null).await;
    (id, created)
}

fn card_ids(session: &GameSession) -> Vec<String> {
    session
        .current_state()
        .unclaimed_strategy_cards
        .iter()
        .map(ToString::to_string)
        .collect()
}

fn temp(tag: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("ti4_card_set_{tag}_{:016x}", rand::random::<u64>()))
}

/// Takes `picks` strategy cards in the draft by choosing the first offered card each time, then
/// waits until the game asks its next question: an accepted choice returns before the game
/// thread has advanced (the last pick ends the strategy phase), and a recovered game replays to
/// that next question.
fn draft(session: &Arc<GameSession>, picks: usize) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut made = 0;
    let mut answered = None;
    while made < picks {
        assert!(Instant::now() < deadline, "the draft stalled");
        if let Some((actor, nonce, version)) = session.open_pending_decision() {
            let option = session.current_state().unclaimed_strategy_cards[0].to_string();
            if session
                .submit_choice(&actor, &nonce, version, &option)
                .is_ok()
            {
                made += 1;
                answered = Some(nonce);
            }
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    while session
        .open_pending_decision()
        .is_none_or(|(_, nonce, _)| Some(nonce) == answered)
    {
        assert!(Instant::now() < deadline, "the game did not ask again");
        assert!(session.error().is_none(), "{:?}", session.error());
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[tokio::test]
async fn every_offered_set_can_be_created_and_the_default_is_thunders_edge() {
    let registry = Arc::new(GameRegistry::new());
    let base = spawn(registry.clone()).await;

    let (id, created) = started(&base, json!({})).await;
    assert_eq!(created["lobby"]["strategy_card_set"], "te", "default");
    let cards = card_ids(&registry.get_game(&id).unwrap());
    assert!(
        cards.contains(&"te4construction".to_owned()) && cards.contains(&"te6warfare".to_owned())
    );
    assert!(!cards.contains(&"pok6warfare".to_owned()));

    for (set, construction, warfare, diplomacy) in [
        ("te", "te4construction", "te6warfare", "pok2diplomacy"),
        ("pok", "pok4construction", "pok6warfare", "pok2diplomacy"),
        ("base_game_codex1", "base4", "pok6warfare", "pok2diplomacy"),
    ] {
        let (id, created) = started(&base, json!({"strategy_card_set": set})).await;
        assert_eq!(created["lobby"]["strategy_card_set"], set);
        let cards = card_ids(&registry.get_game(&id).unwrap());
        assert_eq!(cards.len(), 8, "{set}: {cards:?}");
        for expected in [construction, warfare, diplomacy] {
            assert!(cards.contains(&expected.to_owned()), "{set}: {cards:?}");
        }
        for other in [
            "te4construction",
            "pok4construction",
            "base4",
            "base2",
            "te6warfare",
            "pok6warfare",
        ] {
            assert_eq!(
                cards.contains(&other.to_owned()),
                [construction, warfare].contains(&other),
                "{set} must not hold {other}: {cards:?}"
            );
        }
    }
}

#[tokio::test]
async fn an_unknown_or_unoffered_set_is_a_400_and_creates_nothing() {
    let registry = Arc::new(GameRegistry::new());
    let base = spawn(registry.clone()).await;
    for bad in ["nope", "base_game", "TE", ""] {
        let (status, body) = create(
            &base,
            json!({"player_count": 2, "nickname": "Host", "strategy_card_set": bad}),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "{bad:?}: {body}");
        assert!(
            body.as_str().unwrap_or("").contains("strategy_card_set"),
            "{body}"
        );
    }
    let (status, _) = create(
        &base,
        json!({"player_count": 2, "nickname": "Host", "strategy_card_set": 5}),
    )
    .await;
    assert!(status.is_client_error());
    assert!(registry.list_games().is_empty());
}

#[tokio::test]
async fn the_chosen_set_survives_a_restart_and_the_recovered_game_still_replays() {
    for set in ["te", "pok"] {
        let path = temp(set);
        let registry = Arc::new(
            GameRegistry::new().with_store(Arc::new(FileGameStore::new(&path).expect("store"))),
        );
        let base = spawn(registry.clone()).await;
        let (id, _) = started(&base, json!({"strategy_card_set": set})).await;
        let session = registry.get_game(&id).unwrap();
        draft(&session, 2);
        let before = session.current_state();
        drop(session);

        let reopened = Arc::new(
            GameRegistry::new().with_store(Arc::new(FileGameStore::new(&path).expect("reopen"))),
        );
        reopened
            .recover_all_games()
            .expect("recover (replay hashes match)");
        let recovered = reopened.get_game(&id).expect("recovered");
        // Recovery starts the worker; it reaches the next question on its own thread.
        recovered
            .wait_replayed()
            .expect("the recovered game replays");
        let after = recovered.current_state();
        assert_eq!(
            after.unclaimed_strategy_cards,
            before.unclaimed_strategy_cards
        );
        assert_eq!(
            after
                .players
                .iter()
                .map(|p| p.strategy_cards.clone())
                .collect::<Vec<_>>(),
            before
                .players
                .iter()
                .map(|p| p.strategy_cards.clone())
                .collect::<Vec<_>>()
        );
        let base2 = spawn(reopened).await;
        let lobby: Value = reqwest::get(format!("{base2}/api/games/{id}/lobby"))
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(lobby["strategy_card_set"], set);
        let all = [before.unclaimed_strategy_cards.clone()]
            .into_iter()
            .chain(before.players.iter().map(|p| p.strategy_cards.clone()))
            .flatten()
            .map(|c| c.to_string())
            .collect::<Vec<_>>();
        assert_eq!(all.iter().any(|c| c == "te6warfare"), set == "te");
        assert_eq!(all.iter().any(|c| c == "pok6warfare"), set == "pok");
        let _ = std::fs::remove_dir_all(path);
    }
}

/// Rewrites a stored envelope as a pre-option build wrote it: `field` removed from the payload
/// and the checksum recomputed over the payload as that build serialized it (a typed round
/// trip, so key order and omitted fields match the real writer).
fn strip_field<T>(path: &std::path::Path, field: &str) -> bool
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    use sha2::{Digest, Sha256};
    #[derive(serde::Serialize)]
    struct ChecksumInput<'a, P> {
        format_version: &'a Value,
        content_identity: &'a Value,
        rules_identity: &'a Value,
        payload: &'a P,
    }
    let mut envelope: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let removed = envelope["payload"]
        .as_object_mut()
        .unwrap()
        .remove(field)
        .is_some();
    let payload: T = serde_json::from_value(envelope["payload"].clone()).unwrap();
    let bytes = serde_json::to_vec(&ChecksumInput {
        format_version: &envelope["format_version"],
        content_identity: &envelope["content_identity"],
        rules_identity: &envelope["rules_identity"],
        payload: &payload,
    })
    .unwrap();
    envelope["checksum"] = Value::String(format!("{:x}", Sha256::digest(bytes)));
    envelope["payload"] = serde_json::to_value(&payload).unwrap();
    std::fs::write(path, serde_json::to_vec(&envelope).unwrap()).unwrap();
    removed
}

#[tokio::test]
async fn a_game_saved_before_the_option_existed_is_a_pok_game_and_replays_identically() {
    let path = temp("legacy");
    let registry = Arc::new(
        GameRegistry::new().with_store(Arc::new(FileGameStore::new(&path).expect("store"))),
    );
    let base = spawn(registry.clone()).await;
    // A PoK game is exactly what every game was before the option; strip the new keys from its
    // saved records so the files are byte-compatible with a pre-option build's.
    let (id, _) = started(&base, json!({"strategy_card_set": "pok"})).await;
    let session = registry.get_game(&id).unwrap();
    draft(&session, 2);
    let before = session.current_state();
    drop(session);
    drop(registry);

    let dir = path.join(&id);
    let lobby_file = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|file| {
            std::fs::read_to_string(file).is_ok_and(|text| {
                text.contains("host_player_id") && text.contains("strategy_card_set")
            })
        })
        .expect("the lobby record carries the field");
    assert!(strip_field::<ti4_server::storage::PlayerLobbyRecord>(
        &lobby_file,
        "strategy_card_set"
    ));
    assert!(strip_field::<ti4_server::storage::PlayerGameInitRecord>(
        &dir.join("init.json"),
        "strategy_card_set"
    ));
    for entry in std::fs::read_dir(&dir).unwrap() {
        let text = std::fs::read_to_string(entry.unwrap().path()).unwrap_or_default();
        assert!(
            !text.contains("strategy_card_set"),
            "a pre-option file has no such key"
        );
    }

    let reopened = Arc::new(
        GameRegistry::new().with_store(Arc::new(FileGameStore::new(&path).expect("reopen"))),
    );
    reopened
        .recover_all_games()
        .expect("legacy records recover and replay hashes match");
    let recovered = reopened.get_game(&id).expect("recovered");
    // Recovery starts the worker; it reaches the next question on its own thread.
    recovered
        .wait_replayed()
        .expect("the recovered game replays");
    let after = recovered.current_state();
    drop(recovered);
    assert_eq!(
        after, before,
        "the legacy game replays to the identical state"
    );
    let base2 = spawn(reopened).await;
    let lobby: Value = reqwest::get(format!("{base2}/api/games/{id}/lobby"))
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_eq!(lobby["strategy_card_set"], "pok", "missing means pok");
    let _ = std::fs::remove_dir_all(path);
}

#[test]
fn the_legacy_path_and_the_pok_set_build_the_identical_opening_state() {
    use ti4_host::map::{create_game_with_template, create_game_with_template_seated};
    let content = ContentStore::embedded();
    let players: Vec<PlayerId> = (1..=4).map(|i| PlayerId::new(format!("p{i}"))).collect();
    let seated = ti4_engine::seating::seat_in_scope(&players);
    for template in [None, Some("4pStandard")] {
        let Ok((legacy, legacy_galaxy)) = create_game_with_template(content, &players, 5, template)
        else {
            continue;
        };
        let (named, named_galaxy) =
            create_game_with_template_seated(content, &players, 5, template, &seated, Some("pok"))
                .unwrap();
        assert_eq!(legacy, named, "the PoK set is the pre-option game");
        assert_eq!(
            ti4_host::map::build_board_tiles(content, &legacy_galaxy),
            ti4_host::map::build_board_tiles(content, &named_galaxy)
        );
        let (te, te_galaxy) =
            create_game_with_template_seated(content, &players, 5, template, &seated, Some("te"))
                .unwrap();
        assert_ne!(legacy.unclaimed_strategy_cards, te.unclaimed_strategy_cards);
        // Only the cards differ: the board and every deck are the same game.
        assert_eq!(
            ti4_host::map::build_board_tiles(content, &legacy_galaxy),
            ti4_host::map::build_board_tiles(content, &te_galaxy)
        );
        assert_eq!(legacy.agenda_deck, te.agenda_deck);
        assert_eq!(legacy.objective_deck, te.objective_deck);
    }
    assert!(
        create_game_with_template_seated(content, &players, 5, None, &seated, Some("nope"))
            .is_err()
    );
}

#[test]
fn an_old_lobby_record_without_the_field_loads_as_pok_and_a_new_one_round_trips() {
    let record = json!({
        "schema_version": ti4_server::storage::PLAYER_RECORD_VERSION,
        "game_id": "old_lobby",
        "phase": "lobby",
        "host_player_id": "player_1",
        "slots": [{"slot_id": "slot_1", "occupant": "player_1"}],
        "players": {"player_1": {"ready": false, "session": "x".repeat(43), "nickname": "Host"}},
        "seed": 5,
        "lobby_version": 3
    });
    let mut parsed: ti4_server::storage::PlayerLobbyRecord =
        serde_json::from_value(record).expect("old record parses");
    assert_eq!(parsed.strategy_card_set, None);
    assert_eq!(parsed.public_view().strategy_card_set, "pok");
    parsed.strategy_card_set = Some("te".to_owned());
    let back: ti4_server::storage::PlayerLobbyRecord =
        serde_json::from_str(&serde_json::to_string(&parsed).unwrap()).unwrap();
    assert_eq!(back.public_view().strategy_card_set, "te");
}
