// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Blocked } from "./Blocked.ts";
import type { MoveFact } from "./MoveFact.ts";

/**
 * One ship of the seat outside the active system.
 */
export type ShipFact = { origin: string, 
/**
 * The place of the ship among the units of the seat in the space area of `origin`.
 */
index: number, unit: string, damaged: boolean, capacity: number, 
/**
 * The ship counts against the fleet pool.
 */
fleet: boolean, 
/**
 * The move value of the ship in this activation, without Gravity Drive and the Ionian Fuel
 * Refinery: a move that uses one of them says so, and has 1 more for each.
 */
move_value: number, 
/**
 * The ship starts in a nebula: it moves with a value of 1, whatever `move_value` is.
 */
nebula?: boolean, move?: MoveFact, blocked?: Blocked, 
/**
 * What the ship can load: places in [`MovementFacts::cargo`].
 */
loads?: Array<number>, };
