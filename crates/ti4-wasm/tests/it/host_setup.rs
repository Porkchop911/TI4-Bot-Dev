//! The setup form's side of the hosted game: what is offered, and what is refused.

use serde_json::{Value, json};
use ti4_wasm::host_api;

fn query(request: &Value) -> Result<Value, String> {
    let len = host_api::put_input(&request.to_string());
    let status = host_api::query(len);
    let out = host_api::out_text();
    if status < 0 {
        Err(out)
    } else {
        Ok(serde_json::from_str(&out).expect("a query answers JSON"))
    }
}

fn start_error(setup: &Value) -> String {
    let request = json!({ "setup": setup });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    assert!(status < 0, "the table should be refused");
    host_api::out_text()
}

#[test]
fn the_form_is_offered_cards_factions_maps_and_bots_of_the_table_setup() {
    let _serial = crate::common::serial();
    let model = ti4_host::bots::test_model();
    let form = query(&json!({ "query": "describe" })).unwrap();
    // The options of `ti4_host::setup`, as the server and web2 have them.
    assert_eq!(form["min_players"], *ti4_host::setup::PLAYER_COUNTS.start());
    assert_eq!(form["max_players"], *ti4_host::setup::PLAYER_COUNTS.end());
    let sets = form["strategy_card_sets"].as_array().unwrap();
    assert_eq!(sets.len(), 3);
    assert_eq!(sets.iter().filter(|set| set["default"] == true).count(), 1);
    assert!(
        sets.iter()
            .all(|set| set["cards"].as_array().unwrap().len() == 8)
    );
    let groups = form["faction_groups"].as_array().unwrap();
    assert_eq!(groups.len(), 2);
    assert!(
        groups
            .iter()
            .all(|g| !g["factions"].as_array().unwrap().is_empty())
    );
    // The kinds of the models that the engine loaded follow the rule bot and the random seat.
    let kinds: Vec<&str> = form["bot_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|kind| kind["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            ti4_host::setup::bot_kind::RULE,
            ti4_host::setup::bot_kind::RANDOM,
            &model.kind_id(),
        ]
    );
    for count in 3..=6 {
        let maps = form["maps"][count.to_string()].as_array().unwrap();
        assert!(!maps.is_empty(), "maps for {count}");
        assert!(
            maps.iter().any(|map| map["recommended"] == true),
            "a recommended map for {count}"
        );
    }
    assert!(form["maps"].get("8").is_some());
    assert!(form["presets"].as_array().unwrap().len() > 5);
    assert!(form["content_digest"].as_str().unwrap().len() > 10);
}

#[test]
fn a_map_is_pictured_for_the_form_with_a_place_for_each_seat() {
    let _serial = crate::common::serial();
    let picture = query(&json!({ "query": "map_preview", "player_count": 4,
                                 "choice": { "kind": "random" }, "variant": 1 }))
    .unwrap();
    assert_eq!(picture["player_count"], 4);
    assert_eq!(picture["seats"].as_array().unwrap().len(), 4);
    assert!(picture["tiles"].as_array().unwrap().len() > 20);
    let error = query(&json!({ "query": "map_preview", "player_count": 4,
                               "choice": { "kind": "template", "alias": "no-such-map" } }))
    .unwrap_err();
    assert!(!error.is_empty());
}

#[test]
fn a_table_the_host_does_not_offer_is_refused_with_a_reason() {
    let _serial = crate::common::serial();
    for (setup, wanted) in [
        (json!({ "seed": 1, "players": 1 }), "player_count"),
        (json!({ "seed": 1, "players": 9 }), "player_count"),
        (
            json!({ "seed": 1, "players": 4, "human_seat": 4 }),
            "seat 4",
        ),
        (
            json!({ "seed": 1, "players": 4, "human_faction": "nope" }),
            "faction",
        ),
        (
            json!({ "seed": 1, "players": 4, "card_set": "nope" }),
            "strategy_card_set",
        ),
        (json!({ "seed": 1, "players": 4, "bots": "nope" }), "nope"),
        (
            json!({ "seed": 1, "players": 4, "preset": "nope" }),
            "start_preset",
        ),
        (
            json!({ "seed": 1, "players": 4, "map": "no-such-map" }),
            "map",
        ),
    ] {
        let error = start_error(&setup);
        assert!(error.contains(wanted), "{setup}: {error}");
    }
}

#[test]
fn the_person_gets_the_faction_and_the_seat_that_was_asked_for() {
    let _serial = crate::common::serial();
    let page = std::sync::Arc::new(std::sync::Mutex::new(crate::common::Page {
        script: vec![crate::common::Act::Leave].into(),
        ..crate::common::Page::default()
    }));
    crate::common::install(&page, 2, "c");
    let setup = json!({ "game_id": "t", "seed": 2, "players": 5, "human_seat": 2,
                        "human_faction": "mentak", "card_set": "pok" });
    let end = crate::common::start(&setup, None);
    assert_eq!(end["end"]["left"], true);
    let page = page.lock().unwrap();
    let snapshot = &page.messages[0];
    assert_eq!(snapshot["viewer"]["seat"], "c");
    let players = snapshot["view"]["players"].as_array().unwrap();
    assert_eq!(players.len(), 5);
    let mine = players.iter().find(|p| p["id"] == "c").unwrap();
    assert_eq!(mine["faction"], "mentak");
    assert_eq!(snapshot["pending_choice"]["choice"]["player"], "c");
    // The card set of the table is the one that was asked for.
    let options = snapshot["pending_choice"]["choice"]["options"]
        .as_array()
        .unwrap();
    assert!(
        options
            .iter()
            .all(|o| o["id"].as_str().unwrap().starts_with("pok")),
        "{options:?}"
    );
}

/// `ti4_model_load` takes the bytes of a `.ti4m` through the input buffer, checks its trailer and
/// keeps the model by its name; a table with a model that is not loaded does not start.
#[test]
fn a_model_is_loaded_by_its_bytes_and_a_table_needs_its_models() {
    let _serial = crate::common::serial();
    let bytes = ti4_host::bots::test_model_bytes();
    let load = |bytes: &[u8]| {
        let len = host_api::put_bytes(bytes);
        let status = ti4_wasm::ti4_model_load(u32::try_from(len).unwrap());
        (status, host_api::out_text())
    };
    let (status, out) = load(&bytes);
    assert!(status >= 0, "{out}");
    let loaded: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(loaded["name"], "test");
    assert_eq!(loaded["label"], "Test model");
    // The SHA-256 of the whole file (`sha256sum`), not its trailer.
    assert_eq!(
        loaded["sha256"].as_str().unwrap(),
        hex::encode(<sha2::Sha256 as sha2::Digest>::digest(&bytes))
    );
    assert!(ti4_host::bots::loaded("test").is_some());
    // A changed byte is refused by the trailer.
    let mut changed = bytes.clone();
    let middle = changed.len() / 2;
    changed[middle] ^= 1;
    let (status, out) = load(&changed);
    assert!(status < 0);
    assert!(out.contains("not loaded"), "{out}");
    // A table whose model is not loaded is refused, with the name of the model.
    let table = json!({
        "table": {
            "setup": { "player_count": 3, "seed": 1 },
            "seats": [{ "kind": "human" }, { "kind": "mlp:nobody" }, { "kind": "smart" }],
        }
    });
    let len = host_api::put_input(&table.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    assert!(status < 0);
    let error = host_api::out_text();
    assert!(error.contains("'nobody'"), "{error}");
}

/// A save of format 2 names the rule bot's seats by the mask `rule_seats`: they are `smart`.
#[test]
fn the_rule_seats_of_an_older_save_are_smart() {
    let local: ti4_wasm::host_setup::Local =
        serde_json::from_value(json!({ "seed": 3, "players": 4, "humans": 1, "rule_seats": 6 }))
            .unwrap();
    let built = local.build().unwrap();
    assert_eq!(
        built.bots,
        [
            None,
            Some("smart".to_owned()),
            Some("smart".to_owned()),
            None
        ]
    );
    let local: ti4_wasm::host_setup::Local = serde_json::from_value(
        json!({ "seed": 3, "players": 4, "humans": 1, "bots": [null, "mlp:test", "random"] }),
    )
    .unwrap();
    assert_eq!(
        local.build().unwrap().bots,
        [None, Some("mlp:test".to_owned()), None, None]
    );
    // A save writes `bots`, never `rule_seats`.
    let written = serde_json::to_value(&local).unwrap();
    assert!(written.get("rule_seats").is_none(), "{written}");
}

/// The first question of a game of `setup`: (the seat asked, its subtype, the option ids).
fn first_question(setup: &Value) -> (String, String, Vec<String>) {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(None));
    let sink = seen.clone();
    ti4_wasm::set_native_host(move |output| {
        let envelope: Value = serde_json::from_str(output).expect("an envelope");
        if envelope["kind"] != "ask" {
            return host_api::CONTINUE;
        }
        let update = envelope["messages"]
            .as_array()
            .and_then(|m| m.last())
            .expect("an update");
        let choice = &update["pending_choice"]["choice"];
        let ids = choice["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| o["id"].as_str().unwrap().to_owned())
            .collect();
        *sink.lock().unwrap() = Some((
            choice["player"].as_str().unwrap_or_default().to_owned(),
            choice["context"]["subtype"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            ids,
        ));
        -1
    });
    let request = json!({ "setup": setup });
    let len = host_api::put_input(&request.to_string());
    let _ = ti4_wasm::ti4_host_start(u32::try_from(len).unwrap());
    let found = seen.lock().unwrap().take();
    found.expect("the game asks the person something")
}

#[test]
fn a_new_game_asks_the_winnu_for_their_starting_technology_and_a_version_2_game_does_not() {
    let _serial = crate::common::serial();
    let base = json!({ "game_id": "stc", "seed": 5, "players": 4,
                       "human_seat": 0, "human_faction": "winnu", "speaker": "seat" });
    let (seat, subtype, ids) = first_question(&base);
    assert_eq!(subtype, "starting_technology");
    assert_eq!(seat, "a");
    assert_eq!(ids, ["aida", "amd", "det", "nm", "pa", "ps", "sdn", "st"]);
    // A game of bot version 2 (a save made before the choice) is played as it was: the first
    // question is the strategy card draft.
    let mut old = base.clone();
    old["bot_version"] = json!(2);
    let (_, subtype, _) = first_question(&old);
    assert_eq!(subtype, "draft_strategy_card");
}
