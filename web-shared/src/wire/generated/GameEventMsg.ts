// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { GameEvent } from "./GameEvent.ts";

/**
 * Server message carrying a new game event to all subscribers.
 */
export type GameEventMsg = { protocol_version: number, game_id: string, entry: GameEvent, };
