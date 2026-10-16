// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One exploration card as printed.
 */
export type ExploreRecord = { id: string, name: string, 
/**
 * The deck, as the corpus names it: `cultural`, `hazardous`, `industrial`, `frontier`.
 */
kind: string, 
/**
 * `Instant`, `Fragment`, `Attach` or `Token`.
 */
resolution: string, text: string, flavor_text?: string, };
