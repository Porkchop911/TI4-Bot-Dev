// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One exploration card as printed.
 */
export type ExplorationCard = { id: string, name: string, 
/**
 * The deck of the card, as [`EXPLORATION_TRAITS`] names it.
 */
deck: string, 
/**
 * `Instant`, `Fragment`, `Attach` or `Token`.
 */
resolution: string, text: string, };
