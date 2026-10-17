// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The look-ahead of Politics' speaker question, what the primary does after the speaker: its
 * fields are top-level keys of the details, `details.draws` and `details.looks`, written with
 * [`Choice::hinted_fields`], so that a client can say it (display only).
 */
export type PoliticsHints = { 
/**
 * How many action cards the primary draws ([`POLITICS_DRAWS`]).
 */
draws: number, 
/**
 * How many agenda cards the primary looks at: [`POLITICS_LOOKS`], or fewer when the deck
 * has fewer.
 */
looks: number, };
