// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Units count input, accepting either `{"unit_id": count}` or `[["unit_id", count]]`.
 */
export type UnitCounts = { [key in string]: number } | Array<[string, number]>;
