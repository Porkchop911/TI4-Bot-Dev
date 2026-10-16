// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The combat roll of a kind of ground force in the rollouts: each unit rolls `dice` dice, and a
 * die hits when its result plus `modifier` is at least `hits_on`.
 */
export type GroundRoll = { 
/**
 * The printed combat value of the unit.
 */
hits_on: number, dice: number, 
/**
 * Added to each die: -1 for Fragile, +1 for Unrelenting, else 0.
 */
modifier: number, };
