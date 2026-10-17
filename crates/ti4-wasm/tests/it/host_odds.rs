//! The odds of a combat through `ti4_host_query`: the numbers of `ti4_calc::odds`, with no game
//! and while a game waits for the page.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_calc::odds::{self, GroundOddsRequest, SideInput, SpaceOptions};
use ti4_content::ContentStore;
use ti4_model::content_types::POK;
use ti4_wasm::host_api;

use crate::common::{Act, Page, install, setup, start};

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

/// `{"query": kind, ...request}`.
fn asked(kind: &str, request: &Value) -> Value {
    let mut asked = request.clone();
    asked["query"] = json!(kind);
    asked
}

fn space_requests() -> Vec<Value> {
    vec![
        json!({ "attacker": { "units": { "cruiser": 1 } }, "defender": { "units": { "fighter": 1 } } }),
        json!({
            "attacker": { "faction": "sol", "units": [["carrier", 1], ["fighter", 4], ["dreadnought", 2]],
                          "damaged": { "dreadnought": 1 } },
            "defender": { "faction": "letnev", "units": { "dreadnought": 1, "destroyer": 2 },
                          "guns": { "pds": 2 } },
        }),
        json!({
            "attacker": { "faction": "jolnar", "units": { "warsun": 1, "fighter": 3 } },
            "defender": { "faction": "naalu", "units": { "fighter": 6, "cruiser": 2 } },
        }),
        json!({
            "attacker": { "faction": "sol", "units": { "destroyer": 2, "carrier": 1 } },
            "defender": { "faction": "naalu", "units": { "fighter": 4, "cruiser": 1 } },
            "in_progress": true,
        }),
    ]
}

fn ground_requests() -> Vec<Value> {
    vec![
        json!({
            "attacker": { "faction": "sol", "units": { "infantry": 3 } },
            "defender": { "faction": "letnev", "units": { "infantry": 2 } },
        }),
        json!({
            "attacker": { "faction": "l1z1x", "units": { "infantry": 2, "mech": 1 } },
            "defender": { "faction": "hacan", "units": { "infantry": 1, "mech": 1 },
                          "damaged": { "mech": 1 }, "guns": { "pds": 1 } },
            "harrow": { "dreadnought": 2 },
        }),
        json!({
            "attacker": { "faction": "sardakk", "units": { "infantry": 4 } },
            "defender": { "faction": "jolnar", "units": { "infantry": 4 } },
            "simulations": 500,
        }),
    ]
}

/// What `ti4_calc` gives for the sides of a space request, as the query asks it.
fn space_expected(request: &Value) -> Value {
    let side = |key: &str| -> SideInput { serde_json::from_value(request[key].clone()).unwrap() };
    let odds = odds::space_odds_of(
        ContentStore::embedded(),
        POK,
        &side("attacker"),
        &side("defender"),
        SpaceOptions {
            seed: Some(0),
            in_progress: request["in_progress"].as_bool(),
            ..Default::default()
        },
    )
    .unwrap();
    serde_json::to_value(odds).unwrap()
}

fn ground_expected(request: &Value) -> Value {
    let request: GroundOddsRequest = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(odds::ground_odds(ContentStore::embedded(), &request).unwrap()).unwrap()
}

/// Every request asked of the engine, with the answers in order.
fn ask_all() -> Vec<Value> {
    let space = space_requests()
        .into_iter()
        .map(|request| query(&asked("space_odds", &request)).unwrap());
    let ground = ground_requests()
        .into_iter()
        .map(|request| query(&asked("ground_odds", &request)).unwrap());
    space.chain(ground).collect()
}

fn expected_all() -> Vec<Value> {
    let space = space_requests()
        .into_iter()
        .map(|request| space_expected(&request));
    let ground = ground_requests()
        .into_iter()
        .map(|request| ground_expected(&request));
    space.chain(ground).collect()
}

#[test]
fn the_odds_queries_give_the_numbers_of_ti4_calc() {
    let _serial = crate::common::serial();
    let answers = ask_all();
    assert_eq!(answers, expected_all());
    // The same sides give the same numbers (a fixed seed).
    assert_eq!(answers, ask_all());
    assert_eq!(answers[0]["attacker_fielded"]["cruiser"], 1);
    assert_eq!(answers[6]["simulations"], 500);
    // A combat that has begun fires no barrage before its round: the fighters fare better.
    let mut begun = space_requests()[3].clone();
    begun["in_progress"] = json!(false);
    assert_ne!(answers[3], query(&asked("space_odds", &begun)).unwrap());
}

#[test]
fn an_odds_query_that_ti4_calc_refuses_is_an_error() {
    let _serial = crate::common::serial();
    let refused = |request: Value| query(&request).unwrap_err();
    assert!(
        refused(
            json!({ "query": "space_odds", "attacker": { "units": { "no_such_ship": 1 } },
                        "defender": { "units": { "fighter": 1 } } })
        )
        .starts_with("space odds:")
    );
    assert!(
        refused(json!({ "query": "space_odds", "attacker": {}, "defender": {}, "seed": 3 }))
            .starts_with("query:")
    );
    assert!(
        refused(json!({ "query": "ground_odds",
                        "attacker": { "faction": "sol", "units": { "cruiser": 1 } },
                        "defender": { "faction": "sol", "units": { "infantry": 1 } } }))
        .contains("invalid ground force: cruiser")
    );
    assert!(
        refused(json!({ "query": "ground_odds",
                        "attacker": { "faction": "sol", "units": { "infantry": 1 }, "guns": { "pds": 1 } },
                        "defender": { "faction": "sol", "units": { "infantry": 1 } } }))
        .starts_with("ground odds:")
    );
    assert!(refused(json!({ "query": "ground_odds", "attacker": {} })).starts_with("query:"));
}

#[test]
fn the_odds_are_asked_while_a_game_waits_and_the_game_goes_on() {
    let _serial = crate::common::serial();
    let seed = 21;
    let answers: Arc<Mutex<Vec<Value>>> = Arc::default();
    let kept = answers.clone();
    let page = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Answer(3),
            Act::Probe(Box::new(move |_| *kept.lock().unwrap() = ask_all())),
            Act::Answer(3),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&page, seed, "a");
    let end = start(&setup(seed, 3), None);
    assert_eq!(end["end"]["left"], true, "{end}");
    assert_eq!(end["end"]["error"], Value::Null, "{end}");
    assert_eq!(page.lock().unwrap().answers.len(), 6);
    assert_eq!(*answers.lock().unwrap(), expected_all());
}
