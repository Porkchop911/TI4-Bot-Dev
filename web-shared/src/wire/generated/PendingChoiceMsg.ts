// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { Choice } from "./Choice.ts";
import type { GalaxyLayout } from "./GalaxyLayout.ts";

/**
 * Pending choice sent only to the acting seat.
 */
export type PendingChoiceMsg = { protocol_version: number, game_id: string, game_version: number, nonce: string, choice: Choice, 
/**
 * The engine's `GameState`, passed through without a projection.
 */
state: Record<string, unknown>, galaxy_layout: GalaxyLayout, };
