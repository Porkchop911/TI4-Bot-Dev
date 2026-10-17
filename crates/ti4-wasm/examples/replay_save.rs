//! Replays an offline save natively, with the same host crate the browser runs, to reproduce the
//! position of a feedback report.
//!
//! ```text
//! server feedback-save --data-dir /var/lib/ti4-public --out save.json fb_1760227200_ab12cd34
//! cargo run -p ti4-wasm --example replay_save -- save.json [--snapshot snapshot.json]
//! ```
//!
//! The file may be the save blob itself, a stored report (`context.local.save`) or a request body.
//! The game is played again from its seed and every saved decision until the saved position;
//! the tool prints whether the decisions came back the same (count and hash of the save) and what
//! the person was asked there. A save of a finished game has no question: the whole game's decisions
//! (count and hash) are compared instead. `--snapshot FILE` also writes the full `initial_snapshot` message
//! (the board as the person saw it) for inspection. Exit code 1 when the replay differs.

use std::sync::{Arc, Mutex};

use serde_json::{Value, json};
use ti4_wasm::host_api;

fn save_of(file: &Value) -> Option<Value> {
    [
        file.pointer("/context/local/save"),
        file.pointer("/local/save"),
        file.get("save"),
        Some(file),
    ]
    .into_iter()
    .flatten()
    .find(|v| v.get("game").is_some() && v.get("decisions").is_some())
    .cloned()
}

fn main() -> Result<(), String> {
    let mut path = None;
    let mut snapshot_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--snapshot" => snapshot_path = Some(args.next().ok_or("--snapshot needs a file")?),
            _ if path.is_none() => path = Some(arg),
            _ => return Err(format!("unexpected argument {arg:?}")),
        }
    }
    let path = path.ok_or("usage: replay_save SAVE.json [--snapshot FILE]")?;
    let file: Value =
        serde_json::from_slice(&std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?)
            .map_err(|e| format!("{path}: {e}"))?;
    let save = save_of(&file).ok_or("no save found (need game and decisions)")?;
    let seed = save["seed"].as_u64().ok_or("save.seed")?;
    let humans = save["humans"].as_u64().unwrap_or(0);
    if let Some(local) = file.pointer("/context/local").or_else(|| file.get("local")) {
        println!(
            "report: seed {seed}, engine build {}, content {}, wasm {}",
            local.pointer("/engine/build").unwrap_or(&Value::Null),
            local
                .pointer("/engine/content_digest")
                .unwrap_or(&Value::Null),
            local.get("wasm_sha256").unwrap_or(&Value::Null),
        );
    }

    // The first question after the replay ends the run; keep what it showed.
    let seen: Arc<Mutex<Option<Value>>> = Arc::default();
    let sink = seen.clone();
    ti4_wasm::set_native_host(move |text: &str| {
        let envelope: Value = serde_json::from_str(text).expect("the output is JSON");
        match envelope["kind"].as_str() {
            Some("progress") => return host_api::CONTINUE,
            Some("ask") => *sink.lock().expect("sink") = Some(envelope),
            _ => {}
        }
        -1
    });
    let request = json!({ "resume": save });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).map_err(|e| e.to_string())?);
    let end = host_api::out_text();
    if status < 0 {
        return Err(format!("the replay failed: {end}"));
    }
    let asked = seen.lock().expect("seen").take();
    if asked.is_none() && save["finished"] == true {
        // A finished game has no question to compare at: compare everything it decided instead.
        let (found, hash) = host_api::with(|session| {
            (
                session.decision_log().len(),
                ti4_wasm::host_session::decisions_hash(session.decision_log()),
            )
        })
        .ok_or("the replay left no session")?;
        println!(
            "replayed {} answers of the page's seats {humans:#b}: the game ended",
            save["answers"].as_array().map_or(0, Vec::len),
        );
        println!(
            "saved: decisions {} hash {}",
            save["decisions"], save["decisions_hash"]
        );
        println!("found: decisions {found} hash {hash}");
        if save["decisions"] == json!(found) && save["decisions_hash"] == json!(hash) {
            println!("OK: the finished game is reproduced");
            return Ok(());
        }
        println!("DIFFERENT: this build decides otherwise than the one that made the save");
        std::process::exit(1);
    }
    let ask = asked.ok_or_else(|| format!("the game ended before a question: {end}"))?;
    let snapshot = ask["messages"]
        .as_array()
        .and_then(|m| m.iter().find(|m| m["type"] == "initial_snapshot"))
        .cloned()
        .unwrap_or(Value::Null);
    let check = &ask["resume_check"];
    println!(
        "replayed {} decisions, the page's seats {humans:#b}: round {} phase {} version {}",
        save["decisions"].as_array().map_or(0, Vec::len),
        snapshot.pointer("/state/round").unwrap_or(&Value::Null),
        snapshot.pointer("/state/phase").unwrap_or(&Value::Null),
        snapshot.get("game_version").unwrap_or(&Value::Null),
    );
    println!(
        "saved: format {} hash {}",
        save["format"], save["decisions_hash"]
    );
    println!("check: {check}");
    println!(
        "question: {}",
        snapshot
            .pointer("/pending_choice/choice/prompt")
            .unwrap_or(&Value::Null)
    );
    if let Some(out) = snapshot_path {
        std::fs::write(
            &out,
            serde_json::to_string_pretty(&snapshot).map_err(|e| e.to_string())?,
        )
        .map_err(|e| format!("{out}: {e}"))?;
        println!("wrote {out}");
    }
    if check["ok"] == true {
        println!("OK: the position is reproduced");
        Ok(())
    } else {
        println!("DIFFERENT: this build decides otherwise than the one that made the save");
        std::process::exit(1);
    }
}
