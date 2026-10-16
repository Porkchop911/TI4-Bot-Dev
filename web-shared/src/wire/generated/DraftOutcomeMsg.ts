// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DraftOutcome } from "./DraftOutcome.ts";

/**
 * The answer to `ClientMessage::RunDraft`: where the run of the script ended.
 */
export type DraftOutcomeMsg = { protocol_version: number, game_id: string, 
/**
 * The `request_id` of the request.
 */
request_id: string, 
/**
 * The version of the game that the script was played on. When the game has a newer one,
 * the same script may end elsewhere.
 */
base_version: number, 
/**
 * Boxed: an outcome has up to four updates, and a message is moved through queues.
 */
outcome: DraftOutcome, };
