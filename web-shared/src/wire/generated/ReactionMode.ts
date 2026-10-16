// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * How a player wants to handle a specific reaction card during a game (M06-016): offered
 * always, never, or only in the turns of the seat (of the action phase). Resets at the start
 * of each new game.
 */
export type ReactionMode = "always" | "never" | "ownturn";
