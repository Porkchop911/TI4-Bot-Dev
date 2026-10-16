// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One step of a movement, of the landings of an invasion, or of a production.
 */
export type PlanStep = { "kind": "move", origin: string, unit: string, damaged: boolean, gravity_drive: boolean, ionian: boolean, } | { "kind": "load", 
/**
 * The system that the ship starts in.
 */
origin: string, 
/**
 * The system where the unit is picked up.
 */
pickup_system: string, unit: string, 
/**
 * The planet, or `None` for the space area.
 */
source: string | null, damaged: boolean, galvanized: boolean, } | { "kind": "done_loading" } | { "kind": "done_moving" } | { "kind": "land", planet: string, unit: string, damaged: boolean, 
/**
 * Where the unit is, when it is not the space area of the active system.
 */
from_system: string | null, from_planet: string | null, } | { "kind": "done_committing" } | { "kind": "produce", unit: string, } | { "kind": "place", destination: string, } | { "kind": "pay", source: string, } | { "kind": "done_producing" };
