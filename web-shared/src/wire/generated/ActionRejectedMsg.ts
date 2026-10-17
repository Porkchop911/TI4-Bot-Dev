// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { RejectionReason } from "./RejectionReason.ts";

/**
 * Notification that an action submission was rejected.
 */
export type ActionRejectedMsg = { protocol_version: number, game_id: string, game_version: number, reason: RejectionReason, };
