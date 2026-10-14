//! Replays an offline save natively, with the same host crate the browser runs, to reproduce the
//! position of a feedback report.
//!
//! ```text
//! server feedback-save --data-dir /var/lib/ti4-public --out save.json fb_1760227200_ab12cd34
//! cargo run -p ti4-wasm --example replay_save -- save.json [--snapshot snapshot.json]
//! ```
//!
//! The file may be the save blob itself, a stored report (`context.local.save`) or a request body.
//! The game is played again from its seed and the person's answers until the saved position;
//! the tool prints whether the decisions came back the same (count and hash of the save) and what
//! the person was asked there. `--snapshot FILE` also writes the full `initial_snapshot` message
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
    .find(|v| v.get("setup").is_some() && v.get("answers").is_some())
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
    let save = save_of(&file).ok_or("no save found (need setup and answers)")?;
    let seed = save["seed"].as_u64().ok_or("save.seed")?;
    let human =
        usize::try_from(save["human_seat"].as_u64().unwrap_or(0)).map_err(|e| e.to_string())?;
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
    let request = json!({ "setup": save["setup"], "resume": save });
    let len = host_api::put_input(&request.to_string());
    let status = ti4_wasm::ti4_host_start(u32::try_from(len).map_err(|e| e.to_string())?);
    let end = host_api::out_text();
    if status < 0 {
        return Err(format!("the replay failed: {end}"));
    }
    let ask = seen
        .lock()
        .expect("seen")
        .take()
        .ok_or_else(|| format!("the game ended before a question: {end}"))?;
    let snapshot = ask["messages"]
        .as_array()
        .and_then(|m| m.iter().find(|m| m["type"] == "initial_snapshot"))
        .cloned()
        .unwrap_or(Value::Null);
    let check = &ask["resume_check"];
    println!(
        "replayed {} answers of seat {human}: round {} phase {} version {}",
        save["answers"].as_array().map_or(0, Vec::len),
        snapshot.pointer("/state/round").unwrap_or(&Value::Null),
        snapshot.pointer("/state/phase").unwrap_or(&Value::Null),
        snapshot.get("game_version").unwrap_or(&Value::Null),
    );
    println!(
        "saved: decisions {} hash {}",
        save["decisions"], save["decisions_hash"]
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
