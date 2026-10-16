// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";

/**
 * Public invasion boundary, independent of whichever nested choice is currently offered.
 */
export type InvasionDie = { planet: PlanetId, player: PlayerId, 
/**
 * Ground pools are named by combat threshold, not by an individual unit.
 */
group: string, face: number, target: number, hit: boolean, };
