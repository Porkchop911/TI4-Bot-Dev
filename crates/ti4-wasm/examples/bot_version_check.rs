//! Offline games through the native host, and the bot version of their saves.
//!
//! ```text
//! cargo run -p ti4-wasm --release --example bot_version_check -- [games] [players] [asks]
//! ```
//!
//! For each seed: (1) a new game (no `bot_version` in the setup) is played to the end, the person's
//! seat answering the first option of every question, and must finish; (2) the same game is
//! stopped after `asks` questions, saved, and resumed from the save: the replay must reach the
//! saved position (`resume_check.ok`) and the save must record the current bot version; (3) the
//! same with a version-1 game whose save has the field removed (a save made before the field
//! existed): resumed, it must replay as version 1; (4) a version-2 save must replay as version 2. Exit code 1 on any failure.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_wasm::host_api;

/// Play `request` with the first option for every question of the person; stop after `stop`
/// questions (`None`: to the end). Returns (finished, asks, save at the stop, resume check).
fn play(request: &Value, stop: Option<u32>) -> Result<(bool, u32, Value, Value), String> {
    let asks = Arc::new(Mutex::new(0_u32));
    let check: Arc<Mutex<Value>> = Arc::default();
    let (a, c) = (asks.clone(), check.clone());
    ti4_wasm::set_native_host(move |text: &str| {
        let envelope: Value = serde_json::from_str(text).expect("the output is JSON");
        if envelope["kind"] != "ask" {
            return host_api::CONTINUE;
        }
        let mut n = a.lock().expect("asks");
        if *n == 0 {
            *c.lock().expect("check") = envelope["resume_check"].clone();
        }
        *n += 1;
        if stop.is_some_and(|stop| *n > stop) {
            return -1;
        }
        0
    });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).map_err(|e| e.to_string())?);
    let end = host_api::out_text();
    if status < 0 {
        return Err(format!("host failed: {end}"));
    }
    let saved = host_api::with(|session| session.save_state(Value::Null)).unwrap_or(Value::Null);
    let finished = saved["finished"] == true;
    let n = *asks.lock().expect("asks");
    let checked = check.lock().expect("check").clone();
    Ok((finished, n, saved, checked))
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let games: u64 = args
        .next()
        .map_or(Ok(3), |t| t.parse())
        .map_err(|e| format!("{e}"))?;
    let players: u64 = args
        .next()
        .map_or(Ok(4), |t| t.parse())
        .map_err(|e| format!("{e}"))?;
    let asks: u32 = args
        .next()
        .map_or(Ok(60), |t| t.parse())
        .map_err(|e| format!("{e}"))?;
    let mut failed = false;
    for seed in 1..=games {
        let setup = json!({ "game_id": format!("check{seed}"), "seed": seed, "players": players });
        let (finished, n, _, _) = play(&json!({ "setup": setup }), None)?;
        let decisions = host_api::with(|s| s.decision_log().len()).unwrap_or(0);
        println!(
            "seed {seed}: full game finished={finished} after {n} questions of the person, {decisions} decisions"
        );
        failed |= !finished;

        let (_, _, save, _) = play(&json!({ "setup": setup }), Some(asks))?;
        let version = save.get("bot_version").cloned().unwrap_or(Value::Null);
        let (_, _, _, check) = play(&json!({ "resume": save }), Some(0))?;
        let ok = check["ok"] == true && version == json!(ti4_rule_bot::tuning::BOT_VERSION);
        println!(
            "seed {seed}: new game saved with bot_version {version}, resumed: {check} -> {}",
            if ok { "OK" } else { "FAIL" }
        );
        failed |= !ok;

        // A `local` game (the page's own table) has no `setup` to record the version in: the
        // save must still carry it.
        let local =
            json!({ "local": { "seed": seed, "players": 3, "humans": 1, "rule_seats": 6 } });
        let (_, _, save, _) = play(&local, Some(asks))?;
        let version = save.get("bot_version").cloned().unwrap_or(Value::Null);
        let (_, _, _, check) = play(&json!({ "resume": save }), Some(0))?;
        let ok = check["ok"] == true && version == json!(ti4_rule_bot::tuning::BOT_VERSION);
        println!(
            "seed {seed}: local game saved with bot_version {version}, resumed: {check} -> {}",
            if ok { "OK" } else { "FAIL" }
        );
        failed |= !ok;

        // The starting-technology choice (bot version 3): the person plays the Winnu. A version-2
        // game is made and resumed without the choice, a current game with it, and both replay.
        for (version, choice) in [
            (2_u32, false),
            (ti4_rule_bot::tuning::BOT_VERSION, true),
            (ti4_rule_bot::tuning::MAX_BOT_VERSION, true),
        ] {
            let mut winnu = setup.clone();
            winnu["human_faction"] = json!("winnu");
            winnu["bot_version"] = json!(version);
            let (_, _, save, _) = play(&json!({ "setup": winnu }), Some(asks))?;
            let recorded = save.pointer("/game/setup/starting_tech") == Some(&json!(true));
            let resumed = play(&json!({ "resume": save }), Some(0));
            if let Err(error) = &resumed {
                println!("  resume error: {error}");
            }
            let (_, _, _, check) = resumed?;
            let ok = check["ok"] == true && recorded == choice;
            println!(
                "seed {seed}: Winnu, bot version {version}, choice recorded {recorded} (wanted {choice}), resumed: {check} -> {}",
                if ok { "OK" } else { "FAIL" }
            );
            failed |= !ok;
        }

        let mut old = setup.clone();
        old["bot_version"] = json!(1);
        let (_, _, mut save, _) = play(&json!({ "setup": old }), Some(asks))?;
        let version_1 = save.get("bot_version") == Some(&json!(1));
        if let Some(object) = save.as_object_mut() {
            object.remove("bot_version");
        }
        if let Some(setup) = save
            .pointer_mut("/game/setup")
            .and_then(Value::as_object_mut)
        {
            setup.remove("bot_version");
        }
        let (_, _, _, check) = play(&json!({ "resume": save }), Some(0))?;
        let ok = check["ok"] == true && version_1;
        println!(
            "seed {seed}: version-1 save without the field (recorded as 1: {version_1}), resumed: {check} -> {}",
            if ok { "OK" } else { "FAIL" }
        );
        failed |= !ok;
        // A version-2 save (recorded explicitly) must replay as version 2 under the current bot.
        let mut v2 = setup.clone();
        v2["bot_version"] = json!(2);
        let (_, _, save2, _) = play(&json!({ "setup": v2 }), Some(asks))?;
        let recorded = save2.get("bot_version").cloned().unwrap_or(Value::Null);
        let (_, _, _, check) = play(&json!({ "resume": save2 }), Some(0))?;
        let ok = check["ok"] == true && recorded == json!(2);
        println!(
            "seed {seed}: version-2 save (bot_version {recorded}), resumed: {check} -> {}",
            if ok { "OK" } else { "FAIL" }
        );
        failed |= !ok;
        // A version-3 save (recorded explicitly) must replay as version 3 under the current bot.
        let mut v3 = setup.clone();
        v3["bot_version"] = json!(3);
        let (_, _, save3, _) = play(&json!({ "setup": v3 }), Some(asks))?;
        let recorded = save3.get("bot_version").cloned().unwrap_or(Value::Null);
        let (_, _, _, check) = play(&json!({ "resume": save3 }), Some(0))?;
        let ok = check["ok"] == true && recorded == json!(3);
        println!(
            "seed {seed}: version-3 save (bot_version {recorded}), resumed: {check} -> {}",
            if ok { "OK" } else { "FAIL" }
        );
        failed |= !ok;
        // For information: the same save resumed with the current bot (what every old save
        // would do without the field), usually a different game.
        let mut forced = save.clone();
        forced["bot_version"] = json!(ti4_rule_bot::tuning::BOT_VERSION);
        forced["game"]["setup"]["bot_version"] = json!(ti4_rule_bot::tuning::BOT_VERSION);
        let outcome = play(&json!({ "resume": forced }), Some(0));
        let said = match &outcome {
            Ok((_, asks, _, check)) if *asks > 0 => format!("resume_check {check}"),
            Ok(_) => {
                "the replay asked nothing (the person's saved answers no longer fit)".to_owned()
            }
            Err(error) => error.chars().take(160).collect(),
        };
        println!("seed {seed}: the same save resumed with the current bot: {said}");
    }
    if failed {
        std::process::exit(1);
    }
    Ok(())
}
