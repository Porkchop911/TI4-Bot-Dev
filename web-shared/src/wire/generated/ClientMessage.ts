// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { DraftStep } from "./DraftStep.ts";
import type { PlanScope } from "./PlanScope.ts";
import type { ReactionMode } from "./ReactionMode.ts";
import type { StagedSlot } from "./StagedSlot.ts";

/**
 * Messages submitted from a client to the authoritative server.
 */
export type ClientMessage = { "type": "subscribe", protocol_version: number, game_id: string, player_session?: string, 
/**
 * The connection takes part in the staged work of its seat: it is sent
 * `ServerMessage::SeatStaged` after the snapshot and after every change, from any
 * connection of the seat. Left out by a client that keeps its staged work itself.
 */
staged?: boolean, } | { "type": "submit_choice", protocol_version: number, game_id: string, nonce: string, expected_version: number, option_id: string, } | { "type": "set_reaction_mode", protocol_version: number, game_id: string, card: string, mode: ReactionMode, } | { "type": "run_draft", protocol_version: number, game_id: string, request_id: string, script: Array<DraftStep>, 
/**
 * Left out: the next tactical action.
 */
scope?: PlanScope, } | { "type": "apply_draft", protocol_version: number, game_id: string, nonce: string, expected_version: number, script: Array<DraftStep>, } | { "type": "undo", protocol_version: number, game_id: string, expected_version: number, } | { "type": "set_reaction_intent", protocol_version: number, game_id: string, triggers: Array<string>, } | { "type": "pass_reaction_hold", protocol_version: number, game_id: string, } | { "type": "preview_secondary", protocol_version: number, game_id: string, 
/**
 * Echoed in the answer so a client can match replies to requests.
 */
request_id: number, 
/**
 * The strategy card in progress (as the client read it from the public log).
 */
card: string, 
/**
 * The seat that played it.
 */
primary: string, answers: Array<string>, } | { "type": "query_reach", protocol_version: number, game_id: string, 
/**
 * Echoed in the answer so a client can match replies to requests.
 */
request_id: number, system_id: string, } | { "type": "put_staged", protocol_version: number, game_id: string, slot: StagedSlot, base_revision: number, value?: unknown, } | { "type": "ping", protocol_version: number, sequence: number, };
