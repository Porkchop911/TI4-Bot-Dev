// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One action card as printed. Copies of a card have ids of their own and one name.
 */
export type ActionCardRecord = { id: string, name: string, phase?: string, 
/**
 * When the card is played, as printed: `Action`, or a timing.
 */
window?: string, text: string, 
/**
 * The id that the engine may name the card by (`direct_hit` for `dh1` to `dh4`).
 */
automation_id?: string, };
