// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One side of a space combat in the active system as the state has it, before a ship moves
 * in: the ships there by unit id, how many of them are damaged, and the units of the side that
 * fire SPACE CANNON in that system before the combat. The shape of a side of `ti4_calc`.
 */
export type CombatSide = { faction: string, 
/**
 * The ships of the side in the active system.
 */
units?: { [key in string]: number }, 
/**
 * How many of them are damaged.
 */
damaged?: { [key in string]: number }, 
/**
 * The units of the side that fire SPACE CANNON in the active system before the combat.
 */
guns?: { [key in string]: number }, };
