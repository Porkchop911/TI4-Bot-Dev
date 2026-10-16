// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * How a ship gets to the active system.
 */
export type MoveFact = { 
/**
 * The move uses Gravity Drive: the ship cannot arrive without it.
 */
gravity_drive: boolean, 
/**
 * The move exhausts the Ionian Fuel Refinery.
 */
ionian: boolean, 
/**
 * The systems that the ship enters, in order. The last one is the active system.
 */
path: Array<string>, 
/**
 * The gravity rifts that the ship leaves on this path. It rolls once for each.
 */
rifts: Array<string>, };
