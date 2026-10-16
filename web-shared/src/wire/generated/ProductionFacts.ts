// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BuildFact } from "./BuildFact.ts";
import type { FleetSupplyFact } from "./FleetSupplyFact.ts";
import type { LimitFact } from "./LimitFact.ts";
import type { PayingPlanet } from "./PayingPlanet.ts";

export type ProductionFacts = { 
/**
 * The system that produces.
 */
system: string, 
/**
 * The production limit of this use, and the units that it already made.
 */
capacity: LimitFact, units: Array<BuildFact>, 
/**
 * What the combined cost of this use is still reduced by (Sarween Tools).
 */
discount: number, 
/**
 * Resources that are paid and not used yet.
 */
credit: number, 
/**
 * Every unit of this use costs nothing.
 */
free?: boolean, 
/**
 * The ready planets of the seat that pay resources.
 */
planets: Array<PayingPlanet>, trade_goods: number, 
/**
 * What one trade good pays.
 */
trade_good_worth: number, fleet_supply: FleetSupplyFact, 
/**
 * The capacity of the ships of the seat in the system, and what uses it.
 */
transport: LimitFact, };
