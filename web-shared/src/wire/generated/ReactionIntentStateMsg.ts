// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * A seat's own bluff settings (private to that seat; carries no game version on purpose, so a
 * stale-version filter never drops it).
 */
export type ReactionIntentStateMsg = { protocol_version: number, game_id: string, 
/**
 * Declared trigger ids.
 */
triggers: Array<string>, max_triggers: number, 
/**
 * Whether this seat may bluff right now (cards in hand, no Never setting).
 */
eligible: boolean, ineligible_reason?: string, 
/**
 * The declaration cannot change before this round starts.
 */
locked_until_round?: number, 
/**
 * A stall budget (per round, per game or table-wide) is used up: no more holds.
 */
budget_used_up: boolean, 
/**
 * A hold for this seat is running now: show the Pass control.
 */
holding: boolean, };
