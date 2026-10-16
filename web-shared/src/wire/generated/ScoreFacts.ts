// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { PointSource } from "./PointSource.ts";
import type { PublicObjective } from "./PublicObjective.ts";

/**
 * Where the points of every seat come from.
 */
export type ScoreFacts = { 
/**
 * The points that win the game.
 */
target: number, 
/**
 * The revealed public objectives in the order of the reveal, then any that a seat scored and
 * that is no longer revealed.
 */
public: Array<PublicObjective>, 
/**
 * The other sources. Secret objectives, the custodians, Imperial and Support for the Throne
 * are always there, in this order; the rest only when a seat has a point from it.
 */
sources: Array<PointSource>, 
/**
 * The points of a seat that no row above explains. Empty in a game that the engine played.
 */
unexplained?: { [key in PlayerId]: number }, };
