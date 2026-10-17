// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One leader as printed.
 */
export type LeaderRecord = { id: string, faction: string, 
/**
 * `agent`, `commander` or `hero`.
 */
kind: string, name: string, title?: string, ability_name?: string, ability_window?: string, ability_text?: string, 
/**
 * As printed, `Always Unlocked` too.
 */
unlock_condition?: string, source?: string, };
