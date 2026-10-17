// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * One decision made on a seat's behalf because it had a single legal option.
 */
export type AutoResolvedNote = { 
/**
 * Unique per note, so a client can drop a repeat.
 */
id: string, 
/**
 * The question that was not asked.
 */
prompt: string, 
/**
 * What was chosen, as the option was labelled.
 */
selected: string, 
/**
 * Why there was no real choice.
 */
reason: string, 
/**
 * How many identical notes this one stands for (a bill paid in several steps).
 */
count?: number, 
/**
 * The printed names of the cards of a reaction window that was declined because the seat
 * set each of them to never offer. Empty for every other note.
 */
passed?: Array<string>, };
