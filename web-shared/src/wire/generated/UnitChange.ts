// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { SystemId } from "./SystemId.ts";
import type { UnitTypeId } from "./UnitTypeId.ts";

/**
 * The count of the units of one kind at one spot, before and after.
 */
export type UnitChange = { system: SystemId, 
/**
 * `None`: the space area.
 */
planet: PlanetId | null, owner: PlayerId, unit_type: UnitTypeId, damaged: boolean, galvanized: boolean, before: number, after: number, };
