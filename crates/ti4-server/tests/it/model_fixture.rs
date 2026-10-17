//! `fixtures/models/`: a model directory for the servers of web2's tests (`TI4_MODEL_DIR` of the
//! e2e and of `tableContract.test.ts`). Three offered copies of the synthetic model of the tests
//! (`ti4_host::bots::test_model_bytes_named`) under three names, so that a lobby shows three
//! models, and a game with an MLP seat is fast in a debug build. `cargo wire-web` writes it; this
//! test fails while it differs from what the code packs, and checks that the server loads it.

use std::path::PathBuf;

use serde_json::json;
use sha2::{Digest, Sha256};

/// The models of the fixture: the name and the label of each.
const MODELS: [(&str, &str); 3] = [
    ("synthetic-1", "Synthetic model 1"),
    ("synthetic-2", "Synthetic model 2"),
    ("synthetic-3", "Synthetic model 3"),
];

#[test]
fn model_fixture_is_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/models");
    let files: Vec<(String, Vec<u8>)> = MODELS
        .iter()
        .map(|(name, label)| {
            (
                format!("{name}.ti4m"),
                ti4_host::bots::test_model_bytes_named(name, label),
            )
        })
        .collect();
    let index = json!({
        "models": MODELS.iter().zip(&files).map(|((name, label), (_, bytes))| json!({
            "name": name,
            "label": label,
            "temperature": 1.0,
            "sha256": format!("{:x}", Sha256::digest(bytes)),
            "offered": true,
        })).collect::<Vec<_>>()
    });
    let index = format!("{}\n", serde_json::to_string_pretty(&index).unwrap());
    if std::env::var_os("TI4_WRITE_WIRE").is_some_and(|value| value == "1") {
        if dir.exists() {
            std::fs::remove_dir_all(&dir).unwrap();
        }
        std::fs::create_dir_all(&dir).unwrap();
        for (file, bytes) in &files {
            std::fs::write(dir.join(file), bytes).unwrap();
        }
        std::fs::write(dir.join("index.json"), index).unwrap();
        return;
    }
    let stale = "fixtures/models differs from the models that the code packs; write it again \
                 with `cargo wire-web`";
    assert_eq!(
        std::fs::read_to_string(dir.join("index.json"))
            .ok()
            .as_deref(),
        Some(index.as_str()),
        "{stale}"
    );
    for (file, bytes) in &files {
        assert!(
            std::fs::read(dir.join(file)).ok().as_ref() == Some(bytes),
            "{file}: {stale}"
        );
    }
    let served = ti4_server::models::load_dir(&dir, ti4_infer::Backend::Scalar).unwrap();
    assert_eq!(served.len(), 3);
    assert!(served.iter().all(|model| model.offered));
}
