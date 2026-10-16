// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * From the decision with the number `decision` on, the page plays the seats of `humans` (bit 0
 * is seat `a`). An undo does not go back before it.
 */
export type SeatSwitch = { decision: number, humans: number, };
