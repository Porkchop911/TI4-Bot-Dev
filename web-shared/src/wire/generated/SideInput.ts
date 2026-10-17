// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlayerId } from "./PlayerId.ts";
import type { UnitCounts } from "./UnitCounts.ts";

/**
 * Fleet configuration for one participant in space combat.
 */
export type SideInput = { 
/**
 * Player ID whose fleet/guns at the combat system should be included.
 */
player?: PlayerId, 
/**
 * Faction identifier (e.g. "sol", "letnev", "jolnar"). If omitted and `player` is given, read from state.
 */
faction?: string, 
/**
 * Ships to field as `{"unit_id": count}` or `[["unit_id", count]]`. Added to units from state.
 */
units?: UnitCounts, 
/**
 * Ships that start damaged as `{"unit_id": count}` or `[["unit_id", count]]`.
 */
damaged?: UnitCounts, 
/**
 * Units firing space cannon before combat as `{"unit_id": count}` or `[["unit_id", count]]`.
 */
guns?: UnitCounts, };
