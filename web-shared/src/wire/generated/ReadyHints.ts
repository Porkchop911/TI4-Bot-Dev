// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ReadyDetails } from "./ReadyDetails.ts";

/**
 * The look-ahead of a readying of exhausted planets (Diplomacy's system question, the window of
 * its secondary, and each planet to ready): its fields are top-level keys of the details,
 * `details.ready_hints` and `details.ready_count`, written with [`Choice::hinted_fields`], so
 * that a client can stage the whole readying at once (display only).
 */
export type ReadyHints = { ready_hints: ReadyDetails, 
/**
 * How many planets the game readies at most, with this question and the ones after it.
 */
ready_count: number, };
