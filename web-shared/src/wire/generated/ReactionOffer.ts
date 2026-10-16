// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One thing that a reaction window offers, as printed.
 */
export type ReactionOffer = { 
/**
 * The id of the option that uses it. A slot of several action cards has one offer for each
 * printed name, all with the id of the slot.
 */
option: string, 
/**
 * What a seat mutes: `leader:nomadagentmercer`, or the printed name of an action card.
 */
key: string, 
/**
 * `leader`, `ability`, `technology`, `promissory`, `unit`, `breakthrough` or `action_card`.
 */
kind: string, 
/**
 * Of a leader: `agent`, `commander` or `hero`.
 */
role?: string, name: string, 
/**
 * When it is used, as printed. Empty for a card whose text says it.
 */
window?: string, 
/**
 * The printed text. Empty when the content has no card for the option.
 */
text?: string, };
