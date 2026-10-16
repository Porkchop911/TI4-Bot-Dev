// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One side of a ground combat on a planet: its ground forces by unit id, and for the defender
 * the units that fire as defense guns before the combat (`space_cannon` of a PDS).
 */
export type GroundOddsSide = { 
/**
 * Faction identifier (e.g. "sol"): its unit upgrades and abilities apply.
 */
faction: string, 
/**
 * Ground forces as `{"unit_id": count}`.
 */
units: { [key in string]: number }, 
/**
 * Of `units`, those that start damaged.
 */
damaged?: { [key in string]: number }, 
/**
 * The defender only: units on the planet that fire as defense guns.
 */
guns?: { [key in string]: number }, };
