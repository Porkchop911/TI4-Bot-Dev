// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PreviewResult } from "./PreviewResult.ts";

/**
 * The answer to a `PreviewSecondary` request: what the asking seat would be asked for the
 * secondary in progress if its window opened now. Sent only to the connection that asked;
 * ephemeral (never persisted, logged or broadcast) and carries no game version in the envelope
 * sense, so a stale-version filter never drops it: `as_of_version` says which position it was
 * computed at, and the client re-checks the plan against the real question when it opens.
 */
export type SecondaryPreviewMsg = { protocol_version: number, game_id: string, request_id: number, 
/**
 * The game version of the position the preview was computed from.
 */
as_of_version: number, 
/**
 * How many decisions the game had recorded then.
 */
as_of_decisions: number, card: string, outcome: PreviewResult, };
