// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { PlanetId } from "./PlanetId.ts";
import type { PlayerId } from "./PlayerId.ts";
import type { UnitTypeId } from "./UnitTypeId.ts";

/**
 * Redacted view of a unit placed on the board.
 */
export type PlacedUnitView = { unit_type: UnitTypeId, owner: PlayerId, planet: PlanetId | null, damaged: boolean, galvanized: boolean, };
