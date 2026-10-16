// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.
import type { BuildFact } from "./BuildFact.ts";
import type { FleetCountsFact } from "./FleetCountsFact.ts";
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
 * The ready planets of the seat that pay resources, each with its best face.
 */
planets: Array<PayingPlanet>, trade_goods: number, 
/**
 * Commodities that the seat may spend as trade goods in this payment (the Keleres agent).
 */
commodities?: number, 
/**
 * What one trade good pays.
 */
trade_good_worth: number, fleet_supply: FleetSupplyFact, 
/**
 * The capacity of the ships of the seat in the system, and what uses it.
 */
transport: LimitFact, 
/**
 * What the fleet supply and the capacity in the system are read from, before the build.
 * A fighter that the seat produces counts as its own kind (Fighter II).
 */
fleet: FleetCountsFact, };
