//! Stub: supported executable entry points.
//! Full implementation in M13.

fn main() {
    println!(
        "ti4 v{} (schema 1, content 1, RNG 1)",
        env!("CARGO_PKG_VERSION")
    );
}
