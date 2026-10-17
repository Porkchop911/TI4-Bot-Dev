//! The catalogue of `fixtures/catalog/catalog.json`: what the engine answers to
//! `{"query":"catalog"}` (and the server at `GET /api/catalog`). The tests of web read it in
//! place of an engine. `cargo wire-web` writes it (this test, with `TI4_WRITE_WIRE`); otherwise
//! the test fails while it differs from what the engine answers now.

use std::path::PathBuf;

use serde_json::Value;
use ti4_wasm::host_api;

use crate::common::serial;

#[test]
fn catalog_fixture_is_current() {
    let _serial = serial();
    let len = host_api::put_input(r#"{"query":"catalog"}"#);
    let status = host_api::query(len);
    let text = host_api::out_text();
    assert!(status >= 0, "the catalogue query failed: {text}");
    let catalog: Value = serde_json::from_str(&text).expect("the catalogue is JSON");
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/catalog/catalog.json");
    if std::env::var_os("TI4_WRITE_WIRE").is_some_and(|value| value == "1") {
        std::fs::create_dir_all(path.parent().expect("a dir")).expect("the dir is made");
        let pretty = serde_json::to_string_pretty(&catalog).expect("the catalogue serializes");
        std::fs::write(&path, format!("{pretty}\n")).expect("the fixture is written");
        return;
    }
    let kept: Value = std::fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(Value::Null);
    assert!(
        kept == catalog,
        "fixtures/catalog/catalog.json differs from the catalogue of the engine; write it again \
         with `cargo wire-web`"
    );
}
