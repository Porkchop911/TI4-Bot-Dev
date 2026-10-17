// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * Notification of game completion.
 */
export type GameOverMsg = { protocol_version: number, game_id: string, game_version: number, winner: PlayerId | null, final_scores: { [key in PlayerId]: number }, };
