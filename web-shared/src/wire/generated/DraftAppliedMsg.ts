// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ScriptStopped } from "./ScriptStopped.ts";

/**
 * What became of `ClientMessage::ApplyDraft`, for the seat that sent it.
 */
export type DraftAppliedMsg = { protocol_version: number, game_id: string, 
/**
 * The nonce of the request: the choice that the script began with.
 */
nonce: string, 
/**
 * Absent when the game played the whole script.
 */
stopped?: ScriptStopped, };
