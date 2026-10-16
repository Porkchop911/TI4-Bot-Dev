// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PublicTurnStatus } from "./PublicTurnStatus.ts";

/**
 * Public turn status update.
 */
export type TurnStatusMsg = { protocol_version: number, game_id: string, game_version: number, status: PublicTurnStatus, };
