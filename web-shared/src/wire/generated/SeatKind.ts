// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Who plays a seat of a table ([`TableRequest`]), by the names of a lobby: `human` (the page),
 * or a kind of `ti4_host::bots`: `smart`, `random` or `mlp:<name>`.
 */
export type SeatKind = "human" | "smart" | "random" | `mlp:${string}`;
