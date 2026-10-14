//! An offline game's save travels in a feedback report and replays from the stored file.

use std::sync::{Arc, Mutex};

use crate::common::{Act, Page, install, setup, start};
use serde_json::{Value, json};
use ti4_server::feedback::{self, FeedbackRequest};

#[test]
fn a_save_in_a_stored_report_replays_to_the_same_position() {
    let _serial = crate::common::serial();
    let seed = 33;
    let first = Arc::new(Mutex::new(Page {
        script: vec![
            Act::Answer(40),
            Act::Cmd(json!({ "type": "save_state" })),
            Act::Leave,
        ]
        .into(),
        ..Page::default()
    }));
    install(&first, seed, "a");
    start(&setup(seed, 3), None);
    let save = first.lock().unwrap().replies.last().unwrap()["save"].clone();

    // What the web queue sends.
    let body = json!({
        "category": "bug", "text": "wrong at this point", "report_id": "test-report-0001",
        "context": { "sender": { "nickname": "Ada" }, "local": {
            "seed": seed, "setup": save["setup"], "answers": save["answers"],
            "engine": { "build": "test", "content_digest": "none" }, "wasm_sha256": "none",
            "save": save,
        } },
    });
    let request: FeedbackRequest = serde_json::from_value(body).unwrap();
    feedback::validate(&request).unwrap();
    let dir = std::env::temp_dir().join(format!("fbreplay_{}", std::process::id()));
    let (id, _) = feedback::store_once(&dir, request, 1_760_227_200, "srv").unwrap();

    // The lister's extraction gives the blob back; replaying it reaches the saved position.
    let (path, record) = feedback::find(&dir, &id).unwrap();
    let blob: Value = feedback::save_blob(&record).unwrap().clone();
    assert_eq!(blob, save);
    if let Ok(out) = std::env::var("TI4_REPLAY_FIXTURE") {
        std::fs::copy(path, out).unwrap();
    }
    let second = Arc::new(Mutex::new(Page {
        script: vec![Act::Leave].into(),
        ..Page::default()
    }));
    install(&second, seed, "a");
    start(&blob["setup"], Some(&blob));
    let second = second.lock().unwrap();
    assert_eq!(
        second.asks[0]["resume_check"]["ok"], true,
        "{}",
        second.asks[0]["resume_check"]
    );
    let _ = std::fs::remove_dir_all(dir);
}
