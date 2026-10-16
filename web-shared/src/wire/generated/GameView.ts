// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BoardView } from "./BoardView.ts";
import type { Phase } from "./Phase.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { PlayerView } from "./PlayerView.ts";
import type { TableView } from "./TableView.ts";

/**
 * Complete redacted view of the game for one viewer.
 */
export type GameView = { round: number, 
/**
 * The turn of the action phase (LRR 3), counted from 1; zero outside it.
 */
turn: number, phase: Phase, speaker: PlayerId, seating_order: Array<PlayerId>, 
/**
 * The order of the turns of the action phase (LRR 3, 47): by the lowest initiative of each
 * seat, an override such as the Naalu "0" token counted in its place, ties and seats with no
 * card by seating. Empty, and absent, outside the action phase.
 */
initiative_order?: Array<PlayerId>, active_player?: PlayerId, finished: boolean, players: Array<PlayerView>, board: BoardView, table: TableView, };
