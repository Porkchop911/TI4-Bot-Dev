// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BattleFacts } from "./BattleFacts.ts";
import type { CargoPool } from "./CargoPool.ts";
import type { FleetSupplyFact } from "./FleetSupplyFact.ts";
import type { ShipFact } from "./ShipFact.ts";

export type MovementFacts = { active: string, ships: Array<ShipFact>, cargo: Array<CargoPool>, 
/**
 * Gravity Drive can still give one ship of this action its move.
 */
gravity_drive: boolean, 
/**
 * The Ionian Fuel Refinery can still give one ship its move.
 */
ionian: boolean, fleet_supply: FleetSupplyFact, 
/**
 * How many ships already moved in this activation.
 */
moved: number, 
/**
 * The space combat that ships moving in start: set only when another seat has ships in
 * the active system.
 */
battle?: BattleFacts, };
