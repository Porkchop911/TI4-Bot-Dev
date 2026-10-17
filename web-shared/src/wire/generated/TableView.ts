// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { ObjectiveId } from "./ObjectiveId.ts";
import type { ObjectiveProgressView } from "./ObjectiveProgressView.ts";
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { StrategyCardId } from "./StrategyCardId.ts";

/**
 * Public table state: objectives, laws, strategy card goods.
 */
export type TableView = { revealed_objectives: Array<ObjectiveId>, scored_objectives: { [key in PlayerId]: Array<ObjectiveId> }, objective_progress?: { [key in PlayerId]: { [key in ObjectiveId]: ObjectiveProgressView } }, 
/**
 * Rule 61.16: the home planets a seat does not control, for each seat that may therefore
 * score no public objective. Seats that may score are absent. Planet control is public.
 */
home_system_lost?: { [key in PlayerId]: Array<PlanetId> }, unclaimed_strategy_cards: Array<StrategyCardId>, strategy_card_goods: { [key in StrategyCardId]: number }, laws: { [key in string]: string }, };
