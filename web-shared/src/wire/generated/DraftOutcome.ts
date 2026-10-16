// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DraftStop } from "./DraftStop.ts";
import type { SessionUpdate } from "./SessionUpdate.ts";

/**
 * Where a run of a script ended, and what the planner is shown there.
 */
export type DraftOutcome = { 
/**
 * The position where the run ended, with the open question when the stop is
 * [`DraftStop::Open`]. The same shape as a live update.
 */
update: SessionUpdate, 
/**
 * The question of the first ship to move, before any ship moved: what a whole movement is
 * staged against. `None` when the run did not reach the movement.
 */
movement?: SessionUpdate, 
/**
 * The question of the first ground force to land: what the landings are staged against.
 * `None` when the run did not reach them, or the action has none.
 */
invasion?: SessionUpdate, 
/**
 * The question of the first unit to produce: what a production is staged against.
 */
production?: SessionUpdate, 
/**
 * How many steps of the script the game took, or did not need.
 */
consumed: number, stop: DraftStop, };
