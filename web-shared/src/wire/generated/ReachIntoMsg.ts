// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ReachOrigin } from "./ReachOrigin.ts";
import type { ReachRefusal } from "./ReachRefusal.ts";

/**
 * The answer to a `query_reach`: ships that could move into `system_id`, "as of now".
 */
export type ReachIntoMsg = { protocol_version: number, game_id: string, request_id: number, system_id: string, 
/**
 * The game version of the position the answer was computed from. Not a stale-version
 * envelope field: the client compares it with the version it shows.
 */
as_of_version: number, 
/**
 * Ships grouped by owner seat (seating order) and origin system. Empty when refused.
 */
origins: Array<ReachOrigin>, refused?: ReachRefusal, };
