// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One entry of a starting fleet: so many of a unit, in one place.
 */
export type StartingUnits = { 
/**
 * The unit as the sheet names it: `carrier`, not the unit of the faction (`sol_carrier`).
 */
unit: string, count: number, 
/**
 * The id of the home planet that the units are on. Absent: the space area.
 */
planet?: string, };
