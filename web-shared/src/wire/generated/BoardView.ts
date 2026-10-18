// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BoardTileView } from "./BoardTileView.ts";
import type { CombatView } from "./CombatView.ts";
import type { GroundStepView } from "./GroundStepView.ts";
import type { InvasionView } from "./InvasionView.ts";
import type { SpaceRoundView } from "./SpaceRoundView.ts";
import type { SystemId } from "./SystemId.ts";
import type { SystemView } from "./SystemView.ts";

/**
 * Redacted view of the galaxy board.
 */
export type BoardView = { systems: { [key in SystemId]: SystemView }, active_system?: SystemId, map_tiles?: Array<BoardTileView>, combat?: CombatView, invasion?: InvasionView, 
/**
 * The rounds of the space combats of the turn, as they resolved: a combat that began and
 * ended between two updates is here too. Cleared when the turn ends.
 */
space_rounds?: Array<SpaceRoundView>, 
/**
 * The steps of the invasions of the turn (bombardment, space cannon, each ground round),
 * as they resolved. Cleared when the turn ends.
 */
ground_steps?: Array<GroundStepView>, };
