// Generated from the Rust types by `cargo wire-web` (ti4_wasm::wire_ts). Do not edit.

/**
 * The numbers that the fleet supply and the capacity of a seat in a system are read from
 * (`ti4_engine::fleet::FleetCounts`). A client adds the units that it stages and asks the engine
 * for the standing ([`fleet_standing`]), so the rules of 16 and 37 stay in the engine: the
 * fighters that a space dock holds, and the fighters over capacity that Fighter II puts on the
 * fleet pool.
 */
export type FleetCountsFact = { fleet_limit: number, 
/**
 * Ships that count against the fleet pool.
 */
ships: number, 
/**
 * Capacity of the ships in the space area.
 */
transport: number, 
/**
 * Capacity that ground forces in the space area use.
 */
carried: number, 
/**
 * Capacity that fighters in the space area use.
 */
fighters: number, 
/**
 * Fighters that the structures of the seat in the system hold without capacity.
 */
fighter_support: number, 
/**
 * Fighters that the capacity cannot hold count against the fleet pool (Fighter II).
 */
fighters_to_fleet_pool: boolean, 
/**
 * Such a fighter counts as half a ship.
 */
half_fighters: boolean, };
