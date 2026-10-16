// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One kind of ship in one origin system, with how many of it could arrive.
 */
export type ReachShip = { type_id: string, count: number, 
/**
 * Of `count`, how many are sustained-damage (they still move; shown for completeness).
 */
damaged: number, 
/**
 * Of `count`, how many arrive only by spending Gravity Drive's +1.
 */
gravity_drive: number, 
/**
 * Of `count`, how many arrive only by exhausting the Ionian Fuel Refinery.
 */
ionian: number, };
