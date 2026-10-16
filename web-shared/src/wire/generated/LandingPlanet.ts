// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One planet of the active system.
 */
export type LandingPlanet = { planet: string, 
/**
 * What can land here: places in [`InvasionFacts::forces`]. Empty when nothing can.
 */
lands?: Array<number>, };
