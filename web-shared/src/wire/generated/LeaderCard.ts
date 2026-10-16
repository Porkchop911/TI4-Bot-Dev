// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One leader as printed.
 */
export type LeaderCard = { id: string, 
/**
 * `agent`, `commander` or `hero`.
 */
kind: string, name: string, title: string, window?: string, text: string, 
/**
 * How the leader is unlocked. Empty for a leader that is never locked.
 */
unlock?: string, };
