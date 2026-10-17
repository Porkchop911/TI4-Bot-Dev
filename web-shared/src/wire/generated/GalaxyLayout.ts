// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { GalaxyPlacement } from "./GalaxyPlacement.ts";

/**
 * Stable, reconstructible galaxy geometry for stateless consumers.
 */
export type GalaxyLayout = { version: number, active_sources: Array<string>, placements: Array<GalaxyPlacement>, off_map_system_ids?: Array<string>, };
