// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The capacity of the producing system today (LRR 16), display only (`details.capacity` of a
 * production question).
 */
export type CapacityHints = { 
/**
 * The capacity of the seat's ships in the system ([`crate::fleet::Standing::transport`]).
 */
transport: number, 
/**
 * The capacity already taken there ([`crate::fleet::Standing::consumed`]).
 */
consumed: number, 
/**
 * The capacity cost of the seat's fighters in the system.
 */
fighters: number, 
/**
 * The space dock fighter support not yet used by fighters there, which excuses queued
 * fighters (never ground forces) before they take capacity.
 */
unused_support: number, };
