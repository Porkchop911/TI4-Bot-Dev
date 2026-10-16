// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * Ground forces of one kind in one place that the seat can land.
 */
export type LandingPool = { unit: string, damaged: boolean, 
/**
 * Where the units are, when it is not the space area of the active system: a faction lets
 * them land from a planet.
 */
from_system?: string, from_planet?: string, count: number, };
