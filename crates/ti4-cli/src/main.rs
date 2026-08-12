//! Stub: supported executable entry points.
//! Full implementation in M13.

fn main() {
    println!(
        "ti4 v{} (schema {}, content {}, RNG {})",
        env!("CARGO_PKG_VERSION"),
        "1",
        "1",
        "1"
    );
}
