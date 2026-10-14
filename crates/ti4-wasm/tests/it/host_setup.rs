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
fn the_form_is_offered_cards_factions_maps_and_bots_for_three_to_six() {
    let _serial = crate::common::serial();
    let form = query(&json!({ "query": "describe" })).unwrap();
    assert_eq!(form["min_players"], 3);
    assert_eq!(form["max_players"], 6);
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
    assert_eq!(form["bot_kinds"].as_array().unwrap().len(), 2);
    for count in 3..=6 {
        let maps = form["maps"][count.to_string()].as_array().unwrap();
        assert!(!maps.is_empty(), "maps for {count}");
        assert!(
            maps.iter().any(|map| map["recommended"] == true),
            "a recommended map for {count}"
        );
    }
    assert!(form["maps"].get("7").is_none());
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
        (json!({ "seed": 1, "players": 2 }), "3 to 6"),
        (json!({ "seed": 1, "players": 7 }), "3 to 6"),
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
        (json!({ "seed": 1, "players": 4, "bots": "smart" }), "bots"),
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
