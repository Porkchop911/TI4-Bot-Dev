// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetMetaView } from "./PlanetMetaView.ts";

/**
 * Static geometry and metadata of a star system tile.
 */
export type BoardTileView = { system_id: string, label: string, q: number, r: number, hyperlane?: boolean, special_area?: string, anomalies?: Array<string>, wormholes?: Array<string>, egress?: boolean, planets?: Array<PlanetMetaView>, };
