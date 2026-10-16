// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * What a seat cannot undo past: after it, the seat knows something that it did not know
 * when it decided, or the game is no longer its own to take back.
 */
export type UndoBarrier = "revealed" | "random" | "other_seat";
