// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BoardTileView } from "./BoardTileView.ts";
import type { CombatView } from "./CombatView.ts";
import type { InvasionView } from "./InvasionView.ts";
import type { SystemId } from "./SystemId.ts";
import type { SystemView } from "./SystemView.ts";

/**
 * Redacted view of the galaxy board.
 */
export type BoardView = { systems: { [key in SystemId]: SystemView }, active_system?: SystemId, map_tiles?: Array<BoardTileView>, combat?: CombatView, invasion?: InvasionView, };
