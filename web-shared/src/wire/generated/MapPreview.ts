// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BoardTileView } from "./BoardTileView.ts";
import type { MapChoice } from "./MapChoice.ts";
import type { SeatPreview } from "./SeatPreview.ts";

/**
 * A pictured map: the tiles a game with this choice has, and where each seat sits.
 */
export type MapPreview = { choice: MapChoice, player_count: number, tiles: Array<BoardTileView>, seats: Array<SeatPreview>, };
