// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ActionCardId } from "./ActionCardId.ts";

/**
 * The action cards as every player at the table sees them, and the hand limit of the viewer.
 */
export type ActionCardFacts = { 
/**
 * How many cards the deck still holds. Which cards is hidden.
 */
deck: number, 
/**
 * The discard pile, sorted by id: every card that was played or discarded.
 */
discard: Array<ActionCardId>, 
/**
 * The cards that lie face up on the card of Garbozia, sorted by id.
 */
salvage?: Array<ActionCardId>, 
/**
 * How many action cards the viewer may hold. Absent for a spectator, and for a seat that
 * has no limit.
 */
hand_limit?: number, };
