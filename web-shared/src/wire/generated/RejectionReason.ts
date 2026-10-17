// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { UndoBarrier } from "./UndoBarrier.ts";

/**
 * Structured explanation when a choice submission is rejected.
 */
export type RejectionReason = { "reason": "stale_version", expected: number, current: number, } | { "reason": "stale_nonce" } | { "reason": "unauthorized_seat", seat: PlayerId | null, } | { "reason": "no_pending_choice" } | { "reason": "unknown_option", option_id: string, } | { "reason": "validation_failed", message: string, } | { "reason": "undo_barrier", barrier: UndoBarrier, } | { "reason": "answered_elsewhere" };
