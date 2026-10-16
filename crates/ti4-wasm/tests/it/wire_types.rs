//! The TypeScript types of the wire in `web-shared/src/wire/generated/` are those that
//! `ti4_wasm::wire_ts` writes. `cargo wire-web` writes them (this test, with `TI4_WRITE_WIRE`).

use std::path::PathBuf;

use ti4_wasm::wire_ts;

#[test]
fn generated_wire_types_are_current() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(wire_ts::GENERATED_DIR);
    if std::env::var_os("TI4_WRITE_WIRE").is_some_and(|value| value == "1") {
        wire_ts::write(&dir).expect("the generated wire types are written");
        return;
    }
    let stale = wire_ts::stale(&dir).expect("the generated wire types are read");
    assert!(
        stale.is_empty(),
        "{} differs from the Rust types; write it again with `cargo wire-web`:\n{}",
        wire_ts::GENERATED_DIR,
        stale.join("\n")
    );
}
