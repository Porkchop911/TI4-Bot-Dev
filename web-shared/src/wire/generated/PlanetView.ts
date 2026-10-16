// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Redacted view of a planet on the board.
 */
export type PlanetView = { planet_id: PlanetId, controlled_by: PlayerId | null, exhausted: boolean, attachments?: Array<string>, 
/**
 * The resources of the planet now, when they differ from the printed value: with its
 * attachments and the laws on it (`ti4_engine::production::planet_value_now`).
 */
resources?: number, 
/**
 * The influence of the planet now, when it differs from the printed value.
 */
influence?: number, };
