// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";

/**
 * One neighbour row of a planet-count comparison (Push Boundaries).
 */
export type NeighbourProgressView = { player: PlayerId, 
/**
 * Planets the progress owner controls.
 */
mine: number, 
/**
 * Planets this neighbour controls.
 */
theirs: number, 
/**
 * Strictly more planets than this neighbour, so the row counts toward the objective.
 */
ahead: boolean, };
