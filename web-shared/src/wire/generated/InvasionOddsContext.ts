// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

export type InvasionOddsContext = { opponent: PlayerId | null, 
/**
 * False when a live defense modifier cannot be represented in the ground odds request.
 */
available: boolean, ground_force_types: Array<string>, 
/**
 * Guns not already carried by the immediate opponent's modeled ground forces.
 */
additional_guns: { [key in string]: number }, 
/**
 * Legal Harrow bombarders in the space area; bombardment already resolved.
 */
harrow_units: { [key in string]: number }, };
