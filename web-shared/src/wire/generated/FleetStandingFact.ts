// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The fleet supply and the capacity of some counts, as the engine reads them.
 */
export type FleetStandingFact = { fleet_limit: number, 
/**
 * Ships that count against the fleet pool, with the fighters that it carries.
 */
fleet_charged: number, 
/**
 * Capacity of the ships in the space area.
 */
transport: number, 
/**
 * Capacity that is used, after the fighters that structures hold.
 */
consumed: number, 
/**
 * Fighters that the fleet pool carries in place of capacity.
 */
fighters_charged: number, 
/**
 * Units that use capacity and cannot stay (16.3).
 */
capacity_excess: number, };
