// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One faction ability as printed.
 */
export type AbilityRecord = { id: string, name: string, faction?: string, 
/**
 * The part of the ability that always holds.
 */
permanent_effect?: string, 
/**
 * When the other part is used: `ACTION`, or a timing.
 */
window?: string, window_effect?: string, };
