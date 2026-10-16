// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BoardTileView } from "./BoardTileView.ts";
import type { MapChoice } from "./MapChoice.ts";
import type { MapChoiceView } from "./MapChoiceView.ts";
import type { SeatPreview } from "./SeatPreview.ts";

/**
 * What a table looks like before it starts.
 */
export type TablePreview = { 
/**
 * What the table says of its map: its name, its author, its size.
 */
map: MapChoiceView, choice: MapChoice, player_count: number, tiles: Array<BoardTileView>, seats: Array<SeatPreview>, };
