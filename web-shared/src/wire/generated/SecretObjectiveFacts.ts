// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { SecretObjectiveId } from "./SecretObjectiveId.ts";

/**
 * The secret objectives as every player at the table sees them.
 */
export type SecretObjectiveFacts = { 
/**
 * How many cards the deck still holds. Which cards is hidden.
 */
deck: number, 
/**
 * The secret objectives that each seat has scored, sorted by id. A seat that has scored none
 * is left out.
 */
scored: { [key in PlayerId]: Array<SecretObjectiveId> }, 
/**
 * How many secret objectives a seat may have, scored and unscored together.
 */
hand_limit: number, };
